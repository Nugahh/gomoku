#![forbid(unsafe_code)]

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
}
