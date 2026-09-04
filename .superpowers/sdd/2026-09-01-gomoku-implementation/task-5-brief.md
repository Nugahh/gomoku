### Task 5: `board.rs` — `play`/`undo` with incremental accumulator

**Files:**
- Modify: `src/board.rs`

**Interfaces:**
- Consumes: `patterns::{PatternTable, POW3}` (Task 2), `Board`/`captures_of` (Task 4).
- Produces: `board::{Undo, Board::play, Board::undo}`, used by `rules.rs` (Task 6/7) and `search.rs` (Task 9+).

**Implementation note — reach beyond radius 4 (resolves an underspecified corner of spec §6.4/§8.2):** recomputing a *neighbor* stone's own window (not `mv`'s) means that neighbor's window extends another 4 cells past it, so the true reach from `mv` is up to 8 cells, not 4. `PAD = 4` (spec §4) is sized only for `mv`'s own window, not this secondary reach. Rather than enlarging the padded buffer (which the Global Constraints fix at `PAD = 4`), this task adds a bounds-checked accessor, `cell_at`, that treats any index outside the physical `TOTAL`-sized array as `Cell::Wall`. This is exactly correct, not a workaround: anything past the physical buffer is conceptually further off-board than the pad cells already are, so it must be a wall too.

**Implementation note — vulnerability penalty reach:** a pair's vulnerability (spec §8.3) depends only on 4 cells spanning from one step behind the pair's first stone to one step past its second, so a change at `mv` can only affect pairs anchored within 2 steps of `mv` — much tighter than the pattern table's radius 8. It is implemented with the same "subtract old, mutate, add new" shape as the pattern-table walk, at a smaller radius, so the two are easy to keep straight when reading the code side by side.

- [ ] **Step 1: Write the failing test — play/undo round-trip identity**

Add to `src/board.rs`, inside `#[cfg(test)] mod tests`:

```rust
    fn random_empty_cell(b: &Board, rng: &mut Xorshift64) -> Option<Idx> {
        let mut empties = Vec::new();
        for y in 0..SIZE {
            for x in 0..SIZE {
                let i = idx(x, y);
                if b.get(i) == Cell::Empty {
                    empties.push(i);
                }
            }
        }
        if empties.is_empty() {
            return None;
        }
        let pick = (rng.next() as usize) % empties.len();
        empties.get(pick).copied()
    }

    #[test]
    fn play_undo_round_trip_restores_exact_state() {
        // Uses "any empty cell" rather than full rule legality: play/undo's
        // bookkeeping (accumulator, zobrist, neighbor grid, captures) does
        // not care about double-three, which is a search/UI-level filter,
        // not a board-mechanics concern. This keeps board.rs's own test
        // independent of rules.rs, which does not exist yet.
        let pt = PatternTable::build();
        for seed in 0..1000u64 {
            let mut rng = Xorshift64(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
            let mut b = Board::new();
            let snapshot = b.clone();
            let mut undos = Vec::new();
            for _ in 0..50 {
                let Some(mv) = random_empty_cell(&b, &mut rng) else {
                    break;
                };
                undos.push(b.play(mv, &pt));
            }
            for u in undos.iter().rev() {
                b.undo(u);
            }
            assert_eq!(b.cells, snapshot.cells, "seed {seed}: cells differ after undo");
            assert_eq!(b.captures, snapshot.captures, "seed {seed}: captures differ");
            assert_eq!(b.to_move, snapshot.to_move, "seed {seed}: to_move differs");
            assert_eq!(b.zobrist, snapshot.zobrist, "seed {seed}: zobrist differs");
            assert_eq!(b.stone_count, snapshot.stone_count, "seed {seed}: stone_count differs");
            assert_eq!(b.neighbor, snapshot.neighbor, "seed {seed}: neighbor grid differs");
            assert_eq!(b.acc, snapshot.acc, "seed {seed}: accumulator differs");
        }
    }

    #[test]
    fn play_capture_updates_captures_and_frees_cells() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        // White _ Black Black _  ->  White plays at (3,0), captures the pair.
        b.set_raw(idx(0, 0), Cell::White);
        b.to_move = Player::Black;
        b.play(idx(1, 0), &pt); // Black
        b.to_move = Player::White;
        // set up manually instead of alternating turns, to isolate the capture:
        b.set_raw(idx(2, 0), Cell::Black);
        let before_black_stone_count = b.stone_count;
        b.to_move = Player::White;
        b.play(idx(3, 0), &pt);
        assert_eq!(b.get(idx(1, 0)), Cell::Empty, "captured stone not removed");
        assert_eq!(b.get(idx(2, 0)), Cell::Empty, "captured stone not removed");
        assert_eq!(b.captures[Player::White as usize], 2);
        assert_eq!(b.stone_count, before_black_stone_count + 1 - 1); // +White's mv, -2 captured +... see below
    }
```

