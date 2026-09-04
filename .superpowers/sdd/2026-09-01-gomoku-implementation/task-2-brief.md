### Task 2: `patterns.rs` — pattern lookup table

This is written before `board.rs` deliberately (spec §13, test #1: "write first" — it is the foundation both rules and heuristic build on, and it has zero dependencies on board state).

**Files:**
- Create: `src/patterns.rs`
- Modify: `src/main.rs` (add `mod patterns;`)
- Test: inline `#[cfg(test)] mod tests` in `src/patterns.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `patterns::{W, C, TABLE_SIZE, POW3, F_FIVE, F_OPEN_FOUR, F_FOUR, F_FREE_THREE, F_THREE, Pat, PatternTable}`, used by `board.rs` (Task 4+) to encode window codes and look up scores/flags.

- [ ] **Step 1: Write the failing test — naive oracle agreement on all 19683 codes**

This is the single most load-bearing test in the project (spec §5.5). Add to `src/patterns.rs`:

```rust
#![forbid(unsafe_code)]

pub const W: usize = 9;
pub const C: usize = 4;
pub const TABLE_SIZE: usize = 19_683; // 3^9
pub const POW3: [u32; W] = [1, 3, 9, 27, 81, 243, 729, 2187, 6561];

pub const F_FIVE: u8 = 1 << 0;
pub const F_OPEN_FOUR: u8 = 1 << 1;
pub const F_FOUR: u8 = 1 << 2;
pub const F_FREE_THREE: u8 = 1 << 3;
pub const F_THREE: u8 = 1 << 4;

#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub struct Pat {
    pub score: i32,
    pub flags: u8,
}

pub struct PatternTable {
    entries: Box<[Pat]>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decode a base-3 `code` into 9 trits, w[0] is the cell furthest in the
    /// negative direction, w[4] is the center, w[8] is furthest positive.
    fn decode(mut code: u32) -> [u8; W] {
        let mut w = [0u8; W];
        for slot in w.iter_mut() {
            *slot = (code % 3) as u8;
            code /= 3;
        }
        w
    }

    /// Deliberately naive, unoptimized oracle. Re-derives flags/score from
    /// scratch for a single window using straightforward scanning, so it
    /// shares no logic (and therefore no bugs) with `PatternTable::build`.
    fn naive_classify(w: [u8; W]) -> Pat {
        if w[C] != 1 {
            return Pat::default();
        }
        // contiguous run through the center
        let mut l = C;
        while l > 0 && w[l - 1] == 1 {
            l -= 1;
        }
        let mut r = C;
        while r < W - 1 && w[r + 1] == 1 {
            r += 1;
        }
        let n = r - l + 1;

        let mut flags = 0u8;
        if n >= 5 {
            flags |= F_FIVE;
        } else {
            let open_left = l == 0 || w[l - 1] == 0;
            let open_right = r == W - 1 || w[r + 1] == 0;
            if n == 4 {
                flags |= if open_left && open_right { F_OPEN_FOUR } else { F_FOUR };
            } else if n == 3 && (open_left || open_right) {
                flags |= F_THREE;
            }
        }

        // constructive four: fill one empty cell, check for a 5-run through C
        if flags & F_FIVE == 0 {
            for e in 0..W {
                if w[e] != 0 {
                    continue;
                }
                let mut w2 = w;
                w2[e] = 1;
                let mut l2 = C;
                while l2 > 0 && w2[l2 - 1] == 1 {
                    l2 -= 1;
                }
                let mut r2 = C;
                while r2 < W - 1 && w2[r2 + 1] == 1 {
                    r2 += 1;
                }
                if r2 - l2 + 1 >= 5 {
                    flags |= F_FOUR;
                    break;
                }
            }
        }

        // constructive free-three: fill one empty cell in 1..=7, check for
        // an *open* four (exactly 4, both ends empty) through the center
        if flags & (F_FIVE | F_FOUR | F_OPEN_FOUR) == 0 {
            for e in 1..=7 {
                if w[e] != 0 {
                    continue;
                }
                let mut w2 = w;
                w2[e] = 1;
                let mut l2 = C;
                while l2 > 0 && w2[l2 - 1] == 1 {
                    l2 -= 1;
                }
                let mut r2 = C;
                while r2 < W - 1 && w2[r2 + 1] == 1 {
                    r2 += 1;
                }
                let n2 = r2 - l2 + 1;
                let open_left2 = l2 == 0 || w2[l2 - 1] == 0;
                let open_right2 = r2 == W - 1 || w2[r2 + 1] == 0;
                if n2 == 4 && open_left2 && open_right2 {
                    flags |= F_FREE_THREE;
                    break;
                }
            }
        }

        let score = if flags & F_FIVE != 0 {
            10_000_000
        } else if flags & F_OPEN_FOUR != 0 {
            500_000
        } else if flags & F_FOUR != 0 {
            50_000
        } else if flags & F_FREE_THREE != 0 {
            20_000
        } else if flags & F_THREE != 0 {
            2_000
        } else if n == 2 {
            let open_left = l == 0 || w[l - 1] == 0;
            let open_right = r == W - 1 || w[r + 1] == 0;
            if open_left && open_right { 300 } else if open_left || open_right { 50 } else { 0 }
        } else if n == 1 {
            let open_left = l == 0 || w[l - 1] == 0;
            let open_right = r == W - 1 || w[r + 1] == 0;
            if open_left && open_right { 5 } else { 0 }
        } else {
            0
        };

        Pat { score, flags }
    }

