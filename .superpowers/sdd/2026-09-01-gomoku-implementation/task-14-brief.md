### Task 14: Performance benchmark gate

**Files:**
- Modify: `src/search.rs`

**Interfaces:**
- Consumes: `search::{find_best_move, SearchConfig, TranspositionTable}` (Tasks 9-12), `rules::generate`, `board::Board`, `patterns::PatternTable`.
- Produces: a test that is, per spec §14, **the project's validation gate** — if it fails, the two hard numeric requirements (R14: depth >= 10, R15: under 0.5s/move) are not met, regardless of what any other test says.

This stays as one more test inside `search.rs`'s existing test module rather than a separate `tests/` integration file — an integration test would need `src/main.rs`'s modules exposed through a `src/lib.rs`, which nothing else in this plan needs (`ui.rs`/`main.rs`, added next, stay single-binary). Restructuring the whole crate into a lib+bin split for one test's sake is exactly the kind of unrequested structural change this plan avoids elsewhere.

- [ ] **Step 1: Write the benchmark test**

Add to `src/search.rs`'s test module (this one is written directly as the deliverable, not as a "failing test first" — there's no smaller increment to red-green here, the whole point is measuring the finished search):

```rust
    struct BenchXs(u64);
    impl BenchXs {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
    }

    /// Spec §14: the project's validation gate. Generates 10 varied
    /// middlegame-ish positions via seeded random legal-move walks (real
    /// recorded games aren't available yet — this project has no finished
    /// AI to record them with — but a legal, moderately dense position
    /// exercises the same branching factor a real middlegame would), then
    /// asserts the two hard numeric requirements: average move time under
    /// 400ms, minimum depth reached at least 10 (spec R14/R15).
    ///
    /// Debug builds are 10-50x slower than release for CPU-bound Rust and
    /// would fail this gate even with entirely correct code, so the
    /// assertions are skipped (with a printed note) unless run with
    /// `cargo test --release`.
    #[test]
    fn benchmark_gate_depth_and_time() {
        let pt = PatternTable::build();
        let cfg = SearchConfig { max_depth: 12, time_budget_ms: 400, max_candidates: 20 };

        let mut total_elapsed = Duration::ZERO;
        let mut min_depth = u8::MAX;
        let mut benchmarked = 0u32;

        for seed in 0..10u64 {
            let mut rng = BenchXs(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
            let mut b = Board::new();
            let mut tt = TranspositionTable::new();

            for _ in 0..24 {
                let mut candidates = Vec::new();
                rules::generate(&b, b.to_move, &pt, &mut candidates);
                if candidates.is_empty() {
                    break;
                }
                let pick = (rng.next() as usize) % candidates.len();
                let Some(&mv) = candidates.get(pick) else {
                    break;
                };
                b.play(mv, &pt);
            }

            let mut check_candidates = Vec::new();
            rules::generate(&b, b.to_move, &pt, &mut check_candidates);
            if check_candidates.is_empty() {
                continue; // the random walk ended the game; not a usable middlegame position
            }

            let (_mv, stats) = find_best_move(&mut b, &cfg, &pt, &mut tt);
            total_elapsed += stats.elapsed;
            min_depth = min_depth.min(stats.depth_reached);
            benchmarked += 1;

            if cfg!(debug_assertions) {
                eprintln!(
                    "benchmark seed {seed}: depth {} elapsed {:?} nodes {}",
                    stats.depth_reached, stats.elapsed, stats.nodes
                );
            }
        }

        assert!(benchmarked > 0, "no valid middlegame positions were generated to benchmark");

        if cfg!(debug_assertions) {
            eprintln!(
                "benchmark gate not enforced in a debug build — re-run with \
                 `cargo test --release --lib search::tests::benchmark_gate_depth_and_time -- --nocapture` \
                 to check it for real"
            );
            return;
        }

        let avg = total_elapsed / benchmarked;
        assert!(
            avg < Duration::from_millis(400),
            "average AI move time {avg:?} over {benchmarked} positions exceeds the 400ms target (spec §14, R15)"
        );
        assert!(
            min_depth >= 10,
            "minimum depth reached across {benchmarked} positions was {min_depth}, below the required 10 (spec §14, R14)"
        );
    }
```

- [ ] **Step 2: Run it in release mode**

Run: `cargo test --release --lib search::tests::benchmark_gate_depth_and_time -- --nocapture`
Expected: PASS, with printed per-seed diagnostics. If it fails, apply spec §14's tuning order, cheapest first, re-running this exact command after each change:

1. Lower `max_candidates` in this test's `cfg` (and reconsider the default in `search::SearchConfig::default`, Task 9) from 20 to 14.
2. Check `stats.tt_hits as f64 / stats.tt_probes as f64` on a slow position — if it's under 20%, the Zobrist incremental update (Task 5) likely has a bug; re-run Task 5's `play_undo_round_trip_restores_exact_state` test first, since a Zobrist bug there would silently corrupt TT lookups without failing that test (it only checks the *final* zobrist after a full undo, not that every intermediate value was a correct hash of that intermediate position — consider strengthening it if this happens).
3. Check move-ordering quality: instrument `negamax` to log whether the *first* candidate in `scored` caused the beta cutoff; it should for at least ~85% of cutoffs. A much lower rate points at `order_score`'s tier logic, not the search shape.
4. Only after 1-3: consider parallel search — explicitly out of scope for this plan (spec §16), would need its own design/spec pass first.

- [ ] **Step 3: Commit**

```bash
git add src/search.rs
git commit -m "test: performance benchmark gate — depth >=10 under 400ms average"
```


---

