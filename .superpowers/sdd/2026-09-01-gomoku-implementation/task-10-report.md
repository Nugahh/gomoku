# Task 10 Report: `search.rs` — move ordering and candidate truncation

## What I implemented

Followed the brief (`task-10-brief.md`) as a transcription task, in order:

1. **`Board::hypothetical_window_code`** (`src/board.rs`, added right after `window_code`): pure-read variant of `window_code` that treats the center cell `c` as if it already held `p`'s stone (trit forced to `1` at `k == 0`), used so move ordering can ask "what pattern would playing here create" without mutating the board via `play`/`undo`.
2. **Task 9 test mechanical update**: replaced the `SearchCtx { ... }` struct literal in `negamax_recognizes_an_immediate_win` with `SearchCtx::new(&pt, &mut tt, &cfg, far_deadline())`.
3. **Two new failing tests** in `search.rs`'s test module: `order_score_ranks_five_above_quiet_move` and `order_score_ranks_tt_move_above_everything`.
4. **Ordering constants** (`ORD_TT` down through `ORD_HISTORY_CAP`, `MAX_PLY`) and new imports (`Cell, DIRS, TOTAL` from `board`; `F_FIVE, F_OPEN_FOUR` from `patterns`).
5. **Extended `SearchCtx`** with `killers: [[Idx; 2]; MAX_PLY]`, `history: [i32; TOTAL]`, `tt_hits: u64`, `tt_probes: u64`, plus a `SearchCtx::new` constructor zeroing the new fields.
6. **`order_score`** function implementing the priority ladder: TT move → own five → block opponent's five/open-four → own open-four → captures (scaled by pairs captured) → killer-1/killer-2 → history (capped) + static positional gain for ordinary quiet moves.
7. **Rewired `negamax`**: now probes the TT at the top (counting `tt_probes`/`tt_hits`, returning early on a sufficiently-deep exact/lower/upper cutoff), scores and sorts all generated candidates by `order_score` (descending), truncates to `cfg.max_candidates`, records killer moves and history-heuristic bonus on beta cutoffs, and stores a TT entry (bound derived from `orig_alpha`/`beta`) after the move loop.

All code was transcribed verbatim from the brief; no design decisions were needed and no brief-internal contradictions were found.

## TDD Evidence

### RED

```
$ cargo test search:: -- --nocapture
error[E0425]: cannot find value `ORD_FIVE` in this scope
   --> src/search.rs:249:35
error[E0425]: cannot find value `ORD_TT` in this scope
   --> src/search.rs:266:35
error[E0599]: no associated function or constant named `new` found for struct `search::SearchCtx<'a>` in the current scope
   --> src/search.rs:226:34
error[E0425]: cannot find function `order_score` in this scope
   --> src/search.rs:243:29
   --> src/search.rs:246:27
   --> src/search.rs:262:29
error: could not compile `gomoku` (bin "gomoku" test) due to 6 previous errors
```

Matches the brief's Step 4 prediction exactly: `order_score`, `SearchCtx::new`, `ORD_FIVE`, `ORD_TT` undefined.

(Note: the crate has no `[lib]` target, only `[[bin]]` — `cargo test --lib search::` fails with "no library targets found in package `gomoku`". Used `cargo test search:: -- --nocapture` instead, consistent with how Task 9's report ran its tests.)

### GREEN

```
$ cargo test search:: -- --nocapture
running 4 tests
test search::tests::order_score_ranks_five_above_quiet_move ... ok
test search::tests::order_score_ranks_tt_move_above_everything ... ok
test search::tests::tt_round_trip_and_replacement_policy ... ok
test search::tests::negamax_recognizes_an_immediate_win ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 28 filtered out; finished in 0.04s
```

All 4 search.rs tests green (2 from Task 9, 2 new), as expected by Step 6.

### Full suite + release build

```
$ cargo build --release
    Finished `release` profile [optimized] target(s) in 1.82s
