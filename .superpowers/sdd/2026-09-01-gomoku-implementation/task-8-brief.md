### Task 8: `eval.rs` — leaf evaluation and accumulator-drift guard

**Files:**
- Create: `src/eval.rs`
- Modify: `src/board.rs` (add a `#[cfg(test)]`-only full-recompute helper)
- Modify: `src/main.rs` (add `mod eval;`)

**Interfaces:**
- Consumes: `board::{Board, Player}` (Tasks 3-5).
- Produces: `eval::{WIN, CAP_BONUS, evaluate}`, used by `search.rs` (Task 9+) as the leaf/terminal scoring function.

- [ ] **Step 1: Add the test-only full-recompute helper to `board.rs`**

This underwrites the accumulator-drift test below and belongs in `board.rs` because it needs `stone_window_score`/`pair_vuln_score`, both private to that module. Append to `src/board.rs`, outside the existing `impl Board` block (a separate `#[cfg(test)]`-gated impl block, so it compiles only for `cargo test`, never into the release binary):

```rust
#[cfg(test)]
impl Board {
    /// Recomputes the accumulator from scratch by scanning every occupied
    /// cell and axis, independent of the incremental machinery in `play`.
    /// Used only to verify `play`/`undo` never let `acc` drift (spec
    /// §8.2). `pub(crate)` so `eval.rs`'s tests can call it.
    pub(crate) fn full_recompute_acc(&self, pt: &PatternTable) -> [i32; 2] {
        let mut acc = [0i32; 2];
        for y in 0..SIZE {
            for x in 0..SIZE {
                let i = idx(x, y);
                let owner = match self.get(i) {
                    Cell::Black => Player::Black,
                    Cell::White => Player::White,
                    _ => continue,
                };
                for &d in DIRS.iter() {
                    acc[owner as usize] += self.stone_window_score(i, d, owner, pt);
                    acc[owner as usize] += self.pair_vuln_score(i, d, owner);
                }
            }
        }
        acc
    }
}
```

Run: `cargo build --release` to confirm this doesn't break the release build (it shouldn't compile into it at all — `cfg(test)` strips it).

- [ ] **Step 2: Write the failing tests — evaluation sanity and accumulator drift**

Create `src/eval.rs`:

```rust
#![forbid(unsafe_code)]

use crate::board::{Board, Player};

pub const WIN: i32 = 100_000_000;

/// Non-linear on purpose: the 4th pair captured is worth far more than the
/// 1st, because it puts the opponent one capture from losing outright and
/// makes every one of their fives breakable (spec §8.3).
pub const CAP_BONUS: [i32; 6] = [0, 4_000, 12_000, 30_000, 90_000, 10_000_000];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::idx;
    use crate::patterns::PatternTable;
    use crate::rules;

    struct Xs(u64);
    impl Xs {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
    }

    #[test]
    fn evaluate_favors_the_player_with_more_captures() {
        let mut b = Board::new();
        b.captures[Player::Black as usize] = 4;
        b.to_move = Player::Black;
        let score_black_ahead = evaluate(&b);
        b.captures = [0, 0];
        b.captures[Player::White as usize] = 4;
        let score_white_ahead = evaluate(&b);
        assert!(score_black_ahead > score_white_ahead);
    }

    #[test]
    fn accumulator_never_drifts_from_full_recompute() {
        let pt = PatternTable::build();
        for seed in 0..200u64 {
            let mut rng = Xs(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
            let mut b = Board::new();
            for _ in 0..40 {
                let mut candidates = Vec::new();
                rules::generate(&b, b.to_move, &pt, &mut candidates);
                if candidates.is_empty() {
                    break;
                }
                let pick = (rng.next() as usize) % candidates.len();
                let Some(&mv) = candidates.get(pick) else {
                    break;
                };
                b.play(mv, &pt);
                let full = b.full_recompute_acc(&pt);
                assert_eq!(b.acc, full, "seed {seed}: accumulator drifted from full recompute");
            }
        }
        let _ = idx(0, 0); // silence an unused-import warning if idx ends up unused above
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --lib eval:: -- --nocapture`
Expected: FAIL to compile — `evaluate` doesn't exist yet.

- [ ] **Step 4: Implement `evaluate`**

Add to `src/eval.rs`, above `#[cfg(test)]`:

```rust
/// Score of the current position from `b.to_move`'s point of view
/// (negamax convention). `acc` already includes the incremental
/// vulnerability penalty (spec §8.2/§8.3); the capture bonus is applied
/// fresh here via array lookup, since it is a cheap O(1) function of the
/// already-incrementally-maintained `captures` counters, not itself
/// incremental.
#[inline]
pub fn evaluate(b: &Board) -> i32 {
    let me = b.to_move;
    let op = me.other();
    let me_bonus = cap_bonus(b.captures[me as usize]);
    let op_bonus = cap_bonus(b.captures[op as usize]);
    (b.acc[me as usize] + me_bonus) - (b.acc[op as usize] + op_bonus)
}

#[inline]
fn cap_bonus(stones_captured: u8) -> i32 {
    let pairs = (stones_captured / 2) as usize;
    CAP_BONUS
        .get(pairs)
        .copied()
        .unwrap_or_else(|| CAP_BONUS.iter().copied().last().unwrap_or(0))
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib eval:: -- --nocapture`
Expected: PASS, both tests green. `accumulator_never_drifts_from_full_recompute` is the important one — if it fails, the bug is in Task 5's `play`, not here; recheck `adjust_axis_neighbors`/`adjust_axis_vuln` call sites against `full_recompute_acc`'s scan.

- [ ] **Step 6: Wire into `main.rs`**

```rust
mod eval;
```

Run: `cargo build --release` — expect success.

- [ ] **Step 7: Commit**

```bash
git add src/eval.rs src/board.rs src/main.rs
git commit -m "feat: leaf evaluation with capture bonus, accumulator-drift test"
```


---

