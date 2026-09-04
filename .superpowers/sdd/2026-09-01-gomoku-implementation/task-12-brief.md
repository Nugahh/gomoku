### Task 12: `search.rs` — pruning, extensions, forced-response shortcut; mate and determinism tests

**Files:**
- Modify: `src/search.rs`

**Interfaces:**
- Consumes: everything from Tasks 9-11.
- Produces: the finished `search.rs` — no new public items, but `negamax`'s signature changes (gains an `extensions_used: u8` parameter), so every call site is updated in this task.

**Implementation note:** this task changes `negamax`'s signature to thread a threat-extension counter through the recursion, and extracts a small `score_order_and_truncate` helper so `negamax` and `root_search` share the exact same candidate-scoring-and-filtering logic (they were near-duplicates already after Task 11; adding a third piece of shared logic — the forced-response filter below — makes keeping them in sync by hand too risky to leave duplicated).

- [ ] **Step 1: Widen `order_score`'s opponent-threat detection to include closed fours**

Spec §9.5's threat extension triggers on "the side to move faces a four", not only an *open* four. Task 10's `order_score` currently only flags `F_FIVE | F_OPEN_FOUR` for the opponent-threat check (`opp_threat`), which this task's threat-extension and forced-response logic both reuse — so it needs widening first. In `src/search.rs`:

Change the import line:

```rust
use crate::patterns::{F_FIVE, F_FOUR, F_OPEN_FOUR};
```

In `order_score`, change:

```rust
        let pat_opp = pt.get(b.hypothetical_window_code(mv, d, opp));
        if pat_opp.flags & (F_FIVE | F_OPEN_FOUR) != 0 {
            opp_threat = true;
        }
```

to:

```rust
        let pat_opp = pt.get(b.hypothetical_window_code(mv, d, opp));
        if pat_opp.flags & (F_FIVE | F_OPEN_FOUR | F_FOUR) != 0 {
            opp_threat = true;
        }
```

Run: `cargo test --lib search:: -- --nocapture` — expect all existing tests still pass (this only widens what counts as urgent, it doesn't change any already-tested scenario's outcome).

- [ ] **Step 2: Write the failing tests — a multi-ply forced win, and determinism**

Add to `src/search.rs`'s test module:

```rust
    #[test]
    fn find_best_move_extends_a_three_into_an_open_four() {
        // Black has an open three at (3,5)-(5,5). Playing either open end
        // creates an open four, which is an unstoppable win in 2 more
        // plies (White can only block one end). This is legal — the move
        // creates a FOUR directly, not two THREEs, so the double-three
        // rule never applies to it.
        let pt = PatternTable::build();
        let mut tt = TranspositionTable::new();
        let mut b = Board::new();
        for x in 3..6 {
            b.to_move = Player::Black;
            b.play(idx(x, 5), &pt);
        }
        b.to_move = Player::Black;
        let cfg = SearchConfig { max_depth: 6, time_budget_ms: 400, max_candidates: 20 };
        let (mv, stats) = find_best_move(&mut b, &cfg, &pt, &mut tt);
        assert!(
            mv == idx(6, 5) || mv == idx(2, 5),
            "expected one of the two open-four completions, got {mv:?}"
        );
        assert!(
            stats.root_scores.iter().any(|&(m, s)| m == mv && s > WIN - 100),
            "expected a near-forced-win score for the chosen move"
        );
    }

    #[test]
    fn find_best_move_is_deterministic() {
        // A generous time budget relative to a shallow fixed depth means
        // the clock never actually cuts the search short in either run —
        // this isolates the assertion to the algorithm's own determinism,
        // not wall-clock jitter between two runs.
        let pt = PatternTable::build();
        let cfg = SearchConfig { max_depth: 4, time_budget_ms: 5_000, max_candidates: 20 };
        let setup: [(usize, usize, Player); 4] = [
            (9, 9, Player::Black),
            (9, 10, Player::White),
            (10, 9, Player::Black),
            (8, 8, Player::White),
        ];

        let mut b1 = Board::new();
        for &(x, y, p) in &setup {
            b1.to_move = p;
            b1.play(idx(x, y), &pt);
        }
        b1.to_move = Player::Black;
        let mut tt1 = TranspositionTable::new();
        let (mv1, stats1) = find_best_move(&mut b1, &cfg, &pt, &mut tt1);

        let mut b2 = Board::new();
        for &(x, y, p) in &setup {
            b2.to_move = p;
            b2.play(idx(x, y), &pt);
        }
        b2.to_move = Player::Black;
        let mut tt2 = TranspositionTable::new();
        let (mv2, stats2) = find_best_move(&mut b2, &cfg, &pt, &mut tt2);

        assert_eq!(mv1, mv2, "identical input must produce an identical move");
        assert_eq!(stats1.depth_reached, stats2.depth_reached);
        assert_eq!(stats1.nodes, stats2.nodes, "no source of randomness should affect node count at a fixed, comfortably-met depth");
    }
```