    #[test]
    fn table_matches_naive_oracle_on_all_codes() {
        let table = PatternTable::build();
        for code in 0..TABLE_SIZE as u32 {
            let w = decode(code);
            let expected = naive_classify(w);
            let actual = table.get(code);
            assert_eq!(
                actual, expected,
                "mismatch at code {code} (window {w:?})"
            );
        }
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib patterns:: -- --nocapture`
Expected: FAIL to compile — `PatternTable::build` and `PatternTable::get` don't exist yet.

- [ ] **Step 3: Implement `PatternTable::build` and `get`**

Append to `src/patterns.rs`, above the `#[cfg(test)]` block:

```rust
impl PatternTable {
    /// Builds all 19,683 entries. Deterministic, called once at startup.
    pub fn build() -> Self {
        let mut entries = vec![Pat::default(); TABLE_SIZE].into_boxed_slice();
        for (code, slot) in entries.iter_mut().enumerate() {
            *slot = classify(decode_window(code as u32));
        }
        PatternTable { entries }
    }

    #[inline]
    pub fn get(&self, code: u32) -> Pat {
        // code is always < TABLE_SIZE by construction of the caller (board.rs
        // encodes exactly 9 trits). Bounds-checked indexing here is cheap
        // insurance and satisfies the crate's `indexing_slicing` deny lint
        // via a checked accessor rather than `[]`.
        self.entries
            .get(code as usize)
            .copied()
            .unwrap_or_default()
    }
}

fn decode_window(mut code: u32) -> [u8; W] {
    let mut w = [0u8; W];
    for slot in w.iter_mut() {
        *slot = (code % 3) as u8;
        code /= 3;
    }
    w
}

/// Run of same-value cells through the center, inclusive bounds `[l, r]`.
fn center_run(w: &[u8; W]) -> (usize, usize) {
    let mut l = C;
    while l > 0 && w[l - 1] == 1 {
        l -= 1;
    }
    let mut r = C;
    while r < W - 1 && w[r + 1] == 1 {
        r += 1;
    }
    (l, r)
}

fn classify(w: [u8; W]) -> Pat {
    if w[C] != 1 {
        return Pat::default();
    }

    let (l, r) = center_run(&w);
    let n = r - l + 1;
    let mut flags = 0u8;

    if n >= 5 {
        flags |= F_FIVE;
    } else {
        let open_left = l == 0 || w[l - 1] == 0;
        let open_right = r == W - 1 || w[r + 1] == 0;
        if n == 4 {
            flags |= if open_left && open_right { F_OPEN_FOUR } else { F_FOUR };
        } else if n == 3 && (open_left || open_right) {
            flags |= F_THREE;
        }
    }

    if flags & F_FIVE == 0 {
        for e in 0..W {
            if w[e] != 0 {
                continue;
            }
            let mut w2 = w;
            w2[e] = 1;
            let (l2, r2) = center_run(&w2);
            if r2 - l2 + 1 >= 5 {
                flags |= F_FOUR;
                break;
            }
        }
    }

    if flags & (F_FIVE | F_FOUR | F_OPEN_FOUR) == 0 {
        for e in 1..=7 {
            if w[e] != 0 {
                continue;
            }
            let mut w2 = w;
            w2[e] = 1;
            let (l2, r2) = center_run(&w2);
            let n2 = r2 - l2 + 1;
            let open_left2 = l2 == 0 || w2[l2 - 1] == 0;
            let open_right2 = r2 == W - 1 || w2[r2 + 1] == 0;
            if n2 == 4 && open_left2 && open_right2 {
                flags |= F_FREE_THREE;
                break;
            }
        }
    }

    let score = if flags & F_FIVE != 0 {
        10_000_000
    } else if flags & F_OPEN_FOUR != 0 {
        500_000
    } else if flags & F_FOUR != 0 {
        50_000
    } else if flags & F_FREE_THREE != 0 {
        20_000
    } else if flags & F_THREE != 0 {
        2_000
    } else if n == 2 {
        let open_left = l == 0 || w[l - 1] == 0;
        let open_right = r == W - 1 || w[r + 1] == 0;
        if open_left && open_right { 300 } else if open_left || open_right { 50 } else { 0 }
    } else if n == 1 {
        let open_left = l == 0 || w[l - 1] == 0;
        let open_right = r == W - 1 || w[r + 1] == 0;
        if open_left && open_right { 5 } else { 0 }
    } else {
        0
    };

    Pat { score, flags }
}
```

This is intentionally line-for-line structurally identical to the test's `naive_classify` — that duplication is not a bug to fix. The test's oracle must stay independent of the implementation for the test to mean anything; the plan's Global Constraints ban a test that imports the function it's testing.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib patterns:: -- --nocapture`
Expected: PASS — `table_matches_naive_oracle_on_all_codes` succeeds on all 19,683 codes.

- [ ] **Step 5: Wire the module into `main.rs`**

In `src/main.rs`, add near the top:

```rust
mod patterns;
```

Run: `cargo build --release` — expect it still compiles (module is unused so far; add `#[allow(dead_code)]` on the `patterns` mod line temporarily if the compiler warns-as-error anywhere, but plain `cargo build` only warns, it doesn't fail).

- [ ] **Step 6: Commit**

```bash
git add src/patterns.rs src/main.rs
git commit -m "feat: pattern lookup table with naive-oracle verification"
```


---

