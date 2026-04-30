use std::time::Duration;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::chat::ui::{
        DashboardChatView, dashboard_pinned_height, draw_dashboard_chat_card,
        draw_dashboard_pinned_messages,
    },
    app::common::{
        primitives::{format_duration_mmss, genre_label},
        theme,
    },
    app::rooms::{
        blackjack::state::BlackjackSnapshot,
        svc::{GameKind, RoomListItem, RoomsSnapshot},
    },
    app::vote::svc::{Genre, VoteCount},
    app::vote::ui::{VoteCardView, draw_vote_card},
};
use late_core::models::chat_message::ChatMessage;

// `draw_dashboard` receives the content pane after the outer app border and the
// fixed 24-column sidebar are removed. A 77-column terminal yields 51 columns
// of dashboard content, which is where we want to hide voting and keep screen 1
// usable for chat.
const DASHBOARD_HIDE_VOTE_AT_WIDTH: u16 = 51;
// A 65-column terminal yields 39 columns of dashboard content, which is where
// we drop the top stream/music card entirely so chat can use the reclaimed
// vertical space.
const DASHBOARD_HIDE_STREAM_AT_WIDTH: u16 = 39;
// Below this many rows the fixed 5-row stream card plus chat card no longer
// fit cleanly, so we collapse to chat-only rather than render clipped blocks.
const DASHBOARD_MIN_FULL_HEIGHT: u16 = 16;
const BLACKJACK_GRID_MIN_WIDTH: u16 = 48;
const BLACKJACK_GRID_MIN_CHAT_HEIGHT: u16 = 6;
const BLACKJACK_GRID_COLUMNS: usize = 3;
const BLACKJACK_GRID_TEXT_ROWS: u16 = 3;
const BLACKJACK_GRID_HEIGHT: u16 = BLACKJACK_GRID_TEXT_ROWS + 1; // + bottom rule
const AUDIO_BUTTON_PREFIX: &str = "No audio? ";
const CLI_BUTTON_TEXT: &str = "[B] CLI";
const PAIR_BUTTON_TEXT: &str = "[P] web";

pub struct DashboardRenderInput<'a> {
    pub now_playing: Option<&'a str>,
    pub vote_counts: &'a VoteCount,
    pub current_genre: Genre,
    pub next_switch_in: Duration,
    pub my_vote: Option<Genre>,
    pub show_header: bool,
    /// When `Some`, the user has 2+ favorites pinned and we render a
    /// quick-switch strip directly above the chat card. Each entry is
    /// `(room_id, label, is_active, unread_count)`. `None` hides the strip.
    pub favorites_strip: Option<&'a [(uuid::Uuid, String, bool, i64)]>,
    /// Pinned chat messages visible to this user; rendered as a slim amber
    /// strip above the favorites strip. Empty slice = no strip.
    pub pinned_messages: &'a [ChatMessage],
    pub show_room_showcases: bool,
    pub rooms_snapshot: &'a RoomsSnapshot,
    pub blackjack_snapshots: &'a std::collections::HashMap<uuid::Uuid, BlackjackSnapshot>,
    pub blackjack_prefix_armed: bool,
    pub chat_view: DashboardChatView<'a>,
}

pub fn draw_dashboard(frame: &mut Frame, area: Rect, view: DashboardRenderInput<'_>) {
    if !view.show_header {
        draw_chat_section(
            frame,
            area,
            view.pinned_messages,
            view.favorites_strip,
            view.chat_view,
        );
        return;
    }

    let stream_props = StreamCardProps {
        now_playing: view.now_playing.unwrap_or("Waiting for stream..."),
        current_genre: view.current_genre,
        leading_genre: view.vote_counts.winner_or(view.current_genre),
        next_switch_in: view.next_switch_in,
    };
    if area.width <= DASHBOARD_HIDE_STREAM_AT_WIDTH || area.height < DASHBOARD_MIN_FULL_HEIGHT {
        draw_chat_section(
            frame,
            area,
            view.pinned_messages,
            view.favorites_strip,
            view.chat_view,
        );
        return;
    }

    let sections = Layout::vertical([Constraint::Length(5), Constraint::Fill(1)]).split(area);

    if area.width <= DASHBOARD_HIDE_VOTE_AT_WIDTH {
        draw_stream_card(frame, sections[0], &stream_props);
    } else {
        let top = Layout::horizontal([Constraint::Fill(2), Constraint::Fill(1)]).split(sections[0]);
        draw_stream_card(frame, top[0], &stream_props);
        draw_vote_card(
            frame,
            top[1],
            &VoteCardView {
                vote_counts: view.vote_counts,
                my_vote: view.my_vote,
            },
        );
    }

    draw_blackjack_and_chat_section(frame, sections[1], view);
}

