// The watch view, drawn beside the Games hub's rail (in the landing's place)
// while this session sits on one of the rail's live rows. One header row
// (who, which game, how long, who else is watching), then the player's
// screen with the watch chat docked beside it (`chat_dock`). The screen is
// the player's size, not ours: a smaller one is centered, a larger one is
// cropped to a window that follows the cursor (crawl parks it on the `@`).
//
// `own_game_split` and the two drawers under it are the other end of the
// same chat: what a player sees of it around their own running game, a
// read-only pane on the right, or one row underneath on a narrow terminal.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::chat::WatchLine;
use super::proxy::{LiveGame, WatchStatus};
use super::state::State;
use crate::app::chat::ui::{
    EmbeddedRoomChatView, draw_embedded_room_chat, draw_embedded_room_messages,
};
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

/// The watch view's pieces inside its `area`.
#[derive(Debug, PartialEq, Eq)]
struct WatchLayout {
    header: Rect,
    screen: Rect,
    /// The rule and the chat pane, when the chat is docked.
    chat: Option<(Rect, Rect)>,
}

/// Lay the watch view out. A chat docked on the right takes the full height
/// of the view, so its rule runs from the frame's top border down and the
/// header sits over the watched screen alone; a chat docked below leaves the
/// header the full width.
fn watch_layout(area: Rect, dock: ChatDock) -> WatchLayout {
    let header_over = |area: Rect| {
        let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(area);
        (rows[0], rows[1])
    };
    match dock {
        ChatDock::Right => {
            let (left, chat) = dock_areas(area, ChatDock::Right);
            let (header, screen) = header_over(left);
            WatchLayout {
                header,
                screen,
                chat,
            }
        }
        ChatDock::Below | ChatDock::Hidden => {
            let (header, body) = header_over(area);
            let (screen, chat) = dock_areas(body, dock);
            WatchLayout {
                header,
                screen,
                chat,
            }
        }
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
    /// The watched game's roster entry, for how long it has run and who else
    /// is watching; `None` once the roster no longer lists it.
    pub entry: Option<&'a LiveGame>,
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
    let dock = match chat {
        Some(_) => chat_dock(area),
        None => ChatDock::Hidden,
    };
    let layout = watch_layout(area, dock);
    let body = layout.screen;
    if let (Some(chat), Some((rule, pane))) = (chat, layout.chat) {
        draw_rule(frame, rule);
        draw_embedded_room_chat(frame, pane, chat, terminal_images);
    }

    let (rows, cols) = view.state.with_screen(|screen| screen.size());
    let cropped = cols > body.width || rows > body.height;
    draw_header(
        frame,
        layout.header,
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
    if let Some(game) = view.entry {
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

    // The hub's footer carries the keys; the header only says when the chat
    // has no room to dock, since `i` then does nothing.
    match dock {
        ChatDock::Right | ChatDock::Below => {}
        ChatDock::Hidden => frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "widen the terminal to chat ",
                Style::default().fg(theme::TEXT_FAINT()),
            )))
            .alignment(Alignment::Right),
            area,
        ),
    }
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

/// How a player's own running game shares its area with the watchers' chat.
#[derive(Debug, PartialEq, Eq)]
pub enum OwnChat {
    /// A read-only pane on the right, behind a rule: the same dock the
    /// watchers get.
    Pane { rule: Rect, pane: Rect },
    /// One row under the game, on a terminal too narrow for the pane.
    Line(Rect),
    /// No room for either without costing crawl its 80x24.
    Hidden,
}

/// Split a player's own game area into the game and the watchers' chat. The
/// chat comes off the game's PTY, never over it, and only where the game
/// keeps the 80x24 crawl needs.
pub fn own_game_split(area: Rect) -> (Rect, OwnChat) {
    if area.width > SCREEN_MIN_COLS + DOCK_RIGHT_WIDTH {
        return match dock_areas(area, ChatDock::Right) {
            (game, Some((rule, pane))) => (game, OwnChat::Pane { rule, pane }),
            (game, None) => (game, OwnChat::Hidden),
        };
    }
    if area.height <= SCREEN_MIN_ROWS {
        return (area, OwnChat::Hidden);
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
    (game, OwnChat::Line(row))
}

/// The pane beside a player's own running game: their watchers' chat, read
/// only. One faint header row says what it is (the player has no composer to
/// say it for them), then the room's messages.
pub fn draw_own_chat_pane(
    frame: &mut Frame,
    rule: Rect,
    pane: Rect,
    chat: Option<EmbeddedRoomChatView<'_>>,
    watchers: Option<usize>,
    terminal_images: &mut TerminalImageFrame,
) {
    draw_rule(frame, rule);
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(pane);
    let header = match watchers {
        Some(watchers) if watchers > 0 => format!(" watcher chat \u{b7} {watchers} watching"),
        Some(_) | None => " watcher chat".to_string(),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            header,
            Style::default().fg(theme::TEXT_FAINT()),
        ))),
        rows[0],
    );
    // No room yet (it is still resolving, or the join has not landed): the
    // pane stays reserved and empty rather than resizing the game later.
    if let Some(chat) = chat {
        draw_embedded_room_messages(frame, rows[1], chat, terminal_images);
    }
}

/// The one row under a player's own running game, where the terminal is too
/// narrow for the pane: the newest thing a watcher
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
/// hub rail's live rows.
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
