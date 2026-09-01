# Gomoku (42) — Design Specification

**Date:** 2026-09-01
**Language:** Rust (2021 edition)
**GUI:** macroquad
**Status:** Approved design, ready for implementation planning
**Audience:** the engineer or model implementing this project. Every architectural decision is made here; implementation should not need to invent structure.

---

## 1. Purpose and scope

Build `Gomoku`, a 19x19 Gomoku program with the 42 rule variant (captures, endgame capture, no double-three) and a Min-Max AI that searches at least 10 plies in under 0.5 seconds per move on average.

**In scope:** the entire mandatory part of the subject.

**Out of scope for this spec:** all bonuses (rule variants, Swap/Swap2/Pro openings). The subject states bonuses are only assessed if the mandatory part is perfect, so they are deferred. No abstraction is added in anticipation of them.

---

## 2. Requirements traceability

Every hard requirement from the subject, and where this design satisfies it.

| # | Requirement | Satisfied by |
|---|---|---|
| R1 | 19x19 board, unlimited stones | §4 board representation |
| R2 | Alignment of 5 **or more** wins | §5 `F_FIVE`, §7 `check_end` |
| R3 | Capture pairs by flanking (`X O O X`), 8 directions | §6 `captures_of` |
| R4 | Capturing 10 stones (5 pairs) wins | §7 `check_end` case 1 |
| R5 | Cannot "move into a capture" | §6, capture triggers only on the mover's own move |
| R6 | A five wins only if the opponent cannot break it by capture | §7 `five_is_unbreakable` |
| R7 | 4 pairs lost + opponent can take a fifth → opponent wins | §7 `check_end` case 2b |
| R8 | Double-three forbidden | §7 `is_legal` |
| R9 | Double-three by capture is allowed | §7 `is_legal`, capture exemption |
| R10 | Executable named `Gomoku` | §12 Makefile |
| R11 | Makefile with `$(NAME) all clean fclean re`, no relink | §12 |
| R12 | Never crashes, even on OOM | §11 robustness |
| R13 | Min-Max algorithm | §9 negamax alpha-beta |
| R14 | Search at least 10 plies | §9 iterative deepening, §13 benchmark gate |
| R15 | Under 0.5 s average per move | §9 time budget 400 ms |
| R16 | Usable graphical interface | §10 |
| R17 | Timer of AI think time displayed | §10 status bar (validation-critical) |
| R18 | Human vs AI mode | §10 |
| R19 | Hotseat mode with move suggestion | §10 |
| R20 | Debug view of AI reasoning | §10 debug panel |

---

## 3. Module structure

Seven modules, strictly descending dependencies. No cycles.

```
main.rs  →  ui.rs  →  search.rs  →  eval.rs  →  rules.rs  →  board.rs  →  patterns.rs
```

| Module | Responsibility | Must not know about |
|---|---|---|
| `patterns.rs` | Precomputed pattern lookup table | the board |
| `board.rs` | Board state, `play`/`undo`, captures, Zobrist, incremental accumulator | legality rules |
| `rules.rs` | Move legality (double-three), game end detection | the heuristic |
| `eval.rs` | Leaf evaluation from the accumulator | the search |
| `search.rs` | Negamax, alpha-beta, transposition table, ordering | rendering |
| `ui.rs` | macroquad rendering, input, timer, debug panel | search internals |
| `main.rs` | Wiring, game loop, mode selection | — |

Each module owns one file. If a file exceeds roughly 500 lines, that is a signal its responsibility has drifted, not a reason to add a module.

---

## 4. Coordinate system and core types

The board is stored **padded**: a 27x27 array with a 4-cell border of `Cell::Wall`. This removes every bounds check and every edge special case from the hot path, at a cost of 729 bytes.