fn draw_blackjack_and_chat_section(frame: &mut Frame, area: Rect, view: DashboardRenderInput<'_>) {
    let rooms = dashboard_blackjack_rooms(view.rooms_snapshot);
    if view.show_room_showcases
        && let Some(grid_height) = blackjack_grid_height(area)
    {
        let split =
            Layout::vertical([Constraint::Length(grid_height), Constraint::Fill(1)]).split(area);
        draw_blackjack_grid(
            frame,
            split[0],
            &rooms,
            view.blackjack_snapshots,
            view.blackjack_prefix_armed,
        );
        draw_chat_section(
            frame,
            split[1],
            view.pinned_messages,
            view.favorites_strip,
            view.chat_view,
        );
    } else {
        draw_chat_section(
            frame,
            area,
            view.pinned_messages,
            view.favorites_strip,
            view.chat_view,
        );
    }
}

fn blackjack_grid_height(area: Rect) -> Option<u16> {
    if area.width < BLACKJACK_GRID_MIN_WIDTH {
        return None;
    }

    if area.height >= BLACKJACK_GRID_HEIGHT.saturating_add(BLACKJACK_GRID_MIN_CHAT_HEIGHT) {
        Some(BLACKJACK_GRID_HEIGHT)
    } else {
        None
    }
}

fn dashboard_blackjack_rooms(snapshot: &RoomsSnapshot) -> Vec<&RoomListItem> {
    snapshot
        .rooms
        .iter()
        .filter(|room| matches!(room.game_kind, GameKind::Blackjack))
        .take(BLACKJACK_GRID_COLUMNS)
        .collect()
}

fn draw_blackjack_grid(
    frame: &mut Frame,
    area: Rect,
    rooms: &[&RoomListItem],
    snapshots: &std::collections::HashMap<uuid::Uuid, BlackjackSnapshot>,
    prefix_armed: bool,
) {
    if area.height < BLACKJACK_GRID_HEIGHT {
        return;
    }

    let chunks = Layout::vertical([
        Constraint::Length(BLACKJACK_GRID_TEXT_ROWS),
        Constraint::Length(1),
    ])
    .split(area);

    // 7-track horizontal layout: vert | col1 | vert | col2 | vert | col3 | vert
    let cols = Layout::horizontal([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .split(chunks[0]);

    let border_style = Style::default().fg(theme::BORDER_DIM());
    for &vert_idx in &[0usize, 2, 4, 6] {
        let lines = vec![Line::from("│"), Line::from("│"), Line::from("│")];
        frame.render_widget(Paragraph::new(lines).style(border_style), cols[vert_idx]);
    }

    let slot_areas = [cols[1], cols[3], cols[5]];
    let loading = rooms.is_empty();
    for (slot, area) in slot_areas.iter().enumerate().take(BLACKJACK_GRID_COLUMNS) {
        let room = rooms.get(slot).copied();
        // 1-char left/right padding inside each column.
        let padded = if area.width >= 4 {
            Rect {
                x: area.x + 1,
                width: area.width - 2,
                ..*area
            }
        } else {
            *area
        };
        draw_blackjack_slot(frame, padded, slot, room, snapshots, prefix_armed, loading);
    }

    let rule_area = chunks[1];
    let junctions = [
        (cols[0].x.saturating_sub(rule_area.x) as usize, '└'),
        (cols[2].x.saturating_sub(rule_area.x) as usize, '┴'),
        (cols[4].x.saturating_sub(rule_area.x) as usize, '┴'),
        (cols[6].x.saturating_sub(rule_area.x) as usize, '┘'),
    ];
    draw_blackjack_bottom_rule(frame, rule_area, &junctions, prefix_armed);
}

fn draw_blackjack_bottom_rule(
    frame: &mut Frame,
    area: Rect,
    junctions: &[(usize, char)],
    prefix_armed: bool,
) {
    let total_w = area.width as usize;
    if total_w == 0 {
        return;
    }

    let hint_text = if prefix_armed {
        " press 1/2/3 "
    } else {
        " b + 1/2/3 to join "
    };
    let hint_chars: Vec<char> = hint_text.chars().collect();
    let hint_w = hint_chars.len();

    let make_rule_char = |i: usize| -> char {
        junctions
            .iter()
            .find_map(|(off, ch)| if *off == i { Some(*ch) } else { None })
            .unwrap_or('─')
    };

    let border_style = Style::default().fg(theme::BORDER_DIM());

    // No room for hint — render plain rule (still with junctions).
    if hint_w + 4 > total_w {
        let rule: String = (0..total_w).map(make_rule_char).collect();
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(rule, border_style))),
            area,
        );
        return;
    }

    let hint_start = (total_w - hint_w) / 2;
    let hint_end = hint_start + hint_w;
    let left: String = (0..hint_start).map(make_rule_char).collect();
    let right: String = (hint_end..total_w).map(make_rule_char).collect();

    let hint_style = if prefix_armed {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(left, border_style),
            Span::styled(hint_text.to_string(), hint_style),
            Span::styled(right, border_style),
        ])),
        area,
    );
}

