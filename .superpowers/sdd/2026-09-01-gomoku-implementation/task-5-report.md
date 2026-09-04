# Task 5 Report: `board.rs` — `play`/`undo` with incremental accumulator

## Summary

Implemented `Undo`, `Board::play`, `Board::undo`, and the private incremental-accumulator
helpers (`cell_at`, `window_code`, `stone_window_score`, `pair_vuln_score`,
`adjust_axis_neighbors`, `adjust_axis_vuln`, `adjust_neighbor_grid`), transcribed
verbatim from the task brief. Added `#[derive(Clone)]` to `Board` and the two brief-specified
tests (`play_undo_round_trip_restores_exact_state`, `play_capture_updates_captures_and_frees_cells`).

This was flagged as the highest-risk task in the plan (the incremental accumulator's
"subtract old, mutate, add new" bookkeeping across three call sites in `play`). I followed
the brief's TDD steps literally, then independently verified transcription fidelity by
diffing the brief's fenced code blocks against the corresponding regions of the committed
file — see "Self-Review" below.

## Code added to `src/board.rs`

- `use crate::patterns::{PatternTable, POW3};` — required import not spelled out
  verbatim in the brief's diffs but necessary for the brief's own code (`PatternTable`,
  `POW3` used unqualified) to compile; added under the file's existing `#![forbid(unsafe_code)]`
  header.
- `#[derive(Clone)]` on `Board` (needed only for the round-trip test's snapshot; `Board`
  deliberately does not derive `Copy`).
- `const VULN_PENALTY: i32 = -1_200;` and `pub struct Undo { .. }`, placed below the
  existing `DIRS` constant.
- Private helpers `cell_at`, `window_code`, `stone_window_score`, `pair_vuln_score`,
  `adjust_axis_neighbors`, `adjust_axis_vuln`, `adjust_neighbor_grid`, plus
  `pub fn play` and `pub fn undo`, added to the existing `impl Board` block, appended
  right after `captures_of`.
- Two new tests in `#[cfg(test)] mod tests`: `random_empty_cell` helper,
  `play_undo_round_trip_restores_exact_state` (1000-seed property test), and
  `play_capture_updates_captures_and_frees_cells`.

### One deliberate deviation from the brief's literal text

The brief's test code included this comment on its own admission:

> Note on the last assertion: `before_black_stone_count` is the count right after
> manually placing the two black stones via `set_raw`... the implementer should
> replace it with a direct computed expectation... Simplify this assertion during
> Step 3 if it proves confusing to read.

The brief's literal text was `assert_eq!(b.stone_count, before_black_stone_count + 1 - 1);`
which is arithmetically wrong (see RED/GREEN transcript below — it failed on first run).
Correct bookkeeping: `before_black_stone_count` (1, the one Black stone placed via `play`
before this point) + White's `mv` (+1) − the captured pair (−2) = 0. I fixed the assertion
to `before_black_stone_count + 1 - 2` with an explanatory comment, exactly as the brief's
own note authorized. No other line of implementation or test code was changed relative to
the brief.

## Step 2 — RED (verified before writing any implementation code)

Built a file containing the original (Task 1-4) `src/board.rs` plus *only* the Step-1 test
additions (no `Clone` derive, no `Undo`, no helpers, no `play`/`undo`), and ran it to confirm
a genuine compile failure before touching the implementation.

