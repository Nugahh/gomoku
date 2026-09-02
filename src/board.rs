#![forbid(unsafe_code)]

use crate::patterns::{PatternTable, POW3};

pub const SIZE: usize = 19;
pub const PAD: usize = 4;
pub const STRIDE: usize = SIZE + 2 * PAD; // 27
pub const TOTAL: usize = STRIDE * STRIDE; // 729

pub type Idx = u16;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Cell {
    Empty = 0,
    Black = 1,
    White = 2,
    Wall = 3,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Player {
    Black = 0,
    White = 1,
}

#[inline]
pub const fn idx(x: usize, y: usize) -> Idx {
    ((y + PAD) * STRIDE + (x + PAD)) as Idx
}

#[inline]
pub const fn to_xy(i: Idx) -> (usize, usize) {
    let i = i as usize;
    (i % STRIDE - PAD, i / STRIDE - PAD)
}

/// The four axes: horizontal, vertical, and the two diagonals. Each is
/// walked in both the `+d` and `-d` direction to cover all 8 directions.
pub const DIRS: [i16; 4] = [1, STRIDE as i16, STRIDE as i16 + 1, STRIDE as i16 - 1];

/// Vulnerability penalty per pair (spec §8.3): a pair of same-color stones
/// where one flank is an opponent stone and the other is empty is one move
/// away from being captured.
const VULN_PENALTY: i32 = -1_200;

pub struct Undo {
    pub mv: Idx,
    pub captured: [Idx; 16], // 4 axes x 2 signs x 2 stones per capture, max
    pub n_captured: u8,
    pub prev_zobrist: u64,
    pub prev_acc: [i32; 2],
    pub prev_captures: [u8; 2],
}

impl Player {
    #[inline]
    pub fn other(self) -> Player {
        match self {
            Player::Black => Player::White,
            Player::White => Player::Black,
        }
    }

    #[inline]
    pub fn cell(self) -> Cell {
        match self {
            Player::Black => Cell::Black,
            Player::White => Cell::White,
        }
    }
}

/// Fixed-seed xorshift64, used only to generate deterministic Zobrist keys
/// at startup. Not a general-purpose RNG and not used for gameplay
/// randomness (there is none — the AI is deterministic).
struct Xorshift64(u64);

impl Xorshift64 {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

#[derive(Clone)]
pub struct Board {
    cells: [Cell; TOTAL],
    /// Number of *stones* captured by each player, indexed by
    /// `Player as usize`. A win triggers at 10 (five pairs) — spec R4.
    pub captures: [u8; 2],
    pub to_move: Player,
    pub zobrist: u64,
    pub stone_count: u32,
    /// Count of stones within Chebyshev radius 2 of each cell. Drives
    /// candidate generation in rules.rs (spec §7.4).
    neighbor: [u8; TOTAL],
    /// Incremental heuristic accumulator, one per player (spec §8). `pub`
    /// because `eval.rs` reads it directly; only `board.rs` ever writes it.
    pub acc: [i32; 2],

    // Zobrist key tables, built once at construction from a fixed seed so
    // runs are reproducible without a `rand` dependency (spec §6.2).
    key_cell: [[u64; TOTAL]; 2],
    key_side: u64,
    key_captures: [[u64; 11]; 2],
}

impl Board {
    pub fn new() -> Self {
        let mut rng = Xorshift64(0x9E3779B97F4A7C15);
        let mut key_cell = [[0u64; TOTAL]; 2];
        for player_keys in key_cell.iter_mut() {
            for k in player_keys.iter_mut() {
                *k = rng.next();
            }
        }
        let key_side = rng.next();
        let mut key_captures = [[0u64; 11]; 2];
        for player_keys in key_captures.iter_mut() {
            for k in player_keys.iter_mut() {
                *k = rng.next();
            }
        }

        let mut cells = [Cell::Wall; TOTAL];
        for y in 0..SIZE {
            for x in 0..SIZE {
                cells[idx(x, y) as usize] = Cell::Empty;
            }
        }

        Board {
            cells,
            captures: [0, 0],
            to_move: Player::Black,
            zobrist: key_side, // Black moves first; folded in once, consistently
            stone_count: 0,
            neighbor: [0u8; TOTAL],
            acc: [0, 0],
            key_cell,
            key_side,
            key_captures,
        }
    }

    #[inline]
    pub fn get(&self, i: Idx) -> Cell {
        self.cells.get(i as usize).copied().unwrap_or(Cell::Wall)
    }

    /// Sets a cell directly with no bookkeeping (no Zobrist/acc/neighbor
    /// update, no capture check). Used by tests to build fixture positions,
    /// and internally by `play`/`undo` (Task 5) alongside their own explicit
    /// bookkeeping. Never call this mid-search.
    fn set_raw(&mut self, i: Idx, c: Cell) {
        if let Some(slot) = self.cells.get_mut(i as usize) {
            *slot = c;
        }
    }

    /// Returns the stones captured if `p` plays at `mv`, without mutating
    /// the board. A capture is the exact pattern `p O O p` starting at `mv`
    /// and walking outward: `cell(mv+d)==opp && cell(mv+2d)==opp &&
    /// cell(mv+3d)==p`, checked in all 8 directions (4 axes x 2 signs).
    /// `Wall` fails every comparison against `opp` or `p`, so board edges
    /// need no special-casing (spec §6.3).
    pub fn captures_of(&self, mv: Idx, p: Player) -> ([Idx; 16], usize) {
        let opp = p.other().cell();
        let mine = p.cell();
        let mut out = [0 as Idx; 16];
        let mut n = 0usize;

        for &d in DIRS.iter() {
            for &sign in &[1i32, -1i32] {
                let step = sign * d as i32;
                let p1 = mv as i32 + step;
                let p2 = mv as i32 + 2 * step;
                let p3 = mv as i32 + 3 * step;
                if p1 < 0 || p2 < 0 || p3 < 0 {
                    continue;
                }
                let (p1, p2, p3) = (p1 as Idx, p2 as Idx, p3 as Idx);
                if self.get(p1) == opp && self.get(p2) == opp && self.get(p3) == mine {
                    if let Some(slot) = out.get_mut(n) {
                        *slot = p1;
                    }
                    n += 1;
                    if let Some(slot) = out.get_mut(n) {
                        *slot = p2;
                    }
                    n += 1;
                }
            }
        }
        (out, n)
    }

    /// Safe accessor for offsets that may reach beyond the physical buffer
    /// (see this task's implementation note on reach). Anything outside the
    /// array is `Wall` — semantically correct, since nothing real is ever
    /// there.
    #[inline]
    fn cell_at(&self, center: Idx, offset: i32) -> Cell {
        let i = center as i32 + offset;
        if i < 0 || i as usize >= TOTAL {
            return Cell::Wall;
        }
        self.cells.get(i as usize).copied().unwrap_or(Cell::Wall)
    }

    /// Encodes the 9-cell window centered at `c` along axis `d`, relative
    /// to `p` (own stone = 1, empty = 0, opponent-or-wall = 2), matching
    /// `patterns::PatternTable`'s encoding (spec §5.1).
    fn window_code(&self, c: Idx, d: i16, p: Player) -> u32 {
        let mut code = 0u32;
        for (slot, k) in (-4..=4i32).enumerate() {
            let cell = self.cell_at(c, k * d as i32);
            let trit: u32 = if cell == Cell::Empty {
                0
            } else if cell == p.cell() {
                1
            } else {
                2
            };
            code += trit * POW3.get(slot).copied().unwrap_or(0);
        }
        code
    }

    #[inline]
    fn stone_window_score(&self, c: Idx, d: i16, owner: Player, pt: &PatternTable) -> i32 {
        pt.get(self.window_code(c, d, owner)).score
    }

    /// Vulnerability contribution of the pair `(c, c+d)`, anchored at `c`
    /// so each pair is scored exactly once (spec §8.3).
    fn pair_vuln_score(&self, c: Idx, d: i16, owner: Player) -> i32 {
        let opp = owner.other().cell();
        if self.cell_at(c, d as i32) != owner.cell() {
            return 0;
        }
        let before = self.cell_at(c, -(d as i32));
        let after = self.cell_at(c, 2 * d as i32);
        let vulnerable =
            (before == opp && after == Cell::Empty) || (before == Cell::Empty && after == opp);
        if vulnerable {
            VULN_PENALTY
        } else {
            0
        }
    }

    /// Adds `sign * score` to the accumulator entry of every stone whose
    /// pattern-table window could be affected by a change at `center`
    /// (spec §6.4 step 2/5, §8.2). Call with `sign = -1` before mutating
    /// the board, `sign = 1` after.
    fn adjust_axis_neighbors(&mut self, center: Idx, pt: &PatternTable, sign: i32) {
        for &d in DIRS.iter() {
            for k in (-4..=4i32).filter(|&k| k != 0) {
                let cell = self.cell_at(center, k * d as i32);
                let owner = match cell {
                    Cell::Black => Player::Black,
                    Cell::White => Player::White,
                    _ => continue,
                };
                let c_off = center as i32 + k * d as i32;
                if c_off < 0 || c_off as usize >= TOTAL {
                    continue;
                }
                let c = c_off as Idx;
                let score = self.stone_window_score(c, d, owner, pt);
                self.acc[owner as usize] += sign * score;
            }
        }
    }

    /// Same shape as `adjust_axis_neighbors`, at the smaller radius the
    /// vulnerability term needs (see this task's implementation note).
    fn adjust_axis_vuln(&mut self, center: Idx, sign: i32) {
        for &d in DIRS.iter() {
            for k in -2..=1i32 {
                let c_off = center as i32 + k * d as i32;
                if c_off < 0 || c_off as usize >= TOTAL {
                    continue;
                }
                let c = c_off as Idx;
                let owner = match self.get(c) {
                    Cell::Black => Player::Black,
                    Cell::White => Player::White,
                    _ => continue,
                };
                let score = self.pair_vuln_score(c, d, owner);
                self.acc[owner as usize] += sign * score;
            }
        }
    }

    /// Adds `sign * score` to the accumulator entry of every `(cell, axis)`
    /// pattern-table window that could be affected by changes across ALL of
    /// `changed` at once, touching each such window exactly once. Used only
    /// on the capturing path, where `{mv} ∪ captured` can share overlapping
    /// radius-4 influence zones (they are frequently collinear, since a
    /// capture's flanking stone sits within a few cells of both `mv` and
    /// the stones it captures) — calling `adjust_axis_neighbors` once per
    /// center in that case double-counts any window in the overlap using
    /// mismatched intermediate board snapshots. Call with `sign = -1`
    /// before any mutation (every touched window reads the true pre-move
    /// board), and `sign = 1` after all mutations are complete (every
    /// touched window reads the true post-move board).
    fn adjust_axis_dedup(&mut self, changed: &[Idx], pt: &PatternTable, sign: i32) {
        let mut touched: Vec<(Idx, u8)> = Vec::new();
        for &center in changed {
            for (di, &d) in DIRS.iter().enumerate() {
                for k in -4..=4i32 {
                    let c_off = center as i32 + k * d as i32;
                    if c_off < 0 || c_off as usize >= TOTAL {
                        continue;
                    }
                    let key = (c_off as Idx, di as u8);
                    if !touched.contains(&key) {
                        touched.push(key);
                    }
                }
            }
        }
        for &(c, di) in &touched {
            let owner = match self.get(c) {
                Cell::Black => Player::Black,
                Cell::White => Player::White,
                _ => continue,
            };
            let d = DIRS.get(di as usize).copied().unwrap_or(1);
            let score = self.stone_window_score(c, d, owner, pt);
            self.acc[owner as usize] += sign * score;
        }
    }

    /// Same shape as `adjust_axis_dedup`, at `adjust_axis_vuln`'s smaller
    /// (-2..=1) radius, for the capturing path.
    fn adjust_axis_vuln_dedup(&mut self, changed: &[Idx], sign: i32) {
        let mut touched: Vec<(Idx, u8)> = Vec::new();
        for &center in changed {
            for (di, &d) in DIRS.iter().enumerate() {
                for k in -2..=1i32 {
                    let c_off = center as i32 + k * d as i32;
                    if c_off < 0 || c_off as usize >= TOTAL {
                        continue;
                    }
                    let key = (c_off as Idx, di as u8);
                    if !touched.contains(&key) {
                        touched.push(key);
                    }
                }
            }
        }
        for &(c, di) in &touched {
            let owner = match self.get(c) {
                Cell::Black => Player::Black,
                Cell::White => Player::White,
                _ => continue,
            };
            let d = DIRS.get(di as usize).copied().unwrap_or(1);
            let score = self.pair_vuln_score(c, d, owner);
            self.acc[owner as usize] += sign * score;
        }
    }

    /// Updates the radius-2 neighbor-count grid used by `rules::generate`
    /// (spec §7.4) around `center` by `delta`, saturating rather than
    /// over/underflowing.
    fn adjust_neighbor_grid(&mut self, center: Idx, delta: i32) {
        let stride = STRIDE as i32;
        for dy in -2..=2i32 {
            for dx in -2..=2i32 {
                let i = center as i32 + dy * stride + dx;
                if i < 0 || i as usize >= TOTAL {
                    continue;
                }
                if let Some(slot) = self.neighbor.get_mut(i as usize) {
                    *slot = (*slot as i32 + delta).clamp(0, 255) as u8;
                }
            }
        }
    }

    /// Applies `mv` for `self.to_move`. Assumes `mv` is legal — callers
    /// must check `rules::is_legal` first (spec §6.4).
    pub fn play(&mut self, mv: Idx, pt: &PatternTable) -> Undo {
        let p = self.to_move;
        let (captured, n) = self.captures_of(mv, p);
        let owner = p.other();

        let undo = Undo {
            mv,
            captured,
            n_captured: n as u8,
            prev_zobrist: self.zobrist,
            prev_acc: self.acc,
            prev_captures: self.captures,
        };

        if n == 0 {
            self.adjust_axis_neighbors(mv, pt, -1);
            self.adjust_axis_vuln(mv, -1);

            self.set_raw(mv, p.cell());
            self.zobrist ^= self.key_cell[p as usize][mv as usize];
            self.stone_count += 1;
            self.adjust_neighbor_grid(mv, 1);

            self.adjust_axis_neighbors(mv, pt, 1);
            self.adjust_axis_vuln(mv, 1);
            for &d in DIRS.iter() {
                self.acc[p as usize] += self.stone_window_score(mv, d, p, pt);
            }
        } else {
            let mut changed: Vec<Idx> = Vec::with_capacity(1 + n);
            changed.push(mv);
            changed.extend(captured.iter().take(n));

            self.adjust_axis_dedup(&changed, pt, -1);
            self.adjust_axis_vuln_dedup(&changed, -1);

            self.set_raw(mv, p.cell());
            self.zobrist ^= self.key_cell[p as usize][mv as usize];
            self.stone_count += 1;
            self.adjust_neighbor_grid(mv, 1);

            for cc in captured.iter().take(n) {
                self.set_raw(*cc, Cell::Empty);
                self.zobrist ^= self.key_cell[owner as usize][*cc as usize];
                self.adjust_neighbor_grid(*cc, -1);
                self.stone_count -= 1;
            }

            self.adjust_axis_dedup(&changed, pt, 1);
            self.adjust_axis_vuln_dedup(&changed, 1);
        }

        let old_idx = self.captures[p as usize].min(10) as usize;
        self.zobrist ^= self.key_captures[p as usize].get(old_idx).copied().unwrap_or(0);
        self.captures[p as usize] = self.captures[p as usize].saturating_add(n as u8);
        let new_idx = self.captures[p as usize].min(10) as usize;
        self.zobrist ^= self.key_captures[p as usize].get(new_idx).copied().unwrap_or(0);

        self.to_move = owner;
        self.zobrist ^= self.key_side;

        undo
    }

    /// Exactly reverses a `play`. Restores cells, neighbor grid and
    /// stone_count by replaying the recorded change; restores zobrist, acc
    /// and captures by direct snapshot rather than recomputing them (spec
    /// §6.4) — the incremental math above is complex enough that re-running
    /// it backwards would just be a second place to get it wrong.
    pub fn undo(&mut self, u: &Undo) {
        let mover = self.to_move.other();
        let captured_owner = mover.other();

        for cc in u.captured.iter().take(u.n_captured as usize) {
            self.set_raw(*cc, captured_owner.cell());
            self.adjust_neighbor_grid(*cc, 1);
            self.stone_count += 1;
        }
        self.set_raw(u.mv, Cell::Empty);
        self.adjust_neighbor_grid(u.mv, -1);
        self.stone_count -= 1;

        self.zobrist = u.prev_zobrist;
        self.acc = u.prev_acc;
        self.captures = u.prev_captures;
        self.to_move = mover;
    }
}

#[cfg(test)]
impl Board {
    /// Recomputes the accumulator from scratch by scanning every occupied
    /// cell and axis, independent of the incremental machinery in `play`.
    /// Used only to verify `play`/`undo` never let `acc` drift (spec §8.2).
    pub(crate) fn full_recompute_acc(&self, pt: &PatternTable) -> [i32; 2] {
        let mut acc = [0i32; 2];
        for y in 0..SIZE {
            for x in 0..SIZE {
                let i = idx(x, y);
                let owner = match self.get(i) {
                    Cell::Black => Player::Black,
                    Cell::White => Player::White,
                    _ => continue,
                };
                for &d in DIRS.iter() {
                    acc[owner as usize] += self.stone_window_score(i, d, owner, pt);
                    acc[owner as usize] += self.pair_vuln_score(i, d, owner);
                }
            }
        }
        acc
    }
}

impl Default for Board {
    fn default() -> Self {
        Board::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idx_to_xy_roundtrip_covers_full_board() {
        for y in 0..SIZE {
            for x in 0..SIZE {
                let i = idx(x, y);
                assert_eq!(to_xy(i), (x, y), "roundtrip failed at ({x},{y})");
            }
        }
    }

    #[test]
    fn corners_are_within_padded_bounds() {
        assert!((idx(0, 0) as usize) < TOTAL);
        assert!((idx(SIZE - 1, SIZE - 1) as usize) < TOTAL);
    }

    #[test]
    fn four_step_walk_from_every_playable_cell_stays_in_bounds() {
        // The pattern window reaches 4 cells either side of center along
        // each axis (spec §4/§5.2). This is the padding's whole reason to
        // exist: verify no walk of +-4*dir from any real board cell can
        // leave the TOTAL-sized backing array.
        for y in 0..SIZE {
            for x in 0..SIZE {
                let c = idx(x, y) as i32;
                for &d in DIRS.iter() {
                    for k in -4..=4i32 {
                        let i = c + k * d as i32;
                        assert!(
                            i >= 0 && (i as usize) < TOTAL,
                            "out of bounds at ({x},{y}) dir {d} k {k}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn dirs_are_the_four_distinct_axes() {
        // horizontal, vertical, and the two diagonals — walking +-d covers
        // all 8 directions with only 4 stored values.
        assert_eq!(DIRS[0], 1);
        assert_eq!(DIRS[1], STRIDE as i16);
        assert_eq!(DIRS[2], STRIDE as i16 + 1);
        assert_eq!(DIRS[3], STRIDE as i16 - 1);
    }

    #[test]
    fn player_other_and_cell_mapping() {
        assert_eq!(Player::Black.other(), Player::White);
        assert_eq!(Player::White.other(), Player::Black);
        assert_eq!(Player::Black.cell(), Cell::Black);
        assert_eq!(Player::White.cell(), Cell::White);
    }

    #[test]
    fn new_board_is_all_empty_except_border() {
        let b = Board::new();
        for y in 0..SIZE {
            for x in 0..SIZE {
                assert_eq!(b.get(idx(x, y)), Cell::Empty);
            }
        }
        // one cell outside the playable area, inside the padded buffer
        assert_eq!(b.get(idx(0, 0).wrapping_sub(1)), Cell::Wall);
    }

    #[test]
    fn captures_of_detects_flanking_pair_horizontally() {
        // Blue Red Red _   -> Blue plays at position 3, captures the pair.
        let mut b = Board::new();
        b.set_raw(idx(0, 0), Cell::White); // Blue's existing stone
        b.set_raw(idx(1, 0), Cell::Black); // Red pair
        b.set_raw(idx(2, 0), Cell::Black);
        // Blue's flanking move lands at (3,0)
        let (captured, n) = b.captures_of(idx(3, 0), Player::White);
        assert_eq!(n, 2);
        assert!(captured[..n].contains(&idx(1, 0)));
        assert!(captured[..n].contains(&idx(2, 0)));
    }

    #[test]
    fn captures_of_ignores_single_stone() {
        // Blue _ Red _ : playing at the empty cell flanks only one stone,
        // not a pair, so no capture.
        let mut b = Board::new();
        b.set_raw(idx(0, 0), Cell::White);
        b.set_raw(idx(1, 0), Cell::Black);
        let (_captured, n) = b.captures_of(idx(2, 0), Player::White);
        assert_eq!(n, 0);
    }

    #[test]
    fn captures_of_ignores_three_in_a_row() {
        // one can only capture PAIRS, not 3+ stones in a row (spec appendix VI.1)
        let mut b = Board::new();
        b.set_raw(idx(0, 0), Cell::White);
        b.set_raw(idx(1, 0), Cell::Black);
        b.set_raw(idx(2, 0), Cell::Black);
        b.set_raw(idx(3, 0), Cell::Black);
        let (_captured, n) = b.captures_of(idx(4, 0), Player::White);
        assert_eq!(n, 0);
    }

    #[test]
    fn captures_of_checks_all_eight_directions() {
        let mut b = Board::new();
        // vertical pair above the played cell
        b.set_raw(idx(5, 5), Cell::White);
        b.set_raw(idx(5, 4), Cell::Black);
        b.set_raw(idx(5, 3), Cell::Black);
        let (captured, n) = b.captures_of(idx(5, 2), Player::White);
        assert_eq!(n, 2);
        assert!(captured[..n].contains(&idx(5, 4)));
        assert!(captured[..n].contains(&idx(5, 3)));
    }

    #[test]
    fn captures_of_a_wall_neighbor_finds_nothing() {
        // near the edge, the pattern reads Wall instead of an opponent stone;
        // must not capture and must not panic.
        let b = Board::new();
        let (_captured, n) = b.captures_of(idx(0, 0), Player::White);
        assert_eq!(n, 0);
    }

    fn random_empty_cell(b: &Board, rng: &mut Xorshift64) -> Option<Idx> {
        let mut empties = Vec::new();
        for y in 0..SIZE {
            for x in 0..SIZE {
                let i = idx(x, y);
                if b.get(i) == Cell::Empty {
                    empties.push(i);
                }
            }
        }
        if empties.is_empty() {
            return None;
        }
        let pick = (rng.next() as usize) % empties.len();
        empties.get(pick).copied()
    }

    #[test]
    fn play_undo_round_trip_restores_exact_state() {
        // Uses "any empty cell" rather than full rule legality: play/undo's
        // bookkeeping (accumulator, zobrist, neighbor grid, captures) does
        // not care about double-three, which is a search/UI-level filter,
        // not a board-mechanics concern. This keeps board.rs's own test
        // independent of rules.rs, which does not exist yet.
        let pt = PatternTable::build();
        for seed in 0..1000u64 {
            let mut rng = Xorshift64(seed.wrapping_mul(0x9E3779B97F4A7C15) | 1);
            let mut b = Board::new();
            let snapshot = b.clone();
            let mut undos = Vec::new();
            for _ in 0..50 {
                let Some(mv) = random_empty_cell(&b, &mut rng) else {
                    break;
                };
                undos.push(b.play(mv, &pt));
            }
            for u in undos.iter().rev() {
                b.undo(u);
            }
            assert_eq!(b.cells, snapshot.cells, "seed {seed}: cells differ after undo");
            assert_eq!(b.captures, snapshot.captures, "seed {seed}: captures differ");
            assert_eq!(b.to_move, snapshot.to_move, "seed {seed}: to_move differs");
            assert_eq!(b.zobrist, snapshot.zobrist, "seed {seed}: zobrist differs");
            assert_eq!(b.stone_count, snapshot.stone_count, "seed {seed}: stone_count differs");
            assert_eq!(b.neighbor, snapshot.neighbor, "seed {seed}: neighbor grid differs");
            assert_eq!(b.acc, snapshot.acc, "seed {seed}: accumulator differs");
        }
    }

    #[test]
    fn play_capture_updates_captures_and_frees_cells() {
        let pt = PatternTable::build();
        let mut b = Board::new();
        // White _ Black Black _  ->  White plays at (3,0), captures the pair.
        // Built entirely through `play()` with natural turn alternation
        // (Black moves first) rather than `set_raw`: `set_raw` bypasses acc
        // bookkeeping entirely, which would taint the accumulator-vs-full-
        // recompute assertion below with drift from *before* the capture,
        // unrelated to what this test is actually verifying.
        b.play(idx(1, 0), &pt); // Black
        b.play(idx(0, 0), &pt); // White
        b.play(idx(2, 0), &pt); // Black
        let before_capture_stone_count = b.stone_count;
        b.play(idx(3, 0), &pt); // White, captures the pair
        assert_eq!(b.get(idx(1, 0)), Cell::Empty, "captured stone not removed");
        assert_eq!(b.get(idx(2, 0)), Cell::Empty, "captured stone not removed");
        assert_eq!(b.captures[Player::White as usize], 2);
        // White's capturing move adds 1 stone (itself), removes 2 (the pair).
        assert_eq!(b.stone_count, before_capture_stone_count + 1 - 2);
        assert_eq!(
            b.acc,
            b.full_recompute_acc(&pt),
            "accumulator drifted from full recompute after a capturing play()"
        );
    }

    #[test]
    fn accumulator_matches_full_recompute_including_captures() {
        let pt = PatternTable::build();
        for seed in 0..300u64 {
            let mut rng = Xorshift64(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
            let mut b = Board::new();
            for _ in 0..60 {
                let Some(mv) = random_empty_cell(&b, &mut rng) else {
                    break;
                };
                b.play(mv, &pt);
                let full = b.full_recompute_acc(&pt);
                assert_eq!(
                    b.acc, full,
                    "seed {seed}: accumulator drifted from full recompute after a play()"
                );
            }
        }
    }
}
