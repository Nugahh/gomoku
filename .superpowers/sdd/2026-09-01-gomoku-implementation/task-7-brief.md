### Task 7: `rules.rs` — `check_end` (win conditions, endgame capture rule)

**Files:**
- Modify: `src/rules.rs`

**Interfaces:**
- Consumes: `board::{Board, Player, Idx, DIRS, TOTAL}`, `patterns::{PatternTable, F_FIVE}`, `rules::generate` (Task 6).
- Produces: `rules::{GameEnd, check_end}`, used by `main.rs` (Task 15) after every move and by `search.rs` (Task 9+) to score terminal nodes correctly (a breakable five must never be valued as a win).

- [ ] **Step 1: Write the failing tests — the four end-of-game fixtures**

Add to `src/rules.rs`'s `#[cfg(test)] mod tests` block:

```rust
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib rules:: -- --nocapture`
Expected: FAIL to compile — `GameEnd`, `check_end` don't exist.

- [ ] **Step 3: Implement `GameEnd` and `check_end`**

Add to `src/rules.rs`, above `#[cfg(test)]`:

```rust
use crate::board::TOTAL;
use crate::patterns::F_FIVE;

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

    let p_lost_stones = b.captures[opp as usize];
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
        if captured[..n].iter().any(|c| alignment.contains(c)) {
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

    if b.captures[p as usize] >= 10 {
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
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib rules:: -- --nocapture`
Expected: PASS, all 11 tests in `rules.rs` green (7 from Task 6, 4 from this task).

If `five_broken_by_available_capture_is_not_a_win` fails, check that `(5,7)` actually appears in `generate`'s candidate list for White — it must be within Chebyshev radius 2 of an existing stone (it is, of `(5,6)`) and `is_legal` must return `true` for it (it will, since it captures a pair, which short-circuits the free-three check).

- [ ] **Step 5: Commit**

```bash
git add src/rules.rs
git commit -m "feat: game end detection — endgame capture rule, win by capture, draw"
```


---

