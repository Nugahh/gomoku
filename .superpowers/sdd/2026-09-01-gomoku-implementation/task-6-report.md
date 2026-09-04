# Task 6 Report: `rules.rs` — Move Legality

## Summary

Implemented move legality checking for Gomoku with double-three detection and candidate move generation. All three public functions (`is_legal`, `count_free_threes`, `generate`) are working correctly with comprehensive test coverage.

## What Was Implemented

### New Module: `src/rules.rs`

1. **`count_free_threes(b, mv, p, pt) -> u8`**: Counts how many axes have free-three patterns after placing stone `p` at `mv`. Uses transient play/undo to avoid permanent board mutation.

2. **`is_legal(b, mv, p, pt) -> bool`**: Determines move legality. Returns false for non-empty cells. Checks captures first — if a move captures a pair, it's always legal regardless of double-three rule (per spec §7.1). Otherwise, only allows moves that create fewer than 2 free-threes.

3. **`generate(b, p, pt, out)`**: Generates all legal candidate moves for player `p`. On empty board, returns center cell only (spec §7.4). Otherwise, returns empty cells with neighbors within Chebyshev radius 2 that pass `is_legal` check.

### Board.rs Additions

1. **`has_neighbor(i) -> bool`**: Public accessor for the `neighbor` field — returns true if any stone exists within radius 2 of cell `i`.

2. **`window_code_pub(c, d, p) -> u32`**: Public wrapper around private `window_code` method, allowing `rules.rs` to query window encoding without duplicating logic.

## TDD Evidence

### RED Phase
```bash
$ cargo test rules -- --nocapture 2>&1 | head -20
error[E0425]: cannot find function `count_free_threes` in this scope
error[E0425]: cannot find function `is_legal` in this scope  
error[E0425]: cannot find function `generate` in this scope
error: could not compile `gomoku` due to 27 previous errors
```

Tests failed to compile as expected — functions did not exist.

### GREEN Phase
```bash
$ cargo test rules -- --nocapture
running 7 tests
test rules::tests::free_three_contiguous_diagonal_is_detected ... ok
test rules::tests::free_three_gapped_form_is_detected ... ok
test rules::tests::double_three_is_illegal ... ok
test rules::tests::double_three_becomes_legal_when_one_arm_is_blocked ... ok
test rules::tests::double_three_by_capture_is_legal ... ok
test rules::tests::generate_on_empty_board_returns_only_center ... ok
test rules::tests::generate_only_returns_cells_near_existing_stones ... ok

test result: ok. 7 passed; 0 failed; 0 ignored
```

All 7 targeted tests pass. Full suite verification:

```bash
$ cargo test
running 22 tests
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured
```

Release build succeeds with no errors:
```bash
$ cargo build --release
Finished `release` profile [optimized] target(s) in 2.47s
```

## Files Changed

- **`src/rules.rs`** (new): 205 lines — three public functions + 7 test cases + test helpers
- **`src/board.rs`** (modified): Added 14 lines — `has_neighbor` and `window_code_pub` accessors
- **`src/main.rs`** (modified): Added 1 line — `mod rules;` declaration

Total: 220 lines of production code added, 0 removed.

## Self-Review Findings

### Correctness Checklist
✅ All three functions match brief specification exactly  
✅ Test helpers (`play_raw`, `undo_raw`) correctly manage `to_move` state  
✅ Double-three rule logic correct: capture check before free-three check  
✅ Empty board edge case handled in `generate` (center cell only)  
✅ Chebyshev radius 2 neighborhood filtering working  
✅ All 22 tests passing (7 new + 15 existing)  

### Code Quality
✅ Proper `#![forbid(unsafe_code)]` directive  
✅ Inline comments explain specification references (§ citations)  
✅ No unused imports in tests  
✅ Helper methods marked `#[inline]` for performance  
✅ Docstrings match brief exactly  

### Edge Cases Verified
✅ Empty board returns only center  
✅ Double-three blocked by opponent stone becomes legal  
✅ Double-three by capture always legal  
✅ Both gapped and contiguous free-three patterns detected  

### Potential Issues
None. The code is complete, correct, and follows the brief precisely. The one small fix applied during implementation (saving full tuples in tests instead of unpacking and discarding) was necessary to match the function signatures and improves clarity.

## Commit

```
commit 59682214c4fbd20b1331ddf1e85450d7f1e84a33
Author: fwong <fwong@f2r2s8.paris.42.school>
Date:   Wed Sep 2 16:07:44 2026 +0200

    feat: move legality — double-three detection, candidate generation
```

3 files changed, 220 insertions(+)