```rust
pub const SIZE: usize = 19;
pub const PAD: usize = 4;
pub const STRIDE: usize = SIZE + 2 * PAD;   // 27
pub const TOTAL: usize = STRIDE * STRIDE;   // 729

pub type Idx = u16;   // index into the padded array

#[inline]
pub const fn idx(x: usize, y: usize) -> Idx {
    ((y + PAD) * STRIDE + (x + PAD)) as Idx
}

#[inline]
pub const fn to_xy(i: Idx) -> (usize, usize) {
    let i = i as usize;
    (i % STRIDE - PAD, i / STRIDE - PAD)
}

/// The four axes. Each is bidirectional: walking `+d` and `-d` covers all 8 directions.
pub const DIRS: [i16; 4] = [
    1,                    // horizontal
    STRIDE as i16,        // vertical
    STRIDE as i16 + 1,    // diagonal down-right
    STRIDE as i16 - 1,    // diagonal down-left
];

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Cell { Empty = 0, Black = 1, White = 2, Wall = 3 }

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Player { Black = 0, White = 1 }

impl Player {
    #[inline] pub fn other(self) -> Player;
    #[inline] pub fn cell(self) -> Cell;   // Black -> Cell::Black
}
```

The padding is 4 cells because the pattern window reaches 4 cells either side of a stone (§5). Any index derived as `center + k * dir` for `k` in `-4..=4` is guaranteed in bounds.

---

## 5. `patterns.rs` — the pattern lookup table

### 5.1 Window and encoding

For a stone at index `c` and axis `d`, the **window** is the 9 cells at `c + k*d` for `k` in `-4..=4`. The center is at window position 4.

Each cell is encoded **relative to the player being scored**:

| Cell state | Code |
|---|---|
| `Empty` | 0 |
| the player's own stone | 1 |
| opponent stone **or** `Wall` | 2 |

`Wall` collapses to the same code as an opponent stone. This is exact for every pattern property in this table: a wall blocks an alignment extremity exactly as an opponent stone does, and a wall can never be one of the player's own stones. The collapse keeps the table at `3^9 = 19683` entries.

**Captures are the one exception and are deliberately NOT in this table.** `Wall Mine Mine Empty` is not capturable, because the opponent cannot play on a wall, whereas `Opp Mine Mine Empty` is. Capture detection and capture-vulnerability therefore read cells directly (§6.3, §8.3). Rejected alternative: a base-4 table with `4^9 = 262144` entries (~1 MB), which is worse in cache for no gain in clarity.

```rust
pub const W: usize = 9;
pub const C: usize = 4;
pub const TABLE_SIZE: usize = 19683;   // 3^9
pub const POW3: [u32; W] = [1, 3, 9, 27, 81, 243, 729, 2187, 6561];
```

The window code is `sum over k in 0..9 of code(cell at c + (k-4)*d) * POW3[k]`.

### 5.2 Why 9 cells is exactly the right size

The largest property the table must judge is "does this window contain an open four through the center". A four containing the center spans at most positions `center-3 ..= center+3`, and judging its openness requires the two cells at `center-4` and `center+4`. Nothing outside `center +/- 4` can affect any property in this table. A smaller window would be wrong; a larger one would waste table space.

### 5.3 Entry

```rust
pub const F_FIVE:       u8 = 1 << 0;
pub const F_OPEN_FOUR:  u8 = 1 << 1;
pub const F_FOUR:       u8 = 1 << 2;   // four that is not open (blocked end, or one gap)
pub const F_FREE_THREE: u8 = 1 << 3;
pub const F_THREE:      u8 = 1 << 4;   // three that is not free

#[derive(Copy, Clone, Default)]
pub struct Pat {
    pub score: i32,
    pub flags: u8,
}

pub struct PatternTable {
    entries: Box<[Pat]>,   // length TABLE_SIZE
}

impl PatternTable {
    /// Builds the whole table. Called once at startup. Must be deterministic.
    pub fn build() -> Self;

    #[inline]
    pub fn get(&self, code: u32) -> Pat;
}
```

Only entries whose center code is 1 are ever queried (windows are always centered on a stone of the player being scored). Entries with any other center are left at `Pat::default()`. Building all 19683 keeps indexing branchless.

### 5.4 Classification algorithm

For each code, decode the window into `w: [u8; 9]`. If `w[4] != 1`, leave the entry zeroed. Otherwise:

**Step 1 — contiguous run.** Extend left from position 4 while `w[i] == 1`, giving `l`; extend right giving `r`. Let `n = r - l + 1`.

