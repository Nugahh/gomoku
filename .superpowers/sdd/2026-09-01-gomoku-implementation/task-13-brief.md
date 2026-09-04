### Task 13: Clippy compliance pass — closing the gap between the Global Constraints and the code written so far

**Files:**
- Modify: `src/patterns.rs`, `src/board.rs`, `src/rules.rs`, `src/eval.rs`, `src/search.rs`

**Interfaces:**
- Consumes: nothing new.
- Produces: a crate that actually satisfies the Global Constraints' `deny(clippy::indexing_slicing, unwrap_used, expect_used, panic)`, which several steps in Tasks 2, 4, 5, 7, and 8 did not fully honor — this task is the checkpoint that catches and fixes that gap before it compounds further. No task from here on should introduce a new raw `[]`/`[..]` access in production code without a similar justified exception. Also adds `board::{player_slot, player_slot_mut}`, used from here on by any code that indexes a `[T; 2]` array by `Player`.

**Why this task exists:** `clippy::indexing_slicing` flags *every* `[]` index or `[..n]` slice, including ones that are provably in-bounds by construction (a loop bounded to `0..W` indexing a `[T; W]` array). Tasks 2, 4, and 7 wrote several such accesses without noticing the lint would still flag them. This task is a dedicated pass to find and resolve every case, rather than leaving each later task to silently accumulate more.

- [ ] **Step 1: Run clippy and read the findings**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: a list of `deny`-level errors. The three categories below cover everything this plan is expected to have introduced through Task 12; if clippy reports something not covered by one of them, fix it the same way the nearest category does (bounds-checked accessor for production board/array code, or a scoped, justified `#[allow]` for a small array whose bounds are fixed by an enclosing loop and already covered by an exhaustive test).

**Category A — `patterns.rs`'s `classify`/`decode_window`:** every `w[i]`/`w2[i]` access indexes a local `[u8; 9]` whose only ever-used indices are `0..=8`, fixed by the enclosing loop ranges (`-4..=4` via a `+4` offset, `0..=8`, or `1..=7`), and the *entire* function is checked against a naive, independent oracle on all 19,683 possible inputs (Task 2's `table_matches_naive_oracle_on_all_codes` test) — a stronger correctness guarantee than the lint provides. Retrofitting every access to `.get()` here would hurt readability for no real safety gain. Add a scoped, justified exception instead. At the top of `src/patterns.rs`, change:

```rust
#![forbid(unsafe_code)]
```

to:

```rust
#![forbid(unsafe_code)]
#![allow(clippy::indexing_slicing)]
// Every index into the [u8; W] window arrays in this file is bounded by
// its enclosing loop range (0..W, or a +-4 offset from the center), never
// by external input, and the classification logic is exhaustively checked
// against an independent oracle on all 19,683 possible codes (see the
// `table_matches_naive_oracle_on_all_codes` test below). The
// indexing_slicing lint denied elsewhere in this crate (Cargo.toml)
// guards against unproven runtime bounds on board-sized data — it doesn't
// have anything to add here.
```

**Category B — raw indexing in `board.rs`'s production code:**

In `Board::new`, the setup loop currently writes `cells[idx(x, y) as usize] = Cell::Empty;` directly. Change it to:

```rust
        for y in 0..SIZE {
            for x in 0..SIZE {
                if let Some(slot) = cells.get_mut(idx(x, y) as usize) {
                    *slot = Cell::Empty;
                }
            }
        }
```