(warnings only: unused `Cell` import in search.rs — expected per brief §Step 5 note,
 pending Task 12; `order_score`/`negamax` "never used" — expected, wired up in Task 11)

$ cargo test
running 32 tests
...
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.61s
```

## Files changed

- `/home/fwong/Desktop/42/gomoku/.claude/worktrees/gomoku-impl/src/board.rs` — added `hypothetical_window_code` (+25 lines)
- `/home/fwong/Desktop/42/gomoku/.claude/worktrees/gomoku-impl/src/search.rs` — ordering constants, extended `SearchCtx` + constructor, `order_score`, rewired `negamax`, updated Task 9 test, 2 new tests (+223/-10 lines total across both files)

## Self-review findings

- Diffed the final state against the brief's code blocks line-by-line; both `hypothetical_window_code` and `order_score`/`negamax`/`SearchCtx` match verbatim.
- Confirmed `b.zobrist` (used by the new TT probe/store in `negamax`) is a real, already-public field on `Board` (set in Task 5/9's `play`/`undo`), not something I needed to add.
- Confirmed `DIRS: [i16; 4]`, `Idx = u16` (so `Idx::MAX` and array-of-`Idx` init work natively), `F_FIVE`/`F_OPEN_FOUR` on `Pat.flags: u8`, and `captures_of(&self, mv: Idx, p: Player) -> ([Idx; 16], usize)` all match the brief's usage exactly — no signature mismatches.
- Ran `cargo clippy --all-targets` and plain `cargo clippy`; both report pre-existing errors (indexing_slicing/expect_used deny-lints) entirely in code and tests from Tasks 1-9 (`board.rs`, `patterns.rs`, `rules.rs`, `eval.rs`, and Task 9's original `tt_round_trip_and_replacement_policy` test). Verified via `git stash` that the exact same 71 (`--all-targets`) / 39 (plain) errors exist at the parent commit `783f334`, before any of my changes — none are in my diff (`hypothetical_window_code`, `order_score`, the rewired `negamax`, or the two new tests are all clippy-clean). Not this task's concern to fix.
- Did not add anything beyond the brief's scope: no pruning/extensions/LMR (Task 12), no iterative deepening or `find_best_move` (Task 11). `order_score` and `negamax` are currently unreferenced outside their own tests — expected, since nothing outside `search.rs` calls into search yet.
- Test output is pristine: `cargo test search::` and `cargo test` (full suite, 32/32) both pass cleanly with no failures, no flakes, no `#[ignore]`.

## Issues or concerns

None. No brief-internal contradictions found this time — straightforward transcription task, verified against actual current file state (not assumed Task 9 line numbers).

## Post-review fix (Ruling 5)

Review of commit `542fd1c` found one Important, plan-mandated issue: the history-heuristic write in `negamax`'s beta-cutoff branch, `*slot += (depth as i32) * (depth as i32);`, had no write-side overflow cap — could wrap in release (no `overflow-checks` in the release profile) or panic in debug/test over a very long search. Ruled low real-world risk (history resets fresh each `find_best_move` call, bounded to one ~400ms search) but cheap to close.

Fix: changed the line to `*slot = slot.saturating_add((depth as i32) * (depth as i32));` in `src/search.rs`. Nothing else touched — the `.min(ORD_HISTORY_CAP)` read-side clamp in `order_score` is unchanged, no decay/aging added.

```
$ cargo test search:: -- --nocapture
running 4 tests
test search::tests::order_score_ranks_tt_move_above_everything ... ok
test search::tests::order_score_ranks_five_above_quiet_move ... ok
test search::tests::tt_round_trip_and_replacement_policy ... ok
test search::tests::negamax_recognizes_an_immediate_win ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 28 filtered out; finished in 0.03s

$ cargo build --release
    Finished `release` profile [optimized] target(s) in 1.82s
```

Same pre-existing warnings as before (unused `Cell` import, `order_score`/`negamax` unused outside tests — both expected until Tasks 11-12 wire them in); no new warnings introduced. Committed as `4ca8fa8` (separate commit, `542fd1c` not amended).
