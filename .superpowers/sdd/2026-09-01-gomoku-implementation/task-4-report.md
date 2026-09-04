# Task 4 Report: `board.rs` — `Board` struct, Zobrist, `captures_of`

## Summary

Implemented the `Board` struct with Zobrist key generation and capture detection for Gomoku's Ninuki-renju rule (pair capture: `X O O X` pattern). Added 6 new test cases and verified all 11 board tests pass.

## Implementation Details

### Code Added to `src/board.rs`

**1. Xorshift64 RNG** (deterministic, fixed-seed for reproducible Zobrist initialization)
- Simple 64-bit xorshift with shifts [13, 7, 17]
- Seed: `0x9E3779B97F4A7C15`
- No external `rand` dependency required

**2. Board struct** (143 lines)
- `cells: [Cell; TOTAL]` — 729-cell backing array (27×27 with 4-cell padding border)
- `captures: [u8; 2]` — capture counters per player (win at 10 stones = 5 pairs)
- `to_move: Player` — whose turn it is
- `zobrist: u64` — incremental hash for transposition tables
- `stone_count: u32` — total stones played
- `neighbor: [u8; TOTAL]` — Chebyshev radius-2 stone count (drives candidate generation)
- `acc: [i32; 2]` — heuristic accumulator per player
- `key_cell, key_side, key_captures` — Zobrist tables (generated once at init)

**3. Board::new()** — initializes all fields
- Builds Zobrist tables via Xorshift64 (deterministic, ~no cost)
- Sets all playable cells (0..SIZE × 0..SIZE) to `Empty`
- Pads outer ring with `Wall`
- `zobrist` starts with `key_side` XOR'd in (Black moves first)

**4. Board::get(i: Idx) -> Cell**
- Safe read: `.get(i as usize).copied().unwrap_or(Cell::Wall)`
- Defaults to `Wall` outside bounds (no panics, works at board edges)

**5. Board::set_raw(i: Idx, c: Cell)**
- Direct cell write, no Zobrist/neighbor/acc update
- Used by tests and by Task 5's `play`/`undo` for explicit bookkeeping
- Safe write: `.get_mut(i as usize)`

**6. Board::captures_of(mv: Idx, p: Player) -> ([Idx; 16], usize)**
- Detects stones captured if player `p` plays at move `mv`
- Pattern: `p O O p` in any of 8 directions (4 axes × 2 signs)
- Returns array of captured indices and count
- Array sized to max 16 (4 axes × 2 signs × 2 stones per capture)
- Safe indexing: `.get_mut(n)` to write output
- Rejects 3+ consecutive opponent stones (spec VI.1)
- Wall cells fail all comparisons, no edge-case bugs

**7. Default impl** for Board (delegates to `new()`)

## Test Results

### RED Phase (before implementation)
```
error[E0433]: cannot find type `Board` in this scope
```
6 compilation errors (one per test case).

### GREEN Phase (after implementation)
```
running 11 tests
test board::tests::captures_of_a_wall_neighbor_finds_nothing ... ok
test board::tests::captures_of_ignores_single_stone ... ok
test board::tests::captures_of_checks_all_eight_directions ... ok
test board::tests::captures_of_detects_flanking_pair_horizontally ... ok
test board::tests::dirs_are_the_four_distinct_axes ... ok
test board::tests::corners_are_within_padded_bounds ... ok
test board::tests::idx_to_xy_roundtrip_covers_full_board ... ok
test board::tests::new_board_is_all_empty_except_border ... ok
test board::tests::captures_of_ignores_three_in_a_row ... ok
test board::tests::player_other_and_cell_mapping ... ok
test board::tests::four_step_walk_from_every_playable_cell_stays_in_bounds ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out
```

All 11 tests pass (5 from Task 3 + 6 new).

## Build Verification

```
cargo build --release
Finished `release` profile [optimized] in 1.81s
```

Clean release build, no errors. Warnings are from unused functions in `patterns.rs` (not introduced this task).

## Self-Review

✅ **Tests:** All 11 pass (5 Task-3 regression + 6 new Task-4)
✅ **Coverage:** Board construction, all 8 directions, pair detection, single/triple rejection, edge case (wall neighbor)
✅ **Safety:** All array accesses use `.get()` or `.get_mut()` (no indexing_slicing lint violation)
✅ **No unsafe:** `#![forbid(unsafe_code)]` respected
✅ **Zobrist:** Deterministic seeding, reproducible across runs
✅ **Capture logic:** Exact spec (p O O p, rejects 3+, checks all 8 directions)
✅ **Commit:** Message matches brief exactly

## Files Changed

- `src/board.rs` — added 206 lines (Xorshift64, Board struct, implementations, 6 tests)

## Concerns

None. Implementation matches the brief exactly, all tests pass, release build succeeds, no unsafe code or clippy violations introduced.

## Commit Info

```
4c0c5e7 feat: Board struct, Zobrist keys, capture detection
```
