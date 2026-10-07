// The watch view. As a preview it draws beside the Games hub's rail (in the
// landing's place) while the rail sits on a live row: one header row, then
// the player's screen alone. Opened (Enter on the row) it takes the whole
// page: the header over the screen, and the watch chat docked beside it
// (`chat_dock`). The screen is the player's size, not ours: a smaller one is
// centered, a larger one is cropped to a window around the game's cursor
// (crawl and NetHack park it on the `@`), or pinned top-left while the game
// hides its cursor (`crop_anchor`; Brogue). Brogue's black canvas is keyed
// out the way its own door screen keys it, so the theme shows through.
//
// The chat only ever docks where the game keeps its whole minimum screen
// (`SpectateGame::screen_min`): 80x24 for crawl and NetHack, Brogue's fixed
// 100x34.
//
// `own_game_split` and the two drawers under it are the other end of the
// same chat: what a player sees of it around their own running game, a pane
// on the right, or one row underneath on a narrow terminal. The pane keeps
// the room's composer strip at its foot at all times, inert until F2 or a
// click opens it (`input::wants_own_chat`; the strip's title and
// placeholder say so), so nothing jumps when it does; the row stays
// read-only.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use super::chat::WatchLine;
use super::proxy::{LiveGame, WatchStatus};
use super::state::{SpectateGame, State, WatchMode};
use crate::app::chat::ui::{EmbeddedRoomChatView, draw_embedded_room_chat};
use crate::app::common::theme;
use crate::app::door::rebels::render::blit_screen_from;
use crate::app::files::terminal_image::TerminalImageFrame;

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

/// The dock for a watch of `game` drawn into `area` (header row included).
/// The renderer and the input layer both ask this, so the chat keys are live
/// exactly when the pane is on screen.
pub fn chat_dock(area: Rect, game: SpectateGame) -> ChatDock {
    let (min_cols, min_rows) = game.screen_min();
    let body_height = area.height.saturating_sub(1);
    if area.width > min_cols + DOCK_RIGHT_WIDTH {
        ChatDock::Right
    } else if body_height > min_rows + DOCK_BELOW_HEIGHT {
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

/// Where a cropped window centers on the player's screen, `(row, col)`: the
/// cursor while the game shows one (crawl and NetHack park it on the `@`),
/// the top-left corner while the game hides it. Brogue hides its cursor,
/// repaints only the cells that changed and leaves the cursor after the last
/// one, so a window following it would jump every frame.
pub fn crop_anchor(screen: &vt100::Screen) -> (u16, u16) {
    match screen.hide_cursor() {
        true => (0, 0),
        false => screen.cursor_position(),
    }
}

/// What the watch view draws around the watched screen.
pub enum WatchPane<'a> {
    /// The preview beside the hub's rail: the screen alone.
    Preview,
    /// An open watch: the chat docked beside the screen. The chat view is
    /// `None` until this session is in the room; the dock stays reserved.
    /// Boxed: the view dwarfs the data-less `Preview`.
    Open(Option<Box<EmbeddedRoomChatView<'a>>>),
}

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    view: &SpectateView<'_>,
    pane: WatchPane<'_>,
    terminal_images: &mut TerminalImageFrame,
) {
    let dock = match pane {
        WatchPane::Preview => ChatDock::Hidden,
        WatchPane::Open(_) => chat_dock(area, view.state.game()),
    };
    let layout = watch_layout(area, dock);
    let body = layout.screen;
    if let (WatchPane::Open(chat), Some((rule, chat_area))) = (pane, layout.chat) {
        draw_rule(frame, rule);
        if let Some(chat) = chat {
            let composer = draw_embedded_room_chat(frame, chat_area, *chat, terminal_images);
            join_rule_to_composer(frame, rule, composer);
        }
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
                let (anchor_row, anchor_col) = crop_anchor(screen);
                let x = fit_axis(cols, body.width, anchor_col);
                let y = fit_axis(rows, body.height, anchor_row);
                let target = Rect {
                    x: body.x + x.dst,
                    y: body.y + y.dst,
                    width: x.len,
                    height: y.len,
                };
                blit_screen_from(buf, target, screen, y.src, x.src);
            });
            match view.state.game() {
                SpectateGame::Brogue => {
                    crate::app::door::brogue::render::clear_canvas_black(buf, body);
                }
                SpectateGame::Dcss | SpectateGame::Nethack => {}
            }
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
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let lead = match view.state.mode() {
        WatchMode::Preview => " ",
        WatchMode::Open => " watching ",
    };
    let mut spans = vec![
        Span::styled(lead, dim),
        Span::styled(
            playname.to_string(),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" \u{b7} {}", view.state.game().label()), dim),
    ];
    if let Some(game) = view.entry {
        if !game.status.is_empty() {
            spans.push(Span::styled(format!(" \u{b7} {}", game.status), dim));
        }
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
            faint,
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);

    // A preview leaves its keys to the hub's footer. An open watch has the
    // whole page, so its keys ride the header's right end, with a word when
    // the chat has no room to dock (`i` then does nothing).
    let keys = match (view.state.mode(), dock) {
        (WatchMode::Preview, _) => return,
        (WatchMode::Open, ChatDock::Right | ChatDock::Below) => "` hop out \u{b7} Esc back ",
        (WatchMode::Open, ChatDock::Hidden) => {
            "widen the terminal to chat \u{b7} ` hop out \u{b7} Esc back "
        }
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(keys, faint))).alignment(Alignment::Right),
        area,
    );
}