- If `n >= 5`, set `F_FIVE`.
- Otherwise let `open_left = (w[l-1] == 0)` and `open_right = (w[r+1] == 0)`, guarding `l-1 >= 0` and `r+1 <= 8`. Both are always in range for `n <= 4`: a run of `n` through position 4 has `l >= 4-(n-1)` and `r <= 4+(n-1)`, so `n <= 4` gives `l >= 1` and `r <= 7`. Runs of 5 or more are short-circuited by `F_FIVE` and never reach this step.
- If `n == 4`: set `F_OPEN_FOUR` when both ends are open, else `F_FOUR` when at least one end is open.
- If `n == 3`: set `F_THREE` when at least one end is open.
- If `n == 2` or `n == 1`: record open-end count for scoring only.

**Step 2 — gapped patterns.** A contiguous run misses `X X . X` and `. X . X X .`. Detect these by the *constructive* test below, which subsumes them.

**Step 3 — constructive four test (`F_FOUR`).** If `F_FIVE` is not set, for each position `e` in `0..=8` with `w[e] == 0`: set `w[e] = 1`, recheck for a run of 5 through the center. If found, the original window is a four (gapped or contiguous). Set `F_FOUR`. Restore `w[e]`. The range is `0..=8` here, wider than in step 4: a five through the center can start at position 0, so the completing cell may sit at either extreme of the window.

**Step 4 — constructive free-three test (`F_FREE_THREE`).** This is the subject's definition made operational: *a free-three is a three that, if not blocked, permits an open four.*

For each position `e` in `1..=7` with `w[e] == 0`: set `w[e] = 1` and test whether the result has `F_OPEN_FOUR` (a contiguous run of exactly 4 through the center with both extremities empty). If any such `e` exists, and the original window is not already a four or five, set `F_FREE_THREE`. Restore `w[e]`.

This definition correctly accepts both of the subject's appendix examples: the contiguous `. X X X .` and the gapped `. X . X X .`. It correctly rejects a three with a blocked end, because no single addition can then produce a four with two free extremities.

**Step 5 — score.** Assign from the highest flag set:

| Condition | `score` |
|---|---|
| `F_FIVE` | 10_000_000 |
| `F_OPEN_FOUR` | 500_000 |
| `F_FOUR` | 50_000 |
| `F_FREE_THREE` | 20_000 |
| `F_THREE` | 2_000 |
| run of 2, both ends open | 300 |
| run of 2, one end open | 50 |
| run of 1, both ends open | 5 |
| otherwise | 0 |

These weights are the initial calibration, not a law. They live in one `const` block in `patterns.rs` so they can be tuned from a single place. Expect to revise them after playing games.

### 5.5 Correctness test (mandatory)

Write an independent, deliberately naive oracle that classifies a window by brute force, and assert it agrees with `PatternTable::build()` on **all 19683 codes**. This test is the foundation of both the rules and the heuristic; if it passes, most rule bugs become impossible. Write it first.

---

## 6. `board.rs` — state, moves, captures

### 6.1 Structure

```rust
pub struct Board {
    cells: [Cell; TOTAL],
    /// Number of *stones* captured by each player. Win at 10.
    pub captures: [u8; 2],
    pub to_move: Player,
    pub zobrist: u64,
    pub stone_count: u32,
    /// Count of stones within Chebyshev radius 2. Drives candidate generation.
    neighbor: [u8; TOTAL],
    /// Incremental heuristic accumulator, one per player. See §8.
    /// `pub` because `eval.rs` reads it directly; only `board.rs` ever writes it.
    pub acc: [i32; 2],
}
```

`Undo` carries everything needed to restore exactly:

```rust
pub struct Undo {
    pub mv: Idx,
    pub captured: [Idx; 16],   // theoretical max: 4 axes x 2 directions x 2 stones
    pub n_captured: u8,
    pub prev_zobrist: u64,
    pub prev_acc: [i32; 2],
    pub prev_captures: [u8; 2],
}
```

### 6.2 Zobrist keys

Generated at startup by a fixed-seed xorshift64 so runs are reproducible and no `rand` dependency is added. Keys: `[[u64; TOTAL]; 2]` plus one `side_to_move` key. Captures are *not* hashed; two positions with identical stones but different capture counts are distinguished by folding `captures[0]` and `captures[1]` into the key with two small key arrays (`[[u64; 11]; 2]`), because capture counts change evaluation and win conditions.

### 6.3 Capture detection

```rust
/// Returns the stones captured if `p` plays at `mv`. Does not mutate.
/// A capture is the exact pattern `p O O p` starting at `mv`, walking outward
/// along one direction. Reads 3 cells per direction, 8 directions.
pub fn captures_of(&self, mv: Idx, p: Player) -> ([Idx; 16], usize);
```

