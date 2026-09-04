# Task 9 Report — `search.rs`: transposition table and core negamax

## Status: DONE

## Resolution (Ruling 4, from coordinator)

The coordinator checked the spec directly (`docs/superpowers/specs/2026-09-01-gomoku-design.md:537`):
"Replacement policy: depth-preferred — overwrite only if the new entry's depth is greater
than or equal to the stored one, or the stored key differs." That is word-for-word what the
brief's Step 3 `store()` already implements. **The brief's Step 1 test was the defective
half** — both its second-store scenario (storing a shallower same-key entry) and its inline
comment ("same key always updates") contradict the spec's actual policy. Standard
depth-preferred TT design agrees with the spec/implementation: a shallower re-store must not
clobber a deeper entry for the same position.

Ruling: keep `TranspositionTable::store` exactly as the brief's Step 3 specifies (unchanged).
Replaced only the test's second half with a corrected version that asserts (a) a shallower
same-key store does NOT overwrite a deeper entry, and (b) an equal-or-deeper same-key store
DOES overwrite. First half of the test (initial store + probe assertions) left untouched.

This was a plan-text bug in the test, not a transcription error on my part — confirmed by
diffing my file against the brief's literal text before escalating (see "The contradiction"
section below, still preserved for the record).

Final test run (all 30 project tests, including both search tests):
```
$ cargo test
...
test search::tests::tt_round_trip_and_replacement_policy ... ok
test search::tests::negamax_recognizes_an_immediate_win ... ok
...
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.74s
```

`cargo build --release` succeeds (dead_code warnings only, as the brief predicted — nothing
public is consumed outside the module yet).

