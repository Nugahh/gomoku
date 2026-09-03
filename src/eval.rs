#![forbid(unsafe_code)]

use crate::board::{player_slot, Board, Player};

pub const WIN: i32 = 100_000_000;

/// Non-linear on purpose: the 4th pair captured is worth far more than the
/// 1st, because it puts the opponent one capture from losing outright and
/// makes every one of their fives breakable (spec §8.3).
pub const CAP_BONUS: [i32; 6] = [0, 4_000, 12_000, 30_000, 90_000, 10_000_000];

/// Score of the current position from `b.to_move`'s point of view
/// (negamax convention). `acc` already includes the incremental
/// vulnerability penalty (spec §8.2/§8.3); the capture bonus is applied
/// fresh here via array lookup, since it is a cheap O(1) function of the
/// already-incrementally-maintained `captures` counters, not itself
/// incremental.
#[inline]
pub fn evaluate(b: &Board) -> i32 {
    let me = b.to_move;
    let op = me.other();
    let me_bonus = cap_bonus(player_slot(b.captures, me));
    let op_bonus = cap_bonus(player_slot(b.captures, op));
    (player_slot(b.acc, me) + me_bonus) - (player_slot(b.acc, op) + op_bonus)
}

#[inline]
fn cap_bonus(stones_captured: u8) -> i32 {
    let pairs = (stones_captured / 2) as usize;
    CAP_BONUS
        .get(pairs)
        .copied()
        .unwrap_or_else(|| CAP_BONUS.iter().copied().last().unwrap_or(0))
}

#[allow(clippy::indexing_slicing)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::idx;
    use crate::patterns::PatternTable;
    use crate::rules;

    struct Xs(u64);
    impl Xs {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
    }

    #[test]
    fn evaluate_favors_the_player_with_more_captures() {
        let mut b = Board::new();
        b.captures[Player::Black as usize] = 4;
        b.to_move = Player::Black;
        let score_black_ahead = evaluate(&b);
        b.captures = [0, 0];
        b.captures[Player::White as usize] = 4;
        let score_white_ahead = evaluate(&b);
        assert!(score_black_ahead > score_white_ahead);
    }

    #[test]
    fn accumulator_never_drifts_from_full_recompute() {
        let pt = PatternTable::build();
        for seed in 0..200u64 {
            let mut rng = Xs(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
            let mut b = Board::new();
            for _ in 0..40 {
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
                let full = b.full_recompute_acc(&pt);
                assert_eq!(b.acc, full, "seed {seed}: accumulator drifted from full recompute");
            }
        }
        let _ = idx(0, 0); // silence an unused-import warning if idx ends up unused above
    }
}