For each of the 4 axes and each sign, check `cell(mv + d) == opp && cell(mv + 2d) == opp && cell(mv + 3d) == p.cell()`. `Wall` fails all three tests, so board edges need no special handling.

R5 ("cannot move into a capture") requires no code: captures are only computed for the player who is moving. A player placing a stone into the middle of `X _ O X` simply produces no capture for anyone.

### 6.4 `play` and `undo`

```rust
/// Applies `mv` for `self.to_move`. Assumes the move is legal (call `rules::is_legal` first).
pub fn play(&mut self, mv: Idx, pt: &PatternTable) -> Undo;

/// Exactly reverses a `play`. Must restore cells, captures, zobrist, acc,
/// neighbor, stone_count and to_move bit for bit.
pub fn undo(&mut self, u: &Undo);
```

`play` order of operations, which the accumulator update depends on:

1. Save `prev_*` fields into `Undo`.
2. **Subtract** the accumulator contribution of every occupied cell within 4 steps of `mv` along each of the 4 axes (both players), for that axis only. See §8.2.
3. Write the stone; update `zobrist`, `stone_count`, `neighbor` (+1 in radius 2).
4. Compute captures. For each captured stone: subtract its neighbours' contributions along the 4 axes, remove the stone, update `zobrist`/`neighbor`, and record it in `Undo`.
5. **Add** the contribution of every occupied cell within 4 steps of `mv` and of each captured cell, along each axis, plus the new stone's own 4 contributions.
6. Update `captures[p]`, flip `to_move`.

`undo` restores `cells`, `neighbor` and `stone_count` by replaying the recorded changes in reverse, then assigns the saved `zobrist`, `acc` and `captures` directly rather than recomputing them.

**Test (mandatory):** a property test that plays 50 random legal moves, undoes all of them, and asserts every field of `Board` is byte-identical to the starting state. Run it over at least 1000 random seeded sequences. `play`/`undo` asymmetry is the single most likely source of a silently wrong AI.

---

## 7. `rules.rs` — legality and game end

```rust
pub fn is_legal(b: &Board, mv: Idx, p: Player, pt: &PatternTable) -> bool;

/// Number of axes on which placing `p` at `mv` creates a free-three.
pub fn count_free_threes(b: &mut Board, mv: Idx, p: Player, pt: &PatternTable) -> u8;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum GameEnd { None, Win(Player), Draw }

pub fn check_end(b: &mut Board, last: Idx, pt: &PatternTable) -> GameEnd;

/// All legal candidate moves for the side to move, unordered.
pub fn generate(b: &Board, p: Player, pt: &PatternTable, out: &mut Vec<Idx>);
```

### 7.1 `is_legal`

1. `mv` must be `Cell::Empty` and inside the playing area (not `Wall`).
2. If the move captures at least one pair, it is legal. The subject states explicitly that introducing a double-three by capturing is allowed. **This check must come before the double-three check.**
3. Otherwise, if `count_free_threes(b, mv, p, pt) >= 2`, the move is illegal.

### 7.2 `count_free_threes`

Temporarily place the stone, query the pattern table for each of the 4 axes, count axes where `F_FREE_THREE` is set, remove the stone. Uses a scratch place/remove that does not touch the accumulator or Zobrist, for speed.

> **`ponytail:` known ceiling.** Counts at most one free-three per axis. A single move creating two *disjoint* free-threes on the same axis is counted once. This is what standard implementations do and matches every example in the subject. Upgrade path: scan the axis for disjoint free-three spans if a grader raises it.

### 7.3 `check_end` — including the endgame capture rule

Evaluated after each move, with `last` being the move just played by player `p`:

1. **Win by capture.** If `b.captures[p] >= 10`, return `Win(p)`.
2. **Win by alignment.** If the move created a five (any axis has `F_FIVE`):
   - **2a.** Collect the cells of that alignment by walking outward from `last` along the axis.
   - **2b.** If `p` has already **lost** 4 pairs (`b.captures[p.other()] >= 8`) and any legal opponent move captures a pair, return `None`: the opponent is about to win by capture and the game continues. Note the direction — `captures[q]` counts stones taken *by* `q`, so "`p` has lost 4 pairs" reads as `captures[opponent] >= 8`. Getting this index backwards is an easy and invisible bug.
   - **2c.** For each opponent candidate move, if it captures a pair in which **at least one stone belongs to the five**, the five is breakable. Return `None`.
   - **2d.** Otherwise return `Win(p)`.