fn draw_blackjack_slot(
    frame: &mut Frame,
    area: Rect,
    slot: usize,
    room: Option<&RoomListItem>,
    snapshots: &std::collections::HashMap<uuid::Uuid, BlackjackSnapshot>,
    prefix_armed: bool,
    loading: bool,
) {
    if area.width < 10 || area.height < BLACKJACK_GRID_TEXT_ROWS {
        return;
    }

    let key_char = match slot {
        0 => '1',
        1 => '2',
        2 => '3',
        _ => '?',
    };
    let key_style = if prefix_armed {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else if room.is_some() {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let key_tag = Span::styled(format!("b{key_char} "), key_style);

    let inner_width = area.width as usize;

    let Some(room) = room else {
        let label = if loading { "loading…" } else { "open slot" };
        let lines = vec![
            Line::from(vec![
                key_tag,
                Span::styled(label, Style::default().fg(theme::TEXT_FAINT())),
            ]),
            Line::from(""),
            Line::from(""),
        ];
        frame.render_widget(Paragraph::new(lines), area);
        return;
    };

    let snapshot = snapshots.get(&room.id);
    let max_seats: usize = snapshot.map(|s| s.seats.len()).unwrap_or(4);
    let occupied: Option<usize> =
        snapshot.map(|s| s.seats.iter().filter(|seat| seat.user_id.is_some()).count());

    let name_budget = inner_width.saturating_sub(3).max(4); // room for "bN "
    let line1 = Line::from(vec![
        key_tag,
        Span::styled(
            truncate(&room.display_name, name_budget),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let pace_label = room.blackjack_settings.pace.label();
    let stake_label = room.blackjack_settings.stake_label();
    let line2 = Line::from(vec![
        Span::styled(
            pace_label,
            Style::default()
                .fg(theme::AMBER_DIM())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · ", Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(stake_label, Style::default().fg(theme::TEXT_DIM())),
    ]);

    let mut seat_text = String::new();
    for i in 0..max_seats {
        let filled = occupied.map(|o| i < o).unwrap_or(false);
        seat_text.push(if filled { '●' } else { '○' });
    }
    let count_label = match occupied {
        Some(n) => format!(" {}/{}", n, max_seats),
        None => format!(" ?/{}", max_seats),
    };
    let phase_label = blackjack_phase_label(snapshot);
    let phase_color = match snapshot.map(|s| s.phase) {
        Some(crate::app::rooms::blackjack::state::Phase::PlayerTurn)
        | Some(crate::app::rooms::blackjack::state::Phase::DealerTurn) => theme::AMBER_GLOW(),
        Some(_) => theme::TEXT(),
        None => theme::TEXT_FAINT(),
    };
    let line3 = Line::from(vec![
        Span::styled(seat_text, Style::default().fg(theme::AMBER())),
        Span::styled(count_label, Style::default().fg(theme::TEXT_DIM())),
        Span::styled(" · ", Style::default().fg(theme::TEXT_FAINT())),
        Span::styled(phase_label, Style::default().fg(phase_color)),
    ]);

    frame.render_widget(Paragraph::new(vec![line1, line2, line3]), area);
}

fn blackjack_phase_label(snapshot: Option<&BlackjackSnapshot>) -> String {
    use crate::app::rooms::blackjack::state::Phase;
    let Some(snap) = snapshot else {
        return "idle".to_string();
    };
    match snap.phase {
        Phase::Betting => match snap.betting_countdown_secs {
            Some(secs) => format!("betting · {secs}s"),
            None => "betting".to_string(),
        },
        Phase::BetPending => "bet pending".to_string(),
        Phase::PlayerTurn => "player turn".to_string(),
        Phase::DealerTurn => "dealer".to_string(),
        Phase::Settling => "settling".to_string(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

pub(crate) fn favorites_strip_hit_test(
    area: Rect,
    show_header: bool,
    pins: &[(uuid::Uuid, String, bool, i64)],
    pinned_count: usize,
    x: u16,
    y: u16,
) -> Option<uuid::Uuid> {
    let strip_area = favorites_strip_area(area, show_header, pins, pinned_count)?;
    if y != strip_area.y || x < strip_area.x || x >= strip_area.right() {
        return None;
    }

    let mut cursor_x = strip_area.x + 1;
    for (idx, (room_id, label, _, unread)) in pins.iter().enumerate() {
        if idx > 0 {
            cursor_x = cursor_x.saturating_add(1);
        }
        let slot = if idx < 9 {
            format!("{}:", idx + 1)
        } else {
            String::new()
        };
        let unread_suffix = if *unread > 0 {
            format!(" ({unread})")
        } else {
            String::new()
        };
        let pill = format!(" {slot}{label}{unread_suffix} ");
        let width = UnicodeWidthStr::width(pill.as_str()) as u16;
        let end_x = cursor_x.saturating_add(width);
        if x >= cursor_x && x < end_x {
            return Some(*room_id);
        }
        cursor_x = end_x;
    }
    None
}

pub(crate) fn cli_install_button_hit_test(area: Rect, show_header: bool, x: u16, y: u16) -> bool {
    let Some(button_area) = cli_install_button_area(area, show_header) else {
        return false;
    };
    y == button_area.y && x >= button_area.x && x < button_area.right()
}

pub(crate) fn browser_pair_button_hit_test(area: Rect, show_header: bool, x: u16, y: u16) -> bool {
    let Some(button_area) = browser_pair_button_area(area, show_header) else {
        return false;
    };
    y == button_area.y && x >= button_area.x && x < button_area.right()
}

/// Draws the chat card with two optional strips stacked above it: pinned
/// messages first (admin-curated), then the favorites pill strip. Each is
/// only inserted when present and there's room for a useful chat card below.
fn draw_chat_section(
    frame: &mut Frame,
    area: Rect,
    pinned_messages: &[ChatMessage],
    favorites_strip: Option<&[(uuid::Uuid, String, bool, i64)]>,
    chat_view: DashboardChatView<'_>,
) {
    let mut remaining = area;

    let pinned_height = dashboard_pinned_height(pinned_messages.len(), remaining.height);
    if pinned_height > 0 {
        let split = Layout::vertical([Constraint::Length(pinned_height), Constraint::Fill(1)])
            .split(remaining);
        draw_dashboard_pinned_messages(frame, split[0], pinned_messages);
        remaining = split[1];
    }

    if let Some(pins) = favorites_strip
        && pins.len() >= 2
        && remaining.height >= 6
    {
        let split = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(remaining);
        draw_favorites_strip(frame, split[0], pins);
        remaining = split[1];
    }

    draw_dashboard_chat_card(frame, remaining, chat_view);
}

fn favorites_strip_area(
    area: Rect,
    show_header: bool,
    pins: &[(uuid::Uuid, String, bool, i64)],
    pinned_count: usize,
) -> Option<Rect> {
    if pins.len() < 2 {
        return None;
    }

    let chat_area = if show_header {
        if area.width <= DASHBOARD_HIDE_STREAM_AT_WIDTH || area.height < DASHBOARD_MIN_FULL_HEIGHT {
            area
        } else {
            Layout::vertical([Constraint::Length(5), Constraint::Fill(1)]).split(area)[1]
        }
    } else {
        area
    };

    let pinned_height = dashboard_pinned_height(pinned_count, chat_area.height);
    let after_pinned = if pinned_height > 0 {
        Layout::vertical([Constraint::Length(pinned_height), Constraint::Fill(1)]).split(chat_area)
            [1]
    } else {
        chat_area
    };

    if after_pinned.height < 6 {
        return None;
    }

    Some(Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(after_pinned)[0])
}

fn draw_favorites_strip(frame: &mut Frame, area: Rect, pins: &[(uuid::Uuid, String, bool, i64)]) {
    let mut spans: Vec<Span<'static>> = vec![Span::raw(" ")];
    for (idx, (_, label, active, unread)) in pins.iter().enumerate() {
        if idx > 0 {
            spans.push(Span::raw(" "));
        }
        // Slot hint doubles as the `g<digit>` target; only 1..9 are reachable
        // via the prefix, so pins beyond nine render without a number.
        let slot = if idx < 9 {
            format!("{}:", idx + 1)
        } else {
            String::new()
        };
        let style = if *active {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_HIGHLIGHT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        let unread_suffix = if *unread > 0 {
            format!(" ({unread})")
        } else {
            String::new()
        };
        spans.push(Span::styled(
            format!(" {slot}{label}{unread_suffix} "),
            style,
        ));
    }
    spans.push(Span::styled(
        "   [] cycle · , last · g_ jump",
        Style::default().fg(theme::TEXT_FAINT()),
    ));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub struct StreamCardProps<'a> {
    pub now_playing: &'a str,
    pub current_genre: Genre,
    pub leading_genre: Genre,
    pub next_switch_in: Duration,
}

fn draw_stream_card(frame: &mut Frame, area: Rect, props: &StreamCardProps<'_>) {
    let block = Block::default()
        .title(" Stream ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let inner = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(1),
        ..inner
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Playing: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(props.now_playing, Style::default().fg(theme::TEXT_BRIGHT())),
        ]),
        Line::from(vec![
            Span::styled("Vibe: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                genre_label(props.current_genre),
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  Next: ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                genre_label(props.leading_genre),
                Style::default().fg(theme::AMBER_DIM()),
            ),
            Span::styled("  Switch in ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format_duration_mmss(props.next_switch_in),
                Style::default().fg(theme::TEXT()),
            ),
        ]),
        Line::from(vec![
            Span::styled(AUDIO_BUTTON_PREFIX, Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                CLI_BUTTON_TEXT,
                Style::default()
                    .fg(theme::BG_CANVAS())
                    .bg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                PAIR_BUTTON_TEXT,
                Style::default()
                    .fg(theme::BG_CANVAS())
                    .bg(theme::BORDER_ACTIVE())
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
}

fn cli_install_button_area(area: Rect, show_header: bool) -> Option<Rect> {
    audio_button_area(area, show_header, 0)
}

fn browser_pair_button_area(area: Rect, show_header: bool) -> Option<Rect> {
    audio_button_area(area, show_header, 1)
}

fn audio_button_area(area: Rect, show_header: bool, button_index: usize) -> Option<Rect> {
    let inner = stream_text_area(area, show_header)?;
    let prefix_width = UnicodeWidthStr::width(AUDIO_BUTTON_PREFIX) as u16;
    let cli_width = UnicodeWidthStr::width(CLI_BUTTON_TEXT) as u16;
    let gap_width = 2u16;
    let pair_width = UnicodeWidthStr::width(PAIR_BUTTON_TEXT) as u16;
    let (offset, width) = match button_index {
        0 => (prefix_width, cli_width),
        1 => (
            prefix_width
                .saturating_add(cli_width)
                .saturating_add(gap_width),
            pair_width,
        ),
        _ => return None,
    };
    Some(Rect::new(
        inner.x.saturating_add(offset),
        inner.y.saturating_add(2),
        width,
        1,
    ))
}

fn stream_text_area(area: Rect, show_header: bool) -> Option<Rect> {
    if !show_header
        || area.width <= DASHBOARD_HIDE_STREAM_AT_WIDTH
        || area.height < DASHBOARD_MIN_FULL_HEIGHT
    {
        return None;
    }

    let sections = Layout::vertical([Constraint::Length(5), Constraint::Fill(1)]).split(area);
    let stream_area = if area.width <= DASHBOARD_HIDE_VOTE_AT_WIDTH {
        sections[0]
    } else {
        Layout::horizontal([Constraint::Fill(2), Constraint::Fill(1)]).split(sections[0])[0]
    };
    let inner = Block::default().borders(Borders::ALL).inner(stream_area);
    let inner = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(1),
        ..inner
    };
    Some(inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::chat::ui::ChatRowsCache;
    use late_core::models::leaderboard::BadgeTier;
    use ratatui::{Terminal, backend::TestBackend};
    use std::collections::HashMap;
    use uuid::Uuid;

    fn render_dashboard(width: u16) -> Vec<String> {
        render_dashboard_with_size(width, 20)
    }

    fn render_dashboard_with_size(width: u16, height: u16) -> Vec<String> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let vote_counts = VoteCount {
            lofi: 3,
            ambient: 2,
            classic: 1,
            jazz: 0,
        };
        let mut rows_cache = ChatRowsCache::default();
        let usernames: HashMap<Uuid, String> = HashMap::new();
        let countries: HashMap<Uuid, String> = HashMap::new();
        let badges: HashMap<Uuid, BadgeTier> = HashMap::new();
        let bonsai_glyphs: HashMap<Uuid, String> = HashMap::new();
        let message_reactions = HashMap::new();
        let composer = ratatui_textarea::TextArea::default();
        let rooms_snapshot = RoomsSnapshot::default();
        let blackjack_snapshots: HashMap<Uuid, BlackjackSnapshot> = HashMap::new();

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, width, height);
                draw_dashboard(
                    frame,
                    area,
                    DashboardRenderInput {
                        now_playing: Some("Boards of Canada"),
                        vote_counts: &vote_counts,
                        current_genre: Genre::Lofi,
                        next_switch_in: Duration::from_secs(95),
                        my_vote: Some(Genre::Ambient),
                        show_header: true,
                        favorites_strip: None,
                        pinned_messages: &[],
                        show_room_showcases: true,
                        rooms_snapshot: &rooms_snapshot,
                        blackjack_snapshots: &blackjack_snapshots,
                        blackjack_prefix_armed: false,
                        chat_view: DashboardChatView {
                            messages: &[],
                            overlay: None,
                            rows_cache: &mut rows_cache,
                            usernames: &usernames,
                            countries: &countries,
                            badges: &badges,
                            message_reactions: &message_reactions,
                            current_user_id: Uuid::nil(),
                            selected_message_id: None,
                            highlighted_message_id: None,
                            reaction_picker_active: false,
                            composer: &composer,
                            composing: false,
                            mention_matches: &[],
                            mention_selected: 0,
                            mention_active: false,
                            reply_author: None,
                            is_editing: false,
                            bonsai_glyphs: &bonsai_glyphs,
                        },
                    },
                );
            })
            .expect("draw");

        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                let mut out = String::new();
                for x in 0..width {
                    out.push_str(buffer[(x, y)].symbol());
                }
                out
            })
            .collect()
    }

    #[test]
    fn dashboard_hides_vote_card_at_77_columns() {
        let lines = render_dashboard(DASHBOARD_HIDE_VOTE_AT_WIDTH);
        assert!(!lines.join("\n").contains("Vote Next"));
        assert_eq!(lines[0].chars().filter(|c| *c == '┌').count(), 1);
    }

    #[test]
    fn dashboard_keeps_vote_card_above_77_columns() {
        let lines = render_dashboard(DASHBOARD_HIDE_VOTE_AT_WIDTH + 1);
        assert!(lines.join("\n").contains("Vote Next"));
        assert_eq!(lines[0].chars().filter(|c| *c == '┌').count(), 2);
    }

    #[test]
    fn dashboard_still_renders_at_77_column_terminal_content_width() {
        let lines = render_dashboard(DASHBOARD_HIDE_VOTE_AT_WIDTH);
        assert!(!lines.join("\n").contains("Dashboard view too small."));
        assert!(lines.join("\n").contains("Stream"));
        assert!(lines.join("\n").contains("[B] CLI"));
        assert!(lines.join("\n").contains("[P] web"));
        assert!(lines.join("\n").contains("No messages yet."));
    }

    #[test]
    fn dashboard_hides_top_stream_card_at_65_columns() {
        let lines = render_dashboard(DASHBOARD_HIDE_STREAM_AT_WIDTH);
        let rendered = lines.join("\n");
        assert!(!rendered.contains("Dashboard view too small."));
        assert!(!rendered.contains("Stream"));
        assert!(!rendered.contains("Vote Next"));
        assert!(rendered.contains("No messages yet."));
    }

    #[test]
    fn dashboard_collapses_to_chat_when_height_below_minimum() {
        let lines = render_dashboard_with_size(100, DASHBOARD_MIN_FULL_HEIGHT - 1);
        let rendered = lines.join("\n");
        assert!(!rendered.contains("Stream"));
        assert!(!rendered.contains("Vote Next"));
        assert!(rendered.contains("No messages yet."));
    }

    #[test]
    fn dashboard_keeps_full_layout_at_minimum_height() {
        let lines = render_dashboard_with_size(100, DASHBOARD_MIN_FULL_HEIGHT);
        let rendered = lines.join("\n");
        assert!(rendered.contains("Stream"));
        assert!(rendered.contains("No messages yet."));
    }

    #[test]
    fn dashboard_blackjack_grid_renders_room_slots_when_tall_enough() {
        let lines = render_dashboard_with_size(100, 36);
        let rendered = lines.join("\n");
        assert!(rendered.contains("loading…"));
        assert!(rendered.contains("b1"));
        assert!(rendered.contains("b2"));
        assert!(rendered.contains("b3"));
        assert!(!rendered.contains("b4"));
    }

    #[test]
    fn dashboard_hides_stream_and_vote_when_header_setting_disabled() {
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let vote_counts = VoteCount {
            lofi: 3,
            ambient: 2,
            classic: 1,
            jazz: 0,
        };
        let mut rows_cache = ChatRowsCache::default();
        let usernames: HashMap<Uuid, String> = HashMap::new();
        let countries: HashMap<Uuid, String> = HashMap::new();
        let badges: HashMap<Uuid, BadgeTier> = HashMap::new();
        let bonsai_glyphs: HashMap<Uuid, String> = HashMap::new();
        let message_reactions = HashMap::new();
        let composer = ratatui_textarea::TextArea::default();
        let rooms_snapshot = RoomsSnapshot::default();
        let blackjack_snapshots: HashMap<Uuid, BlackjackSnapshot> = HashMap::new();

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, 100, 20);
                draw_dashboard(
                    frame,
                    area,
                    DashboardRenderInput {
                        now_playing: Some("Boards of Canada"),
                        vote_counts: &vote_counts,
                        current_genre: Genre::Lofi,
                        next_switch_in: Duration::from_secs(95),
                        my_vote: Some(Genre::Ambient),
                        show_header: false,
                        favorites_strip: None,
                        pinned_messages: &[],
                        show_room_showcases: true,
                        rooms_snapshot: &rooms_snapshot,
                        blackjack_snapshots: &blackjack_snapshots,
                        blackjack_prefix_armed: false,
                        chat_view: DashboardChatView {
                            messages: &[],
                            overlay: None,
                            rows_cache: &mut rows_cache,
                            usernames: &usernames,
                            countries: &countries,
                            badges: &badges,
                            message_reactions: &message_reactions,
                            current_user_id: Uuid::nil(),
                            selected_message_id: None,
                            highlighted_message_id: None,
                            reaction_picker_active: false,
                            composer: &composer,
                            composing: false,
                            mention_matches: &[],
                            mention_selected: 0,
                            mention_active: false,
                            reply_author: None,
                            is_editing: false,
                            bonsai_glyphs: &bonsai_glyphs,
                        },
                    },
                );
            })
            .expect("draw");

        let rendered = (0..20)
            .map(|y| {
                let mut out = String::new();
                for x in 0..100 {
                    out.push_str(terminal.backend().buffer()[(x, y)].symbol());
                }
                out
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(!rendered.contains("Stream"));
        assert!(!rendered.contains("Vote Next"));
        assert!(rendered.contains("No messages yet."));
    }

    #[test]
    fn dashboard_favorites_strip_renders_unread_counts() {
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let vote_counts = VoteCount {
            lofi: 3,
            ambient: 2,
            classic: 1,
            jazz: 0,
        };
        let mut rows_cache = ChatRowsCache::default();
        let usernames: HashMap<Uuid, String> = HashMap::new();
        let countries: HashMap<Uuid, String> = HashMap::new();
        let badges: HashMap<Uuid, BadgeTier> = HashMap::new();
        let bonsai_glyphs: HashMap<Uuid, String> = HashMap::new();
        let message_reactions = HashMap::new();
        let composer = ratatui_textarea::TextArea::default();
        let rooms_snapshot = RoomsSnapshot::default();
        let blackjack_snapshots: HashMap<Uuid, BlackjackSnapshot> = HashMap::new();
        let rust_room = Uuid::now_v7();
        let go_room = Uuid::now_v7();

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, 100, 20);
                draw_dashboard(
                    frame,
                    area,
                    DashboardRenderInput {
                        now_playing: Some("Boards of Canada"),
                        vote_counts: &vote_counts,
                        current_genre: Genre::Lofi,
                        next_switch_in: Duration::from_secs(95),
                        my_vote: Some(Genre::Ambient),
                        show_header: true,
                        favorites_strip: Some(&[
                            (rust_room, "#rust".to_string(), true, 3),
                            (go_room, "#go".to_string(), false, 0),
                        ]),
                        pinned_messages: &[],
                        show_room_showcases: true,
                        rooms_snapshot: &rooms_snapshot,
                        blackjack_snapshots: &blackjack_snapshots,
                        blackjack_prefix_armed: false,
                        chat_view: DashboardChatView {
                            messages: &[],
                            overlay: None,
                            rows_cache: &mut rows_cache,
                            usernames: &usernames,
                            countries: &countries,
                            badges: &badges,
                            message_reactions: &message_reactions,
                            current_user_id: Uuid::nil(),
                            selected_message_id: None,
                            highlighted_message_id: None,
                            reaction_picker_active: false,
                            composer: &composer,
                            composing: false,
                            mention_matches: &[],
                            mention_selected: 0,
                            mention_active: false,
                            reply_author: None,
                            is_editing: false,
                            bonsai_glyphs: &bonsai_glyphs,
                        },
                    },
                );
            })
            .expect("draw");

        let rendered = (0..20)
            .map(|y| {
                let mut out = String::new();
                for x in 0..100 {
                    out.push_str(terminal.backend().buffer()[(x, y)].symbol());
                }
                out
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("#rust (3)"));
        assert!(rendered.contains("#go"));
    }

    #[test]
    fn favorites_strip_hit_test_returns_clicked_room() {
        let rust_room = Uuid::now_v7();
        let go_room = Uuid::now_v7();
        let pins = vec![
            (rust_room, "#rust".to_string(), true, 3),
            (go_room, "#go".to_string(), false, 0),
        ];
        let area = Rect::new(1, 1, 74, 30);

        assert_eq!(
            favorites_strip_hit_test(area, true, &pins, 0, 10, 6),
            Some(rust_room)
        );
        assert_eq!(
            favorites_strip_hit_test(area, true, &pins, 0, 18, 6),
            Some(go_room)
        );
        assert_eq!(favorites_strip_hit_test(area, true, &pins, 0, 40, 6), None);
    }

    #[test]
    fn favorites_strip_hit_test_returns_none_when_strip_hidden() {
        let room = Uuid::now_v7();
        let pins = vec![(room, "#rust".to_string(), true, 0)];

        assert_eq!(
            favorites_strip_hit_test(Rect::new(1, 1, 74, 30), true, &pins, 0, 5, 7),
            None
        );
        assert_eq!(
            favorites_strip_hit_test(
                Rect::new(1, 1, 74, 5),
                false,
                &[
                    (room, "#rust".to_string(), true, 0),
                    (Uuid::now_v7(), "#go".to_string(), false, 0)
                ],
                0,
                5,
                1
            ),
            None
        );
    }

    #[test]
    fn dashboard_audio_buttons_hit_test_separately() {
        let area = Rect::new(0, 0, 100, 20);

        assert!(cli_install_button_hit_test(area, true, 12, 3));
        assert!(!cli_install_button_hit_test(area, true, 21, 3));
        assert!(browser_pair_button_hit_test(area, true, 21, 3));
        assert!(!browser_pair_button_hit_test(area, true, 12, 3));
    }
}
