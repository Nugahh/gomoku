### Task 4: `board.rs` — `Board` struct, Zobrist, `captures_of`

**Files:**
- Modify: `src/board.rs`

**Interfaces:**
- Consumes: `patterns::{PatternTable, POW3, W, C}` (Task 2), `board::{Cell, Player, Idx, DIRS, TOTAL, idx, to_xy}` (Task 3).
- Produces: `board::{Board, Board::new, Board::get, Board::captures_of}`, used by Task 5 (`play`/`undo`) and by `rules.rs`, `eval.rs`, `search.rs`.

- [ ] **Step 1: Write the failing tests — construction and capture detection**

Add to `src/board.rs`, inside the existing `#[cfg(test)] mod tests` block (append these functions to it):

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib board:: -- --nocapture`
Expected: FAIL to compile — `Board`, `Board::new`, `Board::get`, `Board::set_raw`, `Board::captures_of` don't exist.

- [ ] **Step 3: Implement `Board`, Zobrist init, and `captures_of`**

Add above the `#[cfg(test)]` block:

```rust
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
}

impl Default for Board {
    fn default() -> Self {
        Board::new()
    }
}
```

Note on `n` reaching 16: 4 axes x 2 signs x 2 stones = 16 exactly, so `out: [Idx; 16]` never overflows; the `get_mut` guards are the `indexing_slicing`-lint-compliant way to write to a fixed array without ever panicking, even though this particular bound is provably safe.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib board:: -- --nocapture`
Expected: PASS, all tests green (geometry tests from Task 3 plus the 6 new ones).

- [ ] **Step 5: Commit**

```bash
git add src/board.rs
git commit -m "feat: Board struct, Zobrist keys, capture detection"
```


---