3. **Draw.** If no legal move exists for the side to move, return `Draw`. (With `SIZE * SIZE = 361` intersections and captures freeing cells, this is nearly unreachable, but it must not hang.)

Steps 2b and 2c enumerate opponent moves, which is expensive. They run only when a five actually appears, which is rare, so the cost is irrelevant. During search, the same function is used so the AI never over-values a breakable five — this is the rule most implementations get wrong and the one a grader will probe.

### 7.4 `generate`

Candidates are empty cells with `neighbor[i] > 0` (at least one stone within radius 2), filtered by `is_legal`. On an empty board, return the center cell only. Ordering and truncation happen in `search.rs`, not here.

### 7.5 Fixture tests (mandatory)

One test per appendix figure, encoded as a literal board:

- Diagonal capture: blue plays `a`, the red pair is removed, both intersections become playable again.
- No move into a capture: red plays `a` in `Blue Red _ Blue` and loses nothing.
- Free-three, contiguous diagonal form.
- Free-three, gapped form `. X . X X .`.
- Double-three at `a` is rejected.
- The same position with a blue stone at `b`: `a` becomes legal.
- A double-three that also captures a pair: legal.
- Five broken by a capture: not a win.
- Five that cannot be broken: a win.
- Opponent at 4 pairs lost with a capture available: opponent wins.

---

## 8. `eval.rs` — the heuristic

The subject calls the heuristic the hardest part and grades the ability to explain it. This section defines it precisely enough to explain at a defense without reading the code.

### 8.1 Definition

The score of a position for player `p` is:

```
acc[p] = sum over every cell c holding a stone of p,
         sum over the 4 axes d,
         pattern_table[window_code(c, d, p)].score
```

Each of a player's stones is scored once per axis. A stone in a strong shape contributes a large value on the axis of that shape and near-zero on the other three.

An alignment is counted once per stone it contains, so a five is counted about five times. This over-counting is **deliberate and uniform**: it is monotone in alignment strength, so it never inverts a comparison between two positions, and it makes the accumulator exactly additive, which is what makes incremental update possible. Explaining this trade-off is part of the defense.

### 8.2 Incremental update

Placing a stone at `mv` changes window codes only for cells within 4 steps of `mv` along one of the 4 axes. Everything else is untouched. So:

```
for each axis d in DIRS:
    for k in -4..=4, k != 0:
        let c = mv + k*d
        if cells[c] is a stone of player q:
            acc[q] -= table[window_code(c, d, q)].score      // before the move
```

then place the stone, then the symmetric loop adding the new contributions, plus the new stone's own four windows.

Empty cells and walls contribute nothing and are skipped, so on a sparse board this loop touches only a handful of cells. Removing captured stones uses the same routine.

**The accumulator must never be recomputed from scratch during search.** A `debug_assert` comparing the incremental accumulator against a full recomputation, enabled in tests only, guards against drift.

### 8.3 Capture terms

Two contributions are not expressible in the base-3 table because they depend on distinguishing `Wall` from an opponent stone (§5.1). They are maintained in the same incremental walk, with a direct 4-cell read.

**Capture progress bonus,** indexed by pairs captured (0 to 5):

```rust
pub const CAP_BONUS: [i32; 6] = [0, 4_000, 12_000, 30_000, 90_000, 10_000_000];
```

Non-linear on purpose: the fourth pair is far more valuable than the first, because it puts the opponent one capture from losing and simultaneously makes every one of their fives breakable.

**Vulnerability penalty.** A stone is vulnerable on an axis if it forms a pair with a neighbour and the pair's two flanking cells are one opponent stone and one empty cell — that is, `Opp Mine Mine Empty` or `Empty Mine Mine Opp`. `Wall` counts as neither. Each vulnerable pair costs `-1_200`.

Concretely, for a stone at `c` on axis `d`, with `M` = own stone, `O` = opponent stone (not wall), `.` = empty:

- pair `(c, c+d)`: vulnerable if (`c-d` is `O` and `c+2d` is `.`) or (`c-d` is `.` and `c+2d` is `O`)
- pair `(c-d, c)`: vulnerable if (`c-2d` is `O` and `c+d` is `.`) or (`c-2d` is `.` and `c+d` is `O`)

