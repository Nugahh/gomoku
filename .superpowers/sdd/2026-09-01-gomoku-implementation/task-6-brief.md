### Task 6: `rules.rs` — legality (`is_legal`, `count_free_threes`, `generate`)

**Files:**
- Create: `src/rules.rs`
- Modify: `src/main.rs` (add `mod rules;`)

**Interfaces:**
- Consumes: `board::{Board, Cell, Player, Idx, DIRS, SIZE, idx}` (Tasks 3-5), `patterns::{PatternTable, F_FREE_THREE}` (Task 2). `Board` needs one new accessor from this task: a way to read the four axis flags at a cell without mutating the real board — implemented here as a private scratch helper, not exposed publicly.
- Produces: `rules::{is_legal, count_free_threes, generate}`, used by `search.rs` (Task 9+) and `ui.rs` (Task 15). `check_end` and `GameEnd` are added in Task 7.

- [ ] **Step 1: Write the failing tests — free-three and double-three fixtures from the appendix**

Create `src/rules.rs`:

```rust
#![forbid(unsafe_code)]

use crate::board::{idx, Board, Cell, Idx, Player, DIRS, SIZE};
use crate::patterns::{PatternTable, F_FREE_THREE};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_three_contiguous_diagonal_is_detected() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        b.to_move = Player::Black;
        // three in a diagonal row with open ends: (5,5) (6,6) (7,7)
        let (u1, _) = play_raw(&mut b, idx(5, 5), Player::Black, &pt);
        let (u2, _) = play_raw(&mut b, idx(6, 6), Player::Black, &pt);
        assert_eq!(count_free_threes(&mut b, idx(7, 7), Player::Black, &pt), 1);
        undo_raw(&mut b, u2);
        undo_raw(&mut b, u1);
    }

    #[test]
    fn free_three_gapped_form_is_detected() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        b.to_move = Player::Black;
        // . X . X X . horizontally: stones at (5,5) and (7,5), gap at (6,5),
        // playing at (6,5) completes ". X X X ." shape via the gapped rule.
        let (u1, _) = play_raw(&mut b, idx(5, 5), Player::Black, &pt);
        let (u2, _) = play_raw(&mut b, idx(7, 5), Player::Black, &pt);
        assert_eq!(count_free_threes(&mut b, idx(6, 5), Player::Black, &pt), 1);
        undo_raw(&mut b, u2);
        undo_raw(&mut b, u1);
    }

    #[test]
    fn double_three_is_illegal() {
        // spec appendix VI.2: two red stones placed so that playing `a`
        // creates two simultaneous free-threes.
        let pt = PatternTable::build();
        let mut b = Board::new();
        b.to_move = Player::Black;
        // horizontal three-to-be: (8,5) (9,5) [a=10,5] existing pair, open.
        let (u1, _) = play_raw(&mut b, idx(8, 5), Player::Black, &pt);
        let (u2, _) = play_raw(&mut b, idx(9, 5), Player::Black, &pt);
        // diagonal three-to-be sharing the same point a=(10,5):
        let (u3, _) = play_raw(&mut b, idx(9, 4), Player::Black, &pt);
        let (u4, _) = play_raw(&mut b, idx(11, 6), Player::Black, &pt);
        assert!(!is_legal(&b, idx(10, 5), Player::Black, &pt));
        undo_raw(&mut b, u4);
        undo_raw(&mut b, u3);
        undo_raw(&mut b, u2);
        undo_raw(&mut b, u1);
    }

    #[test]
    fn double_three_becomes_legal_when_one_arm_is_blocked() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        b.to_move = Player::Black;
        let (u1, _) = play_raw(&mut b, idx(8, 5), Player::Black, &pt);
        let (u2, _) = play_raw(&mut b, idx(9, 5), Player::Black, &pt);
        let (u3, _) = play_raw(&mut b, idx(9, 4), Player::Black, &pt);
        let (u4, _) = play_raw(&mut b, idx(11, 6), Player::Black, &pt);
        // block one of the two free-three arms with a white stone
        let (u5, _) = play_raw(&mut b, idx(7, 5), Player::White, &pt);
        assert!(is_legal(&b, idx(10, 5), Player::Black, &pt));
        undo_raw(&mut b, u5);
        undo_raw(&mut b, u4);
        undo_raw(&mut b, u3);
        undo_raw(&mut b, u2);
        undo_raw(&mut b, u1);
    }

    #[test]
    fn double_three_by_capture_is_legal() {
        // spec §7.1/§9 (appendix warning): introducing a double-three by
        // capturing a pair is explicitly allowed. Build a position where
        // the move both captures a pair AND would otherwise be a
        // double-three.
        let pt = PatternTable::build();
        let mut b = Board::new();
        b.to_move = Player::Black;
        // two free-three arms for Black around (10,5), same as above:
        let (u1, _) = play_raw(&mut b, idx(8, 5), Player::Black, &pt);
        let (u2, _) = play_raw(&mut b, idx(9, 5), Player::Black, &pt);
        let (u3, _) = play_raw(&mut b, idx(9, 4), Player::Black, &pt);
        let (u4, _) = play_raw(&mut b, idx(11, 6), Player::Black, &pt);
        // a capturable White pair flanked by Black at (10,5) and an
        // existing Black stone two steps further down the same axis as one
        // arm, positioned off the double-three axes so it only adds a
        // capture, not a third free-three:
        let (u5, _) = play_raw(&mut b, idx(10, 8), Player::Black, &pt);
        let (u6, _) = play_raw(&mut b, idx(10, 6), Player::White, &pt);
        let (u7, _) = play_raw(&mut b, idx(10, 7), Player::White, &pt);
        assert!(is_legal(&b, idx(10, 5), Player::Black, &pt));
        undo_raw(&mut b, u7);
        undo_raw(&mut b, u6);
        undo_raw(&mut b, u5);
        undo_raw(&mut b, u4);
        undo_raw(&mut b, u3);
        undo_raw(&mut b, u2);
        undo_raw(&mut b, u1);
    }

    #[test]
    fn generate_on_empty_board_returns_only_center() {
        let pt = PatternTable::build();
        let b = Board::new();
        let mut out = Vec::new();
        generate(&b, Player::Black, &pt, &mut out);
        assert_eq!(out, vec![idx(SIZE / 2, SIZE / 2)]);
    }

    #[test]
    fn generate_only_returns_cells_near_existing_stones() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        b.to_move = Player::Black;
        let (_u, _) = play_raw(&mut b, idx(9, 9), Player::Black, &pt);
        let mut out = Vec::new();
        generate(&b, Player::White, &pt, &mut out);
        assert!(!out.is_empty());
        for &mv in &out {
            let (x, y) = crate::board::to_xy(mv);
            let (cx, cy) = (9i32, 9i32);
            let dist = (x as i32 - cx).abs().max((y as i32 - cy).abs());
            assert!(dist <= 2, "candidate {mv:?} too far from the only stone");
        }
    }

    /// Test-only helper: plays a move for an arbitrary player regardless of
    /// `b.to_move`, bypassing turn alternation, so fixtures can build
    /// specific positions directly. Returns the `Undo` and restores
    /// `to_move` to what it was, so `undo_raw` can reverse it symmetrically.
    fn play_raw(b: &mut Board, mv: Idx, p: Player, pt: &PatternTable) -> (crate::board::Undo, Player) {
        let saved_to_move = b.to_move;
        b.to_move = p;
        let u = b.play(mv, pt);
        (u, saved_to_move)
    }

    fn undo_raw(b: &mut Board, (u, saved_to_move): (crate::board::Undo, Player)) {
        b.undo(&u);
        b.to_move = saved_to_move;
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib rules:: -- --nocapture`
Expected: FAIL to compile — `is_legal`, `count_free_threes`, `generate` don't exist.