Note on the last assertion: `before_black_stone_count` is the count right after manually placing the two black stones via `set_raw` (which does **not** touch `stone_count`), so the arithmetic there is illustrative only — the implementer should replace it with a direct computed expectation once `play`'s exact stone_count bookkeeping is visible (placed 1, minus 2 captured = net -1 relative to the count just before White's move, which itself only reflects stones placed via `play`, i.e. 1 Black stone from the first `b.play` call). Simplify this assertion during Step 3 if it proves confusing to read; the accumulator/zobrist/capture-count assertions above it are the ones that matter.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib board:: -- --nocapture`
Expected: FAIL to compile — `Board: Clone`, `Undo`, `Board::play`, `Board::undo` don't exist.

- [ ] **Step 3: Implement `Undo`, the incremental helpers, `play`, and `undo`**

First, add `#[derive(Clone)]` to the `Board` struct definition from Task 4 (needed only for this test's snapshot; `Board` deliberately does **not** derive `Copy` — the search hot path must never accidentally deep-copy a ~13KB struct per node, it always uses `play`/`undo` in place):

```rust
#[derive(Clone)]
pub struct Board {
```

Add the `Undo` struct and `VULN_PENALTY` constant near the top of `src/board.rs`, below the existing constants:

```rust
/// Vulnerability penalty per pair (spec §8.3): a pair of same-color stones
/// where one flank is an opponent stone and the other is empty is one move
/// away from being captured.
const VULN_PENALTY: i32 = -1_200;

pub struct Undo {
    pub mv: Idx,
    pub captured: [Idx; 16], // 4 axes x 2 signs x 2 stones per capture, max
    pub n_captured: u8,
    pub prev_zobrist: u64,
    pub prev_acc: [i32; 2],
    pub prev_captures: [u8; 2],
}
```

Add the helper methods and `play`/`undo` to `impl Board`:

```rust
    /// Safe accessor for offsets that may reach beyond the physical buffer
    /// (see this task's implementation note on reach). Anything outside the
    /// array is `Wall` — semantically correct, since nothing real is ever
    /// there.
    #[inline]
    fn cell_at(&self, center: Idx, offset: i32) -> Cell {
        let i = center as i32 + offset;
        if i < 0 || i as usize >= TOTAL {
            return Cell::Wall;
        }
        self.cells.get(i as usize).copied().unwrap_or(Cell::Wall)
    }

    /// Encodes the 9-cell window centered at `c` along axis `d`, relative
    /// to `p` (own stone = 1, empty = 0, opponent-or-wall = 2), matching
    /// `patterns::PatternTable`'s encoding (spec §5.1).
    fn window_code(&self, c: Idx, d: i16, p: Player) -> u32 {
        let mut code = 0u32;
        for (slot, k) in (-4..=4i32).enumerate() {
            let cell = self.cell_at(c, k * d as i32);
            let trit: u32 = if cell == Cell::Empty {
                0
            } else if cell == p.cell() {
                1
            } else {
                2
            };
            code += trit * POW3.get(slot).copied().unwrap_or(0);
        }
        code
    }

    #[inline]
    fn stone_window_score(&self, c: Idx, d: i16, owner: Player, pt: &PatternTable) -> i32 {
        pt.get(self.window_code(c, d, owner)).score
    }

    /// Vulnerability contribution of the pair `(c, c+d)`, anchored at `c`
    /// so each pair is scored exactly once (spec §8.3).
    fn pair_vuln_score(&self, c: Idx, d: i16, owner: Player) -> i32 {
        let opp = owner.other().cell();
        if self.cell_at(c, d as i32) != owner.cell() {
            return 0;
        }
        let before = self.cell_at(c, -(d as i32));
        let after = self.cell_at(c, 2 * d as i32);
        let vulnerable =
            (before == opp && after == Cell::Empty) || (before == Cell::Empty && after == opp);
        if vulnerable {
            VULN_PENALTY
        } else {
            0
        }
    }

    /// Adds `sign * score` to the accumulator entry of every stone whose
    /// pattern-table window could be affected by a change at `center`
    /// (spec §6.4 step 2/5, §8.2). Call with `sign = -1` before mutating
    /// the board, `sign = 1` after.
    fn adjust_axis_neighbors(&mut self, center: Idx, pt: &PatternTable, sign: i32) {
        for &d in DIRS.iter() {
            for k in (-4..=4i32).filter(|&k| k != 0) {
                let cell = self.cell_at(center, k * d as i32);
                let owner = match cell {
                    Cell::Black => Player::Black,
                    Cell::White => Player::White,
                    _ => continue,
                };
                let c_off = center as i32 + k * d as i32;
                if c_off < 0 || c_off as usize >= TOTAL {
                    continue;
                }
                let c = c_off as Idx;
                let score = self.stone_window_score(c, d, owner, pt);
                self.acc[owner as usize] += sign * score;
            }
        }
    }

    /// Same shape as `adjust_axis_neighbors`, at the smaller radius the
    /// vulnerability term needs (see this task's implementation note).
    fn adjust_axis_vuln(&mut self, center: Idx, sign: i32) {
        for &d in DIRS.iter() {
            for k in -2..=1i32 {
                let c_off = center as i32 + k * d as i32;
                if c_off < 0 || c_off as usize >= TOTAL {
                    continue;
                }
                let c = c_off as Idx;
                let owner = match self.get(c) {
                    Cell::Black => Player::Black,
                    Cell::White => Player::White,
                    _ => continue,
                };
                let score = self.pair_vuln_score(c, d, owner);
                self.acc[owner as usize] += sign * score;
            }
        }
    }

    /// Updates the radius-2 neighbor-count grid used by `rules::generate`
    /// (spec §7.4) around `center` by `delta`, saturating rather than
    /// over/underflowing.
    fn adjust_neighbor_grid(&mut self, center: Idx, delta: i32) {
        let stride = STRIDE as i32;
        for dy in -2..=2i32 {
            for dx in -2..=2i32 {
                let i = center as i32 + dy * stride + dx;
                if i < 0 || i as usize >= TOTAL {
                    continue;
                }
                if let Some(slot) = self.neighbor.get_mut(i as usize) {
                    *slot = (*slot as i32 + delta).clamp(0, 255) as u8;
                }
            }
        }
    }

    /// Applies `mv` for `self.to_move`. Assumes `mv` is legal — callers
    /// must check `rules::is_legal` first (spec §6.4).
    pub fn play(&mut self, mv: Idx, pt: &PatternTable) -> Undo {
        let p = self.to_move;
        let mut undo = Undo {
            mv,
            captured: [0; 16],
            n_captured: 0,
            prev_zobrist: self.zobrist,
            prev_acc: self.acc,
            prev_captures: self.captures,
        };

        self.adjust_axis_neighbors(mv, pt, -1);
        self.adjust_axis_vuln(mv, -1);

        self.set_raw(mv, p.cell());
        self.zobrist ^= self.key_cell[p as usize][mv as usize];
        self.stone_count += 1;
        self.adjust_neighbor_grid(mv, 1);

        let (captured, n) = self.captures_of(mv, p);
        let owner = p.other();
        for cc in captured.iter().take(n) {
            self.adjust_axis_neighbors(*cc, pt, -1);
            self.adjust_axis_vuln(*cc, -1);
            for &d in DIRS.iter() {
                self.acc[owner as usize] -= self.stone_window_score(*cc, d, owner, pt);
            }
            self.set_raw(*cc, Cell::Empty);
            self.zobrist ^= self.key_cell[owner as usize][*cc as usize];
            self.adjust_neighbor_grid(*cc, -1);
            self.stone_count -= 1;
        }
        undo.captured = captured;
        undo.n_captured = n as u8;

        self.adjust_axis_neighbors(mv, pt, 1);
        self.adjust_axis_vuln(mv, 1);
        for cc in captured.iter().take(n) {
            self.adjust_axis_neighbors(*cc, pt, 1);
            self.adjust_axis_vuln(*cc, 1);
        }
        for &d in DIRS.iter() {
            self.acc[p as usize] += self.stone_window_score(mv, d, p, pt);
        }

        let old_idx = self.captures[p as usize].min(10) as usize;
        self.zobrist ^= self.key_captures[p as usize].get(old_idx).copied().unwrap_or(0);
        self.captures[p as usize] = self.captures[p as usize].saturating_add(n as u8);
        let new_idx = self.captures[p as usize].min(10) as usize;
        self.zobrist ^= self.key_captures[p as usize].get(new_idx).copied().unwrap_or(0);

        self.to_move = owner;
        self.zobrist ^= self.key_side;

        undo
    }

    /// Exactly reverses a `play`. Restores cells, neighbor grid and
    /// stone_count by replaying the recorded change; restores zobrist, acc
    /// and captures by direct snapshot rather than recomputing them (spec
    /// §6.4) — the incremental math above is complex enough that re-running
    /// it backwards would just be a second place to get it wrong.
    pub fn undo(&mut self, u: &Undo) {
        let mover = self.to_move.other();
        let captured_owner = mover.other();

        for cc in u.captured.iter().take(u.n_captured as usize) {
            self.set_raw(*cc, captured_owner.cell());
            self.adjust_neighbor_grid(*cc, 1);
            self.stone_count += 1;
        }
        self.set_raw(u.mv, Cell::Empty);
        self.adjust_neighbor_grid(u.mv, -1);
        self.stone_count -= 1;

        self.zobrist = u.prev_zobrist;
        self.acc = u.prev_acc;
        self.captures = u.prev_captures;
        self.to_move = mover;
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib board:: -- --nocapture`
Expected: PASS, including `play_undo_round_trip_restores_exact_state` across all 1000 seeds and `play_capture_updates_captures_and_frees_cells`. If the round-trip test fails, check first whether the failing field is `acc` (most likely: a mismatched `adjust_axis_neighbors`/`adjust_axis_vuln` call is missing on one of the three call sites — before `mv`, after each capture removal, after final placement) before suspecting `zobrist` or `neighbor`.

- [ ] **Step 5: Commit**

```bash
git add src/board.rs
git commit -m "feat: incremental play/undo with accumulator, vulnerability term, Zobrist"
```


---

