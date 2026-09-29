//! The live board: one daily match painted small, for the #lounge strip
//! (`live_strip.rs`). Grid boards as two-column cells; pool on its own tiny
//! half-block table (`fit_cloth`) with the shooter's guide and cue stick
//! while an aim is fresh.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::app::common::theme;
use crate::app::games::{
    chess_core::types::{ChessColor, ChessPiece, piece_glyph},
    pool_core::{
        ball::CUE,
        canvas::{Canvas, Rgb},
        table::TableSpec,
        table_ui::{CLOTH, GUIDE, POCKET, RAIL, SURROUND, ball_colour},
    },
};

use super::{
    checkers, connect4,
    live::{LiveBall, LiveBoard, LiveView, ShotTally},
    pool::PoolAimShare,
    reversi,
};

/// Tall enough for a chess or checkers board at one row per rank; every
/// other game centres in it.
pub(crate) const BOARD_ROWS: u16 = crate::app::live::ui::PICTURE_ROWS;

/// `8ball eggy v weslin`, the player on the move in amber; `eggy is aiming`
/// while they line up.
pub(crate) fn live_compact_line(width: u16, view: &LiveView<'_>) -> Line<'static> {
    let item = view.item;
    if view.aim.is_some() {
        let shooter = if item.turn_user_id == Some(item.opponent_id) {
            &item.opponent_username
        } else {
            &item.challenger_username
        };
        return Line::from(Span::styled(
            truncate_chars(&format!("{} is aiming", name(shooter)), width as usize),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ));
    }
    let label = item.game.label();
    let budget = (width as usize).saturating_sub(label.chars().count() + 1 + 3) / 2;
    let styled = |user_id: uuid::Uuid, username: &Option<String>| {
        let style = if item.turn_user_id == Some(user_id) {
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        Span::styled(truncate_chars(&name(username), budget), style)
    };
    Line::from(vec![
        Span::styled(format!("{label} "), Style::default().fg(theme::AMBER_DIM())),
        styled(item.challenger_id, &item.challenger_username),
        Span::styled(" v ", Style::default().fg(theme::TEXT_FAINT())),
        styled(item.opponent_id, &item.opponent_username),
    ])
}

/// The pool canvas paints every cell's background, so it borrows the
/// theme's canvas colour to sit flush on the card. A theme without an RGB
/// canvas gets the table's own surround.
pub(crate) fn canvas_background() -> Rgb {
    match theme::BG_CANVAS() {
        Color::Rgb(r, g, b) => [r, g, b],
        _ => SURROUND,
    }
}

fn name(username: &Option<String>) -> String {
    username.clone().unwrap_or_else(|| "player".to_string())
}

pub(crate) fn board_lines(width: u16, view: &LiveView<'_>, background: Rgb) -> Vec<Line<'static>> {
    let item = view.item;
    match view.board {
        LiveBoard::Chess { pieces, last } => chess_lines(width, pieces, *last),
        LiveBoard::ConnectFour { grid, last } => connect4_lines(width, grid, *last),
        LiveBoard::Reversi { grid, last } => reversi_lines(width, grid, *last),
        LiveBoard::Checkers { grid, last } => checkers_lines(width, grid, *last),
        LiveBoard::Pool {
            spec, balls, cue, ..
        } => pool_lines(width, spec, balls, *cue, view.aim, background),
        LiveBoard::Battleship { sides } => {
            let side = |username: &Option<String>, tally: &ShotTally| {
                vec![
                    name_line(username),
                    stat_line(format!("{} shots · {} hits", tally.shots, tally.hits)),
                    stat_line(format!("{} sunk", tally.sunk)),
                ]
            };
            let mut lines = side(&item.challenger_username, &sides[0]);
            lines.push(Line::from(""));
            lines.extend(side(&item.opponent_username, &sides[1]));
            lines
        }
        LiveBoard::Backgammon { white_id, pips } => {
            let (white, red) = if *white_id == item.challenger_id {
                (&item.challenger_username, &item.opponent_username)
            } else {
                (&item.opponent_username, &item.challenger_username)
            };
            vec![
                name_line(white),
                stat_line(format!("white · {} pips to go", pips[0])),
                Line::from(""),
                name_line(red),
                stat_line(format!("red · {} pips to go", pips[1])),
            ]
        }
        LiveBoard::Briscola {
            seat0_id,
            points,
            stock_remaining,
        } => {
            let (seat0, seat1) = if *seat0_id == item.challenger_id {
                (&item.challenger_username, &item.opponent_username)
            } else {
                (&item.opponent_username, &item.challenger_username)
            };
            vec![
                name_line(seat0),
                stat_line(format!("{} points", points[0])),
                Line::from(""),
                name_line(seat1),
                stat_line(format!("{} points", points[1])),
                Line::from(""),
                stat_line(format!("{stock_remaining} in the stock")),
            ]
        }
    }
}

fn name_line(username: &Option<String>) -> Line<'static> {
    Line::from(Span::styled(
        name(username),
        Style::default().fg(theme::TEXT()),
    ))
}

fn stat_line(text: String) -> Line<'static> {
    Line::from(Span::styled(
        format!("  {text}"),
        Style::default().fg(theme::TEXT_DIM()),
    ))
}

// ── Grid boards ───────────────────────────────────────────────

/// One board cell: a glyph on a square, two columns wide so a board of
/// eight reads as square.
struct Cell {
    glyph: char,
    fg: Color,
    bg: Color,
}

fn grid_line(width: u16, cells: impl Iterator<Item = Cell>) -> Line<'static> {
    let cells: Vec<Cell> = cells.collect();
    let pad = (width as usize).saturating_sub(cells.len() * 2) / 2;
    let mut spans = vec![Span::raw(" ".repeat(pad))];
    for cell in cells {
        spans.push(Span::styled(
            format!("{} ", cell.glyph),
            Style::default()
                .fg(cell.fg)
                .bg(cell.bg)
                .add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

const LIGHT_SQUARE: Color = Color::Rgb(240, 217, 181);
const DARK_SQUARE: Color = Color::Rgb(181, 136, 99);
const LIGHT_MOVED: Color = Color::Rgb(205, 210, 106);
const DARK_MOVED: Color = Color::Rgb(170, 162, 58);
const WHITE_PIECE: Color = Color::Rgb(252, 252, 248);
const BLACK_PIECE: Color = Color::Rgb(18, 18, 20);

/// White at the bottom, as the spectate board opens.
fn chess_lines(
    width: u16,
    pieces: &[Option<ChessPiece>; 64],
    last: Option<(usize, usize)>,
) -> Vec<Line<'static>> {
    (0..8)
        .rev()
        .map(|rank| {
            grid_line(
                width,
                (0..8).map(move |file| {
                    let index = rank * 8 + file;
                    let light = (rank + file) % 2 == 1;
                    let moved = last.is_some_and(|(from, to)| index == from || index == to);
                    let bg = match (light, moved) {
                        (true, false) => LIGHT_SQUARE,
                        (false, false) => DARK_SQUARE,
                        (true, true) => LIGHT_MOVED,
                        (false, true) => DARK_MOVED,
                    };
                    match pieces[index] {
                        Some(piece) => Cell {
                            glyph: piece_glyph(piece.kind),
                            fg: match piece.color {
                                ChessColor::White => WHITE_PIECE,
                                ChessColor::Black => BLACK_PIECE,
                            },
                            bg,
                        },
                        None => Cell {
                            glyph: ' ',
                            fg: bg,
                            bg,
                        },
                    }
                }),
            )
        })
        .collect()
}

const DISC: char = '●';
const KING: char = '◉';
const LAST_CELL: Color = Color::Rgb(150, 132, 60);

/// Row 0 is the bottom of the frame (`connect4::grid` fills upward), so it
/// is drawn last.
fn connect4_lines(
    width: u16,
    grid: &connect4::Grid,
    last: Option<(usize, usize)>,
) -> Vec<Line<'static>> {
    const FRAME: Color = Color::Rgb(34, 72, 158);
    (0..connect4::ROWS)
        .rev()
        .map(|row| {
            grid_line(
                width,
                (0..connect4::COLS).map(move |col| {
                    let bg = if last == Some((row, col)) {
                        LAST_CELL
                    } else {
                        FRAME
                    };
                    match grid[row][col] {
                        Some(connect4::Disc::Red) => Cell {
                            glyph: DISC,
                            fg: Color::Rgb(222, 52, 42),
                            bg,
                        },
                        Some(connect4::Disc::Yellow) => Cell {
                            glyph: DISC,
                            fg: Color::Rgb(242, 200, 44),
                            bg,
                        },
                        None => Cell {
                            glyph: '·',
                            fg: Color::Rgb(70, 106, 190),
                            bg,
                        },
                    }
                }),
            )
        })
        .collect()
}

/// Row 0 at the top, as the full board draws it.
fn reversi_lines(
    width: u16,
    grid: &reversi::Grid,
    last: Option<(usize, usize)>,
) -> Vec<Line<'static>> {
    const FELT: Color = Color::Rgb(32, 112, 64);
    (0..reversi::SIZE)
        .map(|row| {
            grid_line(
                width,
                (0..reversi::SIZE).map(move |col| {
                    let bg = if last == Some((row, col)) {
                        LAST_CELL
                    } else {
                        FELT
                    };
                    match grid[row][col] {
                        Some(reversi::Disc::Black) => Cell {
                            glyph: DISC,
                            fg: BLACK_PIECE,
                            bg,
                        },
                        Some(reversi::Disc::White) => Cell {
                            glyph: DISC,
                            fg: WHITE_PIECE,
                            bg,
                        },
                        None => Cell {
                            glyph: ' ',
                            fg: bg,
                            bg,
                        },
                    }
                }),
            )
        })
        .collect()
}

/// Row 0 at the top, as the full board draws it.
fn checkers_lines(
    width: u16,
    grid: &checkers::Grid,
    last: Option<(usize, usize)>,
) -> Vec<Line<'static>> {
    (0..checkers::SIZE)
        .map(|row| {
            grid_line(
                width,
                (0..checkers::SIZE).map(move |col| {
                    let bg = if last == Some((row, col)) {
                        LAST_CELL
                    } else if (row + col) % 2 == 1 {
                        DARK_SQUARE
                    } else {
                        LIGHT_SQUARE
                    };
                    match grid[row][col] {
                        Some(piece) => Cell {
                            glyph: if piece.king { KING } else { DISC },
                            fg: match piece.color {
                                checkers::Color::Red => Color::Rgb(200, 36, 36),
                                checkers::Color::White => WHITE_PIECE,
                            },
                            bg,
                        },
                        None => Cell {
                            glyph: ' ',
                            fg: bg,
                            bg,
                        },
                    }
                }),
            )
        })
        .collect()
}

