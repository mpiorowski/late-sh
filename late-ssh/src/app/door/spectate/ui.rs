// The watch view. As a preview it draws beside the Games hub's rail (in the
// landing's place) while the rail sits on a live row: one header row, then
// the player's screen alone. Opened (Enter on the row) it takes the whole
// page: the header over the screen, and the watch chat beside it until F4
// hides it. The screen is the player's size, not ours: a smaller one is
// pinned top-left, the way every door draws its own game, a larger one is
// cropped to a window around the game's cursor (crawl and NetHack park it on
// the `@`), or pinned top-left while the game hides its cursor
// (`crop_anchor`; Brogue). Brogue's black canvas is keyed
// out the way its own door screen keys it, so the theme shows through.
//
// The chat is one column for both ends of it (`chat_split`,
// `draw_chat_column`): a watcher's beside the watched screen, a player's
// over their own running game while F4 has it open. As wide as the Realm
// board's chat, beside the game where the game keeps its whole minimum
// screen (`SpectateGame::screen_min`), else drawn over the game's right
// side, so the game is never squeezed below what it needs.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};

use super::proxy::WatchStatus;
use super::state::{LiveRow, SpectateGame, State, WatchMode};
use crate::app::chat::ui::{EmbeddedRoomChatView, draw_embedded_room_chat};
use crate::app::common::theme;
use crate::app::door::rebels::render::blit_screen_from;
use crate::app::files::terminal_image::TerminalImageFrame;

pub struct SpectateView<'a> {
    pub state: &'a State,
    /// The watched game's live row, for who is playing, where they are and
    /// how long they have run; `None` once the roster no longer lists it.
    pub row: Option<&'a LiveRow>,
    /// People with this watch open, this session's user included
    /// (`LiveGamesService::watchers_of`), for the chat column's header row.
    pub watchers: usize,
}

/// One axis of the fit: where to start reading the player's screen and how
/// many cells to draw. Drawing always starts at the view's edge.
#[derive(Debug, PartialEq, Eq)]
pub struct AxisFit {
    pub src: u16,
    pub len: u16,
}

/// Fit `screen_len` cells of the player's screen into `view_len` cells of
/// ours: the whole screen from the view's edge when it fits (the doors draw
/// their own games top-left, so a watch matches what the player sees),
/// otherwise a window that keeps `cursor` in the middle, clamped to the
/// screen's edges.
pub fn fit_axis(screen_len: u16, view_len: u16, cursor: u16) -> AxisFit {
    if screen_len <= view_len {
        return AxisFit {
            src: 0,
            len: screen_len,
        };
    }
    AxisFit {
        src: cursor
            .saturating_sub(view_len / 2)
            .min(screen_len - view_len),
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
// This transient render input stays on the stack to avoid allocating each frame.
#[allow(clippy::large_enum_variant)]
pub enum WatchPane<'a> {
    /// The preview beside the hub's rail: the screen alone.
    Preview,
    /// An open watch: the chat column beside the screen unless F4 hid it.
    /// The chat view is `None` until this session is in the room, or while
    /// the column is hidden; a shown column stays reserved meanwhile.
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
    let game = view.state.game();
    let (left, column) = match pane {
        WatchPane::Open(_) if view.state.chat_shown() => {
            let (left, column) = chat_split(area, game);
            (left, Some(column))
        }
        WatchPane::Open(_) | WatchPane::Preview => (area, None),
    };
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(left);
    // The header stops at the column, even where the column is drawn over
    // the screen.
    let header = match column {
        Some(column) => Rect {
            width: column.rule.x.saturating_sub(rows[0].x).min(rows[0].width),
            ..rows[0]
        },
        None => rows[0],
    };
    let body = rows[1];

    let (rows, cols) = view.state.with_screen(|screen| screen.size());
    let cropped = cols > body.width || rows > body.height;
    draw_header(frame, header, view, cropped.then_some((cols, rows)));

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
                    x: body.x,
                    y: body.y,
                    width: x.len,
                    height: y.len,
                };
                blit_screen_from(buf, target, screen, y.src, x.src);
            });
            match game {
                SpectateGame::Brogue => {
                    crate::app::door::brogue::render::clear_canvas_black(buf, body);
                }
                SpectateGame::Zork => {
                    crate::app::door::zork::render::inherit_theme_text(buf, body);
                }
                SpectateGame::Dcss | SpectateGame::Nethack => {}
            }
        }
    }

    if let (WatchPane::Open(chat), Some(column)) = (pane, column) {
        draw_chat_column(
            frame,
            game,
            ChatSide::Watcher,
            column,
            chat.map(|chat| *chat),
            None,
            view.watchers,
            terminal_images,
        );
    }
}