### 8.4 Leaf evaluation

Negamax convention: always from the point of view of the side to move.

```rust
pub const WIN: i32 = 100_000_000;

#[inline]
pub fn evaluate(b: &Board) -> i32 {
    let me = b.to_move;
    let op = me.other();
    (b.acc[me as usize] + CAP_BONUS[(b.captures[me as usize] / 2) as usize])
  - (b.acc[op as usize] + CAP_BONUS[(b.captures[op as usize] / 2) as usize])
}
```

The vulnerability penalty is already folded into `acc` by the incremental walk.

Terminal scores are `WIN - ply` for a win and `-WIN + ply` for a loss, so the search prefers faster wins and slower losses.

---

## 9. `search.rs` — Min-Max

### 9.1 Public interface

```rust
pub struct SearchConfig {
    pub max_depth: u8,          // 10 minimum; 20 is a reasonable cap
    pub time_budget_ms: u64,    // 400
    pub max_candidates: usize,  // 20
}

pub struct SearchStats {
    pub depth_reached: u8,
    pub nodes: u64,
    pub elapsed: Duration,
    pub pv: Vec<Idx>,
    pub root_scores: Vec<(Idx, i32)>,   // for the debug panel
    pub tt_hits: u64,
    pub tt_probes: u64,
}

pub fn find_best_move(
    b: &mut Board,
    cfg: &SearchConfig,
    pt: &PatternTable,
    tt: &mut TranspositionTable,
) -> (Idx, SearchStats);
```

### 9.2 Algorithm

**Negamax with fail-soft alpha-beta**, wrapped in **iterative deepening** from depth 1 to `max_depth`, stopping when the time budget is spent. The best move from the last *completed* iteration is always returned, so an interrupted search is still valid.

Aspiration windows: after depth 3, search `[prev - 50, prev + 50]`; on a fail, re-search with a full window.

**Principal Variation Search:** the first move at a node is searched with the full window; the rest with a null window `[alpha, alpha+1]`, re-searched fully only if they beat alpha.

### 9.3 Move ordering

Ordering is what makes depth 10 reachable — alpha-beta approaches `b^(d/2)` nodes only when the best move is tried first. Score each candidate, sort descending, take the top `max_candidates`:

| Priority | Condition | Score |
|---|---|---|
| 1 | Transposition table move | 1_000_000 |
| 2 | Creates a five (immediate win) | 900_000 |
| 3 | Blocks an opponent five or open four | 800_000 |
| 4 | Creates an open four | 700_000 |
| 5 | Captures | 500_000 + 1_000 * pairs |
| 6 | Killer move 1 / 2 at this ply | 400_000 / 390_000 |
| 7 | History heuristic `history[mv]` | capped at 300_000 |
| 8 | Static gain: accumulator delta of playing there | raw |

Killers: two slots per ply, updated when a move causes a beta cutoff. History: `history[mv] += depth * depth` on a cutoff, halved for all entries at each new root iteration to prevent overflow and staleness.

### 9.4 Candidate truncation — the central trade-off

`generate` typically yields 40 to 80 legal moves in the middlegame. Searching all of them to depth 10 is far outside the time budget. The search keeps only the **top 20 by ordering score**.

This makes the search **non-exhaustive**: a strong move ranked 21st is invisible. This is the deliberate cost of reaching depth 10, and it must be stated plainly at the defense rather than glossed over. It is mitigated by putting all forcing moves (fives, blocks, open fours, captures) in the top priorities, so tactically critical moves are never truncated away.

`max_candidates` is a tuning knob, not a constant of nature. If benchmarks show depth 10 is not reached inside 400 ms, lower it to 14 before touching anything else.

### 9.5 Pruning and extensions

- **Late move reductions:** at depth >= 3, for candidates at index >= 8 that are not captures and create no threat, reduce depth by 1 (by 2 at index >= 16). If a reduced search beats alpha, re-search at full depth.
- **Threat extension:** if the side to move faces a four and must respond, extend by 1 ply. Cap total extensions per line at 4 to prevent explosion.
- **Immediate win shortcut:** at the root, if any move creates an unbreakable five, play it without searching.
- **Forced block shortcut:** if the opponent has a four, restrict candidates to moves that block it or capture a stone out of it.
- **No null-move pruning.** In Gomoku, forcing threat sequences make the null-move assumption unsound, and passing is never legal. Do not add it.

