# Task 11 Report — `search.rs`: iterative deepening, PVS, time control, `find_best_move`

**Status: DONE.** (Originally reported BLOCKED below pending a ruling on the contradiction; see the "Resolution — Ruling 6" section at the end for how it was closed out.)

## What I did

1. Read the brief in full: `.superpowers/sdd/2026-09-01-gomoku-implementation/task-11-brief.md`.
2. Cross-checked every API surface the brief's code touches against the current `src/board.rs`, `src/rules.rs`, and `src/search.rs` (Tasks 1-10): `Board::get`/`play`/`undo`, `Cell::Empty`, `SIZE`, `idx`, `rules::generate`, `rules::check_end`/`GameEnd`, `SearchCtx` fields, `order_score` signature. All present and matching exactly what the brief assumes — no missing symbols.
3. Read spec §9.1, §9.2, §9.5, §9.7 (`docs/superpowers/specs/2026-09-01-gomoku-design.md`) specifically to check the two things flagged as worth double-checking:
   - **Aspiration window widening/re-search**: spec §9.2 says "after depth 3, search `[prev-50, prev+50]`; on a fail, re-search with a full window" — a direct jump to `(-WIN, WIN)` on failure, no progressive widening. The brief's `find_best_move` does exactly this. No contradiction here.
   - **"Always return last completed depth" invariant**: spec §9.2/§9.7 say the best move from the last completed iteration is always returned, and an abort must not use the incomplete iteration's result. The brief's loop correctly `break`s without updating `best_move`/`last_score`/`depth_reached`/`root_scores` whenever `root_search` (either the aspiration-window call or the full-window re-search) returns `None`. No contradiction here.
4. **TDD Step 1 (RED)**: Added the three tests verbatim from the brief, plus the `SIZE` import extension. Ran `cargo test search:: -- --nocapture` (per the parent task's note: this crate has no `[lib]` target, so `--lib search::` doesn't apply — bare `search::` filters correctly). Confirmed compile failure: `error[E0425]: cannot find function 'find_best_move' in this scope` (3 occurrences, one per new test). This matches the brief's expected RED.
5. **Step 3**: Added `root_search`, `extract_pv`, `find_best_move` verbatim, above `#[cfg(test)]`.
6. **Step 4 (GREEN attempt)**: Ran `cargo test search:: -- --nocapture`. Result: 6 passed, 1 failed.
   - `tt_round_trip_and_replacement_policy` — ok
   - `negamax_recognizes_an_immediate_win` — ok
   - `order_score_ranks_five_above_quiet_move` — ok
   - `order_score_ranks_tt_move_above_everything` — ok
   - `find_best_move_on_empty_board_returns_center` — ok
   - `find_best_move_respects_time_budget` — ok
   - `find_best_move_takes_the_immediate_win` — **FAILED**, deterministically, every run (checked 5x in a row): `assertion 'left == right' failed / left: 250 / right: 255` (`250 = idx(3,5)`, `255 = idx(8,5)`).

## The contradiction