- [ ] **Step 3: Implement `is_legal`, `count_free_threes`, `generate`**

Add to `src/rules.rs`, above `#[cfg(test)]`:

```rust
/// Number of axes on which placing `p` at `mv` creates a free-three (spec
/// §7.2). Places the stone with a scratch `play`/`undo` (the accumulator
/// and Zobrist churn is wasted work here, but reusing the already-correct
/// `play`/`undo` is far less risky than a second bespoke place/remove path
/// — this is not on the search hot path, only on legality checks).
pub fn count_free_threes(b: &mut Board, mv: Idx, p: Player, pt: &PatternTable) -> u8 {
    let saved_to_move = b.to_move;
    b.to_move = p;
    let u = b.play(mv, pt);
    let mut count = 0u8;
    for &d in DIRS.iter() {
        let code = window_code_pub(b, mv, d, p);
        if pt.get(code).flags & F_FREE_THREE != 0 {
            count += 1;
        }
    }
    b.undo(&u);
    b.to_move = saved_to_move;
    count
}

/// `mv` must be an empty, in-bounds cell. Captures are checked before the
/// double-three rule — the subject states explicitly that introducing a
/// double-three by capturing a pair is allowed (spec §7.1, appendix VI.2
/// warning), so a capturing move is legal regardless of free-three count.
pub fn is_legal(b: &Board, mv: Idx, p: Player, pt: &PatternTable) -> bool {
    if b.get(mv) != Cell::Empty {
        return false;
    }
    let (_captured, n) = b.captures_of(mv, p);
    if n > 0 {
        return true;
    }
    let mut scratch = b.clone();
    count_free_threes(&mut scratch, mv, p, pt) < 2
}

/// All legal candidate moves for `p`: empty cells with at least one stone
/// within Chebyshev radius 2, filtered by `is_legal`. On an empty board,
/// only the center cell qualifies (spec §7.4).
pub fn generate(b: &Board, p: Player, pt: &PatternTable, out: &mut Vec<Idx>) {
    out.clear();
    if b.stone_count == 0 {
        let center = idx(SIZE / 2, SIZE / 2);
        out.push(center);
        return;
    }
    for y in 0..SIZE {
        for x in 0..SIZE {
            let i = idx(x, y);
            if b.get(i) == Cell::Empty && b.has_neighbor(i) && is_legal(b, i, p, pt) {
                out.push(i);
            }
        }
    }
}
```

