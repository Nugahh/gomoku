# Task 13 Report: Clippy compliance pass

## Status: DONE_WITH_CONCERNS

See "Scope boundary finding" below for the one concern — the task's own declared
scope (Interfaces section: `deny(indexing_slicing, unwrap_used, expect_used,
panic)`) is fully satisfied and verified clean, but the literal Step 6
verification command (`cargo clippy --all-targets -- -D warnings`, "must be
clean, exit code 0, no output") cannot be satisfied without also touching
~78 pre-existing, unrelated warnings (dead code from `main.rs` not yet wiring
up the engine modules — that's Task 15's job — plus a handful of unrelated
style lints in `search.rs`). I fixed everything in the four categories and
left the out-of-scope items untouched, per the brief's own escalation
instruction ("STOP and escalate when clippy flags something that doesn't fit
any of the four categories" — dead code and style lints don't fit A/B/C/D).

## What I implemented

### Category D: `player_slot`/`player_slot_mut` helpers (`src/board.rs`)

Added verbatim from the brief, right after `impl Player`:

```rust
#[allow(clippy::indexing_slicing)]
#[inline]
pub(crate) fn player_slot<T: Copy>(arr: [T; 2], p: Player) -> T { ... }

#[allow(clippy::indexing_slicing)]
#[inline]
pub(crate) fn player_slot_mut<T>(arr: &mut [T; 2], p: Player) -> &mut T { ... }
```

Used at every `Player`-indexed `[T; 2]` site:
- `board.rs`: `Board::new`'s setup loop (Category B, bounds-checked `.get_mut`, not `player_slot` — it indexes `[Cell; TOTAL]`, not a 2-element array); all four `adjust_axis_*`/`adjust_axis_*_dedup` functions' `self.acc[owner as usize] += ...` sites; `play`'s own-contribution loop (`self.acc[p as usize] += ...`); `play`'s captures bookkeeping (5-line block, both `self.captures` reads and the one write); and — not on the parent's snapshot list, found via clippy — `full_recompute_acc`'s two `acc[owner as usize] += ...` lines. That function lives in a `#[cfg(test)] impl Board` block, **not** inside `mod tests`, so the file-level test-module `#[allow]` doesn't cover it; it needed the same `player_slot_mut` treatment as production code.
- `rules.rs`: `five_is_breakable`'s `b.captures[opp as usize]`, `check_end`'s `b.captures[p as usize] >= 10`.
- `eval.rs`: `evaluate`'s two `cap_bonus(b.captures[...])` calls, and — also not on the snapshot list but flagged by clippy and present in the brief's own Category D table — the `acc` line: `(b.acc[me as usize] + me_bonus) - (b.acc[op as usize] + op_bonus)` → `(player_slot(b.acc, me) + me_bonus) - (player_slot(b.acc, op) + op_bonus)`.

### Category B: nested `.get().and_then()` for double-indexed tables (`src/board.rs`)

`Board::new`'s cell-setup loop: bounds-checked `.get_mut`, exactly as the brief specifies.

`play`'s three `self.key_cell[...][...]` sites (both branches' cell-write XOR, plus the capture-removal loop's XOR) → `self.key_cell.get(p as usize).and_then(|row| row.get(mv as usize)).copied().unwrap_or(0)` (with `*cc as usize` where the loop variable is a reference).

`play`'s captures-bookkeeping block: the brief flagged that `key_captures[p as usize].get(idx)` already had its *inner* index guarded by an earlier task but the *outer* `[p as usize]` was still raw — applied the same nested-get pattern to the outer index too, for both the old-index and new-index XOR lines.

### Category C: `.iter().take(n)` for production slicing, file-level test allows

`rules.rs`'s `five_is_breakable`: `captured[..n].iter().any(...)` → `captured.iter().take(n).any(...)`.

Added `#[allow(clippy::indexing_slicing)]` directly above `mod tests` in `board.rs`, `rules.rs`, `eval.rs`, `search.rs`. `search.rs`'s test module also uses three `.expect(...)` calls (`tt_round_trip_and_replacement_policy`) that trip the crate's `expect_used` deny-lint — not mentioned in the brief's Category C (which only names `indexing_slicing`), but the exact same justification applies verbatim ("a panic in test code ... is a legitimate test failure, not a production crash"), so I extended that one file's allow to `#[allow(clippy::indexing_slicing, clippy::expect_used)]`.

### Category A: `patterns.rs`

Added the file-level `#![allow(clippy::indexing_slicing)]` with the brief's exact justification comment, verbatim.

## Deviations from the parent's snapshot list

- **`full_recompute_acc`'s two `acc[owner as usize]` lines (`board.rs`, then lines 538–539)** — not on the snapshot list at all. It's `#[cfg(test)]` but outside `mod tests`, so it needed the same `player_slot_mut` fix as production code rather than a blanket test allow. Found via clippy, confirmed correct by reasoning about the `#[cfg(test)] impl Board` block's scope.
- **`eval.rs`'s `acc` line** (`(b.acc[me as usize] + me_bonus) - (b.acc[op as usize] + op_bonus)`) — omitted from the parent's "current accurate list" for eval.rs, but present in the brief's own Category D table and flagged by clippy at line 24. Fixed with `player_slot` exactly as the brief's table specifies.
- **`search.rs`'s three `.expect()` calls in test code trip `expect_used`**, not `indexing_slicing`. Not explicitly anticipated by Category C, but the same "test panic = legitimate test failure" reasoning applies; extended that file's test-module allow rather than touching the test bodies.
- Everything else matched the parent's snapshot exactly — no other surprises.

## Scope boundary finding (the reason for DONE_WITH_CONCERNS)

The task's Interfaces section defines the actual target precisely: "a crate
that actually satisfies the Global Constraints' `deny(clippy::indexing_slicing,
unwrap_used, expect_used, panic)`." That's what Cargo.toml's `[lints.clippy]`
section already sets to `deny` (so these fire as hard errors on *any* `cargo
clippy` invocation, `-D warnings` or not). I verified this exact scope is
**100% clean**:

```
$ cargo clippy --all-targets
    Checking gomoku v0.1.0 (...)
    <70 pre-existing dead-code/unused warnings only, zero errors>
cargo clippy exit code: 0
```

Zero `indexing_slicing`/`unwrap_used`/`expect_used`/`panic` errors remain,
confirmed both by exit code and by grepping for their exact diagnostic
strings ("indexing may panic", "slicing may panic", "used ... on an ...
value", "explicit panic") in the `-D warnings` output — 0 matches.

However, Step 6's literal instruction is `cargo clippy --all-targets -- -D
warnings`, "must be clean, exit code 0, no output." That flag promotes every
`warn`-level rustc/clippy lint to an error too — and `main.rs` is still the
Task-1 stub (`mod board; mod patterns; mod rules; mod eval; mod search; fn
main() { println!(...) }`), not yet wired up to any of these modules (that's
Task 15's UI-integration job). From the plain (non-test) binary's reachability
graph, nearly everything in these five files is legitimately dead code. Under
`-D warnings` that surfaces as 78 errors — all `dead_code`/`unused_variables`/
`unused_imports` plus four unrelated `search.rs` style lints
(`unnecessary_sort_by`, `manual_is_multiple_of`, `type_complexity`,
`question_mark`) pre-existing from Tasks 11/12. None of these are
`indexing_slicing`, `unwrap_used`, `expect_used`, or `panic` — confirmed by
category breakdown (0 matches for any of those four).

I verified none of this is new (i.e., not something my diff introduced) via
`git stash` — the exact same technique Task 10's report used for the same
kind of pre-existing-gap verification:

```
$ git stash && cargo clippy --all-targets -- -D warnings ...; echo $?
101
$ grep -c "^error" <output>
147
$ git stash pop
```

147 errors existed at the parent commit before any of my changes (this
matches the 71 real deny-lint errors — 68 `indexing_slicing` + 3
`expect_used` — plus 76 pre-existing dead-code/style errors, all promoted to
`error` only by `-D warnings`). After my fix: 78 errors, all pre-existing
dead-code/style, plus 2 *new* dead-code warnings for `player_slot`/
`player_slot_mut` themselves (unavoidable and correct — they're `pub(crate)`
helpers not yet called from any production code path outside the modules I
just edited, same as everything else in the crate right now). Net: 147 → 78,
exactly accounting for the 71 real fixes minus the 2 new dead-code entries
for the new helpers.

This is not one of the four categories, and per the brief's own escalation
rule ("STOP and escalate when clippy flags something that doesn't fit any of
the four categories") I did not invent a fifth pattern (e.g. a crate-level
`#[allow(dead_code)]`) to paper over it. Recommend either: (a) accept that
`-D warnings`-clean is a Task 15 milestone, not a Task 13 one, since Task 15
is what wires `main.rs` into these modules; or (b) if a fully `-D
warnings`-clean tree is wanted now, that's a small, well-scoped follow-up
(one crate-level `#[allow(dead_code)]` with a comment tying it to Task 15,
plus the four trivial `search.rs` style-lint rewrites and the two unused
import/variable fixes) — happy to do it if asked, but it's outside what this
task's Interfaces section says Task 13 produces.

## Clippy output

**Before (baseline, actual deny-lint scope only, no `-D warnings` — this is
what Cargo.toml's `deny` already enforces regardless of the flag):** 73
`error:` lines = 68 `indexing_slicing` (63 "indexing may panic" + 5 "slicing
may panic") + 3 `expect_used` ("used `expect()` on an `Option` value") + 2
"could not compile" summary lines. Representative sample:

```
error: indexing may panic
   --> src/board.rs:452:29
    |
452 |             self.zobrist ^= self.key_cell[p as usize][mv as usize];
    |                              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
    = help: consider using `.get(n)` or `.get_mut(n)` instead
    = note: requested on the command line with `-D clippy::indexing-slicing`
```

**After (this task's actual scope):**

```
$ cargo clippy --all-targets
    Checking gomoku v0.1.0 (...)
warning: <70 pre-existing dead-code/unused warnings, unrelated to this task>
    Finished `dev` profile [unoptimized + debuginfo] target(s)
$ echo $?
0
```

Zero errors, exit code 0. Every `indexing_slicing`/`unwrap_used`/
`expect_used`/`panic` finding from before is gone; nothing new of that kind
was introduced.

**After, with `-D warnings`:** exit code 101 (not 0), 78 errors — all
`dead_code`/`unused_variables`/`unused_imports`/style lints, zero
`indexing_slicing`/`unwrap_used`/`expect_used`/`panic` (see "Scope boundary
finding" above for the full accounting and the `git stash` proof these are
pre-existing).

## Test suite

```
$ cargo test
running 37 tests
test board::tests::... (11 tests) ... ok
test eval::tests::... (2 tests) ... ok
test patterns::tests::table_matches_naive_oracle_on_all_codes ... ok
test rules::tests::... (9 tests) ... ok
test search::tests::... (14 tests) ... ok

test result: ok. 37 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.92s
```

37/37 passed — the same 37 tests that existed before this task (I added no
tests and removed none; every edit is a mechanical substitution). `cargo
build --release` also succeeds cleanly (only the same pre-existing dead-code
warnings, no errors).

## Files changed

- `/home/fwong/Desktop/42/gomoku/.claude/worktrees/gomoku-impl/src/board.rs` — `player_slot`/`player_slot_mut` added; Categories B and D applied throughout (`Board::new`, all four `adjust_axis_*` functions, `play`, `full_recompute_acc`); file-level test-module allow.
- `/home/fwong/Desktop/42/gomoku/.claude/worktrees/gomoku-impl/src/rules.rs` — Category D (`five_is_breakable`, `check_end`) and Category C (`.iter().take(n)` in `five_is_breakable`); `player_slot` import added; file-level test-module allow.
- `/home/fwong/Desktop/42/gomoku/.claude/worktrees/gomoku-impl/src/eval.rs` — Category D (`evaluate`'s three sites); `player_slot` import added; file-level test-module allow.
- `/home/fwong/Desktop/42/gomoku/.claude/worktrees/gomoku-impl/src/search.rs` — file-level test-module allow only (`indexing_slicing` + `expect_used`); no production raw indexing found, confirmed by clippy.
- `/home/fwong/Desktop/42/gomoku/.claude/worktrees/gomoku-impl/src/patterns.rs` — Category A, file-level allow with justification comment, verbatim from the brief.

## Self-review findings

Read the full diff end-to-end (`git diff` on all five files) before
committing. Every hunk is a 1:1 mechanical substitution — no logic changed,
no control flow changed, no new behavior. The one thing I checked carefully
was the captures-bookkeeping rewrite in `play`:

```rust
*player_slot_mut(&mut self.captures, p) = player_slot(self.captures, p).saturating_add(n as u8);
```

`player_slot_mut` takes `&mut [T; 2]` and `player_slot` takes `[T; 2]` *by
value* (it's `T: Copy`, and `[u8; 2]` is `Copy`), so the RHS's
`player_slot(self.captures, p)` copies the array rather than borrowing it —
no borrow-checker conflict with the LHS's mutable borrow. Confirmed this
compiles (`cargo build` clean) and the round-trip/accumulator-drift tests
(which exercise this exact line on every capturing move across thousands of
random games) all pass.

No other concerns. No commented-out code, no stray `dbg!`/`println!`, no
`TODO`s introduced.

## Commit

```
86f359d chore: satisfy clippy indexing_slicing/unwrap_used/expect_used/panic across the engine modules
 5 files changed, 88 insertions(+), 24 deletions(-)
```