fn draw_header(
    frame: &mut Frame,
    area: Rect,
    view: &SpectateView<'_>,
    cropped: Option<(u16, u16)>,
) {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let lead = match view.state.mode() {
        WatchMode::Preview => " ",
        WatchMode::Open => " watching ",
    };
    let mut spans = vec![Span::styled(lead, dim)];
    spans.extend(player_spans(
        view.state.playname(),
        view.row.and_then(|row| row.username.as_deref()),
    ));
    spans.push(Span::styled(
        format!(" \u{b7} {}", view.state.game().label()),
        dim,
    ));
    if let Some(game) = view.row.map(|row| &row.entry) {
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
    }
    if let Some((cols, rows)) = cropped {
        spans.push(Span::styled(
            format!(" \u{b7} their screen is {cols}x{rows}, yours is smaller"),
            faint,
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);

    // A preview leaves its keys to the hub's footer. An open watch has the
    // whole page, so its keys ride the header's right end; the chat's F4
    // rides the frame title, as on the player's side.
    let keys = match view.state.mode() {
        WatchMode::Preview => return,
        WatchMode::Open => "` hop out \u{b7} Esc back ",
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(keys, faint))).alignment(Alignment::Right),
        area,
    );
}

/// Who is playing, in the header: `mat (Wormsong)`, the late.sh username in
/// amber and the arcade handle the game knows them by after it. The handle
/// alone while the row has no username (`LiveRow::username`), or once the
/// roster no longer lists the game; the username alone when the two only
/// differ in case.
fn player_spans(handle: &str, username: Option<&str>) -> Vec<Span<'static>> {
    let name = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);
    match username {
        Some(username) if !username.eq_ignore_ascii_case(handle) => vec![
            Span::styled(username.to_string(), name),
            Span::styled(
                format!(" ({handle})"),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ],
        Some(username) => vec![Span::styled(username.to_string(), name)],
        None => vec![Span::styled(handle.to_string(), name)],
    }
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

/// The line between the game and the chat column.
fn draw_rule(frame: &mut Frame, area: Rect) {
    let style = Style::default().fg(theme::BORDER());
    let lines: Vec<Line<'static>> = (0..area.height)
        .map(|_| Line::from(Span::styled("\u{2502}", style)))
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// The chat column, as wide as the Realm board's (`realm::ui::chat_width`):
/// a third of the screen, at least 34 columns and at most 64.
const CHAT_MIN_WIDTH: u16 = 34;
const CHAT_MAX_WIDTH: u16 = 64;

/// Where the watch chat sits beside a game: the rule and the column right
/// of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChatColumn {
    pub rule: Rect,
    pub pane: Rect,
}

/// Split `area` into `game` and the chat column. The column comes off the
/// game where the game keeps its whole minimum screen beside it; on a
/// narrower terminal the game keeps the whole area and the column is drawn
/// over its right side.
pub fn chat_split(area: Rect, game: SpectateGame) -> (Rect, ChatColumn) {
    let (min_cols, _) = game.screen_min();
    let width = (area.width / 3)
        .clamp(CHAT_MIN_WIDTH, CHAT_MAX_WIDTH)
        .min(area.width.saturating_sub(1));
    let pane = Rect {
        x: area.x + area.width - width,
        width,
        ..area
    };
    let rule = Rect {
        x: pane.x.saturating_sub(1),
        width: 1,
        ..area
    };
    let game_area = match area.width > min_cols + width {
        true => Rect {
            width: area.width - width - 1,
            ..area
        },
        false => area,
    };
    (game_area, ChatColumn { rule, pane })
}

/// The watch chat column, the same on both ends of it: one faint header row
/// saying how many are watching, then the room's ordinary embedded chat,
/// composer included. Drawn after the game, over it when the two overlap.
///
/// `suggestion` is a command a watcher suggested for the player's own Zork
/// game (`door/zork/suggest.rs`), which F3 types: it rides its own row under
/// the header.
#[allow(clippy::too_many_arguments)]
pub fn draw_chat_column(
    frame: &mut Frame,
    game: SpectateGame,
    side: ChatSide,
    column: ChatColumn,
    chat: Option<EmbeddedRoomChatView<'_>>,
    suggestion: Option<&str>,
    watchers: usize,
    terminal_images: &mut TerminalImageFrame,
) {
    let ChatColumn { rule, pane } = column;
    frame.render_widget(Clear, rule);
    frame.render_widget(Clear, pane);
    draw_rule(frame, rule);
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(pane);
    draw_chat_header(frame, rows[0], watchers, game, side);
    let body = match suggestion {
        Some(command) => {
            let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(rows[1]);
            draw_suggestion(frame, rows[0], command);
            rows[1]
        }
        None => rows[1],
    };
    // No room yet (it is still resolving, or the join has not landed): the
    // column stays up and empty until it does.
    if let Some(chat) = chat {
        let composer = draw_embedded_room_chat(frame, body, chat, terminal_images);
        join_rule_to_composer(frame, rule, composer);
    }
}

/// A watcher's suggested command on one row: the command as F3 would type
/// it, then the key.
fn draw_suggestion(frame: &mut Frame, area: Rect, command: &str) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" > ", Style::default().fg(theme::TEXT_FAINT())),
            Span::styled(
                command.to_string(),
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        area,
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "  F3 types it ",
            Style::default().fg(theme::TEXT_FAINT()),
        )))
        .alignment(Alignment::Right),
        area,
    );
}