Command: `cargo test board:: -- --nocapture` (the brief says `cargo test --lib board::`, but
this is a bin-only crate with no `[lib]` target — `--lib` fails immediately with "no library
targets found"; dropping `--lib` is what Task 4's own report used too).

```
error[E0433]: cannot find type `PatternTable` in this scope
   --> src/board.rs:344:18
    |
344 |         let pt = PatternTable::build();
    |                  ^^^^^^^^^^^^ use of undeclared type `PatternTable`

error[E0433]: cannot find type `PatternTable` in this scope
   --> src/board.rs:371:18
    |
371 |         let pt = PatternTable::build();
    |                  ^^^^^^^^^^^^ use of undeclared type `PatternTable`

error[E0599]: no method named `clone` found for struct `board::Board` in the current scope
   --> src/board.rs:348:30
    |
 75 | pub struct Board {
    | ---------------- method `clone` not found for this struct
...
348 |             let snapshot = b.clone();
    |                              ^^^^^ method not found in `board::Board`

error[E0599]: no method named `play` found for struct `board::Board` in the current scope
   --> src/board.rs:354:30
...
error[E0599]: no method named `undo` found for struct `board::Board` in the current scope
   --> src/board.rs:357:19
...
error[E0599]: no method named `play` found for struct `board::Board` in the current scope
   --> src/board.rs:375:11
...
error[E0599]: no method named `play` found for struct `board::Board` in the current scope
   --> src/board.rs:380:11
...
error: could not compile `gomoku` (bin "gomoku" test) due to 7 previous errors
```

Matches the brief's prediction exactly: `Board: Clone`, `Undo`, `Board::play`, `Board::undo`
don't exist (plus `PatternTable` unresolved, since the import wasn't added yet either).

## Step 3 — implementation

Restored the full implementation (helpers + `play` + `undo` + `use` import + `Clone` derive)
and re-ran.

## Step 4 — GREEN

First run (before fixing the illustrative assertion) surfaced exactly the arithmetic bug
the brief warned about, and nothing else:

```
running 13 tests
test board::tests::captures_of_a_wall_neighbor_finds_nothing ... ok
test board::tests::captures_of_checks_all_eight_directions ... ok
test board::tests::captures_of_detects_flanking_pair_horizontally ... ok
test board::tests::captures_of_ignores_three_in_a_row ... ok
test board::tests::corners_are_within_padded_bounds ... ok
test board::tests::dirs_are_the_four_distinct_axes ... ok
test board::tests::captures_of_ignores_single_stone ... ok
test board::tests::idx_to_xy_roundtrip_covers_full_board ... ok
test board::tests::new_board_is_all_empty_except_border ... ok
test board::tests::four_step_walk_from_every_playable_cell_stays_in_bounds ... ok
test board::tests::player_other_and_cell_mapping ... ok

thread 'board::tests::play_capture_updates_captures_and_frees_cells' (716581) panicked at src/board.rs:609:9:
assertion `left == right` failed
  left: 0
 right: 1
test board::tests::play_capture_updates_captures_and_frees_cells ... FAILED
test board::tests::play_undo_round_trip_restores_exact_state ... ok

test result: FAILED. 12 passed; 1 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.53s
```

Critically, `play_undo_round_trip_restores_exact_state` (the 1000-seed property test — cells,
captures, to_move, zobrist, stone_count, neighbor grid, AND acc all checked per seed) passed
on this very first run with no implementation changes — strong evidence the accumulator
bookkeeping across all three `play` call sites (before `mv`, per capture, after all mutations)
was transcribed correctly the first time. Only the known-flagged test assertion failed.

After fixing the assertion (`+1-1` → `+1-2`, see deviation note above), full re-run:

```
$ cargo test board:: -- --nocapture

running 13 tests
test board::tests::captures_of_a_wall_neighbor_finds_nothing ... ok
test board::tests::captures_of_checks_all_eight_directions ... ok
test board::tests::captures_of_detects_flanking_pair_horizontally ... ok
test board::tests::corners_are_within_padded_bounds ... ok
test board::tests::captures_of_ignores_single_stone ... ok
test board::tests::dirs_are_the_four_distinct_axes ... ok
test board::tests::captures_of_ignores_three_in_a_row ... ok
test board::tests::idx_to_xy_roundtrip_covers_full_board ... ok
test board::tests::new_board_is_all_empty_except_border ... ok
test board::tests::four_step_walk_from_every_playable_cell_stays_in_bounds ... ok
test board::tests::player_other_and_cell_mapping ... ok
test board::tests::play_capture_updates_captures_and_frees_cells ... ok
test board::tests::play_undo_round_trip_restores_exact_state ... ok

test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.50s
```

