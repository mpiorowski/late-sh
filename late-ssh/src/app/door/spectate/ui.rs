// The watch view, drawn by the Games hub in place of its sidebar and landing
// while this session spectates. One header row (who, which game, how long,
// who else is watching, the keys), then the player's screen with the watch
// chat docked beside it (`chat_dock`). The screen is the player's size, not
// ours: a smaller one is centered, a larger one is cropped to a window that
// follows the cursor (crawl parks it on the `@`).
//
// `draw_watch_line` is the other end of the same chat: the one row a player
// sees under their own running game.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::chat::WatchLine;
use super::proxy::{LiveGame, WatchStatus};
use super::state::State;
use crate::app::chat::ui::{EmbeddedRoomChatView, draw_embedded_room_chat};
use crate::app::common::theme;
use crate::app::door::rebels::render::blit_screen_from;
use crate::app::files::terminal_image::TerminalImageFrame;

/// The narrowest and shortest screen crawl draws into. The chat only docks
/// where the watched screen keeps at least this much.
const SCREEN_MIN_COLS: u16 = 80;
const SCREEN_MIN_ROWS: u16 = 24;
const DOCK_RIGHT_WIDTH: u16 = 40;
const DOCK_BELOW_HEIGHT: u16 = 8;

/// Where the watch chat sits around the watched screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatDock {
    /// A column on the right, behind a one-column rule.
    Right,
    /// A strip underneath, under a one-row rule.
    Below,
    /// The terminal has no room for both: the screen keeps all of it.
    Hidden,
}

/// The dock for a watch view drawn into `area` (header row included). The
/// renderer and the input layer both ask this, so the chat keys are live
/// exactly when the pane is on screen.
pub fn chat_dock(area: Rect) -> ChatDock {
    let body_height = area.height.saturating_sub(1);
    if area.width > SCREEN_MIN_COLS + DOCK_RIGHT_WIDTH {
        ChatDock::Right
    } else if body_height > SCREEN_MIN_ROWS + DOCK_BELOW_HEIGHT {
        ChatDock::Below
    } else {
        ChatDock::Hidden
    }
}

/// The watched screen's area, then the rule and the chat pane when docked.
fn dock_areas(body: Rect, dock: ChatDock) -> (Rect, Option<(Rect, Rect)>) {
    match dock {
        ChatDock::Right => {
            let cols = Layout::horizontal([
                Constraint::Min(0),
                Constraint::Length(1),
                Constraint::Length(DOCK_RIGHT_WIDTH),
            ])
            .split(body);
            (cols[0], Some((cols[1], cols[2])))
        }
        ChatDock::Below => {
            let rows = Layout::vertical([
                Constraint::Min(0),
                Constraint::Length(1),
                Constraint::Length(DOCK_BELOW_HEIGHT),
            ])
            .split(body);
            (rows[0], Some((rows[1], rows[2])))
        }
        ChatDock::Hidden => (body, None),
    }
}

pub struct SpectateView<'a> {
    pub state: &'a State,
    /// The watched game's roster, for the position and watcher count.
    pub roster: &'a [LiveGame],
}

/// One axis of the fit: where to start reading the player's screen, how far
/// in to start drawing, and how many cells to draw.
#[derive(Debug, PartialEq, Eq)]
pub struct AxisFit {
    pub src: u16,
    pub dst: u16,
    pub len: u16,
}

/// Fit `screen_len` cells of the player's screen into `view_len` cells of
/// ours: centered when it fits, otherwise a window that keeps `cursor` in
/// the middle, clamped to the screen's edges.
pub fn fit_axis(screen_len: u16, view_len: u16, cursor: u16) -> AxisFit {
    if screen_len <= view_len {
        return AxisFit {
            src: 0,
            dst: (view_len - screen_len) / 2,
            len: screen_len,
        };
    }
    AxisFit {
        src: cursor
            .saturating_sub(view_len / 2)
            .min(screen_len - view_len),
        dst: 0,
        len: view_len,
    }
}

/// `chat` is the watched player's chat room, `None` until this session is in
/// it.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    view: &SpectateView<'_>,
    chat: Option<EmbeddedRoomChatView<'_>>,
    terminal_images: &mut TerminalImageFrame,
) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);
    let dock = match chat {
        Some(_) => chat_dock(area),
        None => ChatDock::Hidden,
    };
    let (body, chat_areas) = dock_areas(layout[1], dock);
    if let (Some(chat), Some((rule, pane))) = (chat, chat_areas) {
        draw_rule(frame, rule);
        draw_embedded_room_chat(frame, pane, chat, terminal_images);
    }

    let (rows, cols) = view.state.with_screen(|screen| screen.size());
    let cropped = cols > body.width || rows > body.height;
    draw_header(
        frame,
        layout[0],
        view,
        cropped.then_some((cols, rows)),
        dock,
    );

    let playname = view.state.playname();
    match view.state.status() {
        WatchStatus::Connecting => {
            draw_notice(frame, body, &format!("Joining {playname}'s game..."))
        }
        WatchStatus::Ended => draw_notice(frame, body, &format!("{playname}'s game has ended.")),
        WatchStatus::Watching => {
            let buf = frame.buffer_mut();
            view.state.with_screen(|screen| {
                let (cursor_row, cursor_col) = screen.cursor_position();
                let x = fit_axis(cols, body.width, cursor_col);
                let y = fit_axis(rows, body.height, cursor_row);
                let target = Rect {
                    x: body.x + x.dst,
                    y: body.y + y.dst,
                    width: x.len,
                    height: y.len,
                };
                blit_screen_from(buf, target, screen, y.src, x.src);
            });
        }
    }
}

