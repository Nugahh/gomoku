# Task 7 Report: `check_end` — Game End Detection

## Summary

Implemented game end detection with the complex endgame capture rule for Gomoku (5-in-a-row with captures). The implementation correctly evaluates:
1. **Win by capture** (unconditional): 10 or more stones captured
2. **Win by alignment** (conditional): Five-in-a-row that cannot be broken
3. **Draw**: No legal moves remain for the current player
4. **In-play**: Game continues

## Implementation Details

### What Was Implemented

Added to `src/rules.rs`:
- **`GameEnd` enum**: Represents terminal game states (None, Win(Player), Draw)
- **`check_end(b, last, pt)`**: Main function that evaluates the game state after a move
- **`collect_alignment(b, last, d, p)`**: Helper that walks outward from a placed stone to collect all contiguous aligned cells
- **`five_is_breakable(b, p, alignment, pt)`**: Helper that checks if an opponent has a legal move that would break a five-in-a-row by capturing a stone from the alignment

### Key Rules Implemented

The `five_is_breakable` function correctly implements the complex endgame rule (spec §7.3):
- If the player has already lost 4 pairs (8 stones) AND the opponent has ANY legal capture available, the five is considered breakable (doesn't win)
- If the opponent has ANY legal move that captures a stone that is part of the five's alignment, the five is breakable (doesn't win)
- Otherwise, the five wins outright

### Import Changes

Updated imports to include:
- `TOTAL`: From `board` module (for boundary checking)
- `F_FIVE`: From `patterns` module (for pattern flag checking)

## TDD Evidence

### RED (Compile Failure Before Implementation)

```
$ cargo test --bin gomoku rules:: -- --nocapture
error[E0425]: cannot find function `check_end` in this scope
error[E0433]: cannot find type `GameEnd` in this scope
```

Four tests were added that referenced non-existent `check_end` function and `GameEnd` type, causing 8 compilation errors.

### GREEN (All Tests Passing After Implementation)

```
$ cargo test --bin gomoku
running 26 tests
test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

All 26 tests pass:
- 7 original tests from Task 6 (free-three detection, generate)
- 4 new tests from this task (game end conditions)
- 15 tests from board and patterns modules

Individual test results:
- ✓ `unbreakable_five_wins`: Five without capture vulnerability wins
- ✓ `five_broken_by_available_capture_is_not_a_win`: Opponent can capture stone from five
- ✓ `five_not_a_win_when_mover_already_lost_four_pairs_and_capture_available`: Endgame capture rule
- ✓ `ten_stones_captured_wins_by_capture`: Win by capture is unconditional

## Files Changed

- **`src/rules.rs`**: Added `GameEnd` enum, `check_end`, `collect_alignment`, and `five_is_breakable` functions. Added 4 new tests to the test module.

## Commit

```
df9b43b feat: game end detection — endgame capture rule, win by capture, draw
```

Message exactly matches the brief specification.

## Self-Review Findings

### What's Correct
- All code matches the brief specification exactly (copy-pasted, no modifications)
- `collect_alignment` correctly walks in both directions along an axis, stopping at board boundaries
- `five_is_breakable` correctly implements both branches of the endgame rule:
  - Loss threshold check (8 stones) with any-capture search
  - Move validation with capture-intersection check
- `check_end` checks conditions in the correct order (capture win, then alignment win, then draw)
- No extraneous code or over-engineering
- Tests are comprehensive and test real behavior:
  - Test 1: Basic unbreakable five
  - Test 2: Five breakable by opponent capture (main complexity)
  - Test 3: Endgame rule with loss threshold
  - Test 4: Win by capture

### No Issues Found
- No warnings introduced by this task (pre-existing warnings in main.rs are unrelated)
- No changes beyond what was specified in the brief
- Release build succeeds with no new errors
- All 26 tests pass with clean output
