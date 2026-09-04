# Task 8 Report: `eval.rs` — Leaf Evaluation and Accumulator-Drift Guard

## Summary

Successfully implemented leaf evaluation function with capture bonus scoring and an accumulator-drift test that verifies incremental play/undo never deviates from full recomputation.

## What Was Implemented

### 1. Helper Status: `full_recompute_acc`
- **Already existed** in `src/board.rs` (line 502) from Task 5's bug-fix round
- Correctly implemented with signature: `pub(crate) fn full_recompute_acc(&self, pt: &PatternTable) -> [i32; 2]`
- Performs full scan of board to independently verify accumulator state
- Skipped adding duplicate per instructions

### 2. Created `src/eval.rs`
- New file with 90 lines of code
- Constants:
  - `WIN`: 100,000,000 (win threshold, reserved for search)
  - `CAP_BONUS`: [0, 4_000, 12_000, 30_000, 90_000, 10_000_000] (non-linear capture scoring)
- Functions:
  - `evaluate(b: &Board) -> i32`: Main public leaf evaluator using negamax convention
  - `cap_bonus(stones_captured: u8) -> i32`: Internal O(1) lookup for capture bonus

### 3. Modified `src/main.rs`
- Added `mod eval;` declaration

## TDD Evidence

### RED Phase
```
$ cargo test eval:: -- --nocapture
error[E0425]: cannot find function `evaluate` in this scope
  --> src/eval.rs:36:33
   |
36 |         let score_black_ahead = evaluate(&b);
```
✓ Tests fail to compile as expected

### GREEN Phase
```
$ cargo test eval:: -- --nocapture
running 2 tests
test eval::tests::evaluate_favors_the_player_with_more_captures ... ok
test eval::tests::accumulator_never_drifts_from_full_recompute ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 26 filtered out
```
✓ Both tests pass

### Full Suite
```
$ cargo test
test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured
```
✓ All 28 tests pass (26 prior + 2 new eval tests)

### Release Build
```
$ cargo build --release
    Finished `release` profile [optimized]
```
✓ Release build succeeds (unused warnings expected; constants will be used in Task 9)

## Files Changed
- **Created**: `/home/fwong/Desktop/42/gomoku/.claude/worktrees/gomoku-impl/src/eval.rs` (90 lines)
- **Modified**: `/home/fwong/Desktop/42/gomoku/.claude/worktrees/gomoku-impl/src/main.rs` (+1 line: `mod eval;`)
- **Not modified**: `src/board.rs` (helper already existed)

## Commit
```
40af77e feat: leaf evaluation with capture bonus, accumulator-drift test
```

## Self-Review

### Correctness
- ✓ `evaluate()` correctly implements negamax convention (me - op from current player's view)
- ✓ `cap_bonus()` safely handles overflow: converts u8 to pairs, uses `.get()` with fallback
- ✓ Test `evaluate_favors_the_player_with_more_captures` verifies evaluation respects capture state
- ✓ Test `accumulator_never_drifts_from_full_recompute` runs 200 random seeds × 40 moves = 8000 play/check cycles

### Implementation Details
- CAP_BONUS array matches spec §8.3: non-linear (0→4k→12k→30k→90k→10M)
- `evaluate()` is `#[inline]` (cheap, hot path for search)
- `cap_bonus()` is `#[inline]` private helper
- Tests use XORshift64 RNG for fast, deterministic pseudorandom play sequences
- Unused constants (WIN, CAP_BONUS) will be used in Task 9's search.rs

### Edge Cases
- cap_bonus correctly handles >10 stones captured (fallback to last value)
- Accumulator drift test uses random move selection to exercise all paths in `play()`/`undo()`
- Test runs 200 different seeds for sufficient coverage of board states

## No Issues Found

- Test suite passes with no failures
- Release build succeeds
- All interface expectations met
- Ready for Task 9 (search.rs integration)
