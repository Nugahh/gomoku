#![forbid(unsafe_code)]

use crate::board::{idx, player_slot, Board, Cell, Idx, Player, DIRS, SIZE, TOTAL};
use crate::patterns::{PatternTable, F_FREE_THREE, F_FIVE};

/// Number of axes on which placing `p` at `mv` creates a free-three (spec
/// §7.2). Places the stone with a scratch `play`/`undo` (the accumulator
/// and Zobrist churn is wasted work here, but reusing the already-correct
/// `play`/`undo` is far less risky than a second bespoke place/remove path
/// — this is not on the search hot path, only on legality checks).
pub fn count_free_threes(b: &mut Board, mv: Idx, p: Player, pt: &PatternTable) -> u8 {
    let saved_to_move = b.to_move;
    b.to_move = p;
    let u = b.play(mv, pt);
    let mut count = 0u8;
    for &d in DIRS.iter() {
        let code = b.window_code_pub(mv, d, p);
        if pt.get(code).flags & F_FREE_THREE != 0 {
            count += 1;
        }
    }
    b.undo(&u);
    b.to_move = saved_to_move;
    count
}

/// `mv` must be an empty, in-bounds cell. Captures are checked before the
/// double-three rule — the subject states explicitly that introducing a
/// double-three by capturing a pair is allowed (spec §7.1, appendix VI.2
/// warning), so a capturing move is legal regardless of free-three count.
pub fn is_legal(b: &Board, mv: Idx, p: Player, pt: &PatternTable) -> bool {
    if b.get(mv) != Cell::Empty {
        return false;
    }
    let (_captured, n) = b.captures_of(mv, p);
    if n > 0 {
        return true;
    }
    let mut scratch = b.clone();
    count_free_threes(&mut scratch, mv, p, pt) < 2
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum GameEnd {
    None,
    Win(Player),
    Draw,
}

/// Walks outward from `last` in both signs of axis `d`, collecting every
/// contiguous cell belonging to `p`'s alignment through `last` (spec §7.3
/// step 2a).
fn collect_alignment(b: &Board, last: Idx, d: i16, p: Player) -> Vec<Idx> {
    let mut cells = vec![last];
    let mut i = last as i32 - d as i32;
    while i >= 0 && (i as usize) < TOTAL && b.get(i as Idx) == p.cell() {
        cells.push(i as Idx);
        i -= d as i32;
    }
    let mut i = last as i32 + d as i32;
    while i >= 0 && (i as usize) < TOTAL && b.get(i as Idx) == p.cell() {
        cells.push(i as Idx);
        i += d as i32;
    }
    cells
}

/// True if `p`'s five along `alignment` does *not* win outright — either
/// because `p` has already lost 4 pairs and the opponent has any capture
/// available (spec §7.3 step 2b), or because some legal opponent move
/// captures a stone that is part of `alignment` (step 2c).
fn five_is_breakable(b: &Board, p: Player, alignment: &[Idx], pt: &PatternTable) -> bool {
    let opp = p.other();
    let mut candidates = Vec::new();
    generate(b, opp, pt, &mut candidates);

    let p_lost_stones = player_slot(b.captures, opp);
    if p_lost_stones >= 8 {
        for &mv2 in &candidates {
            let (_c, n) = b.captures_of(mv2, opp);
            if n > 0 {
                return true;
            }
        }
    }

    for &mv2 in &candidates {
        let (captured, n) = b.captures_of(mv2, opp);
        if captured.iter().take(n).any(|c| alignment.contains(c)) {
            return true;
        }
    }
    false
}

/// Evaluates the game state right after `p = b.to_move.other()` played
/// `last`. Order of checks: win by capture (unconditional, spec R4) before
/// win by alignment (conditional on breakability, spec R6/R7), before draw.
/// Takes `&mut Board` to match this task's own signature contract with
/// `search.rs`'s call sites, even though this implementation never
/// mutates `b` — every helper it calls takes `&Board`.
pub fn check_end(b: &mut Board, last: Idx, pt: &PatternTable) -> GameEnd {
    let p = b.to_move.other();

    if player_slot(b.captures, p) >= 10 {
        return GameEnd::Win(p);
    }

    let mut any_five = false;
    for &d in DIRS.iter() {
        let code = b.window_code_pub(last, d, p);
        if pt.get(code).flags & F_FIVE == 0 {
            continue;
        }
        any_five = true;
        let alignment = collect_alignment(b, last, d, p);
        if !five_is_breakable(b, p, &alignment, pt) {
            return GameEnd::Win(p);
        }
    }
    if any_five {
        return GameEnd::None;
    }

    let mut candidates = Vec::new();
    generate(b, b.to_move, pt, &mut candidates);
    if candidates.is_empty() {
        return GameEnd::Draw;
    }

    GameEnd::None
}

/// All legal candidate moves for `p`: empty cells with at least one stone
/// within Chebyshev radius 2, filtered by `is_legal`. On an empty board,
/// only the center cell qualifies (spec §7.4).
pub fn generate(b: &Board, p: Player, pt: &PatternTable, out: &mut Vec<Idx>) {
    out.clear();
    if b.stone_count == 0 {
        let center = idx(SIZE / 2, SIZE / 2);
        out.push(center);
        return;
    }
    for y in 0..SIZE {
        for x in 0..SIZE {
            let i = idx(x, y);
            if b.get(i) == Cell::Empty && b.has_neighbor(i) && is_legal(b, i, p, pt) {
                out.push(i);
            }
        }
    }
}

#[allow(clippy::indexing_slicing)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_three_contiguous_diagonal_is_detected() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        b.to_move = Player::Black;
        // three in a diagonal row with open ends: (5,5) (6,6) (7,7)
        let u1 = play_raw(&mut b, idx(5, 5), Player::Black, &pt);
        let u2 = play_raw(&mut b, idx(6, 6), Player::Black, &pt);
        assert_eq!(count_free_threes(&mut b, idx(7, 7), Player::Black, &pt), 1);
        undo_raw(&mut b, u2);
        undo_raw(&mut b, u1);
    }

    #[test]
    fn free_three_gapped_form_is_detected() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        b.to_move = Player::Black;
        // . X . X X . horizontally: stones at (5,5) and (7,5), gap at (6,5),
        // playing at (6,5) completes ". X X X ." shape via the gapped rule.
        let u1 = play_raw(&mut b, idx(5, 5), Player::Black, &pt);
        let u2 = play_raw(&mut b, idx(7, 5), Player::Black, &pt);
        assert_eq!(count_free_threes(&mut b, idx(6, 5), Player::Black, &pt), 1);
        undo_raw(&mut b, u2);
        undo_raw(&mut b, u1);
    }

    #[test]
    fn double_three_is_illegal() {
        // spec appendix VI.2: two red stones placed so that playing `a`
        // creates two simultaneous free-threes.
        let pt = PatternTable::build();
        let mut b = Board::new();
        b.to_move = Player::Black;
        // horizontal three-to-be: (8,5) (9,5) [a=10,5] existing pair, open.
        let u1 = play_raw(&mut b, idx(8, 5), Player::Black, &pt);
        let u2 = play_raw(&mut b, idx(9, 5), Player::Black, &pt);
        // diagonal three-to-be sharing the same point a=(10,5):
        let u3 = play_raw(&mut b, idx(9, 4), Player::Black, &pt);
        let u4 = play_raw(&mut b, idx(11, 6), Player::Black, &pt);
        assert!(!is_legal(&b, idx(10, 5), Player::Black, &pt));
        undo_raw(&mut b, u4);
        undo_raw(&mut b, u3);
        undo_raw(&mut b, u2);
        undo_raw(&mut b, u1);
    }

    #[test]
    fn double_three_becomes_legal_when_one_arm_is_blocked() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        b.to_move = Player::Black;
        let u1 = play_raw(&mut b, idx(8, 5), Player::Black, &pt);
        let u2 = play_raw(&mut b, idx(9, 5), Player::Black, &pt);
        let u3 = play_raw(&mut b, idx(9, 4), Player::Black, &pt);
        let u4 = play_raw(&mut b, idx(11, 6), Player::Black, &pt);
        // block one of the two free-three arms with a white stone
        let u5 = play_raw(&mut b, idx(7, 5), Player::White, &pt);
        assert!(is_legal(&b, idx(10, 5), Player::Black, &pt));
        undo_raw(&mut b, u5);
        undo_raw(&mut b, u4);
        undo_raw(&mut b, u3);
        undo_raw(&mut b, u2);
        undo_raw(&mut b, u1);
    }

    #[test]
    fn double_three_by_capture_is_legal() {
        // spec §7.1/§9 (appendix warning): introducing a double-three by
        // capturing a pair is explicitly allowed. Build a position where
        // the move both captures a pair AND would otherwise be a
        // double-three.
        let pt = PatternTable::build();
        let mut b = Board::new();
        b.to_move = Player::Black;
        // two free-three arms for Black around (10,5), same as above:
        let u1 = play_raw(&mut b, idx(8, 5), Player::Black, &pt);
        let u2 = play_raw(&mut b, idx(9, 5), Player::Black, &pt);
        let u3 = play_raw(&mut b, idx(9, 4), Player::Black, &pt);
        let u4 = play_raw(&mut b, idx(11, 6), Player::Black, &pt);
        // a capturable White pair flanked by Black at (10,5) and an
        // existing Black stone two steps further down the same axis as one
        // arm, positioned off the double-three axes so it only adds a
        // capture, not a third free-three:
        let u5 = play_raw(&mut b, idx(10, 8), Player::Black, &pt);
        let u6 = play_raw(&mut b, idx(10, 6), Player::White, &pt);
        let u7 = play_raw(&mut b, idx(10, 7), Player::White, &pt);
        assert!(is_legal(&b, idx(10, 5), Player::Black, &pt));
        undo_raw(&mut b, u7);
        undo_raw(&mut b, u6);
        undo_raw(&mut b, u5);
        undo_raw(&mut b, u4);
        undo_raw(&mut b, u3);
        undo_raw(&mut b, u2);
        undo_raw(&mut b, u1);
    }

    #[test]
    fn generate_on_empty_board_returns_only_center() {
        let pt = PatternTable::build();
        let b = Board::new();
        let mut out = Vec::new();
        generate(&b, Player::Black, &pt, &mut out);
        assert_eq!(out, vec![idx(SIZE / 2, SIZE / 2)]);
    }

    #[test]
    fn generate_only_returns_cells_near_existing_stones() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        b.to_move = Player::Black;
        let _u = play_raw(&mut b, idx(9, 9), Player::Black, &pt);
        let mut out = Vec::new();
        generate(&b, Player::White, &pt, &mut out);
        assert!(!out.is_empty());
        for &mv in &out {
            let (x, y) = crate::board::to_xy(mv);
            let (cx, cy) = (9i32, 9i32);
            let dist = (x as i32 - cx).abs().max((y as i32 - cy).abs());
            assert!(dist <= 2, "candidate {mv:?} too far from the only stone");
        }
    }

    #[test]
    fn unbreakable_five_wins() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        for x in 4..8 {
            let _ = play_raw(&mut b, idx(x, 5), Player::Black, &pt);
        }
        b.to_move = Player::Black;
        b.play(idx(8, 5), &pt);
        let result = check_end(&mut b, idx(8, 5), &pt);
        assert_eq!(result, GameEnd::Win(Player::Black));
    }

    #[test]
    fn five_broken_by_available_capture_is_not_a_win() {
        // Black five along y=5, x=4..8. A vertical Black pair at (5,5)-(5,6)
        // is flanked by an existing White stone at (5,4); White can play
        // (5,7) to capture (5,5) and (5,6), removing (5,5) from the five.
        let pt = PatternTable::build();
        let mut b = Board::new();
        for &(x, y, p) in &[
            (4, 5, Player::Black),
            (6, 5, Player::Black),
            (7, 5, Player::Black),
            (5, 6, Player::Black),
            (5, 4, Player::White),
            (5, 5, Player::Black),
        ] {
            let _ = play_raw(&mut b, idx(x, y), p, &pt);
        }
        b.to_move = Player::Black;
        b.play(idx(8, 5), &pt);
        let result = check_end(&mut b, idx(8, 5), &pt);
        assert_eq!(result, GameEnd::None, "the (5,7) capture should break the five");
    }

    #[test]
    fn five_not_a_win_when_mover_already_lost_four_pairs_and_capture_available() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        for x in 4..8 {
            let _ = play_raw(&mut b, idx(x, 5), Player::Black, &pt);
        }
        b.captures[Player::White as usize] = 8; // Black has lost 4 pairs
        let _ = play_raw(&mut b, idx(15, 15), Player::White, &pt);
        let _ = play_raw(&mut b, idx(15, 16), Player::Black, &pt);
        let _ = play_raw(&mut b, idx(15, 17), Player::Black, &pt);
        b.to_move = Player::Black;
        b.play(idx(8, 5), &pt);
        let result = check_end(&mut b, idx(8, 5), &pt);
        assert_eq!(result, GameEnd::None);
    }

    #[test]
    fn ten_stones_captured_wins_by_capture() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        b.captures[Player::Black as usize] = 10;
        let _ = play_raw(&mut b, idx(9, 9), Player::White, &pt);
        b.to_move = Player::White; // so p = to_move.other() = Black
        let result = check_end(&mut b, idx(9, 9), &pt);
        assert_eq!(result, GameEnd::Win(Player::Black));
    }

    /// Test-only helper: plays a move for an arbitrary player regardless of
    /// `b.to_move`, bypassing turn alternation, so fixtures can build
    /// specific positions directly. Returns the `Undo` and restores
    /// `to_move` to what it was, so `undo_raw` can reverse it symmetrically.
    fn play_raw(b: &mut Board, mv: Idx, p: Player, pt: &PatternTable) -> (crate::board::Undo, Player) {
        let saved_to_move = b.to_move;
        b.to_move = p;
        let u = b.play(mv, pt);
        (u, saved_to_move)
    }

    fn undo_raw(b: &mut Board, (u, saved_to_move): (crate::board::Undo, Player)) {
        b.undo(&u);
        b.to_move = saved_to_move;
    }
}