Committed as `783f334` — "feat: transposition table and core negamax with alpha-beta"
(the brief's exact Step 6 message), touching only `src/search.rs` and `src/main.rs` per the
brief's file list.

---

## Original escalation (superseded by Ruling 4 above, kept for the record)

## Status: NEEDS_CONTEXT (blocked on a contradiction inside the brief itself)

## What I did

1. Verified upstream interfaces before writing anything: `Board::{new, play, undo, to_move}`,
   `board::idx`, `Player::other`, `rules::{generate, check_end, GameEnd}` (confirmed
   `check_end`'s `Win` variant is always computed from `p = b.to_move.other()`, matching the
   brief's doc-comment claim), `eval::{evaluate, WIN}`, `patterns::PatternTable::build`. All
   present and matching the brief's signatures exactly.
2. Created `src/search.rs` with Step 1's test code, transcribed verbatim from the brief
   (`Bound`, `TtEntry`, `TranspositionTable` struct shell, `SearchConfig` + `Default`,
   `SearchStats`, and the two `#[cfg(test)]` tests).
3. Added `mod search;` to `src/main.rs`.
4. Confirmed RED: `cargo test search::` failed to compile with exactly the errors the brief
   predicted (`TranspositionTable::new`/`SearchCtx`/`negamax` not found).
5. Added Step 3's implementation verbatim (`TranspositionTable::{new, probe, store, clear}`,
   `Default for TranspositionTable`, `SearchCtx`, `negamax`).
6. Ran `cargo test search::` to confirm GREEN — **one test failed**. This is not a transcription
   mistake; I diffed my file against the brief and confirmed byte-for-byte match on the
   relevant lines (see below). The brief's own test and the brief's own implementation
   contradict each other.

## The contradiction

Brief lines 93-100 (test, verbatim):
```rust
// shallower entry with the SAME key must still overwrite (key match
// always allows replacement, per §9.6's "or the stored key differs"
// clause read the other way: same key always updates).
tt.store(
    12345,
    TtEntry { key: 12345, score: 99, mv: idx(4, 4), depth: 1, bound: Bound::Exact },
);
assert_eq!(tt.probe(12345).expect("still there").score, 99);
```
This stores a **shallower** entry (`depth: 1`) at the **same key** (`12345`) as an
already-stored `depth: 4` entry, and asserts the shallower entry **wins**.

Brief lines 170-177 (implementation, verbatim):
```rust
pub fn store(&mut self, key: u64, e: TtEntry) {
    let i = (key as usize) & self.mask;
    if let Some(slot) = self.entries.get_mut(i) {
        if e.depth >= slot.depth || slot.key != key {
            *slot = e;
        }
    }
}
```
Evaluated for the second `store` call: `e.depth (1) >= slot.depth (4)` is `false`;
`slot.key (12345) != key (12345)` is `false` (same key). `false || false = false`, so the
slot is **not** overwritten — `probe(12345)` still returns the depth-4 entry with
`score: 77`, not `99`.

This matches the observed failure exactly:
```
thread 'search::tests::tt_round_trip_and_replacement_policy' panicked at src/search.rs:204:9:
assertion `left == right` failed
  left: 77
 right: 99
```

The task prompt I was given explicitly reinforces the *implementation's* semantics as the
intended one ("`TranspositionTable::store`'s replacement policy (only overwrite if the new
entry is at least as deep as what's stored, OR the stored key differs)"), which is also what
the doc comment above `store` in the brief says, and is the standard, sound depth-preferred TT
replacement policy (a shallow re-store should not clobber a valuable deep entry for the *same*
position). But the test's own inline comment argues the opposite: "same key always updates."
The two halves of the brief cannot both be satisfied by any single implementation of `store`
that also keeps the first assertion (`depth: 4` entry stored into an empty slot) passing.

I did not want to silently "fix" the test to match the implementation (or vice versa) for
core TT logic that Tasks 10-12 depend on, since that's a design decision, not a typo I'm
confident about — e.g. maybe the *implementation* should instead be `e.depth >= slot.depth ||
e.key == slot.key` — i.e. "always update on exact key match regardless of depth (fresher
info for the exact same position), only depth-gate when it's actually a different position
colliding into the same slot" is a plausible *alternate* design too, just the opposite of
what's documented. Both readings are defensible engine designs; only one matches both halves
of the brief, and it's neither as literally written.

## TDD Evidence

### RED (compile failure, as predicted by the brief)
```
$ cargo test search:: -- --nocapture
error[E0599]: no associated function or constant named `new` found for struct `search::TranspositionTable` ...
error[E0425]: cannot find function `negamax` in this scope ...
error: could not compile `gomoku` (bin "gomoku" test) due to 4 previous errors
```

### GREEN attempt (1 of 2 tests passes; the replacement-policy test fails)
```
$ cargo test search:: -- --nocapture
running 2 tests
thread 'search::tests::tt_round_trip_and_replacement_policy' panicked at src/search.rs:204:9:
assertion `left == right` failed
  left: 77
 right: 99
test search::tests::tt_round_trip_and_replacement_policy ... FAILED
test search::tests::negamax_recognizes_an_immediate_win ... ok

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 28 filtered out; finished in 0.03s
```

`negamax_recognizes_an_immediate_win` passes as-is — no concerns with `negamax` itself.

## Files changed (uncommitted — work left in this state pending a decision)
- `src/search.rs` (new) — Step 1 tests + Step 3 implementation, both transcribed verbatim
  from the brief.
- `src/main.rs` — added `mod search;`.

No commit was made. Per the task instructions I should not guess on core TT semantics that
later tasks build on, so I'm stopping here rather than picking a resolution unilaterally.

## Requested decision

One of (at minimum):
1. The test's second `store` call should use `depth: 4` (or higher), not `depth: 1` — i.e.
   the test's comment/intent ("shallower entry ... must still overwrite") is the bug, and the
   implementation as given is correct depth-preferred TT semantics. This is what I'd guess if
   forced to pick, since it matches the doc comment, the interface note, and the parent task
   prompt's emphasis — but it requires deviating from the brief's literal Step-1 test text.
2. The implementation's condition should be `e.depth >= slot.depth || e.key == slot.key` (or
   equivalent) — always refresh an exact-key hit regardless of depth, only depth-gate true
   collisions — making the test's literal expectation correct instead.

I did not implement either resolution; I stopped once the contradiction was confirmed with
real command output, since it changes what "correct" looks like for `store`, which Tasks
10-12 depend on.