Full workspace test run (includes `patterns.rs`'s 19,683-code oracle test):

```
$ cargo test

running 14 tests
test board::tests::captures_of_a_wall_neighbor_finds_nothing ... ok
test board::tests::captures_of_checks_all_eight_directions ... ok
test board::tests::captures_of_detects_flanking_pair_horizontally ... ok
test board::tests::captures_of_ignores_single_stone ... ok
test board::tests::corners_are_within_padded_bounds ... ok
test board::tests::dirs_are_the_four_distinct_axes ... ok
test board::tests::captures_of_ignores_three_in_a_row ... ok
test board::tests::idx_to_xy_roundtrip_covers_full_board ... ok
test board::tests::new_board_is_all_empty_except_border ... ok
test board::tests::four_step_walk_from_every_playable_cell_stays_in_bounds ... ok
test board::tests::player_other_and_cell_mapping ... ok
test board::tests::play_capture_updates_captures_and_frees_cells ... ok
test patterns::tests::table_matches_naive_oracle_on_all_codes ... ok
test board::tests::play_undo_round_trip_restores_exact_state ... ok

test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.51s
```

`cargo build --release`:

```
Finished `release` profile [optimized] target(s) in 1.87s
```

Clean build. 32 dead-code warnings, all pre-existing in nature (unused constants/functions/structs
because `rules.rs`/`search.rs`/`eval.rs` don't exist yet to consume them — same pattern noted
in Task 4's own report, nothing new introduced by this task's code).

## Step 5 — Commit

```
git commit -m "feat: incremental play/undo with accumulator, vulnerability term, Zobrist"
```

Commit: `31ba8d4`.

## Files Changed

- `/home/fwong/Desktop/42/gomoku/.claude/worktrees/gomoku-impl/src/board.rs` (+289 lines)

## Self-Review

Checked specifically, per the task instructions:

- **`play_undo_round_trip_restores_exact_state` runs all 1000 seeds and passes:** Yes —
  the test's own loop is `for seed in 0..1000u64`, no early exit; `cargo test` output shows
  `... ok` for this test with no panic/failure message anywhere in either run's output
  (checked both the pre-fix run, where only the *other* test failed, and the post-fix run).
- **`play_capture_updates_captures_and_frees_cells` passes:** Yes, after the one
  brief-authorized assertion fix.
- **No regression on prior tests:** All 11 Task 3/4 tests still pass, plus the pre-existing
  `patterns::tests::table_matches_naive_oracle_on_all_codes` (19,683-code oracle) — 14/14 total.
- **Three `adjust_axis_neighbors`/`adjust_axis_vuln` call sites, `sign` correct:** Verified
  by `grep -n` against the committed file:
  - Line 335–336: `sign: -1` before `mv` is placed.
  - Line 346–347: `sign: -1` before each captured stone is removed (inside the `for cc in
    captured.iter().take(n)` loop, before `set_raw(*cc, Cell::Empty)`).
  - Line 359–360 and 362–363: `sign: 1` after all mutations, for both `mv` and every
    captured cell.
- **Byte-exact transcription check:** I extracted the brief's fenced code blocks (Undo/
  `VULN_PENALTY`, the 200-line helpers+`play`+`undo` block, and the test block) with a
  script and diffed each against the corresponding line range of the final committed
  `src/board.rs`. The `Undo`/`VULN_PENALTY` block and the entire 200-line implementation
  block are byte-for-byte identical to the brief. The only diff in the test block is the
  one assertion the brief explicitly told the implementer to fix (`+1-1` → `+1-2`, with a
  clarifying comment replacing the brief's placeholder comment).
- **`cargo build --release`:** Succeeds, no errors, only pre-existing-pattern dead-code
  warnings (no unused-import or new-category warnings from this task's code).
- **Clippy (informational only — Task 13's job, not this task's):** `cargo clippy
  --all-targets` reports only "indexing may panic" / "slicing may panic" errors, all from
  the raw `[]` indexing the brief's code deliberately uses (e.g. `self.key_cell[p as
  usize][mv as usize]`, `self.acc[owner as usize]`, `self.captures[p as usize]`). No other
  clippy error categories appeared. Per this task's explicit instructions, these are left
  as-is for Task 13's dedicated, pre-planned fix (see `progress.md`'s "Ruling 1", which
  already accounts for exactly this class of site).

## Concerns

None. The round-trip property test passing cleanly on its very first compile — including
the `acc` field, which is the field most likely to reveal a missed or mismatched
`adjust_axis_neighbors`/`adjust_axis_vuln` call — is strong evidence the transcription is
correct, and the block-diff check against the brief's literal text confirms it independently
of test coverage.

---

# Fix Report: capture-path accumulator double-counting (reviewer finding)

## Bug

`play()`'s capturing path called `adjust_axis_neighbors`/`adjust_axis_vuln` once per center
(`mv`, then once per captured stone). Because a capture's flanking stone (`mv`) and the
captured pair are always within a few cells of each other, their radius-4 (resp. radius-2)
influence zones routinely overlap, so cells in the overlap got touched multiple times against
mismatched *intermediate* board snapshots — double-counting. Reviewer's empirical repro: on
`White _ Black Black`, White plays and captures the pair, incremental `acc` came out `[0, 10]`
vs. true recomputed `[0, 5]`.

## Fix applied (exactly as designed by the controller)

- Added two new private methods to `impl Board`, right after `adjust_axis_vuln`:
  `adjust_axis_dedup(&mut self, changed: &[Idx], pt: &PatternTable, sign: i32)` and
  `adjust_axis_vuln_dedup(&mut self, changed: &[Idx], sign: i32)`. Each builds a deduplicated
  `Vec<(Idx, u8)>` of every `(cell, axis)` window reachable from any center in `changed`
  (including the center itself at `k == 0`), then applies `sign * score` to that window exactly
  once — reading the true pre-move board when called with `sign = -1` before any mutation, and
  the true post-move board when called with `sign = 1` after all mutations.
- Replaced `play()`'s body: `captures_of` now runs *before* any mutation (safe/behavior-identical
  — it never reads `cell(mv)`, only `mv±d/±2d/±3d`), giving one clean pre-move snapshot. The
  `Undo` is built directly from that result (no more `mut undo` + late field patch). If `n == 0`
  (no capture), the original single-center subtract/mutate/add logic is kept completely
  unchanged. If `n > 0`, the per-center calls and the old separate "own axis" loops for `mv` and
  each captured stone are deleted, replaced by: `adjust_axis_dedup`/`adjust_axis_vuln_dedup` with
  `sign = -1` over `changed = {mv} ∪ captured` before any mutation, then all mutations (place
  `mv`, remove every captured stone), then the same dedup calls with `sign = 1`.
- Added a `#[cfg(test)]`-gated `impl Board` block (after the main `impl Board` block, before
  `impl Default for Board`) with `pub(crate) fn full_recompute_acc(&self, pt: &PatternTable) ->
  [i32; 2]`, which independently recomputes the accumulator from scratch by scanning every
  occupied cell/axis — pulled forward from Task 8 per the controller's note (Task 8 should reuse
  this, not re-add it).
