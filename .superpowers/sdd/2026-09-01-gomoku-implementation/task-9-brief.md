### Task 9: `search.rs` — transposition table and core negamax

**Files:**
- Create: `src/search.rs`
- Modify: `src/main.rs` (add `mod search;`)

**Interfaces:**
- Consumes: `board::{Board, Idx, Player}`, `rules::{generate, check_end, GameEnd}`, `eval::{evaluate, WIN}`, `patterns::PatternTable`.
- Produces: `search::{Bound, TtEntry, TranspositionTable, SearchConfig, SearchStats}` (public, per spec §9.1/§9.6) and a private `negamax` used internally by this task's test and extended by Tasks 10-12. `find_best_move` (the module's actual public entry point) is added in Task 11 — this task only lays its foundation.

- [ ] **Step 1: Write the failing tests — TT round-trip and a one-move-win position**

Create `src/search.rs`:

```rust
#![forbid(unsafe_code)]

use crate::board::{Board, Idx, Player};
use crate::eval::{self, WIN};
use crate::patterns::PatternTable;
use crate::rules::{self, GameEnd};
use std::time::{Duration, Instant};

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub enum Bound {
    #[default]
    Exact,
    Lower,
    Upper,
}

#[derive(Copy, Clone, Default)]
pub struct TtEntry {
    pub key: u64,
    pub score: i32,
    pub mv: Idx,
    pub depth: u8,
    pub bound: Bound,
}

pub struct TranspositionTable {
    entries: Vec<TtEntry>,
    mask: usize,
}

pub struct SearchConfig {
    pub max_depth: u8,
    pub time_budget_ms: u64,
    pub max_candidates: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        SearchConfig {
            max_depth: 12,
            time_budget_ms: 400,
            max_candidates: 20,
        }
    }
}

pub struct SearchStats {
    pub depth_reached: u8,
    pub nodes: u64,
    pub elapsed: Duration,
    pub pv: Vec<Idx>,
    pub root_scores: Vec<(Idx, i32)>,
    pub tt_hits: u64,
    pub tt_probes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::idx;

    fn far_deadline() -> Instant {
        Instant::now() + Duration::from_secs(30)
    }

    #[test]
    fn tt_round_trip_and_replacement_policy() {
        let mut tt = TranspositionTable::new();
        assert!(tt.probe(12345).is_none());
        tt.store(
            12345,
            TtEntry { key: 12345, score: 77, mv: idx(3, 3), depth: 4, bound: Bound::Exact },
        );
        let got = tt.probe(12345).expect("just stored");
        assert_eq!(got.score, 77);
        assert_eq!(got.depth, 4);

        // shallower entry with the SAME key must still overwrite (key match
        // always allows replacement, per §9.6's "or the stored key differs"
        // clause read the other way: same key always updates).
        tt.store(
            12345,
            TtEntry { key: 12345, score: 99, mv: idx(4, 4), depth: 1, bound: Bound::Exact },
        );
        assert_eq!(tt.probe(12345).expect("still there").score, 99);
    }

    #[test]
    fn negamax_recognizes_an_immediate_win() {
        let pt = PatternTable::build();
        let mut tt = TranspositionTable::new();
        let mut b = Board::new();
        for x in 4..8 {
            b.to_move = Player::Black;
            b.play(idx(x, 5), &pt);
        }
        b.to_move = Player::Black;
        let cfg = SearchConfig::default();
        let mut ctx = SearchCtx {
            pt: &pt,
            tt: &mut tt,
            cfg: &cfg,
            nodes: 0,
            deadline: far_deadline(),
            aborted: false,
        };
        // At depth 1, playing (8,5) wins immediately; negamax should return
        // a score very close to WIN (within a few ply of it).
        let score = negamax(&mut b, &mut ctx, 1, -WIN, WIN, 0);
        assert!(score > WIN - 10, "expected a near-WIN score, got {score}");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib search:: -- --nocapture`
Expected: FAIL to compile — `TranspositionTable::new/probe/store`, `SearchCtx`, `negamax` don't exist.

- [ ] **Step 3: Implement `TranspositionTable` and the core `negamax`**

Add to `src/search.rs`, above `#[cfg(test)]`:

```rust
impl TranspositionTable {
    /// Tries progressively smaller sizes with `try_reserve_exact` so an
    /// allocation failure degrades the table instead of panicking (spec
    /// §9.6, §11 robustness — R12 fails the whole project on any crash,
    /// OOM included).
    pub fn new() -> Self {
        for &bits in &[21usize, 18, 15] {
            let size = 1usize << bits;
            let mut v: Vec<TtEntry> = Vec::new();
            if v.try_reserve_exact(size).is_ok() {
                v.resize(size, TtEntry::default());
                return TranspositionTable { entries: v, mask: size - 1 };
            }
        }
        TranspositionTable { entries: vec![TtEntry::default(); 1], mask: 0 }
    }

    #[inline]
    pub fn probe(&self, key: u64) -> Option<TtEntry> {
        let e = self.entries.get((key as usize) & self.mask).copied()?;
        if e.key == key {
            Some(e)
        } else {
            None
        }
    }

    /// Depth-preferred replacement: only overwrite if the new entry is at
    /// least as deep as what's stored, or the stored slot holds a
    /// different position entirely (spec §9.6).
    pub fn store(&mut self, key: u64, e: TtEntry) {
        let i = (key as usize) & self.mask;
        if let Some(slot) = self.entries.get_mut(i) {
            if e.depth >= slot.depth || slot.key != key {
                *slot = e;
            }
        }
    }

    pub fn clear(&mut self) {
        for e in self.entries.iter_mut() {
            *e = TtEntry::default();
        }
    }
}

impl Default for TranspositionTable {
    fn default() -> Self {
        TranspositionTable::new()
    }
}

/// Per-search mutable context threaded through the recursion: the pattern
/// table and config are read-only, `tt`/`nodes`/`aborted` accumulate state
/// across the whole tree. Kept as one struct instead of separate
/// parameters so Tasks 10-12 can add fields (killers, history) without
/// changing every call site's argument list.
struct SearchCtx<'a> {
    pt: &'a PatternTable,
    tt: &'a mut TranspositionTable,
    cfg: &'a SearchConfig,
    nodes: u64,
    deadline: Instant,
    aborted: bool,
}

/// Negamax with fail-soft alpha-beta (spec §9.2). Returns a score from the
/// perspective of `b.to_move` at the node this call was invoked on.
/// `check_end` can only ever report a win for the player who just moved
/// (verified in Task 7 — every branch of `check_end` computes its `Win`
/// case from `p = b.to_move.other()`), so the wildcard in the match below
/// is exhaustive in practice, not just defensively so.
fn negamax(b: &mut Board, ctx: &mut SearchCtx, depth: u8, alpha: i32, beta: i32, ply: u8) -> i32 {
    ctx.nodes += 1;
    if ctx.nodes % 2048 == 0 && Instant::now() >= ctx.deadline {
        ctx.aborted = true;
    }
    if ctx.aborted {
        return 0;
    }

    if depth == 0 {
        return eval::evaluate(b);
    }

    let mut candidates = Vec::new();
    rules::generate(b, b.to_move, ctx.pt, &mut candidates);
    if candidates.is_empty() {
        return 0; // no legal moves; defensive fallback, see Task 9 notes
    }

    let mut best = i32::MIN + 1;
    let mut alpha = alpha;
    for &mv in &candidates {
        let u = b.play(mv, ctx.pt);
        let end = rules::check_end(b, mv, ctx.pt);
        let score = match end {
            GameEnd::Win(_) => WIN - ply as i32 - 1,
            GameEnd::Draw => 0,
            GameEnd::None => -negamax(b, ctx, depth - 1, -beta, -alpha, ply + 1),
        };
        b.undo(&u);

        if ctx.aborted {
            return 0;
        }
        if score > best {
            best = score;
        }
        if best > alpha {
            alpha = best;
        }
        if alpha >= beta {
            break;
        }
    }
    best
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib search:: -- --nocapture`
Expected: PASS, both tests green.

- [ ] **Step 5: Wire into `main.rs`**

```rust
mod search;
```

Run: `cargo build --release` — expect a `dead_code` warning (nothing public is used outside the module yet) but no error.

- [ ] **Step 6: Commit**

```bash
git add src/search.rs src/main.rs
git commit -m "feat: transposition table and core negamax with alpha-beta"
```


---

