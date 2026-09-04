# Task 14 Report: Performance benchmark gate

## Status: BLOCKED

The benchmark test was added exactly as the brief specifies. It fails in
release mode, and neither of the brief's two cheap tuning steps (lower
`max_candidates`, check TT hit rate for a Zobrist bug) fixes it. The root
cause is a real, pre-existing performance bug — but it lives entirely
outside `search.rs`, in `rules::generate`/`rules::is_legal` (Tasks 3-4),
which this task's scope explicitly excludes touching.

## What I implemented

Added `benchmark_gate_depth_and_time` (plus its `BenchXs` seeded xorshift
helper) to `src/search.rs`'s `#[cfg(test)] mod tests`, verbatim from the
brief — no modifications. `git diff src/search.rs` shows a pure addition:
nothing else in the file changed (I tried tuning step 1 mid-investigation,
lowering `max_candidates` from 20 to 14 in both the test's `cfg` and
`SearchConfig::default()`, then reverted both back to 20 once the data
showed it made no measurable difference — see "Tuning steps" below). The
final diff is exactly the brief's Step 1 code block appended after the
last existing test.

## The actual benchmark output (release mode)

Command: `cargo test --release search::tests::benchmark_gate_depth_and_time -- --nocapture`
(the `--lib` form doesn't apply — this crate is bin-only, no `[lib]`
target, matching how Tasks 9-13 already run tests)

Final state (matching the brief exactly, `max_candidates: 20`):

```
thread 'search::tests::benchmark_gate_depth_and_time' panicked at src/search.rs:902:9:
minimum depth reached across 10 positions was 0, below the required 10 (spec §14, R14)
test search::tests::benchmark_gate_depth_and_time ... FAILED
```

Per-seed breakdown, captured with temporary instrumentation (the brief's
own per-seed `eprintln!` is gated behind `cfg!(debug_assertions)`, so it
does not print in `--release` — I temporarily removed that guard to see
real numbers, then reverted it back to match the brief exactly before
finishing):

| seed | depth | elapsed | nodes | tt_hits/probes | hit rate |
|---|---|---|---|---|---|
| 0 | 2 | 434.78ms | 3584 | 293/1348 | 21.7% |
| 1 | 5 | 454.48ms | 2560 | 613/1320 | 46.4% |
| 2 | 0 | 7.83ms | 0 | 0/0 | — (immediate-win shortcut) |
| 3 | 3 | 403.35ms | 2801 | 316/1086 | 29.1% |
| 4 | 3 | 360.35ms | 2545 | 433/963 | 45.0% |
| 5 | 2 | 410.11ms | 3584 | 192/1215 | 15.8% |
| 6 | 4 | 431.22ms | 2561 | 450/1242 | 36.2% |
| 7 | 2 | 312.06ms | 2554 | 49/518 | 9.5% |
| 8 | 2 | 431.83ms | 3072 | 216/959 | 22.5% |
| 9 | 1 | 407.23ms | 3585 | 100/856 | 11.7% |

- **Average elapsed: ~365ms** (3653ms / 10) — actually *under* the 400ms
  target (R15 would pass), but only because seed 2's 7.8ms immediate-win
  outlier drags the average down; five of the nine searched positions
  individually exceed 400ms.
- **Minimum depth reached: 0** (seed 2), and even excluding that as a
  legitimate zero (see below), the true minimum among searched positions
  is **depth 1** (seed 9) — nowhere near the required 10. **R14 fails
  badly: max depth reached anywhere in the run is 5.**
- Node counts are ~2500-3600 total per full 400ms search. Spec §14 states
  a (non-hard) target of 3M+ nodes/sec; actual throughput here is
  **~6,000-9,000 nodes/sec — roughly 400x below target**.

## Two distinct problems found

**1. One benchmark position (seed 2) legitimately has `depth_reached = 0`
by design, not by failure.** The 24-move random legal-move walk never
calls `rules::check_end`, so it can walk straight through/past a winning
alignment or leave one move away from an unstoppable win. When that
happens, `find_best_move`'s immediate-win shortcut (Task 12, spec §9.5)
correctly returns the winning move with `depth_reached = 0, nodes = 0` —
exactly the same sentinel the existing test
`find_best_move_takes_the_immediate_win` already asserts on. This is
correct behavior from an already-committed, already-tested task; the
benchmark's literal position-generation code (as given verbatim in the
brief) doesn't filter these out before requiring `min_depth >= 10` from
every one of the 10 positions. This alone would fail the gate even if the
engine's real search speed were spec-compliant.

**2. The dominant problem: real search throughput is ~400x below the
spec's own stated target, and it isn't caused by anything tuning options
1-3 can fix.** `rules::generate()` (`src/rules.rs:135-150`) linearly scans
all up to 361 board cells and calls `is_legal()` for every empty
near-stone candidate. `is_legal()` (`src/rules.rs:29-38`), for every
non-capturing candidate, does:

```rust
let mut scratch = b.clone();
count_free_threes(&mut scratch, mv, p, pt) < 2
```

`Board` (`src/board.rs:116-136`) is a ~6.5KB struct dominated by two
`[[u64; 361]; 2]` Zobrist key tables (built once at construction but
carried on every clone regardless). `generate()` is called on **every**
negamax node (`src/search.rs:303`), so every node pays for up to ~dozens
of full 6.5KB `Board::clone()`s just to build its move list — megabytes of
memcopy per node. That fully explains why depth reached tops out at 5 and
node counts stay in the low thousands even after burning the entire 400ms
budget.

## Tuning steps taken, per the brief's prescribed order

**1. Lower `max_candidates` from 20 to 14** (both the test's `cfg` and
`SearchConfig::default()`), re-ran the exact same command:

