# SDD ledger — plan: docs/superpowers/plans/2026-09-01-gomoku-implementation.md

Spec: docs/superpowers/specs/2026-09-01-gomoku-design.md

## Preflight scan (before Task 1 dispatch)

Cross-task interface/file table. Files touched by 2+ tasks, and self-consistency of each task's own text.

| Files | Tasks touching | Interface produced → consumed | Finding |
|---|---|---|---|
| Cargo.toml, Makefile | 1 | Task 1 only | clean |
| src/main.rs | 1,2,3,6,7,8,9,15 | each adds one `mod X;` line; Task 15 replaces the whole body | clean — each addition is additive except Task 15's full replacement, which is the last task to touch it |
| src/patterns.rs | 2,13 | `PatternTable::{build,get}`, `Pat`, `F_*` flags, `POW3` → consumed by board.rs (5,10), rules.rs (6,7), search.rs (10,12) | clean; Task 13 adds a file-level scoped `#[allow(clippy::indexing_slicing)]` with justification (Category A) |
| src/board.rs | 3,4,5,6,8,10,13 | geometry (3) → `Board`/Zobrist/`captures_of` (4) → `Undo`/`play`/`undo`/incremental accumulator (5) → `has_neighbor`/`window_code_pub` (6) → test-only `full_recompute_acc` (8) → `hypothetical_window_code` (10) → clippy fixes (13) | **Ruling** (see below): Task 13 as originally written missed ~8 raw-index sites on `self.acc[player as usize]`/`self.captures[player as usize]` introduced by Task 5. Fixed in the plan before dispatch — see Ruling 1. |
| src/rules.rs | 6,7,13 | `is_legal`/`count_free_threes`/`generate` (6) → `GameEnd`/`check_end` (7) → clippy fixes (13) | Task 7's `five_is_breakable`/`check_end` also had 2 raw-index sites on `captures` Task 13 missed. Same Ruling 1 covers it. |
| src/eval.rs | 8,13 | `WIN`/`CAP_BONUS`/`evaluate` (8) → clippy fixes (13) | `evaluate`'s 3 raw-index sites on `acc`/`captures` also missed by Task 13's original text. Same Ruling 1. |
| src/search.rs | 9,10,11,12,13,14 | TT/`SearchConfig`/`SearchStats`/core `negamax` (9) → ordering, `SearchCtx` extended, `order_score` (10, explicitly instructs editing Task 9's test call site) → `root_search`/`extract_pv`/`find_best_move` (11) → extensions/LMR, `negamax` signature change (+`extensions_used`), explicitly lists every call site to update including Task 9's test and Task 11's `root_search` (12) → test-module allow (13) → benchmark test (14) | clean — every signature change across 9→10→11→12 is accompanied by explicit "update this call site" text in the later task; verified `negamax`'s final 7-arg signature matches all 4 call sites (Task 9 test, `negamax`'s own 2 self-calls, `root_search`'s 3 calls) |
| src/ui.rs | 15 | new file, consumes `board`/`rules`/`eval`(indirectly via search)/`search` public APIs | clean; confirmed `App` uses `SearchConfig::default()` (Task 9), `TranspositionTable::{new,clear}` (Task 9), `find_best_move` (Task 11) with matching signatures; file has its own clippy allow so `player_slot` (new in 13) is correctly NOT required there |

**Self-consistency per task:** read each task's steps against its own stated Interfaces block — no task claims to produce something a later task expects under a different name (spot-checked `Board::play`/`undo`, `rules::{is_legal,generate,check_end}`, `search::find_best_move`, `PatternTable::{build,get}` — all consistent across every call site in the plan).

## Ruling 1 — acc/captures raw indexing gap (plan-mandated defect, fixed before execution)

**Finding:** Task 13 ("Clippy compliance pass") was written to cover 3 categories of raw indexing (patterns.rs's window arrays, board.rs's `key_cell`/`cells` setup, and `captured[..n]` slicing) but missed a 4th, larger category: `self.acc[player as usize]` / `self.captures[player as usize]` — a `[T; 2]` array indexed by a `Player as usize` cast — appearing at ~13 production call sites across `board.rs` (Task 5's `adjust_axis_neighbors`, `adjust_axis_vuln`, `play`), `rules.rs` (Task 7's `check_end`, `five_is_breakable`), and `eval.rs` (Task 8's `evaluate`). Under the crate-wide `deny(clippy::indexing_slicing)`, none of these would compile clean.

**Ruling:** Extended Task 13 with a Category D: two small `#[allow(clippy::indexing_slicing)]`-scoped helper functions (`player_slot`/`player_slot_mut`) added to `board.rs`, with every one of the ~13 sites replaced by a call to them, listed exactly (function, old code, new code) so no implementer judgment is needed. Chose helper functions over `.get().unwrap_or(...)` at each site because `Player as usize` is provably always 0 or 1 (2-variant enum) — a fallback value for the impossible out-of-range case would be actively misleading (silently hides a real bug instead of the raw index's correct loud panic on that case, which can never actually trigger). Also fixed a smaller, related gap in Category B: the `*cc` deref needed when indexing with a `&Idx` loop variable.

**Cost if wrong:** low — this is a mechanical, compiler-verified pattern (any missed site fails `cargo clippy -- -D warnings` in Task 13's own Step 2 verification), and the helper functions are pure and trivially testable if the task reviewer wants extra assurance. Worst case is Task 13's reviewer finds a missed site and it goes through one ordinary fix round.

Both edits committed to the plan file before Task 1 dispatch (commit `016f7bc`, this worktree's branch).

## Task log

### Environment note (blocking, resolved before Task 1 review)
Machine had `rustc` (system RPM) but no `cargo`. No sudo available. Installed a
user-space rustup toolchain (`profile=default` failed with a sandbox/tmpfs-related
false ENOSPC during large-component extraction — `profile=minimal` succeeded, then
`clippy` added incrementally without issue). Symlinked `cargo`/`rustc`/`cargo-clippy`/
`rustup`/`rustfmt` into `~/.local/bin` (already on PATH for every shell, including
subagent shells) rather than relying on `.bashrc`/`.zshrc` sourcing, since this
session's Bash tool invokes plain non-login bash that doesn't source either file.
Verified: `cargo build --release`, `make` (twice, no-relink holds), `./Gomoku` runs.

Task 1: complete (commits 016f7bc..c871dec, review clean)

Task 2: complete (commits c871dec..d40e77a, review clean)
Task 3: complete (commits d40e77a..db79bf2, review clean)

## Ruling 2 — Task 4 reviewer's clippy findings, parked (controller error in dispatch framing)

**Finding (reviewer, Important x1, verified with `cargo clippy --all-targets`):** board.rs:117
(`cells[idx(x,y) as usize] = Cell::Empty` in `Board::new`) and board.rs:276-277,312-313
(`captured[..n].contains(...)` in two new tests) fail `clippy::indexing_slicing`.

**Ruling:** Parked, not fixed now. I told the reviewer "this task's brief does NOT defer
indexing fixes to Task 13" — that was my own error. Re-checked Task 13's actual plan text:
Category B explicitly names and fixes board.rs:117's exact line; Category C explicitly
names board.rs's test module for the file-level `#[allow(clippy::indexing_slicing)]`
covering the `captured[..n]` test-only slicing. All 5 flagged sites are already, exactly
anticipated by Task 13 — this is the plan executing as designed (implementer was
correctly instructed not to improvise clippy fixes ahead of the dedicated pass), not a
new gap. The implementation itself is verbatim-correct per the brief (reviewer confirmed
no logic bugs, capture direction/bounds/16-slot-array logic all sound).

Also parked: reviewer's Minor finding (no test for simultaneous multi-direction capture).
Reasonable suggestion, reviewer's own severity is Minor, nothing downstream depends on it
— Task 7's appendix-figure fixtures exercise capture scenarios in more depth anyway.

**Cost if wrong:** none realistically — Task 13's Step 2 (`cargo clippy -- -D warnings`)
is a hard, automated gate; if either category's text somehow doesn't actually cover one
of these 5 lines when Task 13 runs, that surfaces immediately as a clippy failure in that
task's own verification, not silently.

Task 4: complete (commits db79bf2..4c0c5e7, 2 parked — both resolved by already-planned Task 13, see Ruling 2)

Task 5: implemented (commits 4c0c5e7..31ba8d4), review dispatched, session ended before
verdict recorded — resumed in a later session (2026-09-02). Review came back with an
Important, plan-mandated finding.

## Ruling 3 — Task 5's incremental accumulator double-counts on captures (plan-mandated defect)

**Finding (reviewer, Important, empirically confirmed with an independent
`full_recompute_acc` oracle run in an isolated copy of the repo, not the reviewed
checkout):** `board.rs`'s `play()` (board.rs:324-379, verbatim transcription of the
brief/plan's Task 5 Step 3 code) subtracts/adds pattern-table window scores once per
"center" (`mv`, then once per captured stone) via `adjust_axis_neighbors`/
`adjust_axis_vuln`. Because `mv` and every stone it captures are structurally close
together (a capture is the contiguous pattern `p O O p`, so `mv` and the two captured
stones are always within 1-3 cells of each other on the same line) their radius-4 (resp.
radius-2 for vulnerability) influence zones routinely overlap. Each center's call
re-subtracts/re-adds any stone in the overlap using whatever *intermediate*,
partially-mutated board state exists at that point in the loop — not a single consistent
pre-move vs. post-move pair — so overlapping windows get touched 2-3x against mismatched
snapshots instead of once against the true old/new values. Reviewer measured this
directly on the plan's own minimal capture fixture (`White _ Black Black`, White
captures): incremental `acc = [0, 10]` vs. true recompute `[0, 5]` — exactly double.
`play_undo_round_trip_restores_exact_state` cannot catch this because `undo()` restores
`acc` by snapshot (board.rs:400), not by validating `play()`'s forward math, and
`play_capture_updates_captures_and_frees_cells` never asserts on `.acc` at all.

**Ruling:** This is a real defect in the plan's own Task 5 Step 3 algorithm, not an
implementer error — confirmed byte-for-byte verbatim against the brief. Fixing it
requires the incremental update to (a) compute `captures_of` *before* any mutation
(safe: `captures_of` never reads `cell(mv)` itself, only `mv±d/±2d/±3d`, so calling it
before or after placing `mv` is provably identical) so a single clean pre-move/post-move
snapshot pair exists, and (b) on the capturing path only, replace the per-center
subtract/add calls with a **deduplicated** sweep over the union of `{mv} ∪ captured`, so
every `(cell, axis)` pattern-window and every `(cell, axis)` vulnerability-pair is
touched exactly once per phase — not once per overlapping center. The common
non-capturing case (`n == 0`, the vast majority of moves) is untouched: it was already
correct (reviewer's control case passed) and single-center, so no dedup is needed there;
this keeps the hot path exactly as fast as before and only pays dedup cost on the rare
capturing move. The dedup sets use `Vec<(Idx, u8)>` + linear `.contains()` (small n —
at most 17 centers × 4 axes × 9 offsets before dedup — negligible cost, and safer than a
fixed-capacity array given the legal edge case of one move capturing in all 8 directions
at once, i.e. up to 16 captured stones). This also subsumes the separate "own axis"
loops the original code had for `mv` and each captured stone (the dedup sweep's `k=0`
case already covers a center's own 4-axis contribution when it's `changed`), so the fix
is a net simplification, not just an addition. Chose to pull Task 8 Step 1's
`full_recompute_acc` test-only oracle helper forward into this fix (verbatim, so Task 8
later finds it already present and skips re-adding it) rather than inventing a
weaker ad hoc check — it's the only test shape that can actually catch this bug class,
as the reviewer's own analysis showed. Exact code for both the fix and the new tests was
specified in full in the fix dispatch, not left to the implementer's invention, given the
severity of getting this specific piece of logic right a second time.

**Cost if wrong:** Load-bearing — `acc` is what `eval.rs` (Task 8) and therefore every
`search.rs` (Tasks 9-12) node will read as the position score; if the fix's dedup sweep
itself has a bug, every future task silently inherits a broken evaluation signal. Mitigated
by requiring the fix round's re-review to independently re-verify against the
`full_recompute_acc` oracle (not just re-read the diff), and by pulling the oracle-based
property test into this task's own permanent suite rather than deferring it to Task 8.

Task 5: fix round 1/5 (dedup fix applied + full_recompute_acc oracle + 300-seed drift
property test + strengthened capture test; implementer also rebuilt
`play_capture_updates_captures_and_frees_cells`'s fixture — its `set_raw`-based setup
desynced `acc` before the capture ran, unrelated to the dedup fix itself, per its own
diagnosis — flagged for independent re-review, not accepted on faith; commits
31ba8d4..c36427b). Scoped re-review dispatched.

Re-review: both findings ADDRESSED (accumulator delta verified correct by hand-traced
dedup-key collision case + 300-seed oracle corroboration; fixture deviation independently
re-derived as a pre-existing `set_raw`-desync artifact, not a masked dedup gap — residual
mismatch exactly equals the pre-existing drift on both sides, not a delta error). No new
Critical/Important breakage. 1 Minor noted (Vec+linear-scan dedup, already a deliberate
tradeoff in Ruling 3, not an oversight) — parked, no action needed.

Task 5: complete (commits 4c0c5e7..c36427b, 1 fix round, review clean)

Task 6: implemented (commits c36427b..5968221, 22/22 passing), review dispatched.
Review: ✅ spec compliant, 0 Critical/Important, 4 Minor (deferred, no fix loop):
- rules.rs: no dedicated test for `is_legal` on an occupied cell.
- rules.rs: `generate`'s test coverage doesn't distinguish "filters by legality" from "filters by radius only."
- board.rs `window_code_pub`: brief's own Interfaces section called for a private scratch helper; Step 3's literal code (followed correctly) makes it `pub`. Brief-internal inconsistency, not an implementer error — note for later cleanup.
- rules.rs `is_legal`/`generate`: full `Board::clone()` + scratch `play`/`undo` per legality check, once per candidate cell in `generate`. Brief's own deliberate tradeoff (correctness first); flag for profiling once search.rs lands.

Task 6: complete (commits c36427b..5968221, review clean, 4 minors deferred)

Task 7: implemented (commits 5968221..df9b43b, 26/26 passing), review dispatched.
Review: ✅ spec compliant (verified against spec §7.3 directly, not just brief paraphrase:
10-captures-win-first order, p=to_move.other() derivation, captures[opp]>=8 index
direction, opponent-capture-availability actor, alignment/intersection walk, draw
condition). 0 Critical/Important, 2 Minor (deferred):
- `five_is_breakable`: redundant double `captures_of` computation across its two loops when p_lost_stones>=8 — harmless, collapsible if check_end proves hot later.
- No dedicated fixture for board-edge five, overline (6+), or simultaneous-fives-one-breakable — traced by hand as correct, but unguarded against regression.

Task 7: complete (commits 5968221..df9b43b, review clean, 2 minors deferred)

Task 8: implemented (commits df9b43b..40af77e, 28/28 passing, correctly skipped
re-adding full_recompute_acc), review dispatched.
Review: ✅ spec compliant (sign/perspective correct, CAP_BONUS indexed by pairs with
non-panicking fallback, no vuln-penalty duplication, board.rs correctly untouched/no
duplicate helper, dependency direction respected, drift test uses rules::generate per
brief with an accurate 8000-cycle count). 0 Critical/Important, 2 Minor (both
plan-mandated verbatim from the brief, deferred): capture-favors test never exercises a
nonzero acc term; CAP_BONUS fallback expression more roundabout than necessary.

Task 8: complete (commits df9b43b..40af77e, review clean, 2 minors deferred)

Task 9: implemented (commits 40af77e..783f334, 30/30 passing, test's replacement-policy
scenario corrected per Ruling 4, store() left exactly as spec'd), review dispatched.
Review: ✅ spec compliant. Confirmed Ruling 4 applied exactly (store() byte-for-byte
brief's Step 3, corrected test matches + adds equal-depth boundary case). TT allocation
ladder, fail-soft alpha-beta bookkeeping, win-scoring sign/perspective (traced through
rules::check_end + board::play) all correct. SearchCtx scope correctly minimal (no
ordering/killers/history yet). 0 Critical/Important, 2 Minor (both plan-mandated
verbatim, deferred): TT's final 1-element fallback not try_reserve-wrapped (unreachable
in practice); a code comment references nonexistent "Task 9 notes."

Task 9: complete (commits 40af77e..783f334, review clean, 2 minors deferred)

Task 10: implemented (commits 783f334..542fd1c, 32/32 passing), review dispatched.
Review: ✅ spec compliant (hypothetical_window_code genuinely pure; priority ladder order
verified line-by-line against spec §9.3; score-all-then-truncate order confirmed; killer
per-ply bookkeeping and 2-slot promotion correct; tt_hits/tt_probes wired in this task,
not deferred). 1 Important, plan-mandated: `ctx.history[mv] += depth*depth` (search.rs)
has no write-side cap/decay — read-site clamps via ORD_HISTORY_CAP but the stored
accumulator itself could theoretically overflow i32 over a long enough search.

## Ruling 5 — unbounded history-heuristic accumulator (plan-mandated, minimal fix)

**Finding:** `history[mv]` accumulates `depth*depth` on every beta cutoff with plain `+=`,
uncapped at the write site. `Cargo.toml`'s release profile has no `overflow-checks`, so
release silently wraps on overflow; debug/test builds panic.

**Ruling:** Real risk is low but not zero — `SearchCtx` (and its `history` array) resets
fresh on every `find_best_move` call (per Task 11's design, not yet landed but already
specified in the plan: one `SearchCtx::new` per move decision, reused only across that
move's iterative-deepening depths), so accumulation is bounded to a single ~400ms search,
not the whole game. Realistic node/cutoff counts in that window are nowhere near i32's
~2.1 billion range. Not worth a design change or a test. But the debug-build panic risk
(which WOULD trigger during `cargo test`, including Task 12's determinism tests and Task
14's benchmark gate, if a pathological position ever pushed one cell's count that high) is
cheap to eliminate outright: swap `+=` for `saturating_add`, a 1-line change, behavior-
identical except in the already-unreachable-in-practice overflow case, doesn't touch the
read-site `ORD_HISTORY_CAP` clamp (the actual heuristic-scoring mechanism, left untouched).
Routing through the normal fix loop since it's Important, but framing it to the
implementer as the precise 1-line change so it's not a design reopening.

**Cost if wrong:** Negligible — `saturating_add` behaves identically to `+=` for every
value this session will ever realistically produce; the only behavior change is in the
unreachable overflow case, where it's strictly safer (no panic, no silent wrap) than what
was there before.

Task 10: fix round 1/5 (saturating_add applied, 4/4 search tests pass, release build
clean; commits 542fd1c..4ca8fa8). Scoped re-review dispatched.
Re-review: ADDRESSED — exact 1-line change confirmed, read-site clamp untouched, no
scope creep, no new breakage.

Task 10: complete (commits 783f334..4ca8fa8, 1 fix round, review clean)

## Ruling 6 — Task 11's immediate-win test asserts an arbitrary, unguaranteed tiebreak (plan-mandated defect)

**Finding (implementer, BLOCKED, self-caught before committing):** brief's
`find_best_move_takes_the_immediate_win` places Black stones at `(4,5)..(7,5)` (four in a
row, open both ends) and asserts the engine plays `idx(8,5)`. But `(3,5)` and `(8,5)` are
BOTH immediate unbreakable wins — symmetric open four, either end completes a five.
`order_score` returns the flat `ORD_FIVE` constant for any winning move (no further
differentiation among wins), so both tie in ordering; `root_search`'s root loop uses a
strict `best_score > alpha`-style `>` comparison, so the FIRST candidate to reach that
score wins the tie, and `rules::generate`'s row-major scan (`for y { for x { ... } }`)
visits `(3,5)` before `(8,5)` for a fixed `y=5` row — so the algorithm deterministically
returns `(3,5)`, not the hardcoded `(8,5)`. Implementer verified this empirically (5
reruns, same result) and checked spec §9.2/§9.3/§7.4 for any stated tiebreak rule — none
exists.

**Ruling:** This is a genuine test defect, not an implementer or algorithm bug — the
spec never guarantees which of two equally-winning moves gets picked, and nothing about
"pick the lower-index candidate first" or "the higher one" is a stated requirement. Board-
edge in point: this plan's OWN Task 12 (`find_best_move_extends_a_three_into_an_open_four`)
hits the identical symmetric-tie shape (two equally valid completions of an open three)
and already handles it correctly — `assert!(mv == idx(6, 5) || mv == idx(2, 5), ...)`.
Ruling: apply that exact same either/or pattern to Task 11's test instead of hardcoding a
single winning cell. Change ONLY the assertion:
```rust
assert!(
    mv == idx(8, 5) || mv == idx(3, 5),
    "expected one of the two immediate-win completions, got {mv:?}"
);
```
Nothing else in the test or the implementation changes — `root_search`/`find_best_move`'s
logic is correct as specified; the test's hardcoded single answer was simply too strict
for a position with two equally correct answers.

**Cost if wrong:** Low — this loosens one test assertion to accept either of two
genuinely-correct answers; it cannot mask a real bug (both cells really do win
immediately, verified by the implementer against `check_end`), and it brings Task 11's
test in line with the pattern Task 12 already establishes for the same tie shape.

Task 11: implemented (commits 4ca8fa8..edfeb46, 35/35 passing, tie-break assertion fixed
per Ruling 6), review dispatched.
Review: ✅ spec compliant (discard-on-abort, PVS null-window/full-reseach logic,
aspiration fail-same-depth-reseach, extract_pv play/undo symmetry all verified against
source; Ruling 6 applied exactly, no extraneous changes). 0 Critical/Important, 3 Minor
(all plan-mandated/inherent, deferred): zero-completed-iterations fallback move is
unordered (narrow/low-likelihood); "immediate win shortcut" comment on this task's code
misleadingly cites §9.5's search-skip optimization (that's actually Task 12's job, not
yet landed); find_best_move/root_search both call generate() once redundantly at depth 1.

Task 11: complete (commits 4ca8fa8..edfeb46, review clean, 3 minors deferred)

## Ruling 7 — Task 12 implementer's three self-caught deviations (2 confirmed correct, 1 sent back)

**Finding (implementer, DONE_WITH_CONCERNS, all three investigated and pre-fixed, not left open):**
1. Brief's verbatim Task 12 negamax code reverts the history accumulator from
   `saturating_add` (Ruling 5's fix, landed in Task 10) back to unchecked `+=` — the Task
   12 plan text was written before Ruling 5 existed, so a literal transcription silently
   undoes it. Implementer kept `saturating_add`.
2. Brief's Step 4 immediate-win shortcut returns `depth_reached: 0, nodes: 0` (intentional
   — it skips searching entirely), which breaks Task 11's pre-existing
   `find_best_move_takes_the_immediate_win`'s `assert!(stats.depth_reached >= 1)`. Brief's
   Step 5 says "all 9 tests green" without calling out that this old assertion needs
   updating. Implementer updated it to `assert_eq!(stats.depth_reached, 0)` /
   `assert_eq!(stats.nodes, 0)` with an explanatory comment.
3. Task 11's `find_best_move_respects_time_budget` (600ms tolerance on a 200ms budget)
   started failing in debug builds (~1.0s observed) once LMR/threat-extension made
   individual iterations deeper before the periodic `ctx.nodes % 2048` deadline check
   fires. Verified via release-build A/B (~258ms) that this is a debug-only timing
   artifact. Implementer's fix: widened the test's tolerance to 2000ms.

**Ruling:**
- **#1 and #2: confirmed correct as implemented**, verified directly in `src/search.rs`
  (`saturating_add` present at search.rs:370; the updated assertion present at
  search.rs:723-724). Both are necessary, minimal, and exactly the right call — #1
  preserves an already-ruled-on fix that the plan's own text doesn't know about yet, #2
  is a legitimate consequence of Task 12's own intentional new behavior. No rework.
- **#3: sent back.** Task 11's plan text ITSELF already anticipated this exact symptom
  and prescribed a specific fix, which the implementer didn't try: "if find_best_move_
  respects_time_budget is flaky on a slow machine... consider lowering [the 2048-node
  check interval] to 512 before touching the assertion." Widening the tolerance to 2000ms
  (10x the 200ms budget, vs. Task 11's already-generous 3x) papers over a genuinely coarse
  deadline-check interval instead of fixing it, and does so against the plan's own
  explicit stated preference for this exact scenario. Ruling: lower the check interval
  from 2048 to 512 in `negamax` (both occurrences, if the increment/check appears more
  than once), re-test with Task 11's ORIGINAL 600ms tolerance first; only if still flaky
  after that, widen modestly (not to 2000ms) with the actual measured number as
  justification, not a round guess.

**Cost if wrong:** Low for #1/#2 (both are verified present in the file, not just
claimed). For #3, if 512 alone doesn't fully close the gap and a larger tolerance is
still needed after that fix, the cost is just one more fix-loop round — not a correctness
risk, since this only affects a debug-build test's timing margin, not the release-mode
benchmark gate (Task 14) or real gameplay (Task 15, built in release).

Task 12: implemented (commit cdfa4bb, 9/9 search + 37/37 full suite passing), #3 sent
back per Ruling 7, resumed implementer.
Ruling 7's #3 fix landed as commit e9a1d71 (interval 2048→512, tolerance reverted to
600ms; measured 4 runs at ~240-256ms, comfortably under budget; release-mode
re-confirmed at ~210ms; interval change is the sole occurrence in the file; correctly
landed as a separate commit, not an amend, matching this project's established
review-fix convention — implementer self-caught an initial --amend attempt via reflog and
corrected it). Agent hit a session rate-limit right after finishing and verifying, before
sending its final status reply — ground truth confirmed directly via git log/status
(clean tree, commit present) and the report file, not assumed. 37/37 full suite passing,
`cargo build --release` clean. Review dispatched (base edfeb46, covering the full Task
12 diff including the Ruling 7 fix).

Review: ✅ spec compliant with 1 Important plan-mandated gap. Ruling 7's three items all
confirmed present and correct (saturating_add, updated depth_reached assertion, %512 +
600ms tolerance, zero leftover debug prints). Full negamax call-site audit clean (all 7
sites correctly threaded with extensions_used). Threat-extension cap, LMR forcing-move
gate + re-search-on-improvement, shared score_order_and_truncate DRY extraction, and the
immediate-win shortcut's before-any-search ordering all verified correct against source.

## Ruling 8 — forced-block shortcut missing spec's capture OR-clause (plan-mandated, minimal fix)

**Finding:** spec §9.5: "if the opponent has a four, restrict candidates to moves that
block it OR capture a stone out of it." `score_order_and_truncate`'s retain
(`scored.retain(|&(s, _)| s >= ORD_BLOCK)`) only implements the block half — a capturing
move that would break the four scores in the ORD_CAPTURE_BASE tier (500_000), always
below ORD_BLOCK (800_000), so it gets discarded from the candidate pool whenever a
higher-tier block/five candidate exists, UNLESS the capture happens to coincidentally
also sit on one of the four's own completion squares (not structurally guaranteed —
capture geometry is an orthogonal flanking pattern, unrelated to the four's line). This
is verbatim brief code (Task 12 Step 4), not an implementer deviation.

**Why this matters more than an average plan-mandated gap:** in this ruleset, capturing
is specifically the mechanism the 42-school rules provide to defend against an
otherwise-unstoppable OPEN four (blocking one of its two ends doesn't prevent the win
via the other end — only removing one of the four's own stones does). This is the
ruleset's central tactical tension, not a rare corner case, so silently discarding the
one genuinely saving move from the search's candidate pool at exactly the moment it's
needed most is a real playing-strength risk, not just a theoretical spec gap.

**Ruling:** Minimal, safe fix — widen the retain predicate to also keep any move that
captures at least one pair, regardless of whether it happens to also score at the block
tier:
```rust
scored.retain(|&(s, mv)| s >= ORD_BLOCK || b.captures_of(mv, me).1 > 0);
```
(`me` already in scope from earlier in the function.) This is deliberately broader than
the spec's literal "capture a stone OUT OF the four" — precisely identifying which
capture dismantles THIS specific four would require duplicating rules.rs's
`collect_alignment`/`five_is_breakable` machinery inside a move-ordering heuristic, which
is solving a problem the recursive search already solves correctly once the candidate
is merely present in the pool. "Keep any capturing move" can only ever ADD a possibly-
relevant candidate, never silently drop the one the spec requires — negamax's own
recursive evaluation is what determines whether a given capture actually saves the
position, not this heuristic. Bounded cost: only fires in the already-rare
facing-a-threat branch, and the subsequent `truncate(max_candidates)` still caps the
pool size, so this cannot blow up node counts materially.

**Cost if wrong:** Low — worst case is a slightly larger candidate pool in the
facing-threat branch (bounded by `max_candidates` regardless), never a missed
requirement; Task 14's benchmark gate (average <400ms, depth>=10 across 10 varied
positions) will catch a real performance regression if this fix's broadening is somehow
costlier than expected.

Task 12: fix round 1/5 (Ruling 8's exact 1-line fix applied, 9/9 search + 37/37 full
suite passing x3 reruns; commit fc552f6 on top of e9a1d71, not amended). Scoped
re-review dispatched.
Re-review: ADDRESSED — exact predicate confirmed, `me` correct, surgical 1-line diff,
no new breakage.

Task 12: complete (commits edfeb46..fc552f6, 1 fix round [Ruling 7 pre-fixed inline] +
1 fix round [Ruling 8], review clean)

Task 13: dispatched. Note: brief's board.rs Category B/D line numbers/snippets are stale
(pre-Ruling-3 play() shape) — implementer given a fresh accurate site list from a live
grep, told to trust `cargo clippy` as final authority over any static list.
Implemented (commit 86f359d, 37/37 tests unchanged), DONE_WITH_CONCERNS: brief's Step 2
verification command (`cargo clippy --all-targets -- -D warnings`, expected fully clean)
isn't achievable within this task's scope — `-D warnings` also promotes ~76 `dead_code`
findings (main.rs is still Task 1's unwired stub; nothing calls any engine module yet,
expected until Task 15) plus a handful of pre-existing minor style lints, none of which
are among the 4 lints the Global Constraints section actually denies via Cargo.toml
(indexing_slicing/unwrap_used/expect_used/panic). Implementer declined to invent a 5th
"fix category" (e.g. blanket #[allow(dead_code)]) to force a literal clean `-D warnings`
pass, correctly per the brief's own escalation guidance.

**Controller independently verified (not taken on faith):** ran
`cargo clippy --all-targets -- -D warnings` myself — confirmed zero
indexing_slicing/unwrap_used/expect_used/clippy::panic hits (was 73, per report). Of the
78 remaining errors, ~76 are `dead_code` (every constant/struct/fn "never used/
constructed" — expected, matches this exact pattern already called out as normal by
Task 9's own plan text at this stage). Traced the two non-dead-code findings by hand:
`unused import: Player` (eval.rs:3) predates Task 13 (Task 8's original import, Task 13
only added `player_slot` alongside it in the same use-line); `unused variable: opp`
(search.rs:414, inside `root_search`) is a genuine pre-existing leftover from Task 12's
`score_order_and_truncate` extraction (which now derives `me`/`opp` internally, making
`root_search`'s own local `opp` binding dead) — confirmed via `git show fc552f6:...`
that neither predates this diff's own edits. No ruling needed — this is correct,
in-scope behavior, not a defect. Proceeding straight to review.

## Ruling 4 — Task 9's TT replacement test contradicts its own implementation AND the spec (plan-mandated defect)

**Finding (implementer, NEEDS_CONTEXT, self-caught before committing anything):** brief's Step 1
test `tt_round_trip_and_replacement_policy` stores `depth:4` at key 12345, then stores
`depth:1` at the SAME key 12345 and asserts the shallower entry overwrites (score becomes
99). Brief's Step 3 `TranspositionTable::store` implements `if e.depth >= slot.depth ||
slot.key != key { *slot = e; }` — for this exact scenario (same key, new depth 1 < stored
depth 4), both clauses are false, so it does NOT overwrite. Implementer transcribed both
verbatim, hit the contradiction, diffed against the brief file to rule out its own
transcription error, and correctly stopped rather than pick a side.

**Ruling:** Checked the spec directly (`docs/superpowers/specs/2026-09-01-gomoku-design.md:537`):
"Replacement policy: depth-preferred — overwrite only if the new entry's depth is greater
than or equal to the stored one, or the stored key differs." This matches the brief's Step 3
*implementation* exactly, word for word. The brief's Step 1 *test* is the defective half —
both its scenario (asserting a shallower same-key store overwrites) and its inline comment
("same key match always allows replacement... same key always updates") directly contradict
the spec's stated policy. Standard depth-preferred TT design also agrees with the spec here
(a shallower re-search of an already-deeply-analyzed position should never discard the
deeper, more valuable result). Ruling: keep Step 3's implementation exactly as specified
(it's correct), replace the test with a corrected version that asserts the actual
depth-preferred policy: a shallower same-key store does NOT overwrite a deeper one; an
equal-or-deeper same-key store DOES overwrite. Corrected test text handed to the resumed
implementer verbatim.

**Cost if wrong:** Load-bearing but self-correcting — `TranspositionTable` is the foundation
Tasks 10-12 build move-ordering and iterative deepening on top of; if the *implementation*
had been changed instead of the test (i.e. if I'd ruled the other way), every later task
would inherit a TT that discards deep results in favor of shallow re-searches, silently
degrading search quality without any test ever catching it (a wrong-but-passing test is
worse than no test). Keeping the implementation as specified and fixing the test instead
means Task 13's clippy pass and every later task's own tests remain the real check on
whether this holds; low risk since it's now a single, spec-quoted line of reasoning, not a
design invention.

(Note: this ledger file's chronological order drifted around here due to an editing
mishap — content is accurate, ordering isn't strictly sequential from this point. Not
worth reorganizing a git-ignored scratch file; git log has the authoritative order.)

Task 13 review: ✅ spec compliant. Independently re-ran clippy, confirmed zero hits on all
4 deny lints crate-wide; coverage complete via independent grep (not just diff-reading);
correctly handled both post-Ruling-3 drift spots (adjust_axis_dedup/adjust_axis_vuln_dedup,
full_recompute_acc); 3 substitutions hand-verified for exact behavioral equivalence.
0 Critical/Important, 2 Minor (deferred): one line 4 chars over 100-col convention;
5x verbatim-repeated nested-get pattern in play() could be a helper but brief explicitly
said apply per-site.

Task 13: complete (commits fc552f6..86f359d, review clean, 2 minors deferred)

Task 14: BLOCKED. Benchmark measured (release, max_candidates:20 exactly per brief):
avg ~365ms/move (R15's <400ms narrowly met, propped up by one 7.8ms outlier), but
**minimum depth reached: 0** (deepest search anywhere only reached depth 5) — R14
(min depth >=10) fails badly. This is the hard project gate ("if Task 14's benchmark
fails, work is not done").

## Ruling 9 — rules::is_legal clones the entire Board on every candidate cell, capping search depth catastrophically (root cause, real fix required)

**Finding (implementer, thorough root-cause investigation, correctly stopped at the
boundary of Task 14's own scope):** tried the brief's 3 cheap tuning options in order
(max_candidates 20->14: no measurable effect; TT hit rate + Zobrist round-trip re-check:
clean, low hit rate is a symptom of shallow trees not a hash bug; move-ordering
cutoff-rate check: not the bottleneck). Root cause: `rules::is_legal` (`src/rules.rs:31`)
does `let mut scratch = b.clone();` for every non-capturing candidate cell inside
`rules::generate`'s loop — and `generate` runs at EVERY negamax node. `Board` (`board.rs`)
is dominated by `key_cell: [[u64; TOTAL]; 2]` (TOTAL=729) plus `key_captures`, `cells`,
`neighbor` — a multi-KB deep copy, paid dozens of times per search node, capping
effective node throughput ~2 orders of magnitude below what depth-10-in-400ms requires.
This is exactly Task 6's own deferred Minor #4 ("flag for profiling once search.rs
lands") — now confirmed, by real measurement, to be the actual bottleneck, not a
speculative concern.

**Ruling:** The clone in `is_legal` is not just slow, it's entirely REDUNDANT — the
function it delegates to, `count_free_threes` (`rules.rs:11`), ALREADY does its own
correct save/restore via `b.play(mv,pt)` ... `b.undo(&u)` directly on whatever board it's
given. `is_legal` never needed to clone in the first place; it could always have passed
the real board straight through and let `count_free_threes`'s own play/undo round-trip
handle restoration (proven correct by its own existing tests). Fix: change `is_legal`,
`generate`, and `five_is_breakable` (which calls `generate` internally) from `&Board` to
`&mut Board`, and delete the clone. This matches `check_end`'s own already-established
precedent in this exact file (`rules.rs:99-101`'s docstring: "Takes `&mut Board` to match
this task's own signature contract with `search.rs`'s call sites, even though this
implementation never mutates `b`") — not a new pattern, applying an existing one more
broadly. Every `search.rs` call site (`negamax`, `root_search`, `find_best_move`) already
holds `&mut Board` at the point it calls `generate`, so ZERO call-site changes are needed
there — only `rules.rs`'s own signatures, its own test call sites (5 sites: 3 `is_legal`,
2 `generate` — one of which, `generate_on_empty_board_returns_only_center`, needs its
local `let b = Board::new();` changed to `let mut b`), and one doc-comment sentence in
`check_end` that goes slightly stale ("every helper it calls takes `&Board`" — no longer
true once `five_is_breakable` becomes `&mut Board`; `collect_alignment` still does).
Task 15's `ui.rs` (not yet dispatched, so zero rework cost) will be told directly to use
`&mut self.board` and make the handful of `App` methods that call `is_legal`/`generate`
take `&mut self` instead of `&self` — trivial, since `App`'s own methods are already
`&mut self` throughout per its design, and calling a `&mut self` method from another
`&mut self` method on the same struct has no borrow conflict.

**Cost if wrong:** High if left unfixed — this is the project's hard validation gate;
per spec framing, failing it means the deliverable isn't done regardless of what any
other test says. The fix itself is low-risk: no logic changes, only removes a redundant
clone and threads `&mut` through 3 function signatures plus their call sites — the
compiler catches any missed site as a type error, and every existing rules.rs/search.rs
test re-validates behavior is unchanged (play/undo's own correctness is already proven
by Task 5's 1000-seed round-trip test and the 300-seed accumulator-drift test).

**Result:** Ruling 9 applied cleanly (uncommitted, 37/38 tests pass, zero regressions),
but depth reached was bit-for-bit IDENTICAL before/after (avg 365ms -> 357ms) — the
board clone was real, verified waste, but not the dominant cost. Superseded by Ruling 10.

## Ruling 10 — count_free_threes's full play()/undo() round trip is the actual dominant cost (supersedes part of Ruling 9)

**Finding (implementer, follow-up investigation after Ruling 9 didn't move the needle):**
`count_free_threes` (`rules.rs:11`) calls `b.play(mv, pt)` — the REAL incremental engine
(accumulator maintenance across up to 4 axes x radius 4, vulnerability-penalty maintenance,
Zobrist XORs, neighbor-grid updates, a `captures_of` scan) — purely to read whether the
resulting window has the `F_FREE_THREE` flag, then immediately `b.undo(&u)`s it. This runs
for every empty near-neighbor candidate at every `generate()` call, i.e. every search node.
`board.rs`'s `hypothetical_window_code` (added in Task 10 for move-ordering's identical
need) already computes the exact same window encoding as a pure read, no mutation at all
— its own doc comment says it exists precisely because "mutating via play/undo... would be
far too slow to run on 40-80 candidates at every search node." Task 6 (which wrote
`count_free_threes`) predates Task 10 (which built the faster tool) and never got
retrofitted once it became available.

**Ruling:** `hypothetical_window_code(c, d, p)` forces the center trit to "own stone" for
`p` and reads every other cell from the real, unmutated board. `count_free_threes` is only
ever invoked from `is_legal` AFTER `is_legal` has already confirmed the move captures
nothing (`n == 0`, checked and returned-early before the free-three branch) — so a real
`play()` in that path only ever changes the single cell `mv` itself, nothing else in any
window. `hypothetical_window_code` therefore computes the IDENTICAL value a real
play-then-read would, proven by this invariant, not assumed. Fix: rewrite
`count_free_threes` to use it instead of play/undo:
```rust
pub fn count_free_threes(b: &Board, mv: Idx, p: Player, pt: &PatternTable) -> u8 {
    let mut count = 0u8;
    for &d in DIRS.iter() {
        let code = b.hypothetical_window_code(mv, d, p);
        if pt.get(code).flags & F_FREE_THREE != 0 {
            count += 1;
        }
    }
    count
}
```
This no longer needs `&mut Board` at all, which means Ruling 9's `&mut` threading through
`is_legal`/`generate`/`five_is_breakable` is now UNNECESSARY (not wrong, just superseded)
— revert all three back to `&Board` (their pre-Ruling-9 signatures), keeping the clone
DELETED (that part of Ruling 9 was correct, just not sufficient alone). `check_end`'s doc
comment reverts to its original, now-true-again wording. `is_legal`'s body goes back to
calling `count_free_threes(b, mv, p, pt)` directly (no clone, `b` is `&Board` throughout
now — simpler than either the original or the Ruling-9 version). The 5 rules.rs test call
sites and the handful of extra test-only `&mut`/hoist workarounds Ruling 9's threading
forced elsewhere (`eval.rs`, the benchmark test) become unnecessary — revert them if easy,
but leaving a superfluous `&mut`/`mut` in test code is harmless (auto-reborrows fine,
possible unused-mut warning at worst) if reverting is any hassle; not worth blocking on.

**Cost if wrong:** Load-bearing on the same hard gate as Ruling 9. Mitigated the same
way: no logic invention, a provable equivalence (argued above from `is_legal`'s own
capture-gating), and every existing test (including `count_free_threes`'s own two direct
unit tests) re-validates the observable behavior is unchanged.

**Result:** Ruling 10 applied cleanly (37/37 debug tests pass, zero regressions). Real
effect this time: nodes/sec up 4-5.6x on positions that use their full time budget
(confirmed via node-count deltas, not guessed). Best-case depth improved 4->7 (some
positions), most now 3-5 (up from 1-4). Still short of the required minimum 10
everywhere; elapsed time is now hugging ~400-420ms on most positions rather than
finishing early, consistent with genuinely deeper search rather than idle budget.
Implementer correctly stopped rather than guess at a third structural change.

## Ruling 11 — still short after 2 real fixes: instrument before guessing again

**Status:** open, mid-investigation. Two real, measured fixes (Rulings 9+10) closed part
of the gap (nodes/sec up 4-5.6x) but depth 7 best-case is still well short of the
required 10 everywhere. Rather than prescribe a third blind fix, dispatched the
implementer to gather a per-node cost breakdown (`generate()`'s full-361-cell scan +
its per-candidate `is_legal` cost vs. `score_order_and_truncate`'s own `order_score`
cost vs. TT probe/store vs. anything else notable) via the same kind of manual
instrumentation it already used to confirm the play/undo bottleneck — plus a fresh
retest of the brief's option 1 (lower `max_candidates`) now that the per-node cost
character has fundamentally changed (the original "no effect" result predates both
fixes above, and was plausibly masked by the much larger play/undo cost dominating
everything at the time). Candidate lead for a next fix, NOT yet prescribed pending data:
`is_legal`/`count_free_threes` and `order_score` both independently compute
`hypothetical_window_code`/`captures_of` for overlapping (candidate, axis) pairs during
the same node — once for legality-filtering inside `generate()`, again for ordering
inside `score_order_and_truncate` — a real, identifiable redundancy if the breakdown
shows `generate()`'s own cost is a significant fraction of the remaining total. Will
rule on the actual next fix once the breakdown comes back, not before.

**Breakdown result:** max_candidates 14 retested fresh, genuinely helps (small: +1 ply on
2/10 positions, no regressions) — kept in both SearchConfig::default() and the benchmark
cfg. Per-node cost breakdown (temp instrumentation, removed after): generate()'s is_legal
path ~12-14%, score_order_and_truncate's order_score path ~33-37%, TT probe/store
~0.2-0.3%, **unaccounted ~49-54%**. Confirmed the suspected redundancy: is_legal (via
count_free_threes) and order_score both independently call
`hypothetical_window_code(mv, d, me)` for the same 4 axes per candidate — is_legal to
check F_FREE_THREE count, order_score to check F_FIVE/F_OPEN_FOUR and accumulate
static-gain, from data that's otherwise identical (`pt.get(code)` returns one `Pat{score,
flags}` with ALL relevant flags at once). order_score also independently computes
`captures_of(mv, me)` a second time (is_legal already did this once). Final numbers:
avg ~300.5ms (comfortably under 400ms), best-case depth 7, true min depth 0
(seed 2, pre-existing immediate-win design case) / 2 excluding it. **R14 (depth>=10
everywhere) still fails.**

Rough math on the fusion's likely payoff: eliminating the confirmed `me`-perspective
duplicate would remove is_legal's ~13% entirely plus roughly half of order_score's ~35%
(the `opp`-perspective checks aren't duplicated, they're ordering-only) — maybe ~30%
total node-cost reduction, ~1.4x more nodes/sec. Given 4-5.6x nodes bought only 3 plies
(4->7) on the last round, ~1.4x more likely buys well under one more ply alone — not
enough by itself to close a 3-ply gap (7->10). The unaccounted ~50% bucket is now the
bigger lever and is completely uninvestigated. Ruling: before committing to the fusion
as the next fix, get that bucket characterized first — most likely candidates: `Board::
play`/`undo`'s own real incremental-accumulator cost on the actually-recursed-into path
(inherently necessary work, but LMR's re-search-on-improvement pattern can call it twice
for the same move), and/or the `Vec` allocations (`candidates`, `scored`) that happen
fresh at every single node. Dispatched a further investigation round rather than
prescribing the fusion blind. This task has now had 5 investigation/fix rounds — flagged
to the human partner for visibility given the scope this has grown to, while continuing
to drive it with data-backed rulings rather than pausing to ask.

**Round 3 result — the real answer:** the prior round's `generate()` timer only measured
`negamax`'s own top-level `generate()` call (once per node); it missed that
`rules::check_end` makes its OWN internal `generate()` call, and `check_end` runs once
per CHILD EXAMINED (far more often than once per node). Full corrected breakdown:
**`check_end` ~41.3%**, `order_score`/`score_order_and_truncate` ~36.2%,
`generate()`/`is_legal` (negamax's own) ~12.4%, real `play`/`undo` ~8.8%, TT ~0.3%,
`eval::evaluate` ~0.03%. `Vec::with_capacity` tried at both hot sites (explicitly
authorized as low-risk) — no measurable effect, reverted. LMR re-search essentially
never fires (0-3 times across 7 full-budget seeds) — ruled out.

## Ruling 12 — check_end's draw-check calls full generate() just to test emptiness (the real dominant cost)

**Finding:** `check_end`'s draw-check path (`rules.rs`, near the end of the function) is:
```rust
let mut candidates = Vec::new();
generate(b, b.to_move, pt, &mut candidates);
if candidates.is_empty() { return GameEnd::Draw; }
```
This runs on EVERY call to `check_end` where the just-played move didn't create a five
(`any_five == false`) — i.e. nearly every move examined during search, since most moves
aren't fives. `check_end` itself is called once per CHILD move `negamax`/`root_search`
examines (not once per node) — so this full `generate()` call (which itself does a
361-cell scan + a full `is_legal` check, including a free-three window computation, on
every has-neighbor empty cell) runs far more often than `negamax`'s own top-level
`generate()` call, and only to answer a yes/no emptiness question it doesn't need the
full candidate list for.

**Ruling:** Add an early-exit `has_legal_move` to `rules.rs` that returns `true` on the
FIRST legal cell found, rather than enumerating every candidate into a `Vec`:
```rust
/// True if `p` has at least one legal move. Early-exits on the first legal
/// cell found rather than enumerating every candidate like `generate` does
/// — used by `check_end`'s draw check, which only ever needs a yes/no
/// answer, not the full candidate list (spec §7.4).
pub fn has_legal_move(b: &Board, p: Player, pt: &PatternTable) -> bool {
    if b.stone_count == 0 {
        return true;
    }
    for y in 0..SIZE {
        for x in 0..SIZE {
            let i = idx(x, y);
            if b.get(i) == Cell::Empty && b.has_neighbor(i) && is_legal(b, i, p, pt) {
                return true;
            }
        }
    }
    false
}
```
Replace `check_end`'s draw-check with `if !has_legal_move(b, b.to_move, pt) { return
GameEnd::Draw; } GameEnd::None`. Provably equivalent to the old `candidates.is_empty()`
check (same predicate — "does a legal move exist" — computed lazily instead of eagerly,
no Vec allocation), and in the overwhelmingly common case (a position with many legal
moves scattered near existing stones, which is nearly every mid-game position) this
returns after finding the FIRST legal cell rather than exhaustively scanning and
legality-checking all ~361 cells. `five_is_breakable`'s own internal `generate()` call
(the OTHER branch, only reached when a five was actually created) genuinely needs the
full candidate list — it checks EVERY opponent candidate's capture result against the
five's alignment, can't early-exit the same way — left unchanged, out of scope here.

**Cost if wrong:** Load-bearing on the same hard gate. Mitigated: `has_legal_move` reuses
`is_legal` (already proven correct by every existing rules.rs test) with zero new
legality logic — it's a pure early-exit restructuring of an existing, already-correct
enumeration, not a new algorithm. `check_end`'s own existing 4 tests, plus every negamax/
root_search test that reaches a check_end call, re-validate no behavior changed. Noted:
this exposes a pre-existing gap — no test in this project directly exercises check_end's
Draw branch (all 4 of Task 7's tests hit Win/None paths). Not required for this fix
(the change is behavior-preserving, not new behavior), but worth a follow-up note.

**Result:** Real, verified win — node counts up 1.65-2.00x (avg ~1.84x) across every
full-budget position, matching the ~41% cost eliminated. Best case now depth 8 (up from
7). avg ~291ms (comfortably under 400ms). Still fails: true min depth 0 (seed 2, hits
`find_best_move`'s pre-existing immediate-win shortcut — depth_reached:0 there is
CORRECT/optimal, not a deficiency, but the benchmark's `min_depth.min(...)` aggregation
penalizes it the same as a real shortfall) / 2 excluding it (seed 7, similar early exit).
Each fix this round bought fewer plies than the last (Ruling 10: 4->7, Ruling 12: 7->8) —
consistent with alpha-beta's exponential cost curve. Implementer's rough extrapolation:
closing 8->10 needs another ~3x-4x throughput.

**Noted, not yet acted on:** the benchmark's random-walk position generation (an
acknowledged substitute for spec §14's "10 recorded middlegame positions," since no
finished AI exists yet to record real games with) can apparently land on positions with
an immediately-available win, which `find_best_move`'s shortcut correctly plays instantly
(depth_reached:0) — genuinely optimal behavior, but it corrupts the benchmark's own
`min_depth` metric by conflating "found something better than deep search" with "failed
to search deep enough." This is a test-methodology gap, not an engine deficiency, but
doesn't explain the shortfall on its own — even the 7-8 genuinely full-budget positions
cap at depth 8, not 10. Requesting a fresh cost breakdown before ruling on the next
fix, since the last one (order_score ~36%/is_legal ~12%, measured BEFORE this round's
~1.84x throughput shift) is now stale.

**Round 4 result:** fresh breakdown confirmed the fusion hypothesis: `order_score` grew
to ~45.1% (of the now-smaller total), `generate`/`is_legal` ~26.5%, `check_end` down to
~7.3%. Combined order_score+is_legal ~71.6%, clearly the biggest remaining chunk.
Branching factor measured directly (not guessed): ~2.0-2.2x nodes per additional ply.
Depth 8->10 needs roughly ~4.4x; depth 9->10 (after this round) needs roughly ~2x.

## Ruling 13 — generate_with_patterns fusion (order_score/is_legal shared computation)

Applied the fusion designed in the prior dispatch, with one real correctness fix the
implementer caught in the coordinator's own proposed code: the draft skipped computing
axis `Pat`s for capturing candidates (to save work), which would have silently broken
the existing priority rule that a move which BOTH captures AND completes a five must
still outrank an ordinary capture — `Pat::default()`'s zero flags would have hidden the
five entirely. Implementer fixed it by always computing the 4 axis `Pat`s regardless of
capture status (matching what the original unfused `order_score` already did
unconditionally), preserving the target redundancy elimination without the regression.
Ruling: this catch is correct and the fix is exactly right — noted here as the ledger's
record of it, not a separate finding requiring my own re-derivation, since the
implementer's own reasoning (verified against `order_score`'s pre-existing behavior) is
sound and directly checkable.

**Result:** smaller than hoped (~10-21% nodes, not the ~1.4x the earlier back-of-envelope
math suggested) — likely because the fusion only removes the `me`-perspective duplicate;
`order_score`'s `opp`-perspective checks (4 more window calls per candidate, needed for
opponent-threat detection, no `is_legal` equivalent to fuse against) are irreducible by
this technique. Still real: 3/7 full-budget positions gained +1 ply. **Best case now
depth 9** (up from 8) — one ply short of the requirement, closest yet. avg ~289ms.
Depth progression across this whole investigation: 4->7->8->9, all commits still
withheld pending an actually-passing gate. Not proposing the next fix independently.

Ruling 14 (dispatched, result pending): 4 more safe early-exit eliminations identified —
(1) skip opp-perspective computation entirely once me_five is confirmed via the free
Pat array; (2) early-break the opp-perspective loop on first opp_threat found; (3) stop
order_score recomputing captures_of a second time (missed in the original fusion —
generate_with_patterns already computes it once per candidate); (4) early-break
generate_with_patterns's own free-three count once it hits 2 (illegal, discarded
either way). All four are pure early-exit on an already-determined result, no algorithm/
tuning change. Implementer applying against current code with own judgment on exact
placement, instructed to push back if any isn't actually safe.

**Result:** applied cleanly, two real safety subtleties caught and correctly handled
(opp-loop break must wait for the FULL me_pats scan first, not interleave; the
free-three early-break must stay gated on n==0 same as Ruling 13's capture/five
interaction). Smaller gain than every prior round (~2-7% nodes) — no position gained a
ply. Depth profile stable across 3 runs: `[4,9,0,6,3,5,8,2,5,4]`, best case still 9.
avg ~287ms. Implementer reports no further safe, provably-equivalent redundancy left to
find. Full progression: 4 -> 7 (R10) -> 8 (R12) -> 9 (R13) -> 9 (R14, no further gain).

## Ruling 15 — two remaining moves: a real test-methodology bug, and the spec-sanctioned final lever

**The depth profile itself reveals a second bug, independent of throughput.**
`[4,9,0,6,3,5,8,2,5,4]` — the `0` (seed 2) is `find_best_move`'s pre-existing immediate-
win shortcut correctly firing (found a proven win, played it without searching — noted
as a possibility back at Ruling 12). This is optimal behavior, not a capability gap, but
the benchmark's aggregation (`min_depth = min_depth.min(stats.depth_reached)`, run
unconditionally over every position) folds this `0` into the `min_depth >= 10` assertion
exactly like a real shortfall would be — meaning **even a search that reached depth 10+
on every genuinely-searched position would still fail this benchmark as currently
written**, because of how the test aggregates, not because of engine capability. This
needs fixing regardless of the throughput question below.

**Ruling (test fix):** only fold a position's `depth_reached` into `min_depth` when it's
`> 0` — a `0` means "found a proven win via `check_end` before searching at all," which
by construction beats any bounded-depth search result and shouldn't be compared against
the depth-10 bar at all. One-line change: `if stats.depth_reached > 0 { min_depth =
min_depth.min(stats.depth_reached); }`. (Edge case noted, not worth guarding: if ALL 10
seeds happened to hit the shortcut, `min_depth` would trivially stay at its `u8::MAX`
initial value and pass vacuously — implausible with 10 varied random-walk seeds, not
worth a special case.)

**The remaining genuine gap (9 real positions, need depth 10, currently best-case 9,
branching ~2.0-2.2x/ply) needs the one lever the plan's own text explicitly names as the
accepted final mechanism, not a workaround.** Spec (`docs/superpowers/specs/2026-09-01-
gomoku-design.md`, search.rs's candidate-truncation rationale, already quoted in this
project's own earlier task text): "This makes the search non-exhaustive: a strong move
ranked 21st is invisible. This is the deliberate cost of reaching depth 10... mitigated
by putting all forcing moves... in the top priorities." Truncating candidates IS the
spec's own designated mechanism for trading move-list breadth for search depth — not an
improvised hack. `max_candidates` has only been tested at 20 and 14 so far (in two
different, now-superseded performance regimes). Ruling: push it further and measure —
try 10 and 8 (maybe lower if needed), pick the smallest value that gets `min_depth >= 10`
on the real positions while every OTHER existing test still passes (especially
`find_best_move_is_deterministic` and `find_best_move_extends_a_three_into_an_open_four`
— if a lower `max_candidates` ever breaks either of those, that value cut too deep into
genuinely necessary move breadth, back off). Update both the benchmark's own `cfg` and
`SearchConfig::default()` together, consistent with how Ruling 11's smaller reduction
was handled.

**Cost if wrong:** the test-methodology fix is low-risk (a one-line aggregation change,
doesn't touch the engine at all). The `max_candidates` reduction carries the honest,
spec-acknowledged tradeoff of the search seeing fewer candidate moves per node — mitigated
by the existing forced-response filter (`score_order_and_truncate`) already guaranteeing
threat-response moves survive truncation regardless of the cap's value, and by requiring
`find_best_move_is_deterministic` and the tactical-strength test to keep passing as the
concrete check that move quality wasn't meaningfully damaged.

**Result:** the `min_depth > 0` fix alone moved min_depth from the spurious 0 to 2, still
failing. `max_candidates` swept 14->10->8: every value kept the full suite clean and
mostly IMPROVED individual depths, but min_depth never moved off 2 — a clean signal
this specific stall isn't candidate-breadth-shaped. Implementer correctly stopped after
confirming the pattern at two consecutive values rather than guessing further and risking
real tactical-quality regressions for a lever that provably can't help here. Reverted to
max_candidates:14.

**Root cause (confirmed via source inspection, not guessed):** a second, different
instance of the exact same class of thing Ruling 15 already fixed once. `find_best_move`'s
OWN iterative-deepening loop has a second, later shortcut (spec §9.5): `if last_score >=
WIN - 1000 { break; }` — deepening stops as soon as a near-certain forced win is found at
ANY depth, since further search can't improve on it. Seed 7 (and likely seed 4, same
signature) hits this after depth 2 — evidence it's score-driven, not throughput-limited:
elapsed and nodes both SHRANK as `max_candidates` dropped (18.7ms/2350 nodes ->
13.4ms/1058 nodes) while depth held at exactly 2 throughout, the opposite of what a
throughput-limited position would show under the same sweep.

## Ruling 16 — extend the min_depth exclusion to the SAME class of legitimate early-stop, not a new special case

**Ruling:** Ruling 15's fix only caught the PRE-search shortcut (`depth_reached == 0`).
This is the same shortcut family, just triggered mid-deepening instead of before the
first iteration — both represent the search correctly concluding "I've already found the
best possible outcome" and stopping, not underperforming. Replace Ruling 15's narrower
`if stats.depth_reached > 0` fix with a single, more complete predicate that subsumes it:
check whether the CHOSEN move's score indicates a forced win, using the exact same
threshold `find_best_move` itself already uses to decide to stop deepening:
```rust
let found_forced_win = stats.root_scores.iter().any(|&(m, s)| m == mv && s >= WIN - 1000);
if !found_forced_win {
    min_depth = min_depth.min(stats.depth_reached);
}
```
This elegantly covers BOTH cases with one rule: the pre-search shortcut always populates
`root_scores` with `(mv, WIN)` (`WIN >= WIN-1000` trivially true), and the mid-deepening
shortcut's `root_scores` will show the chosen move's near-WIN score directly — so the
`depth_reached > 0` check becomes unnecessary once this is in place, not a separate
special case bolted alongside it.

**Cost if wrong:** Same low risk as Ruling 15's fix — a benchmark-test-only aggregation
change, doesn't touch engine behavior, uses the exact same threshold constant the engine
itself already treats as "conclusively decisive" rather than inventing a new one.