**Test (brief Step 1, verbatim):**
```rust
let mut b = Board::new();
for x in 4..8 {
    b.to_move = Player::Black;
    b.play(idx(x, 5), &pt);
}
b.to_move = Player::Black;
let cfg = SearchConfig { max_depth: 6, time_budget_ms: 400, max_candidates: 20 };
let (mv, stats) = find_best_move(&mut b, &cfg, &pt, &mut tt);
assert_eq!(mv, idx(8, 5));
```
This places four Black stones at `(4,5)..(7,5)` with **nothing else on the board** — no White stones, no walls in range. That is a symmetric open four: `(3,5)` and `(8,5)` are *both* immediate, unbreakable wins (no captures are possible with zero White stones, so neither end's five can be broken). The test asserts the engine must specifically pick `(8,5)`.

**Implementation (brief Step 3, verbatim, `root_search`):**
```rust
let score = match end {
    GameEnd::Win(_) => WIN - 1,
    ...
};
...
if score > best {   // strict '>' — ties keep the first-seen move
    best = score;
    best_move = mv;
}
```
Every winning root move scores exactly `WIN - 1` — there is no ply- or position-dependent tiebreak for immediate wins at the root (unlike `negamax`'s internal `WIN - ply - 1`, which doesn't apply here since `root_search` short-circuits on `GameEnd::Win` before recursing). Combined with `order_score` (Task 10, unchanged by this task), both `(3,5)` and `(8,5)` score identically at `ORD_FIVE = 900_000` (verified directly, see below), so the candidate list sorts them as a tie, and `scored.sort_unstable_by` + the strict `>` in the loop above deterministically keeps whichever one appears first in `rules::generate`'s output. `generate` walks the board row-major (`y` outer, `x` inner, per `src/rules.rs`), so `(3,5)` (lower `x`) is generated and thus ordered before `(8,5)`.

**Empirical confirmation** (temporary probe added and removed, not part of the diff): with the exact board from the test,
```
order_score left(3,5)=900000 right(8,5)=900000
generate order: left_pos=Some(17) right_pos=Some(18) total=36
```
Both moves tie in ordering score; `(3,5)` is generated first. `find_best_move` picks `(3,5)` — a **correct, optimal, immediate-win move** — but not the one the test hardcodes as `right`.

**Why this isn't a flaky-test situation** (unlike the time-budget test's documented flakiness note in Step 4): it's not clock-dependent or run-to-run variable. It's a structural tie in the position itself, resolved the same way every time by the deterministic tie-break the given algorithm happens to have (first-generated-wins). No code path in the brief's `root_search`/`order_score`/`find_best_move` distinguishes the two symmetric ends of an open four, and neither spec §9.2 nor §9.3 nor §7.4 (`generate`'s ordering, which spec explicitly says is unspecified — "Ordering and truncation happen in `search.rs`, not here") specifies a tiebreak that would favor the higher-index end.

I did not guess at a fix (e.g., changing the strict `>` to `>=` to flip the tiebreak to "last seen wins," which would just as arbitrarily satisfy this one test without being principled; or altering the test's board setup, which isn't mine to redesign) because the brief explicitly says this is transcription, and instructs me to escalate exactly this class of test-vs-implementation conflict rather than pick a side.

## Files changed

- `/home/fwong/Desktop/42/gomoku/.claude/worktrees/gomoku-impl/src/search.rs` — added `root_search`, `extract_pv`, `find_best_move` (all verbatim from the brief), extended the test module's `use crate::board::{idx, SIZE};` import, and added the three new tests verbatim. **Not committed** — one test fails, and per the escalation instructions I'm stopping before committing a known-contradictory state rather than picking a tiebreak fix myself.

Current diff: `git diff --stat` shows only `src/search.rs` changed (+243/-1). Working tree is otherwise clean (an untracked `Cargo.lock` predates this task and is unrelated).

## TDD Evidence

**RED** (`cargo test search:: -- --nocapture`, before Step 3):
```
error[E0425]: cannot find function `find_best_move` in this scope
   --> src/search.rs:434:27
error[E0425]: cannot find function `find_best_move` in this scope
   --> src/search.rs:450:27
error[E0425]: cannot find function `find_best_move` in this scope
   --> src/search.rs:470:28
error: could not compile `gomoku` (bin "gomoku" test) due to 3 previous errors
```

**GREEN attempt** (`cargo test search:: -- --nocapture`, after Step 3):
```
running 7 tests
test search::tests::order_score_ranks_five_above_quiet_move ... ok
test search::tests::order_score_ranks_tt_move_above_everything ... ok
test search::tests::tt_round_trip_and_replacement_policy ... ok
test search::tests::negamax_recognizes_an_immediate_win ... ok
test search::tests::find_best_move_takes_the_immediate_win ... FAILED
test search::tests::find_best_move_on_empty_board_returns_center ... ok
test search::tests::find_best_move_respects_time_budget ... ok

failures:
    search::tests::find_best_move_takes_the_immediate_win

test result: FAILED. 6 passed; 1 failed; 0 ignored; 0 measured; 28 filtered out; finished in 0.37s
```
6 of 7 tests pass, including the other two new public-API tests (`find_best_move_on_empty_board_returns_center`, `find_best_move_respects_time_budget`) and all four pre-existing tests. Reran the failing test 5x in a row — same `left: 250, right: 255` every time (deterministic, not flaky).