// ── Pool ──────────────────────────────────────────────────────

/// The cloth's place on the canvas, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cloth {
    pub x0: i32,
    pub y0: i32,
    pub w: i32,
    pub h: i32,
}

/// Fit the cloth to a canvas `cols` wide and `px_h` pixels tall: a
/// one-pixel rail and one pixel of floor either side, the height following
/// the table's aspect. The full renderer's `View` cannot go this small: it
/// holds every ball at three pixels across, which on a table this size
/// would be a third of the cloth.
pub(crate) fn fit_cloth(spec: &TableSpec, cols: u16, px_h: u16) -> Cloth {
    let w = (cols as i32 - 4).max(1);
    let h = ((w as f64 * spec.width / spec.length).round() as i32).clamp(1, px_h as i32 - 2);
    Cloth {
        x0: 2,
        y0: (px_h as i32 - h) / 2,
        w,
        h,
    }
}

impl Cloth {
    /// Table metres to canvas pixels, fractional.
    fn to_px(self, spec: &TableSpec, at: [f64; 2]) -> (f64, f64) {
        (
            self.x0 as f64 + at[0] / spec.length * self.w as f64,
            self.y0 as f64 + at[1] / spec.width * self.h as f64,
        )
    }

    /// The pixel a ball lands on: never off the cloth, even at the cushion.
    fn cell(self, spec: &TableSpec, at: [f64; 2]) -> (i32, i32) {
        let (x, y) = self.to_px(spec, at);
        (
            (x.floor() as i32).clamp(self.x0, self.x0 + self.w - 1),
            (y.floor() as i32).clamp(self.y0, self.y0 + self.h - 1),
        )
    }
}