- [ ] **Step 3: Run test to verify it fails or behaves incorrectly**

Run: `cargo test --lib search:: -- --nocapture`
Expected: the two new tests likely compile (nothing new is referenced yet) but may already pass by luck, or may not — this task's real point is the *code* below, which the next step adds. If both new tests already pass before Step 4, that's fine; proceed to Step 4 anyway, since the pruning/extension code is part of this task's required deliverable regardless (spec §9.5 lists it as mandatory), and Step 6 re-verifies everything together.

- [ ] **Step 4: Extract shared candidate scoring, add threat extension and LMR to `negamax`, add the immediate-win shortcut to `find_best_move`**

Add this helper to `src/search.rs`, above `negamax`:

```rust
/// Scores, sorts, and truncates candidates — shared between `negamax` and
/// `root_search` so the forced-response filter below can't drift out of
/// sync between them. If the opponent has a four-or-better threat (the
/// highest-scoring candidate is at or above `ORD_BLOCK`), only moves that
/// answer it are kept, before the general `max_candidates` cap is applied
/// (spec §9.5's forced-block shortcut, implemented by reusing ordering
/// scores already computed here rather than a second board scan).
fn score_order_and_truncate(
    b: &Board,
    ctx: &SearchCtx,
    candidates: &[Idx],
    tt_move: Option<Idx>,
    killers_here: (Idx, Idx),
) -> Vec<(i32, Idx)> {
    let me = b.to_move;
    let opp = me.other();
    let mut scored: Vec<(i32, Idx)> = candidates
        .iter()
        .map(|&mv| {
            let s = order_score(b, ctx.pt, mv, me, opp, tt_move, killers_here, &ctx.history);
            (s, mv)
        })
        .collect();
    scored.sort_unstable_by(|a, bb| bb.0.cmp(&a.0));

    if scored.first().map(|&(s, _)| s >= ORD_BLOCK).unwrap_or(false) {
        scored.retain(|&(s, _)| s >= ORD_BLOCK);
    }
    scored.truncate(ctx.cfg.max_candidates);
    scored
}
```

Replace `negamax` in its entirety with:

```rust
/// Negamax with fail-soft alpha-beta (spec §9.2), late move reductions and
/// a capped threat extension (spec §9.5). `extensions_used` counts threat
/// extensions already applied along this line of the tree — capped at 4 so
/// a long forcing sequence can't blow the time budget. Returns a score
/// from the perspective of `b.to_move` at the node this call was invoked
/// on. `check_end` can only ever report a win for the player who just
/// moved (verified in Task 7), so the `GameEnd::Win(_)` wildcard below is
/// exhaustive in practice, not just defensively so.
#[allow(clippy::too_many_arguments)]
fn negamax(
    b: &mut Board,
    ctx: &mut SearchCtx,
    depth: u8,
    alpha: i32,
    beta: i32,
    ply: u8,
    extensions_used: u8,
) -> i32 {
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

    let orig_alpha = alpha;
    let mut alpha = alpha;

    ctx.tt_probes += 1;
    let mut tt_move = None;
    if let Some(e) = ctx.tt.probe(b.zobrist) {
        ctx.tt_hits += 1;
        tt_move = Some(e.mv);
        if e.depth >= depth {
            match e.bound {
                Bound::Exact => return e.score,
                Bound::Lower if e.score >= beta => return e.score,
                Bound::Upper if e.score <= alpha => return e.score,
                _ => {}
            }
        }
    }

    let mut candidates = Vec::new();
    rules::generate(b, b.to_move, ctx.pt, &mut candidates);
    if candidates.is_empty() {
        return 0;
    }

    let killers_here = ctx
        .killers
        .get(ply as usize)
        .map(|k| (k[0], k[1]))
        .unwrap_or((Idx::MAX, Idx::MAX));
    let scored = score_order_and_truncate(b, ctx, &candidates, tt_move, killers_here);

    let facing_threat = scored.first().map(|&(s, _)| s >= ORD_BLOCK).unwrap_or(false);
    let extend = facing_threat && extensions_used < 4;
    let child_extensions = if extend { extensions_used + 1 } else { extensions_used };
    let extra_depth: u8 = u8::from(extend);

    let mut best = i32::MIN + 1;
    let mut best_move = None;
    for (i, &(s, mv)) in scored.iter().enumerate() {
        let is_forcing = s >= ORD_CAPTURE_BASE;
        let u = b.play(mv, ctx.pt);
        let end = rules::check_end(b, mv, ctx.pt);
        let score = match end {
            GameEnd::Win(_) => WIN - ply as i32 - 1,
            GameEnd::Draw => 0,
            GameEnd::None => {
                let reduction: u8 = if depth >= 3 && !is_forcing {
                    if i >= 16 {
                        2
                    } else if i >= 8 {
                        1
                    } else {
                        0
                    }
                } else {
                    0
                };
                let full_child_depth = depth - 1 + extra_depth;
                let reduced_depth = full_child_depth.saturating_sub(reduction);
                let mut sc = -negamax(b, ctx, reduced_depth, -beta, -alpha, ply + 1, child_extensions);
                if reduction > 0 && sc > alpha {
                    sc = -negamax(b, ctx, full_child_depth, -beta, -alpha, ply + 1, child_extensions);
                }
                sc
            }
        };
        b.undo(&u);

        if ctx.aborted {
            return 0;
        }
        if score > best {
            best = score;
            best_move = Some(mv);
        }
        if best > alpha {
            alpha = best;
        }
        if alpha >= beta {
            if let Some(k) = ctx.killers.get_mut(ply as usize) {
                if k[0] != mv {
                    k[1] = k[0];
                    k[0] = mv;
                }
            }
            if let Some(slot) = ctx.history.get_mut(mv as usize) {
                *slot += (depth as i32) * (depth as i32);
            }
            break;
        }
    }

    if let Some(bm) = best_move {
        let bound = if best <= orig_alpha {
            Bound::Upper
        } else if best >= beta {
            Bound::Lower
        } else {
            Bound::Exact
        };
        ctx.tt.store(
            b.zobrist,
            TtEntry { key: b.zobrist, score: best, mv: bm, depth, bound },
        );
    }

    best
}
```