```
seed 0: depth 2, 437.84ms, 3584 nodes   (vs 434.78ms/3584 at 20)
seed 1: depth 5, 451.46ms, 2560 nodes   (vs 454.48ms/2560 at 20)
seed 6: depth 5, 404.55ms, 2538 nodes   (vs 431.22ms/2561 at 20, depth 4→5)
... other seeds: node counts and depths essentially unchanged
```
No meaningful change — confirms the bottleneck is per-node cost, not
branching factor. Reverted both back to 20 (the brief's literal value)
once this was established.

**2. Checked TT hit rate for a possible Zobrist bug.** Several positions
are under the brief's 20% threshold (seed 7: 9.5%, seed 9: 11.7%, seed 5:
15.8%). Re-ran `play_undo_round_trip_restores_exact_state`
(`cargo test --release play_undo_round_trip_restores_exact_state --
--nocapture`) — **it passes**. Given problem #2's node-count evidence
(only ~2500-3600 total nodes reached per search, mostly at shallow
depths), a low hit rate here reads as the natural consequence of a small,
shallow tree with few repeated positions to hit — not as corruption. I
did not have independent evidence pointing at a Zobrist bug specifically
(the round-trip test the brief names as the first check still passes), so
I did not go further down that path (e.g. strengthening the round-trip
test per the brief's suggestion) since the smoking gun in `rules.rs`
already fully explains the timing.

**Did not attempt option 3** (instrumenting `order_score`'s first-move
cutoff rate) — the node counts are so low (2500-3600 total, reaching only
depth 1-5) that the search isn't running long enough for move-ordering
quality to be the binding constraint; the time is being spent inside
`rules::generate`'s per-candidate board cloning before the tree can grow
at all.

**Did not attempt option 4** (parallel search) — explicitly out of scope
per the brief and per my task instructions.

## Why I'm stopping here rather than fixing it

The actual fix (avoid `Board::clone()` per candidate inside
`is_legal`/`count_free_threes`, e.g. by doing the scratch play/undo
directly on `b` instead of a full clone) lives in `src/rules.rs` and
touches a function used across Tasks 3-7's already-committed, already-
tested code. My task instructions are explicit: work only in `search.rs`
(and possibly its own `SearchConfig::default()`), and escalate rather than
guess further once the two cheap options are exhausted. This is exactly
that situation — the fix is real, understood, and localized, but it's a
different file, a different task's territory, and warrants its own
review rather than a fix bundled quietly into "add one test."

## Files changed

- `/home/fwong/Desktop/42/gomoku/.claude/worktrees/gomoku-impl/src/search.rs`
  — added `benchmark_gate_depth_and_time` and its `BenchXs` helper to the
  test module, verbatim per the brief. No other files modified. (I read,
  but did not edit, `src/rules.rs` and `src/board.rs` while diagnosing
  this.)

## Self-review

- Ran in genuine `--release` mode (`Finished \`release\` profile
  [optimized]` confirmed in every run's output, not debug).
- All numbers above are real command output, pasted from actual
  `cargo test --release ... -- --nocapture` runs, not estimates.
- Confirmed via `git diff src/search.rs` that the final working-tree state
  is a pure addition matching the brief's Step 1 code block exactly —
  `max_candidates` is back at 20 in both places I temporarily touched it.
- `cargo clippy --release --all-targets`: no new warnings or errors from
  the added test (the crate's existing dead-code/unused warnings are
  pre-existing — `search.rs` isn't wired into `main.rs` yet, that's
  Task 15+).
- Full suite check: `cargo test --release` (all 38 tests) shows 37 passed,
  only `benchmark_gate_depth_and_time` failed — no regression in anything
  else.
- Did not commit. The brief's Step 3 commit message
  ("test: performance benchmark gate — depth >=10 under 400ms average")
  asserts a passing gate; committing it while the gate fails would
  misrepresent the state of the project. The test code is in the working
  tree, uncommitted, ready to commit once whatever fixes the underlying
  `rules.rs` bottleneck lands.

## Recommendation

This needs a short, separate task/fix in `rules.rs`: stop cloning the
whole `Board` per legality check. The existing `play`/`undo` machinery
already supports scratch mutation in place (`count_free_threes` already
takes `&mut Board` and does `b.play(mv, pt)` / `b.undo(&u)` — it's only
`is_legal` wrapping that in an extra `b.clone()` because `is_legal` itself
takes `&Board`, not `&mut Board`). Changing `is_legal`'s signature to
`&mut Board` (mirroring `count_free_threes`) and threading that through
its callers would let it mutate-then-undo `b` directly instead of cloning
it, which should remove the dominant cost identified above. That's a
`rules.rs`/Task 3-4-scoped change and its own review, not something to
fold into this task.

---

## Update: Ruling 9 applied, gate still fails — new root-cause evidence

The coordinator reviewed the finding above, ruled it in scope as "the fix,
not a workaround" (Ruling 9), and gave exact instructions: change
`is_legal`, `generate`, and `five_is_breakable` in `src/rules.rs` from
`&Board` to `&mut Board`, delete the `b.clone()` in `is_legal`, and update
5 named test call sites. I applied every change exactly as specified.

### What I applied

`src/rules.rs`:
- `is_legal(b: &Board, ...)` → `is_legal(b: &mut Board, ...)`, body now
  calls `count_free_threes(b, mv, p, pt)` directly instead of
  `let mut scratch = b.clone(); count_free_threes(&mut scratch, ...)`.
- `generate(b: &Board, ...)` → `generate(b: &mut Board, ...)`, body
  unchanged.
- `five_is_breakable(b: &Board, ...)` → `five_is_breakable(b: &mut Board, ...)`,
  body unchanged.
- `check_end`'s doc comment sentence updated per the coordinator's exact
  replacement text.
- 5 test call sites updated (`is_legal(&b, ...)` → `is_legal(&mut b, ...)`
  ×3, `generate(&b, ...)` → `generate(&mut b, ...)` ×2, one of which also
  needed `let b = Board::new();` → `let mut b = Board::new();`).

### Two things the coordinator's grep missed, that the compiler caught

`cargo build --release` (non-test) was clean, confirming zero production
`search.rs` call sites needed changes, exactly as predicted. But
`cargo test --release` found **3 more call sites** the grep didn't
catch because they're inside `#[cfg(test)]` modules, which a non-test
build doesn't compile:
- `src/eval.rs:76`, inside the existing test
  `accumulator_never_drifts_from_full_recompute` — unrelated to this
  task, pre-existing code.
- `src/search.rs`, 2 call sites inside my own new
  `benchmark_gate_depth_and_time` test (the random-walk loop and the
  post-walk `check_candidates` check).

All three needed `&b` → `&mut b`. That alone then hit a second compiler
error: `rules::generate(&mut b, b.to_move, &pt, &mut candidates)` doesn't
compile — `&mut b` is an explicit reborrow expression (not a method-call
auto-ref), so two-phase borrows don't cover it, and `b.to_move` in the
same argument list conflicts with the live mutable borrow. Fixed by
hoisting `let to_move = b.to_move;` before the call in all three spots.
Confirmed via `rustc`'s own error (E0503) rather than guessing.

### Verification

- `cargo build --release`: clean, no errors.
- `cargo test --release` (full suite, 38 tests): **37 passed**, only
  `benchmark_gate_depth_and_time` failed — every pre-existing test passes
  unchanged, confirming this really was a pure signature/redundant-clone
  removal with no logic change, as the coordinator described.
- `cargo clippy --release --all-targets`: no new warnings or errors.

### The benchmark gate still fails — and the numbers barely moved

```
thread 'search::tests::benchmark_gate_depth_and_time' panicked at src/search.rs:902:9:
minimum depth reached across 10 positions was 0, below the required 10 (spec §14, R14)
```

Per-seed, before vs. after Ruling 9 (both release, `max_candidates: 20`,
same seeds, temporarily un-gating the `eprintln!` to see release numbers
as before):

| seed | depth before | depth after | nodes before | nodes after | elapsed before | elapsed after |
|---|---|---|---|---|---|---|
| 0 | 2 | 2 | 3584 | 3584 | 434.78ms | 407.16ms |
| 1 | 5 | 5 | 2560 | 2560 | 454.48ms | 424.67ms |
| 2 | 0 | 0 | 0 | 0 | 7.83ms | 6.91ms |
| 3 | 3 | 3 | 2801 | 3072 | 403.35ms | 412.37ms |
| 4 | 3 | 3 | 2545 | 2545 | 360.35ms | 334.14ms |
| 5 | 2 | 2 | 3584 | 4096 | 410.11ms | 428.30ms |
| 6 | 4 | 4 | 2561 | 2561 | 431.22ms | 401.89ms |
| 7 | 2 | 2 | 2554 | 2554 | 312.06ms | 279.60ms |
| 8 | 2 | 2 | 3072 | 3584 | 431.83ms | 451.81ms |
| 9 | 1 | 1 | 3585 | 4097 | 407.23ms | 424.51ms |

**Depth reached is bit-for-bit identical for all 10 positions, before and
after removing the clone.** Node counts moved by low single-digit
percentages in a few positions (seed 3, 5, 8, 9 up ~10-20%; others
unchanged). Average elapsed: ~357ms after vs. ~365ms before — no
meaningful change. This means the `Board::clone()` was real, verified
waste (correct to remove — it's gone now, tests still pass), but it was
**not** the dominant cost. Something else is.

### New finding: `count_free_threes` still does far more work than it needs to

I did not implement a fix for this — per my instructions, I'm reporting
it back rather than guessing further with another structural change.

`src/board.rs:267` already has exactly the tool this needs:
`hypothetical_window_code(c, d, p)` — computes the 9-cell window code as
if `p`'s stone were already at `c`, **without mutating the board at all**.
Its own doc comment states its purpose explicitly: *"Used only for move
ordering, where `c` is always an empty candidate cell and mutating the
board via `play`/`undo` to test each one would be far too slow to run on
40-80 candidates at every search node."* `search.rs`'s `order_score`
already uses it for exactly this reason.

`count_free_threes` (`src/rules.rs:11-25`) does not use it. Instead, for
every non-capturing candidate cell `is_legal` is asked about, it does a
full `b.play(mv, pt)` ... `b.undo(&u)` round trip just to read
`b.window_code_pub(mv, d, p)` afterward for 4 axes — when
`b.hypothetical_window_code(mv, d, p)` would give the identical window
code for each axis directly, with no mutation, no bookkeeping.

`Board::play` (`src/board.rs:459-546`), even on the fast no-capture path,
does substantially more than compute a window code: `captures_of` (8
directions × up to 3 cells), `adjust_axis_neighbors` and
`adjust_axis_vuln` (each called twice, once to remove and once to
re-add), `adjust_neighbor_grid` (touches cells in a radius-2 neighborhood),
and `stone_window_score` for 4 directions to update the incremental
accumulator — plus the matching `undo` to reverse all of it. This runs
once per non-capturing candidate, inside `generate`'s scan, at **every**
negamax node. This — not the clone — looks like the actual dominant
cost, and it's consistent with the data above: removing only the clone
left this untouched, so node throughput barely moved.

I did not check TT hit rate or cutoff-first-move rate further, since
nodes/sec did not in fact rise "dramatically" as the coordinator's
instructions anticipated it would if the clone were the dominant cost —
that premise didn't hold, so those checks wouldn't add information beyond
what's already here.

### Status: still BLOCKED

Ruling 9's fix is real, correct, verified, and committed to the working
tree (uncommitted to git — see below). It just wasn't sufficient by
itself. The benchmark gate (`benchmark_gate_depth_and_time`) still fails
identically to before on the depth requirement (R14): max depth reached
anywhere is still only 5, vs. the required 10.

**Did not commit anything.** The coordinator's commit instructions were
written for the case where this fix makes the gate pass; it didn't. Per
my task's standing instruction ("if depth is still short of 10 after this
fix, do NOT guess further... report back"), I'm holding the working tree
as-is (uncommitted `src/rules.rs`, `src/eval.rs`, `src/search.rs` changes)
pending a ruling on `count_free_threes`/`hypothetical_window_code` before
proceeding further.

---

## Update: Ruling 10 applied — real fix, gate still short of 10

The coordinator confirmed `count_free_threes` is only ever reached from
`is_legal` after captures are already ruled out, so a real `play()` at
`mv` in that path only ever changes cell `mv` itself — meaning
`hypothetical_window_code` (already in `board.rs`, spotted in the prior
update) gives *provably* identical output to the old play/undo round
trip, not just probably-equivalent. Ruling 10: rewrite
`count_free_threes` to use it directly, and revert `is_legal`, `generate`,
`five_is_breakable` back to `&Board` (Ruling 9's `&mut Board` threading is
no longer needed — nothing in this path mutates the board at all now).

### What I applied to `src/rules.rs`

- `count_free_threes(b: &mut Board, ...)` → `count_free_threes(b: &Board, ...)`,
  body replaced: no more `b.to_move` save/restore, no `play`/`undo`; just
  `b.hypothetical_window_code(mv, d, p)` per axis.
- `is_legal`, `generate`, `five_is_breakable`: reverted `&mut Board` →
  `&Board` (signatures and doc comments), bodies otherwise unchanged.
- `check_end`'s doc comment: reverted to original wording ("every helper
  it calls takes `&Board`") — true again.
- Left the 5 rules.rs test call sites at `&mut b`/`let mut b` (Ruling 9's
  edits) and the `let to_move = b.to_move;` hoists in `eval.rs` and my
  benchmark test, per the coordinator's explicit note that these are
  harmless (auto-reborrow) and not worth cleanup time right now.

### Verification

- `cargo build --release`: clean.
- `cargo test` (debug, full suite): **38 passed** (benchmark test passes
  trivially, unenforced, as designed for debug builds).
- `cargo test --release` (full suite): **37 passed**, only
  `benchmark_gate_depth_and_time` failed.
- `cargo clippy --release --all-targets`: no new errors.

### The gate still fails, but the fix is real and large

```
thread 'search::tests::benchmark_gate_depth_and_time' panicked at src/search.rs:902:9:
minimum depth reached across 10 positions was 0, below the required 10 (spec §14, R14)
```

Per-seed, before Ruling 10 vs. after (both release, `max_candidates: 20`):

| seed | depth before | depth after | nodes before | nodes after | node ratio | elapsed after |
|---|---|---|---|---|---|---|
| 0 | 2 | 4 | 3584 | 16384 | 4.6x | 418.57ms |
| 1 | 5 | 7 | 2560 | 12288 | 4.8x | 410.13ms |
| 2 | 0 | 0 | 0 | 0 | — | 1.14ms (immediate-win shortcut) |
| 3 | 3 | 5 | 3072 | 13312 | 4.3x | 409.31ms |
| 4 | 3 | 3 | 2545 | 2545 | 1.0x | 74.01ms (forced-win found, stopped early — unrelated to this fix) |
| 5 | 2 | 3 | 4096 | 16384 | 4.0x | 409.84ms |
| 6 | 4 | 7 | 2561 | 14336 | 5.6x | 400.51ms |
| 7 | 2 | 2 | 2554 | 2554 | 1.0x | 57.99ms (forced-win found, stopped early — unrelated to this fix) |
| 8 | 2 | 3 | 3584 | 14336 | 4.0x | 408.61ms |
| 9 | 1 | 3 | 4097 | 15872 | 3.9x | 400.79ms |

**Node counts jumped 4-5.6x** across every position that actually used
its full time budget (seeds 0, 1, 3, 5, 6, 8, 9) — confirming the
`count_free_threes` play/undo round trip really was the dominant cost, as
predicted. Seeds 4 and 7 show identical node counts before and after:
both hit `find_best_move`'s "found a near-WIN score, stop deepening"
early-exit (spec §9.5) at a shallow depth, finishing in 58-74ms — that
shortcut is unrelated to this fix and was already firing before it.

**Depth reached improved (2→4, 5→7, 3→5, 2→3, 4→7, 2→3, 1→3 across the
seven affected positions) but is still short of the required minimum of
10 everywhere.** Best case is depth 7 (seeds 1 and 6); the true minimum
across all 10 positions is still 0 (seed 2's design-legitimate immediate-
win case, unaddressed — see the first update above) or 2 (seed 7,
excluding that). Average elapsed is now closer to the 400ms ceiling on
most positions (several sit right at 400-419ms, meaning iterative
deepening is now using the full budget rather than finishing early) —
consistent with alpha-beta's exponential node growth per ply: a ~5x node
budget increase buys roughly 2-3 additional plies at this position's
effective branching factor, not the 5-7 additional plies needed to close
the remaining gap to depth 10.

### Status: still BLOCKED

Per my instructions, not guessing at a further structural change on my
own judgment. Reporting the numbers above — the real bottleneck Ruling 10
targeted is confirmed and fixed (4-5.6x more nodes per position), but
closing the remaining gap to depth 10 looks like it needs either a
further reduction in per-node cost, a further count reduction (deeper
pruning/LMR tuning), or accepting that depth 10 in 400ms may not be
reachable with the current architecture without a redesign — none of
which I'm doing unilaterally.

**Did not commit.** Working tree still holds `src/rules.rs`,
`src/eval.rs`, `src/search.rs` uncommitted, pending further direction.

---

## Update: Ruling 11 — max_candidates=14 retest + per-node cost breakdown

Two asks, in order: (1) re-test the brief's option 1 fresh, now that both
of today's fixes are in, since the earlier "no effect" result was measured
under the old play/undo-dominated cost regime; (2) get a rough cost
breakdown of where time goes within a node, specifically checking the
coordinator's hypothesis that `is_legal` and `order_score` redundantly
recompute overlapping window-pattern data for the same candidates.

### 1. `max_candidates` 20 → 14, retested fresh

Lowered it in both the benchmark test's `cfg` and `SearchConfig::default()`,
re-ran 3 times (twice at 14, once more at 20 for a same-session A/B), all
release, all with the seven "full-budget" seeds landing consistently:

| seed | depth @ 20 | depth @ 14 |
|---|---|---|
| 0 | 4 | 4 |
| 1 | 7 | 7 |
| 3 | 5 | 5 |
| 5 | 3 | **4** |
| 6 | 7 | 7 |
| 8 | 3 | **4** |
| 9 | 3 | 3 |

Reproducible across repeated runs at each setting (ran candidates=14 twice,
candidates=20 twice — same depth profile each time). Two positions (seed 5,
seed 8) gain +1 ply at 14; none regress; average elapsed is statistically
the same either way (~300ms both settings, dominated by the same two
early-exit outliers in both cases). **Kept `max_candidates: 14`** in both
the benchmark test's `cfg` and `SearchConfig::default()`, per the
"if it helps, keep it" instruction — small but real, non-negative, and free.

### 2. Cost breakdown (temporary instrumentation, removed afterward)

Added `thread_local!` call counters at the two suspected call sites
(`is_legal`/`count_free_threes`'s `captures_of`/`hypothetical_window_code`
calls, `order_score`'s same two) plus `generate()`'s own call count, and
wrapped `negamax`'s three phases (TT probe+store, `rules::generate`,
`score_order_and_truncate`) with `Instant` deltas accumulated the same way.
Reset before, printed after, each of the 10 benchmark positions. Removed
all of it afterward (verified via `grep -rn "DBG_\|dbg_t" src/*.rs` —
no matches) and reran the full suite clean before finalizing numbers.

Representative data, seed 0 (a full-time-budget position, ~411ms,
15872-16896 nodes depending on the exact run):

```
generate_calls=23901
is_legal:    captures_of=3,115,233   hwc=12,290,668
order_score: captures_of=765,822     hwc=6,346,136
time: generate=50.6ms  order=141.9ms  tt=1.1ms   (of ~412ms total)
```

Consistent proportions held across all 7 full-budget positions (checked
each one, not just seed 0):

| phase | seed 0 | seed 1 | seed 3 | seed 5 | seed 6 | seed 8 | seed 9 |
|---|---|---|---|---|---|---|---|
| generate() (is_legal path) | 12.3% | 13.5% | 13.4% | 12.1% | 11.9% | 13.0% | 12.0% |
| score_order_and_truncate() (order_score path) | 34.4% | 37.3% | 37.0% | 33.1% | 35.3% | 36.7% | 33.8% |
| TT probe+store | 0.27% | 0.18% | 0.28% | 0.30% | 0.25% | 0.17% | 0.29% |
| everything else (unaccounted) | ~53% | ~49% | ~49% | ~54% | ~53% | ~50% | ~54% |

**Answering the coordinator's specific question — `order_score`'s cost is
the larger of the two, not `is_legal`'s, even though `is_legal` is called
roughly 4x more often.** Call counts: `is_legal` runs ~3.0-3.1M times per
search vs. `order_score`'s ~730K-815K derived invocations (hwc/8) — but
`order_score` still costs 2.5-3x more wall-clock time (~135-153ms vs.
~48-57ms). Two structural reasons, visible directly in the code (not
inferred):

- `order_score` (`src/search.rs`) does **8** `hypothetical_window_code`
  calls per candidate (4 axes × {me, opp}) unconditionally, every time,
  before it even checks `mv == tt_mv` is already past or looks at
  captures. `is_legal`/`count_free_threes` (`src/rules.rs`) does at most
  **4** (one axis-loop, one player only), and only reaches them at all
  for non-capturing candidates — captures short-circuit before any HWC
  call, whereas `order_score`'s window loop runs *before* its own capture
  check, so a capturing candidate still pays the full 8-HWC cost there.
- Both are computing `hypothetical_window_code(mv, d, me)` for the exact
  same `(mv, d, me)` pairs on the exact same candidate set — `generate()`
  already proved each candidate legal (partly via this exact computation
  for `me`'s free-three count) immediately before
  `score_order_and_truncate` reruns the equivalent lookup on `me`'s side
  again for ordering. This is the redundancy the coordinator suspected,
  confirmed by code inspection: `is_legal`'s F_FREE_THREE check and
  `order_score`'s F_FIVE/F_OPEN_FOUR check both read
  `hypothetical_window_code(mv, d, me)` — different pattern-table flags
  off the *same* computed code, computed twice.
- One nuance worth flagging: `score_order_and_truncate` scores **every**
  legal candidate `generate()` returns, then truncates to
  `max_candidates` *after* scoring — so `order_score` is not merely
  running on the already-small truncated set, it pays its (larger)
  per-candidate cost on the full legal candidate list too, same as
  `is_legal` does. Truncation only reduces what gets recursed into, not
  what gets scored.

**"Everything else" (~49-54%) is actually the single largest unaccounted
bucket, bigger than either instrumented phase.** I did not break this down
further — it's most plausibly `eval::evaluate()` at leaf nodes (the
majority of the ~16K "nodes" are `depth == 0` leaves that never call
`generate`/`order_score`/TT at all), the real `Board::play`/`undo` calls
for moves actually taken along the search path (as opposed to the
non-mutating `hypothetical_window_code` calls), the `Vec::new()`
allocations for `candidates` and `scored` on every node (no reuse/pooling),
and LMR/PVS/killer/history control flow — but I did not instrument these,
since the two asks were specifically about `is_legal` vs `order_score`.

### Final benchmark numbers (post Ruling 10 + `max_candidates=14`, clean, no instrumentation)

```
benchmark seed 0: depth 4 elapsed 411.073159ms nodes 15872
benchmark seed 1: depth 7 elapsed 413.608242ms nodes 11264
benchmark seed 2: depth 0 elapsed 1.079094ms nodes 0
benchmark seed 3: depth 5 elapsed 408.024227ms nodes 12288
benchmark seed 4: depth 3 elapsed 76.866487ms nodes 2533
benchmark seed 5: depth 4 elapsed 406.924341ms nodes 15872
benchmark seed 6: depth 7 elapsed 409.335714ms nodes 13312
benchmark seed 7: depth 2 elapsed 55.173974ms nodes 2350
benchmark seed 8: depth 4 elapsed 415.128874ms nodes 12800
benchmark seed 9: depth 3 elapsed 408.000312ms nodes 15872
```

Average elapsed: ~300.5ms (comfortably under the 400ms target — R15
passes). Minimum depth: 0 (seed 2's design-legitimate immediate-win case)
/ 2 excluding it (seed 7). Best case: depth 7. **R14 (depth ≥ 10) still
fails everywhere.**

### Status: still BLOCKED

Both asked-for investigations are done with real numbers; not proposing a
third structural fix on my own judgment, per instructions. Summary for the
next ruling: `order_score` costs more than `is_legal` (2.5-3x wall-clock,
despite 4x fewer calls) because of its unconditional 8-HWC-call loop and
lack of a captures short-circuit before that loop runs, and because
`score_order_and_truncate` scores the full legal candidate list before
truncating rather than after. Combined, `is_legal` + `order_score` account
for only ~46-51% of total node time; the remaining ~49-54% is unaccounted
for and not instrumented (likely `eval::evaluate` at leaves, real
`play`/`undo` on the actual search path, and per-node `Vec` allocations).

**Did not commit.** Working tree holds `src/rules.rs`, `src/eval.rs`,
`src/search.rs` uncommitted (rules.rs per Ruling 10's final form,
search.rs/eval.rs with the harmless leftover reborrow sites plus
`max_candidates: 14`), pending further direction.

---

## Update: Ruling 11 continuation — characterizing the unaccounted ~50%

The coordinator did the math on eliminating the `is_legal`/`order_score`
redundancy (~1.4x nodes/sec) against last round's 4-5.6x→3-ply result and
concluded it's very unlikely to close the remaining 7→10 gap alone. Before
ruling on any further fix, they wanted the unaccounted ~49-54% bucket
characterized, in order of suspicion: (1) `play`/`undo`'s real cost on the
actually-recursed path, including how often LMR's re-search-on-improvement
fires; (2) whether the per-node `Vec` allocations in `generate()`/
`score_order_and_truncate` are a meaningful fraction; (3) anything else
that stands out (`sort_unstable_by`, `eval::evaluate`, killer/history
bookkeeping).

### What I found: `check_end`'s own internal `generate()` call was the missing piece

None of the three suspects the coordinator ranked highest turned out to be
the big one. Instrumenting `play`/`undo` and `eval::evaluate` directly
accounted for very little; what closed the gap was noticing that my
*previous* round's `DBG_T_GENERATE` timer only wrapped `negamax`'s own
`rules::generate(...)` call for building its candidate list — it did
**not** cover `rules::check_end`'s (`src/rules.rs`) own, separate
`generate()` call, used purely to test "does this position have zero
legal moves" (a draw). `check_end` is called once per candidate move
*examined* in `negamax`'s loop (i.e., roughly once per `play()`), and
when no five was just completed (the overwhelming common case), it pays
for a full board scan — the same `is_legal`-driven cost already measured
for `negamax`'s own candidate generation — just to answer a boolean.

I added a timer around the `rules::check_end(b, mv, ctx.pt)` call site
itself (`src/search.rs`, inside `negamax`'s loop) to capture this
previously-invisible cost as one bucket, alongside `play`/`undo`,
`eval::evaluate`, and a split of `score_order_and_truncate` into its
`map` (running `order_score`) and `sort_unstable_by` portions. All
instrumentation was `thread_local!` counters/`Instant` deltas, reset per
seed, printed once, then fully removed — verified via
`grep -rn "DBG_\|dbg_t" src/*.rs` (no matches) before finalizing.

Full breakdown, averaged over the 7 full-time-budget seeds (0, 1, 3, 5, 6,
8, 9 — excludes seeds 2, 4, 7, which exit early via the pre-existing
immediate/near-win shortcuts and aren't representative of steady-state
node cost):

| phase | share of node time |
|---|---|
| `check_end` (draw-check `generate()` + five-check) | **~41.3%** |
| `score_order_and_truncate` (`order_score`'s map + sort) | ~36.2% |
| `generate()` (`negamax`'s own candidate list, `is_legal`-driven) | ~12.4% |
| `play`/`undo` (the actual recursed-into path) | ~8.8% |
| TT probe + store | ~0.3% |
| `eval::evaluate` (leaf nodes) | ~0.03% |
| unaccounted | **~0.9%** (down from ~50%) |

This now accounts for ~99% of total node time, not ~50%. **`check_end`'s
own `generate()` call is the single largest cost in the entire node,
larger than `order_score`, larger than `negamax`'s own candidate
generation, larger than `play`/`undo`.** Per-call, it's comparable in
cost to `negamax`'s own `generate()` call (~7-10µs each, both doing the
same `is_legal`-filtered board scan) — it's just called noticeably more
often (once per child *examined*, not once per unique node), so its
aggregate cost ends up larger.

### The coordinator's three specific questions, answered

1. **`play`/`undo` cost and frequency**: ~8.8% of node time, called once
   per child actually recursed into (matches move-ordering quality — a
   near-1:1 ratio of `play` calls to nodes visited across most seeds
   indicates the search is cutting off quickly on well-ordered first
   moves, which is a *good* sign for move ordering, not a cost problem).
   **LMR re-search-on-improvement is not a meaningful cost**: reductions
   happen sometimes (0-186 per search depending on position), but the
   re-search branch itself (`sc > alpha` after a reduced search) fired
   **0 times in 6 of the 7 full-budget seeds, and only 3 times in the
   7th** (seed 8, out of ~13,000+ `play` calls that search). LMR's own
   logic is not contributing meaningfully to the unaccounted time and
   doesn't need touching.
2. **`Vec` allocations**: tested directly — swapped `Vec::new()` for
   `Vec::with_capacity(64)` at the two hot-path candidate-list sites
   (`rules::check_end`'s and `negamax`'s own `rules::generate` calls,
   `src/rules.rs` and `src/search.rs`). Re-ran the full benchmark: node
   counts were bit-for-bit identical (confirms zero behavior change, as
   expected for a capacity hint), and wall-clock time was unchanged
   within run-to-run noise (seed 0: 402.6ms → 409.5ms, i.e., slightly
   *worse*, well within the ~5-10ms noise band seen elsewhere across
   repeated runs). **Reverted both** — no measurable benefit, so the
   change doesn't earn a place in the diff. Separately noted:
   `score_order_and_truncate`'s `scored: Vec<_> = candidates.iter().map(...).collect()`
   was never a candidate for this fix in the first place — `candidates.iter().map(...)`
   is an `ExactSizeIterator`, and `Vec`'s `FromIterator` impl already
   pre-sizes the allocation exactly via that size hint (a standard-library
   specialization), so that particular `.collect()` had no realloc
   overhead to begin with.
3. **Anything else**: `sort_unstable_by`'s own cost is small and already
   folded into the `score_order_and_truncate` number above — isolated it
   specifically (see `map`/`sort` split): sort is consistently only
   ~3-3.5% of `score_order_and_truncate`'s total (e.g. seed 0:
   `map=127.8ms` vs `sort=4.3ms`), not worth a separate line. `TT` and
   `eval::evaluate` are both negligible (<0.5% combined).

### Final numbers (unchanged from the prior update — this round added no functional change)

Both experiments this round (the `Vec::with_capacity` tweak) were
reverted after measurement, so the benchmark's actual behavior is
identical to the previous update's final state:

```
benchmark seed 0: depth 4 elapsed 411.073159ms nodes 15872
benchmark seed 1: depth 7 elapsed 413.608242ms nodes 11264
benchmark seed 2: depth 0 elapsed 1.079094ms nodes 0
benchmark seed 3: depth 5 elapsed 408.024227ms nodes 12288
benchmark seed 4: depth 3 elapsed 76.866487ms nodes 2533
benchmark seed 5: depth 4 elapsed 406.924341ms nodes 15872
benchmark seed 6: depth 7 elapsed 409.335714ms nodes 13312
benchmark seed 7: depth 2 elapsed 55.173974ms nodes 2350
benchmark seed 8: depth 4 elapsed 415.128874ms nodes 12800
benchmark seed 9: depth 3 elapsed 408.000312ms nodes 15872
```

Average ~300ms (R15 passes), min depth 0 (seed 2 design case) / 2
excluding it, best case depth 7. R14 still fails everywhere.

### Status: still BLOCKED

Reported the full breakdown as requested; not proposing which structural
fix to make. For the next ruling, the standout fact is that `check_end`'s
draw-check pays for a full `is_legal`-filtered board scan (the same cost
class as `generate()` itself) just to answer a boolean ("does any legal
move exist"), and does so more often than `generate()` runs — making it
the largest single cost in the node, ahead of the `is_legal`/`order_score`
redundancy already identified. I have not touched `check_end`'s logic —
per the standing instruction, changing what it actually computes (as
opposed to removing dead recomputation) is a real behavioral change to
correctness-critical code, and I was told to check first.

**Did not commit.** Working tree state unchanged from the prior update
(all of this round's instrumentation and the `Vec::with_capacity`
experiment were fully reverted): `src/rules.rs` at Ruling 10's form,
`src/search.rs`/`src/eval.rs` with the harmless leftover reborrow sites
plus `max_candidates: 14`.

---

## Update: Ruling 12 — `has_legal_move` early-exit, gate still short of 10

Applied exactly as specified: added `rules::has_legal_move` (early-exits
on the first legal cell instead of building the full candidate list) and
replaced `check_end`'s draw-check (`generate()` + `is_empty()`) with a
direct call to it. No other files touched — `search.rs`/`eval.rs` are
unchanged from the prior update.

### Verification

- `cargo build --release`: clean.
- `cargo test` (debug, full suite): **38 passed**.
- `cargo test --release` (full suite): **37 passed**, only
  `benchmark_gate_depth_and_time` failed (same as every prior round —
  this is the gate we're trying to pass, not a regression).
- `cargo clippy --release --all-targets`: no errors.
- Did not add the optional extra test the coordinator flagged as not
  required (`has_legal_move` agreeing with `!generate(...).is_empty()`) —
  this is a pure early-exit restructuring of an existing, already-tested
  predicate (`is_legal`, unchanged), not new logic, and the full suite
  (including `rules.rs`'s existing `generate`/`is_legal` tests and every
  `search.rs` test that exercises `check_end` indirectly through
  `find_best_move`) passes unchanged. Flagging this back rather than
  silently skipping it, per the coordinator's own "not required, your
  call" framing.

### The gate still fails — but nodes roughly doubled and depth improved

```
thread 'search::tests::benchmark_gate_depth_and_time' panicked at src/search.rs:904:9:
minimum depth reached across 10 positions was 0, below the required 10 (spec §14, R14)
```

Per-seed, before Ruling 12 vs. after:

| seed | depth before | depth after | nodes before | nodes after | node ratio | elapsed after |
|---|---|---|---|---|---|---|
| 0 | 4 | 4 | 15872 | 27648 | 1.74x | 405.15ms |
| 1 | 7 | **8** | 11264 | 22016 | 1.95x | 403.78ms |
| 2 | 0 | 0 | 0 | 0 | — | 0.11ms (immediate-win shortcut, unaffected) |
| 3 | 5 | 5 | 12288 | 24064 | 1.96x | 400.93ms |
| 4 | 3 | 3 | 2533 | 2533 | 1.0x | 38.94ms (forced-win found early, unaffected — but note: elapsed nearly halved, 76.9ms→38.9ms, since even the shallow pre-shortcut search got cheaper) |
| 5 | 4 | 4 | 15872 | 26112 | 1.65x | 411.00ms |
| 6 | 7 | **8** | 13312 | 25600 | 1.92x | 408.37ms |
| 7 | 2 | 2 | 2350 | 2350 | 1.0x | 27.60ms (forced-win found early, unaffected — elapsed roughly halved here too, 55.2ms→27.6ms) |
| 8 | 4 | **5** | 12800 | 21504 | 1.68x | 413.09ms |
| 9 | 3 | **4** | 15872 | 31744 | 2.00x | 402.04ms |

**Node counts roughly doubled** across every position that uses its full
time budget (1.65x-2.00x, average ~1.84x) — consistent with eliminating
~41% of per-node cost (1/(1-0.41) ≈ 1.69x, in the right ballpark given
this is a rough instrumented estimate from a different position set).
Four of the seven full-budget positions gained **+1 ply** (seeds 1, 6, 8,
9); three stayed the same (seeds 0, 3, 5 — presumably just short of the
node budget for one more full ply at their branching factor). **Best case
is now depth 8** (up from depth 7 last round, depth 4 two rounds ago).
True minimum is still 0 (seed 2's pre-existing, unaddressed
immediate-win-shortcut case) / 2 excluding it (seed 7, which exits early
via the forced-win shortcut regardless of how many more nodes are
available — more speed doesn't help a position that's already decided in
2 plies). **R14 (depth ≥ 10) still fails everywhere; R15 (avg < 400ms)
passes comfortably** — average elapsed ~291ms (down from ~301ms last
round, both well under budget).

Ran the release benchmark three times total (once with per-seed
diagnostics unlocked to get the table above, twice more via the plain
`cargo test --release` full-suite run) — the failure is consistent, not
borderline: minimum depth is 0 every time (seed 2), and even the
depth-2-excluding-outliers minimum (seed 7) is stable at 2, nowhere near
threshold-flaky.

### Status: still BLOCKED, but converging

This is the third real, verified performance fix in this investigation
(Ruling 9: clone removal, ~0% effect alone; Ruling 10: play/undo→
hypothetical_window_code, 4-5.6x nodes, depth 4→7; Ruling 12: check_end's
has_legal_move, ~1.84x nodes, depth 7→8 best case). Each has been smaller
in absolute-depth-ply terms than the last, consistent with alpha-beta's
exponential cost curve — going from depth 8 to depth 10 (2 more plies)
likely needs another ~3-4x node throughput at this position's branching
factor, roughly the same order of magnitude as what Ruling 10 alone
delivered, not what Ruling 12 alone delivered. Not proposing what that
next fix should be or attempting one on my own judgment, per instructions
— reporting the real numbers and stopping here.

**Did not commit anything.** The gate does not pass, so none of this
investigation's commits (Ruling 9 clone removal, Ruling 10's
`hypothetical_window_code` rewrite, Ruling 11's `max_candidates` tuning,
Ruling 12's `has_legal_move` fix, or the benchmark test itself) have been
made yet, per the "commit once it passes" instruction that's held
throughout this investigation. Working tree currently holds `src/rules.rs`
(Ruling 10 + Ruling 12), `src/search.rs` (the benchmark test +
`max_candidates: 14` + harmless leftover `&mut b` reborrow sites from
Ruling 9), and `src/eval.rs` (one harmless leftover reborrow site) all
uncommitted.

---

## Update: Ruling 13 — `generate_with_patterns` fusion, gate still short of 10

Coordinator asked for two things before prescribing anything further: (1)
a fresh cost breakdown on the current (post-Ruling-12) code, since
`check_end`'s ~41% chunk being mostly gone shifts every other share, and
(2) an effective-branching-factor estimate from real per-depth node
counts, to size how much more throughput depth 10 actually needs. Then,
only if the fresh data still pointed at the `is_legal`/`order_score`
`hypothetical_window_code` redundancy as the largest remaining chunk,
apply a specific fusion fix they'd worked out — otherwise stop and report.

### Fresh cost breakdown (same instrumentation style, temp counters/timers, fully removed after — verified via `grep -rn "DBG_\|dbg_t\|dbg-bf" src/*.rs`, no matches)

Averaged over the 7 full-time-budget seeds (0, 1, 3, 5, 6, 8, 9):

| phase | share of node time (post-Ruling-12) | share before Ruling 12 |
|---|---|---|
| `score_order_and_truncate` (`order_score`) | **~45.1%** | ~36.2% |
| `generate()` (`is_legal`-driven) | ~26.5% | ~12.4% |
| `play`/`undo` | ~14.8% | ~8.8% |
| `check_end` | ~7.3% | ~41.3% |
| `sort_unstable_by` | ~1.9% | (folded into order_score's old number) |
| TT probe + store | ~0.33% | ~0.3% |
| `eval::evaluate` | ~0.05% | ~0.03% |
| unaccounted | ~4% | ~0.9% |

`check_end` dropped from the largest single item (41.3%) to one of the
smallest (7.3%), exactly as Ruling 12 intended. **`order_score` is now
unambiguously the single largest cost (45.1%), with `generate`/`is_legal`
second (26.5%) — combined, 71.6% of node time**, confirming the
coordinator's hypothesis before applying anything.

### Branching factor

Logged cumulative `ctx.nodes` at the moment each iterative-deepening
depth completed, for the two positions that reached depth 8 pre-fusion
(seeds 1 and 6). Geometric mean of the depth-over-depth node-count ratio,
using the more stable depths 4-8 (excluding the noisy depth-1/2 iterations
where node counts are single or low double digits):

- Seed 1: ratios 1.50x, 1.75x, 2.72x, 1.45x, 2.83x → geometric mean ≈ **2.0x per ply**
- Seed 6: ratios 2.03x, 2.53x, 2.24x, 1.51x, 2.91x → geometric mean ≈ **2.2x per ply**

So roughly **2.0-2.2x more nodes needed per additional ply** at this
position's effective branching factor once move ordering has kicked in.
Reaching depth 10 from a depth-8 baseline (2 more plies) needs roughly
2.1² ≈ **4.4x more total throughput** — squarely in Ruling-10-sized
territory, not a small tweak's worth, which matched the coordinator's own
expectation going in.

### Applied the fusion, with one correctness fix to the proposed code

Implemented `rules::generate_with_patterns` and threaded it through
`order_score`/`score_order_and_truncate`/`negamax`/`root_search` as
specified — with one deliberate deviation from the exact proposed code,
flagged here rather than silently changed:

**The proposed `generate_with_patterns` skipped axis-`Pat` computation
entirely for a capturing candidate** (`if n > 0 { out.push((i,
[Pat::default(); 4])); continue; }`). I traced this against the *original*
`order_score` and found it would have been a real behavior change: the
original computes `me_five`/`me_open_four`/`opp_threat` *before* checking
whether the move captures, so a move that both captures a pair *and*
completes a five currently scores `ORD_FIVE` (900,000), correctly
outranking an ordinary capture. `Pat::default()` has `flags = 0`, so
feeding a capturing candidate default Pats would have silently dropped
that priority — the same move would score as a mere capture
(`ORD_CAPTURE_BASE`, 500,000) instead, a move-ordering quality regression
for a real (if narrow) case. Fixed by always computing the 4-axis `Pat`s
in `generate_with_patterns` regardless of capture status — this exactly
mirrors what the *original* `order_score` already unconditionally did for
its own me-side loop (it never short-circuited on captures before that
loop; only the *legality* check, i.e. what counts as a candidate at all,
skips the free-three question for captures). This preserves the original
scoring priority exactly, at the cost of not saving the (small, since
captures are rare among candidates) HWC cost for capturing candidates —
the actual target redundancy (the *duplicate* me-side computation between
`is_legal` and `order_score`) is still fully eliminated either way, since
that's what mattered for the win.

Files touched: `src/rules.rs` (`generate_with_patterns`, additive — the
existing `generate`/`is_legal`/`count_free_threes`/`has_legal_move` are
untouched, still used by `check_end`), `src/search.rs` (`order_score`
takes `me_pats: [Pat; 4]` instead of computing them; `score_order_and_truncate`
takes `&[(Idx, [Pat; 4])]`; `negamax` and `root_search` call
`generate_with_patterns` instead of `generate`; 3 existing direct-call
tests of `order_score` updated with a small test-local `me_pats_for`
helper that reproduces what `generate_with_patterns` now computes, so
those tests still exercise the exact scoring logic they did before).

### Verification

- `cargo build --release`: clean.
- `cargo test --release --no-run`: clean (compiler caught the 3 test
  call sites needing the new `me_pats` argument; fixed as above).
- `cargo test` (debug, full suite): **38 passed**, including
  `find_best_move_is_deterministic` (node counts identical between two
  runs of the same position) and `find_best_move_extends_a_three_into_an_open_four`
  (exercises real tactical scoring through the changed `order_score`
  path) — no behavior change from this restructuring.
- `cargo clippy --release --all-targets`: no errors.
- `cargo test --release` (full suite): 37 passed, only the benchmark gate
  failed.

### Real numbers: further improvement, still short of 10

```
benchmark seed 0: depth 4 elapsed 401.25ms nodes 32256
benchmark seed 1: depth 9 elapsed 406.82ms nodes 25600
benchmark seed 2: depth 0 elapsed 0.12ms nodes 0
benchmark seed 3: depth 6 elapsed 406.55ms nodes 29184
benchmark seed 4: depth 3 elapsed 31.52ms nodes 2533
benchmark seed 5: depth 5 elapsed 407.29ms nodes 30720
benchmark seed 6: depth 8 elapsed 407.11ms nodes 28160
benchmark seed 7: depth 2 elapsed 22.03ms nodes 2350
benchmark seed 8: depth 5 elapsed 404.48ms nodes 25088
benchmark seed 9: depth 4 elapsed 403.87ms nodes 36352
```

Per-seed, before this fusion vs. after:

| seed | depth before | depth after | nodes before | nodes after |
|---|---|---|---|---|
| 0 | 4 | 4 | 27648 | 32256 |
| 1 | 8 | **9** | 22016 | 25600 |
| 3 | 5 | **6** | 24064 | 29184 |
| 5 | 4 | **5** | 26112 | 30720 |
| 6 | 8 | 8 | 25600 | 28160 |
| 8 | 5 | 5 | 21504 | 25088 |
| 9 | 4 | 4 | 31744 | 36352 |

Node counts up ~10-21% across the board (smaller than Ruling 10/12's
multi-x jumps, as expected — this redundancy was real but always a
fraction of the node, not the dominant cost by itself). **Three of seven
full-budget positions gained +1 ply** (seeds 1, 3, 5). **Best case is now
depth 9** — the closest this investigation has gotten. Ran the benchmark
3 times total; the depth profile `[4, 9, 0, 6, 3, 5, 8, 2, 5, 4]` was
identical every run — stable, not borderline-flaky. Average elapsed
~289ms (R15 passes comfortably). True minimum is still 0 (seed 2's
pre-existing immediate-win-shortcut case) / 2 excluding it (seed 7, exits
early via forced-win regardless of throughput). **R14 (depth ≥ 10) still
fails everywhere.**

### Status: still BLOCKED, one ply away from best case meeting the bar

Depth progression across this whole investigation's best case: 4 → 7
(Ruling 10) → 8 (Ruling 12) → 9 (Ruling 13). The gap is narrowing quickly
in ply terms even though each fix's node-throughput multiplier has been
smaller than the last (Ruling 10: 4-5.6x; Ruling 12: ~1.84x; Ruling 13:
~1.10-1.21x) — consistent with the earlier branching-factor estimate
(~4.4x needed for 2 more plies from a depth-8 baseline; Ruling 12 + 13
together delivered roughly 1.84 × 1.15 ≈ 2.1x, so per that estimate
another ~2x is still needed to reliably clear depth 10 everywhere, not
just on the best-case positions). Not proposing what that next fix should
be or attempting one on my own judgment.

**Did not commit anything.** Gate still fails. Working tree now holds
`src/rules.rs` (Ruling 10 + 12 + 13), `src/search.rs` (benchmark test +
`max_candidates: 14` + Ruling 13's `order_score`/`score_order_and_truncate`/
`negamax`/`root_search` changes + updated tests + harmless leftover
reborrow sites), and `src/eval.rs` (one harmless leftover reborrow site),
all uncommitted.

---

## Update: Ruling 14 — four early-exit eliminations, gate still short at depth 9

Coordinator proposed four more "stop computing something once its result
is already determined" eliminations against `order_score` and
`generate_with_patterns`, explicitly asking me to verify each against
`order_score`'s existing priority cascade by hand before applying, and to
flag rather than apply anything I found unsafe.

### Safety verification (done before touching any code)

1. **Skip the `opp`-perspective loop entirely when `me_five` is true**:
   safe. `me_five` is fully decidable from the already-in-hand `me_pats`
   array alone (no board access) — and `if me_five { return ORD_FIVE; }`
   is checked before `opp_threat`/`me_open_four`/anything else, so once
   `me_five` is known true, the return value is fixed regardless of what
   the `opp` loop would have found.
2. **Break the `opp` loop as soon as `opp_threat` is found true**: safe,
   **but only given #1 is applied first**. `opp_threat` is checked before
   `me_open_four` and the static-gain fallback, so once true, the result
   is fixed at `ORD_BLOCK` regardless of any remaining axis's `opp` *or*
   `me` data. The dependency: this is only sound because `me_five` was
   already fully resolved from the *complete* `me_pats` array before this
   loop starts — breaking early on a partial scan would risk missing a
   later axis's five and wrongly returning `ORD_BLOCK` instead of the
   higher-priority `ORD_FIVE`. Implemented both together as one pass over
   `me_pats` (deciding `me_five`, `me_open_four`, `me_static_gain` — all
   derivable from `me_pats` alone, no reason to interleave with the `opp`
   loop) followed by a separate `opp` loop that breaks on the first hit.
3. **Thread the capture count through instead of calling `captures_of`
   twice**: safe and mechanical — `generate_with_patterns` already
   computes `b.captures_of(i, p)` per candidate; `order_score` was calling
   `b.captures_of(mv, me)` again for the exact same `(mv, me)`. Only `n`
   (the count) is used by `order_score`, not the captured-cells array, so
   only `n` needed threading through.
4. **Break `generate_with_patterns`'s free-three loop once the count
   hits 2**: safe **only when `n == 0`** (no capture). A capturing
   candidate is pushed to the output unconditionally regardless of
   free-three count (spec §7.1), and — per the fix I made in Ruling 13 —
   its `pats` array genuinely gets read later by `order_score`, so it
   must stay complete. A *non*-capturing candidate that hits 2 free-threes
   is provably illegal and gets discarded, so nothing ever reads the rest
   of its `pats` — safe to stop early there. Gated the break on `n == 0`
   specifically to avoid re-introducing the same "capture + five" bug
   class I caught in Ruling 13's review of the original fusion proposal.

All four verified safe (with #2 and #4 each carrying one necessary
precondition, both correctly threaded through). No search algorithm
change — purely stopping computation whose result can no longer affect
the answer.

### What changed

`src/search.rs`: `order_score` restructured into two passes (a `me_pats`
pass deciding `me_five`/`me_open_four`/`me_static_gain`, returning early
on `me_five`; then an `opp` loop that breaks on the first threat found)
and now takes `captures: usize` instead of `me: Player` (no longer needed
— `me` was only ever used for the now-eliminated `captures_of` call).
`score_order_and_truncate`'s candidate type gained the capture count:
`&[(Idx, [Pat; 4], usize)]`. 3 direct-call tests of `order_score` updated
to pass a `captures` argument (computed via `b.captures_of(...).1`,
matching what `generate_with_patterns` would have produced for those
positions — none of the test setups involve an actual capture, so this
is `0` in all three, same as before).

`src/rules.rs`: `generate_with_patterns`'s output tuple gained the
capture count (`Vec<(Idx, [Pat; 4], usize)>`); its axis loop now breaks
early when `n == 0 && free_three_count >= 2`.

Left `score_order_and_truncate`'s own separate `b.captures_of(mv, me)`
call in its `retain` step untouched — that's a different redundancy
(only exercised when facing an opponent's forcing threat, a much smaller
fraction of nodes) that the coordinator didn't ask about and that would
need restructuring `scored`'s type to fix; flagging it here rather than
expanding scope on my own initiative.

### Verification

- `cargo build --release`: clean.
- `cargo test --release --no-run`: clean.
- `cargo test` (debug, full suite): **38 passed**.
- `cargo clippy --release --all-targets`: no errors.
- `cargo test --release` (full suite): 37 passed, only the benchmark gate
  failed.

### Real numbers: smaller win this round, no depth change

```
benchmark seed 0: depth 4 elapsed 402.14ms nodes 34304
benchmark seed 1: depth 9 elapsed 410.23ms nodes 26624
benchmark seed 2: depth 0 elapsed 0.12ms nodes 0
benchmark seed 3: depth 6 elapsed 400.99ms nodes 30720
benchmark seed 4: depth 3 elapsed 29.67ms nodes 2533
benchmark seed 5: depth 5 elapsed 401.08ms nodes 31232
benchmark seed 6: depth 8 elapsed 400.09ms nodes 27648
benchmark seed 7: depth 2 elapsed 21.28ms nodes 2350
benchmark seed 8: depth 5 elapsed 401.07ms nodes 26624
benchmark seed 9: depth 4 elapsed 402.97ms nodes 38912
```

Node counts up ~2-7% across the full-budget positions (seed 0: 32256→34304,
seed 1: 25600→26624, seed 3: 29184→30720, seed 5: 30720→31232, seed 6:
28160→27648 [flat/noise], seed 8: 25088→26624, seed 9: 36352→38912) —
real, but visibly smaller than every prior round's win, as expected: these
are narrower eliminations (skip-once-decided, not eliminate-a-whole-
redundant-pass). **No position gained an additional ply this round** —
the depth profile `[4, 9, 0, 6, 3, 5, 8, 2, 5, 4]` is identical to before
this round's changes. Ran the benchmark 3 times total; the profile was
bit-for-bit identical every run — stable, not borderline. Average elapsed
~287ms (R15 passes comfortably). **Best case remains depth 9. R14 (depth
≥ 10) still fails everywhere.**

Consistent with the branching-factor estimate from the previous update
(~2.0-2.2x nodes needed per ply): a ~1.02-1.07x node increase this round
is far short of what any position needed to cross into one more ply.

### Status: still BLOCKED, at the closest point this investigation has reached

Depth progression, full investigation: 4 (baseline) → 7 (Ruling 10) → 8
(Ruling 12) → 9 (Ruling 13) → 9 (Ruling 14, no further ply gained). Eight
rounds in, with real, independently-verified fixes at nearly every step
(two of which — the capture+five interaction in Ruling 13's fusion, and
this round's #2/#4 preconditions — were subtle enough that applying the
coordinator's proposal literally would have introduced a real, if narrow,
correctness bug; both were caught by hand-verification before applying,
not after). The remaining gap to depth 10 is 1 ply on the two
best-performing positions (seeds 1's depth 9, needing ~2x more nodes to
reach depth 10 at the ~2.0-2.2x branching factor) and further on the
others. I don't have a fifth safe, provably-equivalent redundancy left to
identify in the code I've examined — everything found so far has been
applied. Reporting the real numbers rather than guessing at what's left.

**Did not commit anything.** Gate still fails, so per the standing rule
held throughout this whole investigation, nothing is committed. Working
tree holds `src/rules.rs` (Ruling 10 + 12 + 13 + 14), `src/search.rs`
(benchmark test + `max_candidates: 14` + Ruling 13 + Ruling 14's
`order_score` restructuring + updated tests + harmless leftover reborrow
sites), and `src/eval.rs` (one harmless leftover reborrow site), all
uncommitted.

---

## Update: Ruling 15 — benchmark aggregation fix + `max_candidates` sweep, new structural finding

Two asks: (1) fix a real bug in the benchmark test's `min_depth`
aggregation (folding `depth_reached == 0` — the pre-existing
immediate-win shortcut correctly firing — into the minimum, which would
fail the gate even with a perfect search); (2) since no more free
redundancy elimination is available, try the spec's own accepted
mechanism for the depth/breadth tradeoff: lower `max_candidates` further
(10, then 8), checking the benchmark and the two correctness guardrails
(`find_best_move_is_deterministic`, `find_best_move_extends_a_three_into_an_open_four`)
at each value.

### 1. Benchmark aggregation fix — applied exactly as specified

```rust
if stats.depth_reached > 0 {
    min_depth = min_depth.min(stats.depth_reached);
}
```

Applied in `src/search.rs`'s `benchmark_gate_depth_and_time`, nothing
else changed. With this fix alone (still at `max_candidates: 14`),
`min_depth` changed from the spurious `0` to `2` — real, but still
failing.

### 2. `max_candidates` sweep — and a second instance of the same shortcut

| value | depth profile `[s0..s9]` | min_depth (fixed agg.) | full suite incl. guardrails |
|---|---|---|---|
| 14 (baseline) | `[4,9,0,6,3,5,8,2,5,4]` | 2 | pass |
| 10 | `[5,9,0,7,3,5,8,2,6,5]` | 2 | pass (38/38, incl. both guardrails) |
| 8 | `[5,9,0,6,3,5,8,2,6,5]` | 2 | pass (38/38, incl. both guardrails) |

Depths *improved* at 10 and 8 on several positions relative to 14 (e.g.
seed 0: 4→5, seed 3: 6→7 at 10) — consistent with the spec's own
rationale that a tighter cap trades completeness for depth, and matches
Ruling 11's earlier finding that this lever is genuinely net-positive at
this position density. But **`min_depth` did not move at all** — stuck
at exactly `2` across all three values.

**Root cause: seed 7 hits the exact same shortcut as seed 2, one
iteration later.** `find_best_move`'s iterative-deepening loop (spec
§9.5) has *two* immediate-win shortcuts, not one:

```rust
// before searching at all (produces depth_reached == 0):
if matches!(end, GameEnd::Win(_)) { return (mv, SearchStats { depth_reached: 0, ... }); }

// mid-iterative-deepening, after completing a depth (src/search.rs:599):
if last_score >= WIN - 1000 {
    break;   // depth_reached == whatever depth just confirmed the win
}
```

Seed 7 hits the *second* one: it completes depth 2, finds a near-WIN
score there, and correctly stops — deepening further cannot improve on a
proven forced win, per the same design principle as the depth-0 case.
Evidence this is genuinely score-driven, not throughput-limited: elapsed
time was 18.7ms → 13.4ms → (not re-measured at lower values) of the
400ms budget, and node count *shrank* monotonically as `max_candidates`
dropped (2350 → 1730 → 1058) while depth stayed exactly 2 every time —
a throughput-limited position would show elapsed climbing toward 400ms
and nodes staying roughly flat as candidates shrink, not both figures
shrinking while depth holds still. **No value of `max_candidates` can
move this position's reported depth**, because it isn't limited by
search breadth or budget at all — the search is *finding the actual best
answer* faster with fewer candidates to consider, and correctly
declining to search deeper once it has a proven win.

I did not sweep further (6, 4, ...) after confirming this at two
consecutive values (10 and 8) with a matching source-code explanation —
per this investigation's own "measure, don't guess further once the
mechanism is understood" discipline, continuing to lower the value
without addressing this specific blocker would only re-confirm the same
result while adding real risk: pushing `max_candidates` low enough to
start missing genuine tactical lines (the failure mode the two guardrail
tests exist to catch) for no chance of moving `min_depth`, since the
blocker isn't candidate-breadth-shaped at all.

**Reverted to `max_candidates: 14`** in both `SearchConfig::default()`
and the benchmark's `cfg` — the last value with a clean, established
track record (Ruling 11) and no advantage from the deeper sweep given
this finding. Verified clean: `cargo build --release`, `cargo clippy
--release --all-targets` (no errors), `cargo test` (38/38, debug),
`cargo test --release` (37/38, only the benchmark gate — unchanged
mechanism as every prior round).

### Current numbers (max_candidates=14, with the aggregation fix)

Depth profile: `[4, 9, 0, 6, 3, 5, 8, 2, 5, 4]` (seeds 0-9). `min_depth`
(excluding the depth-0 case per the fix) = **2**, driven entirely by seed
7's own genuine forced-win-found-at-depth-2 result. Average elapsed still
comfortably under 400ms (R15 passes). **R14 (depth ≥ 10 minimum) still
fails** — not because of throughput, but because the benchmark's own
10-position corpus (generated once, via seeded random walks, spec §14)
happens to include at least one position (seed 7, and arguably seed 4
similarly — elapsed 30ms/29-31ms range, well under budget, also likely
hitting the same shortcut at depth 3) where the *correct, optimal* search
result is a shallow, proven answer, and the benchmark's assertion doesn't
yet distinguish "shallow because correct" from "shallow because slow" for
any case beyond the literal depth-0 one.

### Status: BLOCKED — the gap is no longer a throughput problem

Eight-plus rounds of this investigation found and fixed five distinct
real performance issues (Rulings 9-14) that took best-case depth from 4
to 9, entirely legitimately. This round's finding is different in kind:
it's not a sixth thing to speed up. Two (at least) of the benchmark's 10
generated positions have a *provably optimal* shallow answer, and no
amount of search speed changes that — a faster engine finds the same
proven win faster, not deeper. Whether the fix belongs in the benchmark's
`min_depth` aggregation (extending the same "don't penalize a correctly-
early-terminated position" logic from the depth-0 shortcut to the
score-threshold shortcut) or in regenerating/filtering the benchmark's
random-walk corpus to avoid landing on such positions at all is a
judgment call about what R14 is actually meant to measure — not
something I'm deciding unilaterally, consistent with every prior
escalation in this investigation.

**Did not commit anything.** Gate still fails. Working tree holds the
benchmark aggregation fix (`src/search.rs`) plus everything from Rulings
9-14 (`src/rules.rs`, `src/search.rs`, `src/eval.rs`), `max_candidates`
back at 14 in both places, all uncommitted.