### 9.6 Transposition table

```rust
#[derive(Copy, Clone, Default)]
pub struct TtEntry {
    pub key: u64,
    pub score: i32,
    pub mv: Idx,
    pub depth: u8,
    pub bound: Bound,   // Exact | Lower | Upper
}

pub struct TranspositionTable { entries: Vec<TtEntry>, mask: usize }

impl TranspositionTable {
    /// Tries 2^21 entries, then 2^18, then 2^15, using `try_reserve`.
    /// Never panics; returns the largest table that could be allocated.
    pub fn new() -> Self;
    pub fn probe(&self, key: u64) -> Option<TtEntry>;
    pub fn store(&mut self, key: u64, e: TtEntry);
    pub fn clear(&mut self);
}
```

Index by `key as usize & mask`. Replacement policy: depth-preferred — overwrite only if the new entry's depth is greater than or equal to the stored one, or the stored key differs.

The table is **not** cleared between moves; it is cleared on a new game. Carrying it across moves is a large part of the speedup.

### 9.7 Time control

A node counter checks the clock every 2048 nodes. On expiry, set an abort flag that unwinds the recursion without using the result of the incomplete iteration. The move from the last completed depth is returned.

---

## 10. `ui.rs` and `main.rs` — interface

macroquad, single window, roughly 900x1000 logical pixels.

### 10.1 Screens

**Menu:** three buttons — *Play as Black vs AI*, *Play as White vs AI*, *Hotseat*. Start.

**Game:** the goban, a right or bottom status bar, and an optional debug overlay.

### 10.2 Board rendering

- 19x19 grid with the traditional star points.
- Stones as filled circles with a subtle outline; the last move carries a small marker.
- Hover preview of the stone under the cursor.
- Illegal target under the cursor is shown in red with a one-line reason ("double-three interdit").
- Click to place. Illegal clicks show a transient toast and change nothing.

### 10.3 Status bar — validation-critical

The subject fails the project outright without a visible timer. The status bar always shows:

- **Time for the AI's last move**, in milliseconds.
- **Average AI move time** over the game.
- **Depth reached** on the last search.
- Captured stone counts for both players.
- Whose turn it is.

In hotseat mode the status bar also holds a **Suggest** button, which runs the same search for the player to move and highlights the returned move without playing it. Its think time is displayed the same way.

### 10.4 Debug panel (key `D`)

Toggles an overlay showing:

- Nodes searched and nodes per second.
- Depth reached, and time spent per iterative-deepening iteration.
- The principal variation in board coordinates.
- The top 5 root moves with their scores.
- Transposition table hit rate.

This satisfies R20 and is the material for explaining the algorithm at the defense.

### 10.5 Game loop (`main.rs`)

```
build pattern table
allocate transposition table
loop:
    render
    if human to move: read input, validate with rules::is_legal, play
    if AI to move:    run find_best_move under catch_unwind, play
    after each move: rules::check_end -> maybe show the result banner
```

The search runs synchronously. At a 400 ms budget the UI freeze is short enough not to matter, and a worker thread would add a channel, a cancellation path and a failure mode for no visible gain.

> **`ponytail:` known ceiling.** Synchronous search blocks rendering for up to 400 ms. Move it to a worker thread only if the freeze proves distracting during the defense.

---

## 11. Robustness — the no-crash requirement

The subject awards 0 for any crash, including out of memory. The defence is layered:

1. `#![forbid(unsafe_code)]` at the crate root.
2. Clippy lints denied: `unwrap_used`, `expect_used`, `panic`, `indexing_slicing` in the engine modules (the UI may index its own fixed arrays).
3. All array access in the hot path goes through the padded-index invariant, which is guaranteed by construction (§4), not by runtime checks.
4. The transposition table allocates with `Vec::try_reserve` and degrades to smaller sizes rather than aborting (§9.6).
5. `std::panic::catch_unwind` wraps the call to `find_best_move`. If it ever unwinds, the program logs the event to the debug panel and plays the first legal move from `generate`. The game continues.
6. `Cargo.toml` must **not** set `panic = "abort"`, or `catch_unwind` cannot work.
7. Every recursion is depth-bounded; every loop over the board is bounded by `TOTAL`.