Replace `root_search`'s scoring block (the part between generating `candidates` and the `let mut alpha = window_lo;` line from Task 11) with a call to the shared helper, and update its three `negamax` calls to pass `0` for `extensions_used`:

```rust
    let me = b.to_move;
    let opp = me.other();
    let tt_move = ctx.tt.probe(b.zobrist).map(|e| e.mv);
    let scored = score_order_and_truncate(b, ctx, &candidates, tt_move, (Idx::MAX, Idx::MAX));

    let Some(&(_, mut best_move)) = scored.first() else {
        return None;
    };
    let mut alpha = window_lo;
    let mut best = i32::MIN + 1;
    let mut root_scores = Vec::new();

    for (i, &(_, mv)) in scored.iter().enumerate() {
        let u = b.play(mv, ctx.pt);
        let end = rules::check_end(b, mv, ctx.pt);
        let score = match end {
            GameEnd::Win(_) => WIN - 1,
            GameEnd::Draw => 0,
            GameEnd::None if i == 0 => -negamax(b, ctx, depth - 1, -window_hi, -alpha, 1, 0),
            GameEnd::None => {
                let null_score = -negamax(b, ctx, depth - 1, -alpha - 1, -alpha, 1, 0);
                if null_score > alpha && null_score < window_hi {
                    -negamax(b, ctx, depth - 1, -window_hi, -alpha, 1, 0)
                } else {
                    null_score
                }
            }
        };
```

(the rest of `root_search`, from `b.undo(&u);` onward, is unchanged from Task 11).

Update Task 9's test, `negamax_recognizes_an_immediate_win`, to pass the new parameter:

```rust
        let score = negamax(&mut b, &mut ctx, 1, -WIN, WIN, 0, 0);
```

Finally, add the immediate-win shortcut to `find_best_move` (spec §9.5), right after the `if candidates.is_empty() { ... }` guard from Task 11 and before `let mut ctx = SearchCtx::new(...)`:

```rust
    // Immediate win shortcut (spec §9.5): `check_end` already accounts for
    // breakability (spec §7.3), so if it reports a win here, that's a true,
    // unbreakable win — searching further cannot do better.
    for &mv in &candidates {
        let u = b.play(mv, pt);
        let end = rules::check_end(b, mv, pt);
        b.undo(&u);
        if matches!(end, GameEnd::Win(_)) {
            return (
                mv,
                SearchStats {
                    depth_reached: 0,
                    nodes: 0,
                    elapsed: start.elapsed(),
                    pv: vec![mv],
                    root_scores: vec![(mv, WIN)],
                    tt_hits: 0,
                    tt_probes: 0,
                },
            );
        }
    }
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --lib search:: -- --nocapture`
Expected: PASS, all 9 tests in `search.rs` green. If `find_best_move_is_deterministic` fails on node count specifically, check for any iteration order that depends on something other than the move's own `Idx` value or `HashMap`/`HashSet` usage (none should exist anywhere in this plan — everything is `Vec`-based) — a stray hash-based collection is the classic source of this exact flake.

- [ ] **Step 6: Commit**

```bash
git add src/search.rs
git commit -m "feat: late move reductions, threat extension, forced-response and immediate-win shortcuts"
```


---