/// Which end of a game's watch chat is reading it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSide {
    /// Someone watching the game.
    Watcher,
    /// The player whose game it is.
    Player,
}

/// What the watch chat says after its count: on the player's side, how to
/// get back to the game, and on Zork how a suggested command works from this
/// side of the chat (`door/zork/suggest.rs`).
fn suggestion_hint(game: SpectateGame, side: ChatSide) -> &'static str {
    match (game, side) {
        (SpectateGame::Zork, ChatSide::Watcher) => " \u{b7} >look to suggest",
        (SpectateGame::Zork, ChatSide::Player) => {
            " \u{b7} F3 takes a >suggestion \u{b7} Esc to the game"
        }
        (SpectateGame::Dcss | SpectateGame::Nethack | SpectateGame::Brogue, ChatSide::Watcher) => {
            ""
        }
        (SpectateGame::Dcss | SpectateGame::Nethack | SpectateGame::Brogue, ChatSide::Player) => {
            " \u{b7} Esc to the game"
        }
    }
}

/// The watch chat's header row, the one place a watch view or a player's
/// own game says how many are watching: everyone with the watch open, even
/// when that is nobody. Fits the column's 34-column minimum (Zork's wants
/// the widest column).
fn draw_chat_header(
    frame: &mut Frame,
    area: Rect,
    watchers: usize,
    game: SpectateGame,
    side: ChatSide,
) {
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {watchers} watching{}", suggestion_hint(game, side)),
            Style::default().fg(theme::TEXT_FAINT()),
        ))),
        area,
    );
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