fn draw_header(
    frame: &mut Frame,
    area: Rect,
    view: &SpectateView<'_>,
    cropped: Option<(u16, u16)>,
    dock: ChatDock,
) {
    let playname = view.state.playname();
    let dim = Style::default().fg(theme::TEXT_DIM());
    let mut spans = vec![
        Span::styled(" watching ", dim),
        Span::styled(
            playname.to_string(),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" \u{b7} {}", view.state.game().label()), dim),
    ];
    if let Some(game) = view.roster.iter().find(|g| g.playname == playname) {
        spans.push(Span::styled(
            format!(
                " \u{b7} {} in",
                duration_label(minutes_since(game.started_unix))
            ),
            dim,
        ));
        // The roster counts this session too.
        let others = game.watchers.saturating_sub(1);
        if others > 0 {
            spans.push(Span::styled(format!(" \u{b7} {others} also watching"), dim));
        }
    }
    if let Some((cols, rows)) = cropped {
        spans.push(Span::styled(
            format!(" \u{b7} their screen is {cols}x{rows}, yours is smaller"),
            Style::default().fg(theme::TEXT_FAINT()),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);

    let mut keys = Vec::new();
    match dock {
        ChatDock::Right | ChatDock::Below => keys.push(Span::styled("i chat \u{b7} ", dim)),
        ChatDock::Hidden => {}
    }
    if view.roster.len() > 1 {
        keys.push(Span::styled("\u{2190}/\u{2192} switch \u{b7} ", dim));
    }
    keys.push(Span::styled("Esc back ", dim));
    frame.render_widget(
        Paragraph::new(Line::from(keys)).alignment(Alignment::Right),
        area,
    );
}

/// The line between the watched screen and the chat: a column when the rule
/// area is one cell wide, a row otherwise.
fn draw_rule(frame: &mut Frame, area: Rect) {
    let style = Style::default().fg(theme::BORDER());
    let lines: Vec<Line<'static>> = if area.width == 1 {
        (0..area.height)
            .map(|_| Line::from(Span::styled("\u{2502}", style)))
            .collect()
    } else {
        vec![Line::from(Span::styled(
            "\u{2500}".repeat(area.width as usize),
            style,
        ))]
    };
    frame.render_widget(Paragraph::new(lines), area);
}

/// Split a player's own game area into the game and the watch line's row
/// under it. The row comes off the game's PTY, so it is only taken where the
/// game keeps the 24 rows crawl needs; a shorter terminal shows no line.
pub fn watch_row_split(area: Rect) -> (Rect, Option<Rect>) {
    if area.height <= SCREEN_MIN_ROWS {
        return (area, None);
    }
    let game = Rect {
        height: area.height - 1,
        ..area
    };
    let row = Rect {
        y: area.y + area.height - 1,
        height: 1,
        ..area
    };
    (game, Some(row))
}

/// The one row under a player's own running game: the newest thing a watcher
/// said, or a faint word that people are watching and quiet. Blank when
/// nobody is there, so the row costs a player without an audience nothing
/// but the row.
pub fn draw_watch_line(
    frame: &mut Frame,
    area: Rect,
    line: Option<&WatchLine>,
    watchers: Option<usize>,
) {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let faint = Style::default().fg(theme::TEXT_FAINT());
    match (line, watchers) {
        (Some(line), _) => {
            let name = Style::default().fg(theme::AMBER_DIM());
            let mut spans = vec![Span::styled(" ", dim)];
            if line.is_action {
                spans.push(Span::styled("* ", faint));
                spans.push(Span::styled(line.author.clone(), name));
                spans.push(Span::styled(format!(" {}", line.body), dim));
            } else {
                spans.push(Span::styled(line.author.clone(), name));
                spans.push(Span::styled(format!(": {}", line.body), dim));
            }
            frame.render_widget(Paragraph::new(Line::from(spans)), area);
            // The age sits at the right edge, over the tail of a message too
            // long for the row, so it is never the part that gets cut.
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(format!("  {} ", line.age), faint)))
                    .alignment(Alignment::Right),
                area,
            );
        }
        (None, Some(watchers)) if watchers > 0 => frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!(" {watchers} watching \u{b7} nobody has said anything"),
                faint,
            ))),
            area,
        ),
        (None, _) => {}
    }
}

fn draw_notice(frame: &mut Frame, area: Rect, text: &str) {
    let y = area.y + area.height / 2;
    if y >= area.y + area.height {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            text.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        )))
        .alignment(Alignment::Center),
        Rect {
            y,
            height: 1,
            ..area
        },
    );
}

/// Whole minutes since a game started, by the wall clock.
pub fn minutes_since(started_unix: u64) -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("wall clock after the unix epoch")
        .as_secs();
    now.saturating_sub(started_unix) / 60
}

/// `12m`, `3h 05m`: how long a game has been running, for the header and the
/// hub's watch list.
pub fn duration_label(minutes: u64) -> String {
    if minutes < 60 {
        format!("{minutes}m")
    } else {
        format!("{}h {:02}m", minutes / 60, minutes % 60)
    }
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