- Strengthened `play_capture_updates_captures_and_frees_cells` with a final
  `assert_eq!(b.acc, b.full_recompute_acc(&pt), ...)`.
- Added `accumulator_matches_full_recompute_including_captures`: 300 seeds x up to 60 random
  plays each, asserting `b.acc == b.full_recompute_acc(&pt)` after every single `play()` call.

## Deviation from the literal instructions, and why

Applying the mandated `acc` assertion to `play_capture_updates_captures_and_frees_cells` as
literally specified (append-only, leave the rest of the test alone) **failed**:
`left: [1100, 5]  right: [0, 5]`. Before concluding this was a gap in `adjust_axis_dedup`, I
instrumented the test step-by-step (temporary `eprintln!`s comparing `b.acc` against
`b.full_recompute_acc` after each statement, since removed) and found the drift already present
*before* the capturing move ever ran:

```
after set_raw White(0,0):      acc=[0, 0]      full=[0, 0]
after Black play(1,0):         acc=[0, 0]      full=[0, 0]
after set_raw Black(2,0):      acc=[0, 0]      full=[-1100, 0]
after White play(3,0) capture: acc=[1100, 5]   full=[0, 5]
```

The test's fixture built two of its three stones with `set_raw` (direct cell write, bypassing
`acc` bookkeeping entirely — "set up manually instead of alternating turns, to isolate the
capture", per the original Task 5 test comment). The second `set_raw` call already desyncs
tracked `acc` from the true board by `-1100` — *before* White's capturing move is even played.
White's capturing move then adds exactly `+1100` to `acc[Black]` (`0 - (-1100) = 1100`, precisely
the delta needed to walk the *true* value from `-1100` to `0`), proving the new dedup math itself
computed the mathematically correct delta. The residual mismatch is 100% inherited pre-existing
fixture drift, not a gap in `adjust_axis_dedup`/`adjust_axis_vuln_dedup`. This is corroborated
by `accumulator_matches_full_recompute_including_captures`, which never uses `set_raw` and passed
cleanly across all 300 seeds x 60 plays (many involving captures) on the very first run.