## Self-review findings

- `root_search`, `extract_pv`, `find_best_move` transcribed verbatim from the brief; diffed my file against the brief's code block once more before writing this report — no deviations.
- No pruning/extensions/LMR/forced-response shortcut added — confirmed by re-reading my diff end to end; the only "early exit" is the iterative-deepening loop's own `break` on a near-WIN score after a *completed* full-window search at that depth, which is what the brief's own code does (labeled "Immediate win shortcut" in its comment, referencing spec §9.5, but it is not the §9.5/Task-12 "play a winning move without searching" shortcut — the move is fully searched via `root_search` first; the loop simply stops deepening further once a forced win is confirmed).
- Did not touch `board.rs`, `rules.rs`, `eval.rs`, or anything outside `search.rs`.
- Left no debug artifacts in the diff — a temporary probe test (`debug_probe` module) was added to `src/search.rs` to empirically verify the tie, then fully removed before the final `git diff --stat` check above.

## Issues / concerns

**Blocking**: `find_best_move_takes_the_immediate_win`, as specified in the brief, encodes a position (symmetric open four, no White stones) with two equally-optimal winning moves, and asserts a specific one. The brief's own `root_search`/`order_score` implementation has no mechanism that would favor `(8,5)` over `(3,5)` in this exact tie — it deterministically returns `(3,5)` instead, which is a *correct* winning move, just not the hardcoded one. This needs a controller ruling:
- Is the test's board setup meant to be asymmetric (e.g., one end blocked by a wall/stone so only `(8,5)` wins), and the brief has a transcription typo in the stone-placement loop or coordinates?
- Or is the implementation meant to have an explicit tiebreak (e.g., prefer the last-seen equal-score candidate, or some centering/proximity rule) that the given code doesn't actually implement?
- Or is the test's `assert_eq!` meant to be an `assert!` checking "any winning move" rather than one specific square (e.g., checking the resulting position is a win via `rules::check_end` rather than pinning the exact `Idx`)?

I did not implement a guess at any of these — awaiting the controller's check against the spec/original plan intent.

## Resolution — Ruling 6

The coordinator confirmed against the spec (§9.2/§9.3/§7.4 and the rest of the document specify no tiebreak among tied wins) and against this same plan's Task 12, which hits the identical symmetric-tie shape and already handles it correctly — `find_best_move_extends_a_three_into_an_open_four` asserts `mv == idx(6, 5) || mv == idx(2, 5)` rather than pinning one answer. Ruling: this was a **test defect** (recorded as **Ruling 6** in the project's ruling ledger), not a bug in `root_search`, `order_score`, or `find_best_move`. `(3,5)` and `(8,5)` are both genuinely, immediately winning; nothing in the spec mandates which one gets picked when they tie.

Fix applied: changed only the test assertion in `find_best_move_takes_the_immediate_win`, nothing else (board setup and all implementation code untouched):

```rust
// before
assert_eq!(mv, idx(8, 5));

// after
assert!(
    mv == idx(8, 5) || mv == idx(3, 5),
    "expected one of the two immediate-win completions, got {mv:?}"
);
```

Verification after the fix:
- `cargo test search:: -- --nocapture` — 7 passed, 0 failed.
- `cargo test` (full suite) — 35 passed, 0 failed.
- `cargo build --release` — succeeds (only pre-existing dead-code warnings, since `find_best_move`/`root_search`/`extract_pv` have no caller yet — `ui.rs`/`main.rs` are Task 15).

Committed: `edfeb46` — `feat: iterative deepening, PVS, time control, public find_best_move` (the brief's exact Step 5 message), containing `src/search.rs` with the three new functions and the corrected test.