In `play` and anywhere else `self.key_cell[player as usize][idx as usize]` or `self.key_captures[player as usize][n as usize]` appears as a *double* raw index (the outer `[player as usize]` is always in range since `Player` is a 2-variant enum, but clippy doesn't know that; the inner index is exactly the case Task 5 already guarded with `.get()` in some places but not all), replace every remaining raw form with:

```rust
self.zobrist ^= self
    .key_cell
    .get(p as usize)
    .and_then(|row| row.get(mv as usize))
    .copied()
    .unwrap_or(0);
```

(substitute the right variable names — `p`/`mv`, `owner`/`cc`, etc. — at each of the handful of call sites in `play` that XOR a `key_cell` entry; where the loop variable is `cc: &Idx` rather than `Idx`, index with `.get(*cc as usize)`, dereferencing first — the compiler will reject a bare `cc as usize` there with a type error, which is the mechanical signal to add the `*`). This pattern (`.get(...).and_then(|row| row.get(...)).copied().unwrap_or(0)`) replaces every `key_cell[..][..]` access in the file.

**Category C — raw slicing in test and production code (`board.rs`, `rules.rs`):**

`captured[..n]` (an array slice) appears in Task 4's tests and in `rules.rs`'s `five_is_breakable` (production code). For **production** code, replace:

```rust
if captured[..n].iter().any(|c| alignment.contains(c)) {
```

with:

```rust
if captured.iter().take(n).any(|c| alignment.contains(c)) {
```

(`.iter().take(n)` reads the same n elements without ever forming a slice, so the lint has nothing to flag). Apply the same `.iter().take(n)` substitution to every other `arr[..n]` pattern found in production code.

For **test** code, add `#[allow(clippy::indexing_slicing)]` directly above the `mod tests` line in every file that has one (`patterns.rs` doesn't need this — it already has the file-level allow from Category A; `board.rs`, `rules.rs`, `eval.rs`, `search.rs` each need it):

```rust
#[allow(clippy::indexing_slicing)]
#[cfg(test)]
mod tests {
```

A panic in test code from an out-of-bounds slice is a legitimate test failure, not a production crash — the R12 "never crash" requirement this lint enforces elsewhere doesn't apply to `cargo test` runs the same way.

**Category D — `self.acc[player as usize]` / `self.captures[player as usize]` throughout `board.rs`, `rules.rs`, `eval.rs`:**

`Board::acc: [i32; 2]` and `Board::captures: [u8; 2]` (Task 4) are indexed by `Player as usize` at roughly a dozen production call sites: `board.rs`'s `adjust_axis_neighbors`, `adjust_axis_vuln`, and `play` (Task 5); `rules.rs`'s `check_end` and `five_is_breakable` (Task 7); `eval.rs`'s `evaluate` (Task 8). `Player` is a 2-variant enum, so `p as usize` is always exactly 0 or 1 — provably safe, same as Category A's window arrays — but retrofitting a dozen `+=`/`-=` sites deep in the accumulator's hot path to `.get_mut().unwrap_or(...)` would both hurt readability and be actively misleading: a fallback value for "player index out of range" describes a situation that cannot happen, so silently supplying one would hide a real bug instead of surfacing it (a raw index, by contrast, panics loudly and correctly on the impossible case).

Add two small, scoped-allow helper functions to `src/board.rs`, near the `Player` impl:

```rust
/// Indexes a `[T; 2]` array by `Player` without spreading a raw `[]`
/// through the accumulator's hot path (Tasks 5/7/8). `p as usize` can only
/// ever be 0 or 1 — the 2-variant enum makes it provably safe in a way
/// `clippy::indexing_slicing` can't see — so the raw index is confined to
/// these two functions instead of guarded (meaninglessly: there's no valid
/// fallback for an index that cannot occur) at every call site.
#[allow(clippy::indexing_slicing)]
#[inline]
pub(crate) fn player_slot<T: Copy>(arr: [T; 2], p: Player) -> T {
    match p {
        Player::Black => arr[0],
        Player::White => arr[1],
    }
}

#[allow(clippy::indexing_slicing)]
#[inline]
pub(crate) fn player_slot_mut<T>(arr: &mut [T; 2], p: Player) -> &mut T {
    match p {
        Player::Black => &mut arr[0],
        Player::White => &mut arr[1],
    }
}
```

Then replace every production occurrence. In `src/board.rs`:

| Function | Replace | With |
|---|---|---|
| `adjust_axis_neighbors` | `self.acc[owner as usize] += sign * score;` | `*player_slot_mut(&mut self.acc, owner) += sign * score;` |
| `adjust_axis_vuln` | `self.acc[owner as usize] += sign * score;` | `*player_slot_mut(&mut self.acc, owner) += sign * score;` |
| `play` (capture removal loop) | `self.acc[owner as usize] -= self.stone_window_score(*cc, d, owner, pt);` | `*player_slot_mut(&mut self.acc, owner) -= self.stone_window_score(*cc, d, owner, pt);` |
| `play` (own-contribution loop) | `self.acc[p as usize] += self.stone_window_score(mv, d, p, pt);` | `*player_slot_mut(&mut self.acc, p) += self.stone_window_score(mv, d, p, pt);` |
| `play` (captures bookkeeping, 3 lines) | `self.captures[p as usize].min(10)` / `self.captures[p as usize] = self.captures[p as usize].saturating_add(n as u8)` / `self.captures[p as usize].min(10)` | `player_slot(self.captures, p).min(10)` / `*player_slot_mut(&mut self.captures, p) = player_slot(self.captures, p).saturating_add(n as u8)` / `player_slot(self.captures, p).min(10)` |

In `src/rules.rs` (add `use crate::board::player_slot;` alongside the file's existing imports):

| Function | Replace | With |
|---|---|---|
| `five_is_breakable` | `let p_lost_stones = b.captures[opp as usize];` | `let p_lost_stones = player_slot(b.captures, opp);` |
| `check_end` | `if b.captures[p as usize] >= 10 {` | `if player_slot(b.captures, p) >= 10 {` |

In `src/eval.rs` (add `use crate::board::player_slot;`):

| Function | Replace | With |
|---|---|---|
| `evaluate` | `let me_bonus = cap_bonus(b.captures[me as usize]);` | `let me_bonus = cap_bonus(player_slot(b.captures, me));` |
| `evaluate` | `let op_bonus = cap_bonus(b.captures[op as usize]);` | `let op_bonus = cap_bonus(player_slot(b.captures, op));` |
| `evaluate` | `(b.acc[me as usize] + me_bonus) - (b.acc[op as usize] + op_bonus)` | `(player_slot(b.acc, me) + me_bonus) - (player_slot(b.acc, op) + op_bonus)` |

`ui.rs`'s `self.board.captures[Player::Black as usize]`-style reads in the status bar (Task 15) are **not** touched — that file carries its own file-level `#[allow(clippy::indexing_slicing, ...)]` per the Global Constraints' UI exemption, so nothing there needs `player_slot`.

- [ ] **Step 2: Re-run clippy to verify a clean pass**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: no output, exit code 0.

- [ ] **Step 3: Run the full test suite to confirm none of these mechanical edits changed behavior**

Run: `cargo test`
Expected: PASS, every test from Tasks 2-12 still green. These edits are all behavior-preserving substitutions (a bounds-checked accessor that's never actually out of bounds returns the exact same value as the raw index it replaces); a failure here means a substitution was miscopied, not a real behavior change.

- [ ] **Step 4: Commit**

```bash
git add src/patterns.rs src/board.rs src/rules.rs src/eval.rs src/search.rs
git commit -m "chore: satisfy clippy indexing_slicing/unwrap_used/expect_used/panic across the engine modules"
```


---

