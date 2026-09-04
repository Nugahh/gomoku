### Task 10: `search.rs` — move ordering and candidate truncation

**Files:**
- Modify: `src/board.rs` (add `hypothetical_window_code`)
- Modify: `src/search.rs`

**Interfaces:**
- Consumes: `board::{Board, hypothetical_window_code}` (new in this task), `patterns::{F_FIVE, F_OPEN_FOUR}`.
- Produces: `search::{order_score, SearchCtx::new}` (private to the module, used by `negamax`'s move loop and by Tasks 11-12).

- [ ] **Step 1: Add `hypothetical_window_code` to `board.rs`**

Ordering needs to ask "if `p` played at this empty cell, what would the resulting pattern be?" without actually mutating the board 40-80 times per node (`captures_of` is already pure-read and cheap; the missing piece is a pure-read version of `window_code` that treats the center as filled). Add to `impl Board` in `src/board.rs`, near `window_code`:

```rust
    /// Like `window_code`, but treats the center (`c`) as if it already
    /// held `p`'s stone, regardless of what's actually there. Used only
    /// for move ordering, where `c` is always an empty candidate cell and
    /// mutating the board via `play`/`undo` to test each one would be far
    /// too slow to run on 40-80 candidates at every search node.
    pub fn hypothetical_window_code(&self, c: Idx, d: i16, p: Player) -> u32 {
        let mut code = 0u32;
        for (slot, k) in (-4..=4i32).enumerate() {
            let trit: u32 = if k == 0 {
                1
            } else {
                let cell = self.cell_at(c, k * d as i32);
                if cell == Cell::Empty {
                    0
                } else if cell == p.cell() {
                    1
                } else {
                    2
                }
            };
            code += trit * POW3.get(slot).copied().unwrap_or(0);
        }
        code
    }
```

Run: `cargo build --release` — expect success (unused-method warning only).

- [ ] **Step 2: Update Task 9's test to use a constructor, in preparation for new `SearchCtx` fields**

In `src/search.rs`'s test module, replace the `SearchCtx { ... }` literal in `negamax_recognizes_an_immediate_win` with:

```rust
        let mut ctx = SearchCtx::new(&pt, &mut tt, &cfg, far_deadline());
```

removing the multi-line literal it replaces. This is a mechanical edit — `SearchCtx::new` (added in Step 4 below) takes the same four values the literal set explicitly and fills the new killers/history/tt-stat fields with their zero values.

- [ ] **Step 3: Write the failing test — ordering ranks a winning move above a quiet one**

Add to `src/search.rs`'s test module:

```rust
    #[test]
    fn order_score_ranks_five_above_quiet_move() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        for x in 4..8 {
            b.to_move = Player::Black;
            b.play(idx(x, 5), &pt);
        }
        let history = [0i32; crate::board::TOTAL];
        let no_killers = (Idx::MAX, Idx::MAX);
        let winning_score = order_score(
            &b, &pt, idx(8, 5), Player::Black, Player::White, None, no_killers, &history,
        );
        let quiet_score = order_score(
            &b, &pt, idx(15, 15), Player::Black, Player::White, None, no_killers, &history,
        );
        assert_eq!(winning_score, ORD_FIVE);
        assert!(winning_score > quiet_score);
    }

    #[test]
    fn order_score_ranks_tt_move_above_everything() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        for x in 4..8 {
            b.to_move = Player::Black;
            b.play(idx(x, 5), &pt);
        }
        let history = [0i32; crate::board::TOTAL];
        let tt_move_score = order_score(
            &b, &pt, idx(15, 15), Player::Black, Player::White,
            Some(idx(15, 15)), (Idx::MAX, Idx::MAX), &history,
        );
        assert_eq!(tt_move_score, ORD_TT);
    }
```

- [ ] **Step 4: Run test to verify it fails**

Run: `cargo test --lib search:: -- --nocapture`
Expected: FAIL to compile — `order_score`, `SearchCtx::new`, `ORD_FIVE`, `ORD_TT` don't exist; `SearchCtx` is also missing the `killers`/`history`/`tt_hits`/`tt_probes` fields the new constructor needs to set.

- [ ] **Step 5: Implement ordering constants, `order_score`, extend `SearchCtx`, wire ordering into `negamax`**

Add near the top of `src/search.rs`, after the existing `use` statements:

```rust
use crate::board::{Cell, DIRS, TOTAL};
use crate::patterns::{F_FIVE, F_OPEN_FOUR};

const ORD_TT: i32 = 1_000_000;
const ORD_FIVE: i32 = 900_000;
const ORD_BLOCK: i32 = 800_000;
const ORD_OPEN_FOUR: i32 = 700_000;
const ORD_CAPTURE_BASE: i32 = 500_000;
const ORD_KILLER1: i32 = 400_000;
const ORD_KILLER2: i32 = 390_000;
const ORD_HISTORY_CAP: i32 = 300_000;
const MAX_PLY: usize = 64;
```

(`Cell` is imported for completeness here even if unused directly in this task — Task 12 needs it; remove the import if the compiler warns `unused_imports` before Task 12 lands, or leave it and accept the warning until then.)

Replace the `SearchCtx` struct definition from Task 9 with the extended version, and add its constructor:

```rust
struct SearchCtx<'a> {
    pt: &'a PatternTable,
    tt: &'a mut TranspositionTable,
    cfg: &'a SearchConfig,
    nodes: u64,
    deadline: Instant,
    aborted: bool,
    killers: [[Idx; 2]; MAX_PLY],
    history: [i32; TOTAL],
    tt_hits: u64,
    tt_probes: u64,
}

impl<'a> SearchCtx<'a> {
    fn new(pt: &'a PatternTable, tt: &'a mut TranspositionTable, cfg: &'a SearchConfig, deadline: Instant) -> Self {
        SearchCtx {
            pt,
            tt,
            cfg,
            nodes: 0,
            deadline,
            aborted: false,
            killers: [[Idx::MAX; 2]; MAX_PLY],
            history: [0; TOTAL],
            tt_hits: 0,
            tt_probes: 0,
        }
    }
}
```

Add the ordering function, above `#[cfg(test)]`:

```rust
/// Scores a candidate move for ordering (spec §9.3). Priorities 1-6 are
/// hard overrides (each returns immediately); priorities 7-8 (history and
/// static positional gain) are combined as the score for ordinary quiet
/// moves, since both are small relative to the overrides and either alone
/// is a weak signal.
#[allow(clippy::too_many_arguments)]
fn order_score(
    b: &Board,
    pt: &PatternTable,
    mv: Idx,
    me: Player,
    opp: Player,
    tt_mv: Option<Idx>,
    killers: (Idx, Idx),
    history: &[i32],
) -> i32 {
    if Some(mv) == tt_mv {
        return ORD_TT;
    }

    let mut me_five = false;
    let mut me_open_four = false;
    let mut opp_threat = false;
    let mut me_static_gain = 0i32;
    for &d in DIRS.iter() {
        let pat_me = pt.get(b.hypothetical_window_code(mv, d, me));
        if pat_me.flags & F_FIVE != 0 {
            me_five = true;
        }
        if pat_me.flags & F_OPEN_FOUR != 0 {
            me_open_four = true;
        }
        me_static_gain += pat_me.score;

        let pat_opp = pt.get(b.hypothetical_window_code(mv, d, opp));
        if pat_opp.flags & (F_FIVE | F_OPEN_FOUR) != 0 {
            opp_threat = true;
        }
    }

    if me_five {
        return ORD_FIVE;
    }
    if opp_threat {
        return ORD_BLOCK;
    }
    if me_open_four {
        return ORD_OPEN_FOUR;
    }

    let (_captured, n) = b.captures_of(mv, me);
    if n > 0 {
        return ORD_CAPTURE_BASE + 1_000 * (n as i32 / 2);
    }
    if mv == killers.0 {
        return ORD_KILLER1;
    }
    if mv == killers.1 {
        return ORD_KILLER2;
    }

    let hist = history.get(mv as usize).copied().unwrap_or(0).min(ORD_HISTORY_CAP);
    hist + me_static_gain
}
```

Replace the body of `negamax` (from Task 9) with the ordering-aware version:

```rust
fn negamax(b: &mut Board, ctx: &mut SearchCtx, depth: u8, alpha: i32, beta: i32, ply: u8) -> i32 {
    ctx.nodes += 1;
    if ctx.nodes % 2048 == 0 && Instant::now() >= ctx.deadline {
        ctx.aborted = true;
    }
    if ctx.aborted {
        return 0;
    }

    if depth == 0 {
        return eval::evaluate(b);
    }

    let orig_alpha = alpha;
    let mut alpha = alpha;

    ctx.tt_probes += 1;
    let mut tt_move = None;
    if let Some(e) = ctx.tt.probe(b.zobrist) {
        ctx.tt_hits += 1;
        tt_move = Some(e.mv);
        if e.depth >= depth {
            match e.bound {
                Bound::Exact => return e.score,
                Bound::Lower if e.score >= beta => return e.score,
                Bound::Upper if e.score <= alpha => return e.score,
                _ => {}
            }
        }
    }

    let mut candidates = Vec::new();
    rules::generate(b, b.to_move, ctx.pt, &mut candidates);
    if candidates.is_empty() {
        return 0;
    }

    let me = b.to_move;
    let opp = me.other();
    let killers_here = ctx
        .killers
        .get(ply as usize)
        .map(|k| (k[0], k[1]))
        .unwrap_or((Idx::MAX, Idx::MAX));
    let mut scored: Vec<(i32, Idx)> = candidates
        .iter()
        .map(|&mv| {
            let s = order_score(b, ctx.pt, mv, me, opp, tt_move, killers_here, &ctx.history);
            (s, mv)
        })
        .collect();
    scored.sort_unstable_by(|a, bnd| bnd.0.cmp(&a.0));
    scored.truncate(ctx.cfg.max_candidates);

    let mut best = i32::MIN + 1;
    let mut best_move = None;
    for &(_, mv) in &scored {
        let u = b.play(mv, ctx.pt);
        let end = rules::check_end(b, mv, ctx.pt);
        let score = match end {
            GameEnd::Win(_) => WIN - ply as i32 - 1,
            GameEnd::Draw => 0,
            GameEnd::None => -negamax(b, ctx, depth - 1, -beta, -alpha, ply + 1),
        };
        b.undo(&u);

        if ctx.aborted {
            return 0;
        }
        if score > best {
            best = score;
            best_move = Some(mv);
        }
        if best > alpha {
            alpha = best;
        }
        if alpha >= beta {
            if let Some(k) = ctx.killers.get_mut(ply as usize) {
                if k[0] != mv {
                    k[1] = k[0];
                    k[0] = mv;
                }
            }
            if let Some(slot) = ctx.history.get_mut(mv as usize) {
                *slot += (depth as i32) * (depth as i32);
            }
            break;
        }
    }

    if let Some(bm) = best_move {
        let bound = if best <= orig_alpha {
            Bound::Upper
        } else if best >= beta {
            Bound::Lower
        } else {
            Bound::Exact
        };
        ctx.tt.store(
            b.zobrist,
            TtEntry { key: b.zobrist, score: best, mv: bm, depth, bound },
        );
    }

    best
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test --lib search:: -- --nocapture`
Expected: PASS, all 4 tests in `search.rs` green (2 from Task 9, 2 new).

- [ ] **Step 7: Commit**

```bash
git add src/board.rs src/search.rs
git commit -m "feat: move ordering — TT move, threats, captures, killers, history; candidate truncation"
```


---