`generate` calls `b.has_neighbor(i)`, a one-line accessor that doesn't exist on `Board` yet (the `neighbor` field from Task 4 is private). Add it to `impl Board` in `src/board.rs`:

```rust
    /// True if any stone is within Chebyshev radius 2 of `i` (spec §7.4).
    #[inline]
    pub fn has_neighbor(&self, i: Idx) -> bool {
        self.neighbor.get(i as usize).copied().unwrap_or(0) > 0
    }
```

`count_free_threes` also calls `window_code_pub`, a thin public wrapper this task adds to `Board` around the existing private `window_code` (Task 5), since `rules.rs` needs to read a window's flags without duplicating the encoding logic:

```rust
    /// Public wrapper around the window encoding used internally by
    /// `play`/`undo` (Task 5). Exposed so `rules.rs` can query pattern
    /// flags (e.g. free-three) without re-deriving the encoding.
    #[inline]
    pub fn window_code_pub(&self, c: Idx, d: i16, p: Player) -> u32 {
        self.window_code(c, d, p)
    }
```

And in `rules.rs`, replace the bare call with the qualified one:

```rust
        let code = b.window_code_pub(mv, d, p);
```

(This replaces the placeholder `window_code_pub(b, mv, d, p)` free-function call in Step 1's test-adjacent implementation sketch above — use `b.window_code_pub(...)` as a method call, matching `Board`'s actual API.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib rules:: -- --nocapture`
Expected: PASS, all 7 tests green. If `double_three_by_capture_is_legal` fails while the plain double-three tests pass, check the order of checks in `is_legal` — the capture check must run and return `true` before the free-three count is ever computed.

- [ ] **Step 5: Wire into `main.rs`**

```rust
mod rules;
```

Run: `cargo build --release` — expect success.

- [ ] **Step 6: Commit**

```bash
git add src/rules.rs src/board.rs src/main.rs
git commit -m "feat: move legality — double-three detection, candidate generation"
```


---