---

## 12. Makefile

The binary must be named `Gomoku` (capital G) and the Makefile must not relink.

```make
NAME   := Gomoku
TARGET := target/release/gomoku
SRCS   := $(shell find src -name '*.rs') Cargo.toml Cargo.lock

all: $(NAME)

$(NAME): $(TARGET)
	cp $(TARGET) $(NAME)

$(TARGET): $(SRCS)
	cargo build --release

clean:
	cargo clean

fclean: clean
	rm -f $(NAME)

re: fclean all

.PHONY: all clean fclean re
```

No-relink property: `$(NAME)` depends on `$(TARGET)`, which depends on the sources. With sources unchanged, `cargo build` is a no-op and does not touch the binary's mtime, so `$(TARGET)` stays older than `$(NAME)` and the `cp` does not rerun. Verify by running `make` twice and confirming the second run prints nothing but "up to date".

`Cargo.toml` must declare the binary name explicitly so `$(TARGET)` is correct:

```toml
[[bin]]
name = "gomoku"
path = "src/main.rs"

[profile.release]
opt-level = 3
lto = true
codegen-units = 1
panic = "unwind"   # required for catch_unwind
```

Only one dependency: `macroquad`. Pin an exact version.

---

## 13. Testing strategy

Tests are ordered by how much risk they remove per line written.

| # | Test | Why |
|---|---|---|
| 1 | Pattern table vs naive oracle, all 19683 codes | Foundation of rules and heuristic. Write first. |
| 2 | `play`/`undo` property test, 1000 random sequences | A silent asymmetry corrupts the entire search. |
| 3 | Appendix fixtures (§7.5), one per figure | Direct evidence for the grader that the rules are right. |
| 4 | Endgame capture cases | The rule most implementations get wrong. |
| 5 | Mate in 1 and mate in 3 positions | The search finds forced wins. |
| 6 | Search determinism: identical input, identical output | Catches uninitialised state and TT bugs. |
| 7 | Accumulator drift: incremental vs full recomputation | Guards §8.2. `debug_assert` in tests only. |
| 8 | Benchmark gate (§14) | The two hard numeric requirements. |

No test framework beyond `#[test]`. No fixtures crate. Random sequences use a seeded xorshift written inline, so tests are reproducible and add no dependency.

---

## 14. Performance targets and measurement

| Metric | Target | Hard requirement |
|---|---|---|
| Average move time | under 400 ms | under 500 ms (subject) |
| Depth reached | 10 or more | 10 (subject) |
| Nodes per second | 3M or more | none |
| Startup time (table build) | under 50 ms | none |

A benchmark test loads 10 recorded middlegame positions, runs `find_best_move` on each, and asserts both the average time and the minimum depth. It runs in release mode only. **This benchmark is the project's validation gate — if it fails, nothing else matters.**

Tuning order when the gate fails, cheapest first:

1. Lower `max_candidates` from 20 to 14.
2. Check the transposition table hit rate; if it is below 20%, the Zobrist update is likely wrong.
3. Check move ordering quality: the first move should cause a cutoff at least 85% of the time.
4. Only then consider parallel search (deliberately excluded from this design).

---

## 15. Defense preparation

The subject grades the ability to explain, not only the ability to build. Three explanations must be rehearsed:

**The algorithm.** Negamax with alpha-beta, deepened iteratively under a clock, ordered by a transposition table move and killer heuristics, with the candidate set restricted to cells near existing stones and truncated to the best 20.

**The heuristic.** A precomputed table of 19683 nine-cell patterns. Every stone is scored once per axis by a single table lookup. The board score is the sum, maintained incrementally so a move costs a handful of lookups instead of a full board scan.

**The trade-offs made and why.** Candidate truncation costs completeness for depth. Over-counting in the accumulator costs absolute precision for additivity. `Wall` collapsed onto opponent stones costs a separate capture path for a table that fits in L2. Each was a choice with a reason, and each has a stated upgrade path.

---

## 16. Explicitly out of scope

- All bonuses: rule selection, Pro/Swap/Swap2 openings.
- Parallel search.
- Opening book.
- Network or AI-vs-AI play.
- Game save and load.

None of these have hooks or abstractions prepared for them. Adding one later means writing it then, which is cheaper than carrying unused structure now.