/// Where the composer block's top and bottom borders run into a rule beside
/// it, the rule's cell becomes a tee, so the two read as one line.
fn join_rule_to_composer(frame: &mut Frame, rule: Rect, composer: Rect) {
    if rule.width != 1 || composer.height == 0 {
        return;
    }
    let buf = frame.buffer_mut();
    for y in [composer.y, composer.y + composer.height - 1] {
        if y >= rule.y && y < rule.y + rule.height {
            buf[(rule.x, y)].set_symbol("\u{251c}");
        }
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
    /// No room for either without costing the game its minimum screen.
    Hidden,
}

impl OwnChat {
    /// The pane, where a click opens the player's composer; `None` for the
    /// row, which has none.
    pub fn pane(&self) -> Option<Rect> {
        match self {
            Self::Pane { pane, .. } => Some(*pane),
            Self::Line(_) | Self::Hidden => None,
        }
    }
}

/// Split a player's own `game` area into the game and the watchers' chat.
/// The chat comes off the game's PTY, never over it, and only where the game
/// keeps its whole minimum screen.
pub fn own_game_split(area: Rect, game: SpectateGame) -> (Rect, OwnChat) {
    let (min_cols, min_rows) = game.screen_min();
    if area.width > min_cols + DOCK_RIGHT_WIDTH {
        return match dock_areas(area, ChatDock::Right) {
            (game, Some((rule, pane))) => (game, OwnChat::Pane { rule, pane }),
            (game, None) => (game, OwnChat::Hidden),
        };
    }
    if area.height <= min_rows {
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

/// Draw what `own_game_split` made room for beside a player's own running
/// game: the pane, or the read-only row underneath.
pub fn draw_own_chat(
    frame: &mut Frame,
    own_chat: OwnChat,
    chat: Option<EmbeddedRoomChatView<'_>>,
    line: Option<&WatchLine>,
    watchers: Option<usize>,
    terminal_images: &mut TerminalImageFrame,
) {
    match own_chat {
        OwnChat::Pane { rule, pane } => {
            draw_own_chat_pane(frame, rule, pane, chat, watchers, terminal_images);
        }
        OwnChat::Line(row) => draw_watch_line(frame, row, line, watchers),
        OwnChat::Hidden => {}
    }
}

/// The pane beside a player's own running game: their watchers' chat. One
/// faint header row says what it is, then the room's ordinary embedded chat,
/// composer strip included. The strip is the player's way in and is always
/// there, so the rows never jump: inert, its title and placeholder name F2
/// and the click (`chat::ui::ComposerInert::OwnWatchChat`); open, it names
/// its own keys (Enter sends, Esc hands the keys back to the game).
fn draw_own_chat_pane(
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
    // pane stays reserved and empty rather than resizing the game later, and
    // without the strip, since F2 has nowhere to write until then.
    if let Some(chat) = chat {
        let _composer = draw_embedded_room_chat(frame, rows[1], chat, terminal_images);
    }
}

/// The one row under a player's own running game, where the terminal is too
/// narrow for the pane: the newest thing a watcher
/// said, or a faint word that people are watching and quiet. Blank when
/// nobody is there, so the row costs a player without an audience nothing
/// but the row.
fn draw_watch_line(
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
