### Task 15: `ui.rs` and `main.rs` — macroquad interface, game loop, `catch_unwind`

**Files:**
- Create: `src/ui.rs`
- Modify: `src/main.rs` (replace the placeholder body from Task 1 with the real entry point)

**Interfaces:**
- Consumes: everything from Tasks 2-14 (`board`, `patterns`, `rules`, `eval`, `search`).
- Produces: the finished `Gomoku` binary. No other task depends on this one.

**A note on testing this task:** unlike every other task, this one has no meaningful `#[test]` to write — it's almost entirely macroquad draw calls and mutable UI state, not the kind of pure logic the rest of this plan's TDD steps target (spec §13's testing strategy table has no UI row; it tests the engine, not the interface). Verification here is running the compiled binary and clicking through it, per Step 3's checklist, the same way any GUI change is checked by using it rather than by a unit test.

**A note on macroquad API surface:** the exact function signatures below (`Rect::new`, `.contains`, `draw_circle_lines`, `Color::from_rgba`, etc.) match macroquad `0.4.13` as pinned in Task 1's `Cargo.toml`. If `cargo build` reports a signature mismatch against the actually-resolved version, fix the call to match what the compiler reports and the installed crate's docs (`cargo doc --open -p macroquad`) — treat any such mismatch as a mechanical fix, not a design question.

- [ ] **Step 1: Write `src/ui.rs`**

```rust
#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use crate::board::{idx, to_xy, Board, Cell, Idx, Player, SIZE};
use crate::patterns::PatternTable;
use crate::rules;
use crate::search::{self, SearchConfig, SearchStats, TranspositionTable};
use macroquad::prelude::*;
use std::time::Duration;

const BOARD_ORIGIN_X: f32 = 40.0;
const BOARD_ORIGIN_Y: f32 = 40.0;
const CELL_SIZE: f32 = 38.0;

#[derive(Copy, Clone)]
enum Screen {
    Menu,
    Playing,
    GameOver(Option<Player>), // None = draw
}

#[derive(Copy, Clone)]
enum Mode {
    HumanVsAi { human: Player },
    Hotseat,
}

struct MoveStat {
    elapsed: Duration,
    depth: u8,
}

pub struct App {
    pt: PatternTable,
    tt: TranspositionTable,
    cfg: SearchConfig,
    board: Board,
    screen: Screen,
    mode: Mode,
    move_stats: Vec<MoveStat>,
    last_move: Option<Idx>,
    last_ai_stats: Option<SearchStats>,
    debug_visible: bool,
    suggestion: Option<Idx>,
    error_toast: Option<(String, f64)>,
    ai_crashed_notice: Option<String>,
}

impl App {
    pub fn new() -> Self {
        App {
            pt: PatternTable::build(),
            tt: TranspositionTable::new(),
            cfg: SearchConfig::default(),
            board: Board::new(),
            screen: Screen::Menu,
            mode: Mode::Hotseat,
            move_stats: Vec::new(),
            last_move: None,
            last_ai_stats: None,
            debug_visible: false,
            suggestion: None,
            error_toast: None,
            ai_crashed_notice: None,
        }
    }

    pub fn update_and_draw(&mut self) {
        clear_background(Color::from_rgba(235, 214, 168, 255));
        let screen = self.screen;
        match screen {
            Screen::Menu => self.draw_menu(),
            Screen::Playing => self.update_and_draw_playing(),
            Screen::GameOver(winner) => {
                self.draw_board_and_stones();
                self.draw_status_bar();
                self.draw_game_over_text(winner);
                if is_mouse_button_pressed(MouseButton::Left) {
                    self.screen = Screen::Menu;
                }
            }
        }
    }

    fn start_new_game(&mut self, mode: Mode) {
        self.board = Board::new();
        self.tt.clear();
        self.mode = mode;
        self.move_stats.clear();
        self.last_move = None;
        self.last_ai_stats = None;
        self.suggestion = None;
        self.error_toast = None;
        self.ai_crashed_notice = None;
        self.screen = Screen::Playing;
    }

    fn draw_menu(&mut self) {
        draw_text("Gomoku", 40.0, 80.0, 48.0, BLACK);
        let buttons: [(&str, Mode); 3] = [
            ("Play as Black vs AI", Mode::HumanVsAi { human: Player::Black }),
            ("Play as White vs AI", Mode::HumanVsAi { human: Player::White }),
            ("Hotseat (two players)", Mode::Hotseat),
        ];
        let (mx, my) = mouse_position();
        let mut y = 200.0;
        for (label, mode) in buttons {
            let rect = Rect::new(40.0, y, 420.0, 60.0);
            let hovered = rect.contains(vec2(mx, my));
            draw_rectangle(rect.x, rect.y, rect.w, rect.h, if hovered { LIGHTGRAY } else { GRAY });
            draw_text(label, rect.x + 16.0, rect.y + 38.0, 28.0, BLACK);
            if hovered && is_mouse_button_pressed(MouseButton::Left) {
                self.start_new_game(mode);
            }
            y += 80.0;
        }
    }

    fn update_and_draw_playing(&mut self) {
        if let Some((_, expiry)) = self.error_toast {
            if get_time() > expiry {
                self.error_toast = None;
            }
        }

        self.draw_board_and_stones();
        self.draw_status_bar();
        if self.debug_visible {
            self.draw_debug_panel();
        }
        if is_key_pressed(KeyCode::D) {
            self.debug_visible = !self.debug_visible;
        }

        let ai_to_move = matches!(self.mode, Mode::HumanVsAi { human } if human != self.board.to_move);
        if ai_to_move {
            self.run_ai_move();
            return;
        }

        if is_mouse_button_pressed(MouseButton::Left) {
            if let Some(cell) = self.cell_under_mouse() {
                self.try_human_move(cell);
            }
        }

        if matches!(self.mode, Mode::Hotseat) && self.draw_suggest_button_and_check_click() {
            self.compute_suggestion();
        }
    }

    fn cell_under_mouse(&self) -> Option<(usize, usize)> {
        let (mx, my) = mouse_position();
        let gx = ((mx - BOARD_ORIGIN_X) / CELL_SIZE).round();
        let gy = ((my - BOARD_ORIGIN_Y) / CELL_SIZE).round();
        if gx < 0.0 || gy < 0.0 {
            return None;
        }
        let (gx, gy) = (gx as usize, gy as usize);
        if gx >= SIZE || gy >= SIZE {
            return None;
        }
        let px = BOARD_ORIGIN_X + gx as f32 * CELL_SIZE;
        let py = BOARD_ORIGIN_Y + gy as f32 * CELL_SIZE;
        if (mx - px).abs() < CELL_SIZE * 0.4 && (my - py).abs() < CELL_SIZE * 0.4 {
            Some((gx, gy))
        } else {
            None
        }
    }

    fn try_human_move(&mut self, (x, y): (usize, usize)) {
        let mv = idx(x, y);
        let p = self.board.to_move;
        if !rules::is_legal(&self.board, mv, p, &self.pt) {
            self.error_toast = Some((
                "Illegal move (occupied, or a forbidden double-three)".to_string(),
                get_time() + 2.0,
            ));
            return;
        }
        self.board.play(mv, &self.pt);
        self.last_move = Some(mv);
        self.suggestion = None;
        self.after_move(mv);
    }

    fn after_move(&mut self, mv: Idx) {
        match rules::check_end(&mut self.board, mv, &self.pt) {
            rules::GameEnd::Win(w) => self.screen = Screen::GameOver(Some(w)),
            rules::GameEnd::Draw => self.screen = Screen::GameOver(None),
            rules::GameEnd::None => {}
        }
    }

    /// Wraps the AI call in `catch_unwind` (spec §11): if the search ever
    /// panics despite the `deny` lints and bounds-checked accessors
    /// elsewhere, the game plays a fallback legal move and keeps running
    /// instead of taking the whole grade to zero (spec R12).
    fn run_ai_move(&mut self) {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            search::find_best_move(&mut self.board, &self.cfg, &self.pt, &mut self.tt)
        }));
        match result {
            Ok((mv, stats)) => {
                self.board.play(mv, &self.pt);
                self.last_move = Some(mv);
                self.move_stats.push(MoveStat { elapsed: stats.elapsed, depth: stats.depth_reached });
                self.last_ai_stats = Some(stats);
                self.after_move(mv);
            }
            Err(_) => {
                self.ai_crashed_notice = Some("AI search panicked; played a fallback move".to_string());
                let mut candidates = Vec::new();
                rules::generate(&self.board, self.board.to_move, &self.pt, &mut candidates);
                if let Some(&mv) = candidates.first() {
                    self.board.play(mv, &self.pt);
                    self.last_move = Some(mv);
                    self.after_move(mv);
                }
            }
        }
    }

    fn compute_suggestion(&mut self) {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            search::find_best_move(&mut self.board, &self.cfg, &self.pt, &mut self.tt)
        }));
        if let Ok((mv, stats)) = result {
            self.suggestion = Some(mv);
            self.last_ai_stats = Some(stats);
        }
    }

    fn draw_suggest_button_and_check_click(&self) -> bool {
        let rect = Rect::new(760.0, 40.0, 120.0, 44.0);
        let (mx, my) = mouse_position();
        let hovered = rect.contains(vec2(mx, my));
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, if hovered { LIGHTGRAY } else { GRAY });
        draw_text("Suggest", rect.x + 12.0, rect.y + 28.0, 22.0, BLACK);
        hovered && is_mouse_button_pressed(MouseButton::Left)
    }

    fn draw_board_and_stones(&self) {
        let span = (SIZE - 1) as f32 * CELL_SIZE;
        for i in 0..SIZE {
            let x = BOARD_ORIGIN_X + i as f32 * CELL_SIZE;
            draw_line(x, BOARD_ORIGIN_Y, x, BOARD_ORIGIN_Y + span, 1.5, BLACK);
            let y = BOARD_ORIGIN_Y + i as f32 * CELL_SIZE;
            draw_line(BOARD_ORIGIN_X, y, BOARD_ORIGIN_X + span, y, 1.5, BLACK);
        }
        for &(sx, sy) in &[(3, 3), (3, 9), (3, 15), (9, 3), (9, 9), (9, 15), (15, 3), (15, 9), (15, 15)] {
            let cx = BOARD_ORIGIN_X + sx as f32 * CELL_SIZE;
            let cy = BOARD_ORIGIN_Y + sy as f32 * CELL_SIZE;
            draw_circle(cx, cy, 3.0, BLACK);
        }

        for y in 0..SIZE {
            for x in 0..SIZE {
                let cell = self.board.get(idx(x, y));
                if cell == Cell::Empty {
                    continue;
                }
                let cx = BOARD_ORIGIN_X + x as f32 * CELL_SIZE;
                let cy = BOARD_ORIGIN_Y + y as f32 * CELL_SIZE;
                let color = if cell == Cell::Black { BLACK } else { WHITE };
                draw_circle(cx, cy, CELL_SIZE * 0.42, color);
                draw_circle_lines(cx, cy, CELL_SIZE * 0.42, 1.5, DARKGRAY);
            }
        }

        if let Some(mv) = self.last_move {
            let (x, y) = to_xy(mv);
            let cx = BOARD_ORIGIN_X + x as f32 * CELL_SIZE;
            let cy = BOARD_ORIGIN_Y + y as f32 * CELL_SIZE;
            draw_circle_lines(cx, cy, CELL_SIZE * 0.2, 2.0, RED);
        }

        if let Some(mv) = self.suggestion {
            let (x, y) = to_xy(mv);
            let cx = BOARD_ORIGIN_X + x as f32 * CELL_SIZE;
            let cy = BOARD_ORIGIN_Y + y as f32 * CELL_SIZE;
            draw_circle_lines(cx, cy, CELL_SIZE * 0.42, 3.0, GREEN);
        }

        if let Some((x, y)) = self.cell_under_mouse() {
            let mv = idx(x, y);
            let cx = BOARD_ORIGIN_X + x as f32 * CELL_SIZE;
            let cy = BOARD_ORIGIN_Y + y as f32 * CELL_SIZE;
            let legal = rules::is_legal(&self.board, mv, self.board.to_move, &self.pt);
            let color = if legal { Color::from_rgba(0, 0, 0, 90) } else { Color::from_rgba(255, 0, 0, 90) };
            draw_circle(cx, cy, CELL_SIZE * 0.42, color);
            if !legal {
                draw_text("illegal move", cx - 40.0, cy - 20.0, 16.0, RED);
            }
        }

        if let Some((msg, _)) = &self.error_toast {
            draw_text(msg, BOARD_ORIGIN_X, BOARD_ORIGIN_Y - 12.0, 20.0, RED);
        }
    }

    /// Spec §10.3/R17: this is the display the subject calls
    /// validation-critical — no AI-think-time timer, no project
    /// validation. It is always visible, never behind the debug toggle.
    fn draw_status_bar(&self) {
        let y0 = BOARD_ORIGIN_Y + (SIZE - 1) as f32 * CELL_SIZE + 30.0;
        let turn_label = match self.board.to_move {
            Player::Black => "Black to move",
            Player::White => "White to move",
        };
        draw_text(turn_label, BOARD_ORIGIN_X, y0, 26.0, BLACK);

        let last_ms = self.move_stats.last().map(|s| s.elapsed.as_millis()).unwrap_or(0);
        let avg_ms = if self.move_stats.is_empty() {
            0
        } else {
            let total: u128 = self.move_stats.iter().map(|s| s.elapsed.as_millis()).sum();
            total / self.move_stats.len() as u128
        };
        let depth = self.move_stats.last().map(|s| s.depth).unwrap_or(0);
        draw_text(
            &format!("AI last move: {last_ms} ms   |   average: {avg_ms} ms   |   depth reached: {depth}"),
            BOARD_ORIGIN_X,
            y0 + 30.0,
            22.0,
            BLACK,
        );
        draw_text(
            &format!(
                "Captures  Black: {}   White: {}",
                self.board.captures[Player::Black as usize],
                self.board.captures[Player::White as usize]
            ),
            BOARD_ORIGIN_X,
            y0 + 58.0,
            22.0,
            BLACK,
        );
        if let Some(msg) = &self.ai_crashed_notice {
            draw_text(msg, BOARD_ORIGIN_X, y0 + 86.0, 20.0, RED);
        }
        draw_text("Press D to toggle debug panel", BOARD_ORIGIN_X, y0 + 114.0, 18.0, DARKGRAY);
    }

    fn draw_debug_panel(&self) {
        let x0 = BOARD_ORIGIN_X + (SIZE - 1) as f32 * CELL_SIZE + 40.0;
        let mut y = BOARD_ORIGIN_Y;
        draw_rectangle(x0 - 10.0, y - 10.0, 260.0, 400.0, Color::from_rgba(255, 255, 255, 230));
        draw_text("Debug", x0, y + 14.0, 24.0, BLACK);
        y += 40.0;

        let Some(stats) = &self.last_ai_stats else {
            draw_text("no search run yet", x0, y, 18.0, DARKGRAY);
            return;
        };

        draw_text(&format!("nodes: {}", stats.nodes), x0, y, 18.0, BLACK);
        y += 22.0;
        let nps = if stats.elapsed.as_secs_f64() > 0.0 {
            stats.nodes as f64 / stats.elapsed.as_secs_f64()
        } else {
            0.0
        };
        draw_text(&format!("nodes/sec: {nps:.0}"), x0, y, 18.0, BLACK);
        y += 22.0;
        draw_text(&format!("depth reached: {}", stats.depth_reached), x0, y, 18.0, BLACK);
        y += 22.0;
        let hit_rate = if stats.tt_probes > 0 {
            100.0 * stats.tt_hits as f64 / stats.tt_probes as f64
        } else {
            0.0
        };
        draw_text(&format!("TT hit rate: {hit_rate:.1}%"), x0, y, 18.0, BLACK);
        y += 30.0;

        draw_text("Principal variation:", x0, y, 18.0, BLACK);
        y += 22.0;
        let pv_text: Vec<String> = stats
            .pv
            .iter()
            .map(|&mv| {
                let (px, py) = to_xy(mv);
                format!("({px},{py})")
            })
            .collect();
        draw_text(&pv_text.join(" "), x0, y, 16.0, DARKGRAY);
        y += 30.0;

        draw_text("Top root moves:", x0, y, 18.0, BLACK);
        y += 22.0;
        let mut top = stats.root_scores.clone();
        top.sort_unstable_by(|a, b| b.1.cmp(&a.1));
        for &(mv, score) in top.iter().take(5) {
            let (px, py) = to_xy(mv);
            draw_text(&format!("({px},{py}) = {score}"), x0, y, 16.0, DARKGRAY);
            y += 20.0;
        }
    }

    fn draw_game_over_text(&self, winner: Option<Player>) {
        let text = match winner {
            Some(Player::Black) => "Black wins!",
            Some(Player::White) => "White wins!",
            None => "Draw.",
        };
        draw_rectangle(200.0, 400.0, 500.0, 120.0, Color::from_rgba(0, 0, 0, 200));
        draw_text(text, 240.0, 460.0, 48.0, WHITE);
        draw_text("Click anywhere to return to the menu", 240.0, 495.0, 22.0, WHITE);
    }
}

impl Default for App {
    fn default() -> Self {
        App::new()
    }
}
```

- [ ] **Step 2: Replace `src/main.rs`'s placeholder body**

```rust
#![forbid(unsafe_code)]

mod board;
mod eval;
mod patterns;
mod rules;
mod search;
mod ui;

use macroquad::prelude::*;

fn window_conf() -> Conf {
    Conf {
        window_title: "Gomoku".to_owned(),
        window_width: 900,
        window_height: 1000,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut app = ui::App::new();
    loop {
        app.update_and_draw();
        next_frame().await;
    }
}
```

- [ ] **Step 3: Build and manually verify**

Run: `cargo build --release`
Expected: compiles clean (fix any macroquad signature mismatch per this task's opening note before proceeding).

Run: `make` then `./Gomoku`, and manually walk through:

1. Menu appears with three buttons; each is clickable and starts a game.
2. **Human vs AI, playing Black:** click an empty intersection — a black stone appears there, the status bar's turn label flips to White, and the AI replies within about 400ms (status bar's "AI last move" ms updates). Hovering an empty cell shows a preview stone; hovering an occupied cell or a double-three cell shows a red illegal-move preview and clicking it does nothing but show the toast.
3. **Human vs AI, playing White:** the AI moves first (Black), automatically, without any click.
4. **Hotseat:** both players click to place stones alternately; the **Suggest** button highlights a move (green ring) for whoever is currently to move, without playing it; the suggested cell is cleared after the next real move.
5. Press **D**: the debug panel appears on the right with nodes, nodes/sec, depth, TT hit rate, principal variation, and top-5 root moves; press **D** again to hide it.
6. Play (or fast-forward via repeated AI-vs-AI by picking Hotseat and clicking "Suggest" then clicking its highlighted cell yourself, repeatedly) until a five-in-a-row or a 10-stone capture ends the game: the game-over banner appears with the correct winner, and clicking anywhere returns to the menu with a fresh board.
7. Confirm the status bar's timer is visible in **every** screen state during play — this is spec R17, and the subject fails the whole project without it.

- [ ] **Step 4: Final whole-project verification**

Run: `cargo test --release`
Expected: every test from Tasks 2-14 passes, including the Task 14 benchmark gate (now actually enforced, since this is `--release`).

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean, per Task 13.

Run: `make clean && make && make` (twice in a row)
Expected: the first `make` builds `./Gomoku`; the second prints only an up-to-date message, confirming the Makefile's no-relink property still holds (spec §12) after every source file this plan has added.

- [ ] **Step 5: Commit**

```bash
git add src/ui.rs src/main.rs
git commit -m "feat: macroquad GUI — menu, board, status bar timer, debug panel, hotseat suggest"
```