Per systematic-debugging (root cause, not symptom): I fixed the actual bug — the test fixture's
`set_raw`-based setup — rather than weakening the new assertion or chasing a phantom gap in the
already-proven-correct dedup logic. Rebuilt `play_capture_updates_captures_and_frees_cells`
using natural alternating `play()` calls only (Black plays (1,0), White plays (0,0), Black plays
(2,0), White plays (3,0) and captures) — same final position, same capture, no `set_raw`, so the
`acc` invariant holds throughout and the assertion is a genuine, meaningful guard. Renamed
`before_black_stone_count` to `before_capture_stone_count` and simplified its comment now that
the arithmetic is straightforward (no more `set_raw`-vs-`play()` bookkeeping asymmetry to explain).
No other assertion in that test was touched; its exact scenario (`White _ Black Black`, White
plays and captures) is unchanged.

## Test command and output

```
$ cargo test board:: -- --nocapture

running 14 tests
test board::tests::captures_of_a_wall_neighbor_finds_nothing ... ok
test board::tests::captures_of_detects_flanking_pair_horizontally ... ok
test board::tests::captures_of_checks_all_eight_directions ... ok
test board::tests::captures_of_ignores_three_in_a_row ... ok
test board::tests::idx_to_xy_roundtrip_covers_full_board ... ok
test board::tests::captures_of_ignores_single_stone ... ok
test board::tests::corners_are_within_padded_bounds ... ok
test board::tests::dirs_are_the_four_distinct_axes ... ok
test board::tests::new_board_is_all_empty_except_border ... ok
test board::tests::player_other_and_cell_mapping ... ok
test board::tests::four_step_walk_from_every_playable_cell_stays_in_bounds ... ok
test board::tests::play_capture_updates_captures_and_frees_cells ... ok
test board::tests::play_undo_round_trip_restores_exact_state ... ok
test board::tests::accumulator_matches_full_recompute_including_captures ... ok

test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.67s
```

Confirmed: both `accumulator_matches_full_recompute_including_captures` and the strengthened
`play_capture_updates_captures_and_frees_cells` are in this list and passed (`... ok`), not
filtered out.

`cargo build --release`:

```
warning: `gomoku` (bin "gomoku") generated 32 warnings
    Finished `release` profile [optimized] target(s) in 0.01s
```

Clean build — same 32 pre-existing dead-code warnings as the original Task 5 report (no new
warning categories from this fix). `cargo test` (full workspace, includes `patterns::tests`)
also reconfirmed at 15/15 passing.

## Files changed

- `/home/fwong/Desktop/42/gomoku/.claude/worktrees/gomoku-impl/src/board.rs`

## Commit

`c36427b` — "fix: dedupe incremental accumulator on captures to stop double-counting overlapping
windows" (on top of `31ba8d4`, not amending it).