const CUE_STICK: Rgb = [206, 172, 118];

fn pool_lines(
    width: u16,
    spec: &TableSpec,
    balls: &[LiveBall],
    cue: Option<[f64; 2]>,
    aim: Option<&PoolAimShare>,
    background: Rgb,
) -> Vec<Line<'static>> {
    let mut canvas = Canvas::new(width, BOARD_ROWS, background);
    let cloth = fit_cloth(spec, width, canvas.height());
    let (x0, y0, x1, y1) = (
        cloth.x0,
        cloth.y0,
        cloth.x0 + cloth.w - 1,
        cloth.y0 + cloth.h - 1,
    );
    canvas.fill_rect(x0 - 1, y0 - 1, x1 + 1, y1 + 1, RAIL);
    canvas.fill_rect(x0, y0, x1, y1, CLOTH);
    let mid = x0 + cloth.w / 2;
    for (x, y) in [
        (x0 - 1, y0 - 1),
        (mid, y0 - 1),
        (x1 + 1, y0 - 1),
        (x0 - 1, y1 + 1),
        (mid, y1 + 1),
        (x1 + 1, y1 + 1),
    ] {
        canvas.set(x, y, POCKET);
    }

    // Ball in hand puts the cue ball where the shooter is carrying it.
    let cue = aim.and_then(|aim| aim.place).or(cue);
    let cue_px = cue.map(|at| cloth.to_px(spec, at));
    let dir = aim.and_then(|aim| {
        let (sin, cos) = aim.azimuth.sin_cos();
        let (dx, dy) = (
            cos * cloth.w as f64 / spec.length,
            sin * cloth.h as f64 / spec.width,
        );
        let len = (dx * dx + dy * dy).sqrt();
        (len > 0.0).then_some((dx / len, dy / len, aim.pull.clamp(0.0, 1.0)))
    });

    // The guide goes under the balls, as on the full table: a ball in the
    // way covers it.
    if let (Some((cx, cy)), Some((dx, dy, _))) = (cue_px, dir) {
        canvas.line(
            (cx + dx * 1.5, cy + dy * 1.5),
            (cx + dx * 7.0, cy + dy * 7.0),
            GUIDE,
            1,
            1,
        );
    }
    for ball in balls {
        let (x, y) = cloth.cell(spec, ball.pos);
        canvas.set(x, y, ball_colour(ball.id));
    }
    if let Some(at) = cue {
        let (x, y) = cloth.cell(spec, at);
        canvas.set(x, y, ball_colour(CUE));
    }
    // The stick over everything, drawn back as the shooter pulls it.
    if let (Some((cx, cy)), Some((dx, dy, pull))) = (cue_px, dir) {
        let back = 1.5 + pull * 3.0;
        canvas.line(
            (cx - dx * back, cy - dy * back),
            (cx - dx * (back + 7.0), cy - dy * (back + 7.0)),
            CUE_STICK,
            1,
            0,
        );
    }
    canvas.to_lines()
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    let mut out: String = chars.into_iter().take(max_chars - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
#[path = "live_board_test.rs"]
mod live_board_test;
