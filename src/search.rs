#![forbid(unsafe_code)]

use crate::board::{Board, Idx, Player};
use crate::eval::{self, WIN};
use crate::patterns::PatternTable;
use crate::rules::{self, GameEnd};
use std::time::{Duration, Instant};

use crate::board::{Cell, DIRS, TOTAL};
use crate::patterns::{F_FIVE, F_OPEN_FOUR};

const ORD_TT: i32 = 1_000_000;
const ORD_FIVE: i32 = 900_000;
const ORD_BLOCK: i32 = 800_000;
const ORD_OPEN_FOUR: i32 = 700_000;
const ORD_CAPTURE_BASE: i32 = 500_000;
const ORD_KILLER1: i32 = 400_000;
const ORD_KILLER2: i32 = 390_000;
const ORD_HISTORY_CAP: i32 = 300_000;
const MAX_PLY: usize = 64;

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
    killers: [[Idx; 2]; MAX_PLY],
    history: [i32; TOTAL],
    tt_hits: u64,
    tt_probes: u64,
}

impl<'a> SearchCtx<'a> {
    fn new(pt: &'a PatternTable, tt: &'a mut TranspositionTable, cfg: &'a SearchConfig, deadline: Instant) -> Self {
        SearchCtx {
            pt,
            tt,
            cfg,
            nodes: 0,
            deadline,
            aborted: false,
            killers: [[Idx::MAX; 2]; MAX_PLY],
            history: [0; TOTAL],
            tt_hits: 0,
            tt_probes: 0,
        }
    }
}

/// Scores a candidate move for ordering (spec §9.3). Priorities 1-6 are
/// hard overrides (each returns immediately); priorities 7-8 (history and
/// static positional gain) are combined as the score for ordinary quiet
/// moves, since both are small relative to the overrides and either alone
/// is a weak signal.
#[allow(clippy::too_many_arguments)]
fn order_score(
    b: &Board,
    pt: &PatternTable,
    mv: Idx,
    me: Player,
    opp: Player,
    tt_mv: Option<Idx>,
    killers: (Idx, Idx),
    history: &[i32],
) -> i32 {
    if Some(mv) == tt_mv {
        return ORD_TT;
    }

    let mut me_five = false;
    let mut me_open_four = false;
    let mut opp_threat = false;
    let mut me_static_gain = 0i32;
    for &d in DIRS.iter() {
        let pat_me = pt.get(b.hypothetical_window_code(mv, d, me));
        if pat_me.flags & F_FIVE != 0 {
            me_five = true;
        }
        if pat_me.flags & F_OPEN_FOUR != 0 {
            me_open_four = true;
        }
        me_static_gain += pat_me.score;

        let pat_opp = pt.get(b.hypothetical_window_code(mv, d, opp));
        if pat_opp.flags & (F_FIVE | F_OPEN_FOUR) != 0 {
            opp_threat = true;
        }
    }

    if me_five {
        return ORD_FIVE;
    }
    if opp_threat {
        return ORD_BLOCK;
    }
    if me_open_four {
        return ORD_OPEN_FOUR;
    }

    let (_captured, n) = b.captures_of(mv, me);
    if n > 0 {
        return ORD_CAPTURE_BASE + 1_000 * (n as i32 / 2);
    }
    if mv == killers.0 {
        return ORD_KILLER1;
    }
    if mv == killers.1 {
        return ORD_KILLER2;
    }

    let hist = history.get(mv as usize).copied().unwrap_or(0).min(ORD_HISTORY_CAP);
    hist + me_static_gain
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
        return 0; // no legal moves; defensive fallback, see Task 9 notes
    }

    let me = b.to_move;
    let opp = me.other();
    let killers_here = ctx
        .killers
        .get(ply as usize)
        .map(|k| (k[0], k[1]))
        .unwrap_or((Idx::MAX, Idx::MAX));
    let mut scored: Vec<(i32, Idx)> = candidates
        .iter()
        .map(|&mv| {
            let s = order_score(b, ctx.pt, mv, me, opp, tt_move, killers_here, &ctx.history);
            (s, mv)
        })
        .collect();
    scored.sort_unstable_by(|a, bnd| bnd.0.cmp(&a.0));
    scored.truncate(ctx.cfg.max_candidates);

    let mut best = i32::MIN + 1;
    let mut best_move = None;
    for &(_, mv) in &scored {
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

        // depth-preferred replacement (spec §9.6): a SHALLOWER entry at the
        // same key must NOT overwrite a deeper one already stored.
        tt.store(
            12345,
            TtEntry { key: 12345, score: 99, mv: idx(4, 4), depth: 1, bound: Bound::Exact },
        );
        let still = tt.probe(12345).expect("still there");
        assert_eq!(still.score, 77, "a shallower same-key store must not overwrite a deeper entry");
        assert_eq!(still.depth, 4);

        // an equal-or-deeper entry at the same key DOES overwrite.
        tt.store(
            12345,
            TtEntry { key: 12345, score: 55, mv: idx(5, 5), depth: 4, bound: Bound::Exact },
        );
        assert_eq!(tt.probe(12345).expect("still there").score, 55);
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
        let mut ctx = SearchCtx::new(&pt, &mut tt, &cfg, far_deadline());
        // At depth 1, playing (8,5) wins immediately; negamax should return
        // a score very close to WIN (within a few ply of it).
        let score = negamax(&mut b, &mut ctx, 1, -WIN, WIN, 0);
        assert!(score > WIN - 10, "expected a near-WIN score, got {score}");
    }

    #[test]
    fn order_score_ranks_five_above_quiet_move() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        for x in 4..8 {
            b.to_move = Player::Black;
            b.play(idx(x, 5), &pt);
        }
        let history = [0i32; crate::board::TOTAL];
        let no_killers = (Idx::MAX, Idx::MAX);
        let winning_score = order_score(
            &b, &pt, idx(8, 5), Player::Black, Player::White, None, no_killers, &history,
        );
        let quiet_score = order_score(
            &b, &pt, idx(15, 15), Player::Black, Player::White, None, no_killers, &history,
        );
        assert_eq!(winning_score, ORD_FIVE);
        assert!(winning_score > quiet_score);
    }

    #[test]
    fn order_score_ranks_tt_move_above_everything() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        for x in 4..8 {
            b.to_move = Player::Black;
            b.play(idx(x, 5), &pt);
        }
        let history = [0i32; crate::board::TOTAL];
        let tt_move_score = order_score(
            &b, &pt, idx(15, 15), Player::Black, Player::White,
            Some(idx(15, 15)), (Idx::MAX, Idx::MAX), &history,
        );
        assert_eq!(tt_move_score, ORD_TT);
    }
}
