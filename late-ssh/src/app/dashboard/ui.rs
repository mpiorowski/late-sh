use std::collections::VecDeque;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::{
    activity::event::ActivityEvent,
    chat::ui::{DashboardChatView, draw_dashboard_chat_card},
    common::{markdown::wrap_plain_line, theme},
    dashboard::state::DashboardRoomJoin,
    files::terminal_image::TerminalImageFrame,
    hub::dailies::svc::{QuestItem, QuestSnapshot},
    rooms::{
        registry::{RoomDirectorySummary, RoomGameRegistry},
        svc::{RoomListItem, RoomsSnapshot},
    },
};
use crate::usernames::UsernameLookup;
use late_core::models::chat_message::ChatMessage;

pub(crate) const QUEST_CARD_CYCLE_SECONDS: u64 = 10;
const ACTIVE_FRIEND_MARKER: &str = "★";
const ACTIVE_FRIEND_NAME_LIMIT: usize = 4;

#[derive(Clone, Debug)]
pub struct DashboardRoomCard {
    pub room: RoomListItem,
    pub game_label: &'static str,
    pub occupied_seats: Option<usize>,
    pub total_seats: usize,
    pub recent_join_user_id: Option<uuid::Uuid>,
}

impl DashboardRoomCard {
    fn new(room: &RoomListItem, summary: RoomDirectorySummary) -> Self {
        Self {
            room: room.clone(),
            game_label: summary.game_label,
            occupied_seats: summary.occupied_seats,
            total_seats: summary.total_seats,
            recent_join_user_id: None,
        }
    }

    fn with_recent_join_user(mut self, user_id: uuid::Uuid) -> Self {
        self.recent_join_user_id = Some(user_id);
        self
    }
}

pub(crate) fn recent_dashboard_rooms(
    snapshot: &RoomsSnapshot,
    registry: &RoomGameRegistry,
    recent_joins: &VecDeque<DashboardRoomJoin>,
    max: usize,
) -> Vec<DashboardRoomCard> {
    let mut rooms = Vec::new();
    for join in recent_joins {
        let Some(room) = snapshot.rooms.iter().find(|room| room.id == join.room_id) else {
            continue;
        };
        rooms.push(
            DashboardRoomCard::new(room, registry.directory_summary(room))
                .with_recent_join_user(join.user_id),
        );
        if rooms.len() >= max {
            break;
        }
    }
    rooms
}

pub struct DashboardRenderInput<'a> {
    pub activity: &'a VecDeque<ActivityEvent>,
    pub online_count: usize,
    pub active_friend_names: &'a [String],
    pub multiplayer_rooms: &'a [DashboardRoomCard],
    pub quest_snapshot: &'a QuestSnapshot,
    pub dashboard_cycle_secs: u64,
    pub show_room_top_boxes: bool,
    pub pinned_messages: &'a [ChatMessage],
    pub chat_view: DashboardChatView<'a>,
    /// Mouse-wheel scroll offset for the Activity panel. `0` shows the
    /// newest event at the top; larger values reveal older events.
    pub activity_scroll: u16,
    /// Cell that, when present, receives the Activity panel's rendered
    /// rect so mouse-wheel hit-testing in `app::input` can route scroll
    /// events to it.
    pub activity_rect_slot: Option<&'a std::cell::Cell<Option<Rect>>>,
}

struct TopStripData<'a> {
    activity: &'a VecDeque<ActivityEvent>,
    online_count: usize,
    active_friend_names: &'a [String],
    multiplayer_rooms: &'a [DashboardRoomCard],
    quest_snapshot: &'a QuestSnapshot,
    cycle_secs: u64,
    usernames: &'a UsernameLookup<'a>,
    activity_scroll: u16,
    activity_rect_slot: Option<&'a std::cell::Cell<Option<Rect>>>,
}

/// Page-1 Home surface: top strip (activity/multiplayer/quest) and the
/// selected room's chat. Non-general rooms bypass this and render as full chat
/// in `render.rs`.
pub fn draw_dashboard(
    frame: &mut Frame,
    area: Rect,
    view: DashboardRenderInput<'_>,
    terminal_images: &mut TerminalImageFrame,
) {
    if area.width == 0 || area.height == 0 {
        draw_dashboard_chat_card(frame, area, view.chat_view, terminal_images);
        return;
    }

    let chrome = dashboard_chrome(
        area.height,
        area.width,
        view.show_room_top_boxes,
        view.pinned_messages,
    );

    let mut constraints: Vec<Constraint> = Vec::new();
    if chrome.top {
        constraints.push(Constraint::Length(TOP_STRIP_ROW_HEIGHT));
    }
    if chrome.pinned_top_rule {
        constraints.push(Constraint::Length(1)); // rule between top boxes and pinned message
    }
    if chrome.pinned_height > 0 {
        constraints.push(Constraint::Length(chrome.pinned_height));
    }
    if chrome.chat_rule {
        constraints.push(Constraint::Length(1)); // bottom rule above chat
    }
    constraints.push(Constraint::Fill(1));

    let chunks = Layout::vertical(constraints).split(area);
    let mut idx = 0;

    if chrome.top {
        draw_top_strip(
            frame,
            chunks[idx],
            TopStripData {
                activity: view.activity,
                online_count: view.online_count,
                active_friend_names: view.active_friend_names,
                multiplayer_rooms: view.multiplayer_rooms,
                quest_snapshot: view.quest_snapshot,
                cycle_secs: view.dashboard_cycle_secs,
                usernames: view.chat_view.usernames,
                activity_scroll: view.activity_scroll,
                activity_rect_slot: view.activity_rect_slot,
            },
        );
        idx += 1;
    }
    if chrome.pinned_top_rule {
        draw_horizontal_rule(frame, chunks[idx]);
        idx += 1;
    }
    if chrome.pinned_height > 0 {
        draw_pinned_messages(frame, chunks[idx], view.pinned_messages);
        idx += 1;
    }
    if chrome.pinned_height > 0 {
        draw_amber_rule(frame, chunks[idx]);
        idx += 1;
    } else if chrome.chat_rule {
        draw_horizontal_rule(frame, chunks[idx]);
        idx += 1;
    }
    draw_dashboard_chat_card(frame, chunks[idx], view.chat_view, terminal_images);
}

pub fn draw_chat_with_top_strip(
    frame: &mut Frame,
    area: Rect,
    view: DashboardRenderInput<'_>,
    terminal_images: &mut TerminalImageFrame,
) {
    if area.height < TOP_STRIP_ROW_HEIGHT + CHAT_RULE_HEIGHT + MIN_CHAT_HEIGHT_WITH_LOUNGE {
        draw_dashboard_chat_card(frame, area, view.chat_view, terminal_images);
        return;
    }

    let [top_area, rule_area, chat_area] = Layout::vertical([
        Constraint::Length(TOP_STRIP_ROW_HEIGHT),
        Constraint::Length(CHAT_RULE_HEIGHT),
        Constraint::Fill(1),
    ])
    .areas(area);

    draw_top_strip(
        frame,
        top_area,
        TopStripData {
            activity: view.activity,
            online_count: view.online_count,
            active_friend_names: view.active_friend_names,
            multiplayer_rooms: view.multiplayer_rooms,
            quest_snapshot: view.quest_snapshot,
            cycle_secs: view.dashboard_cycle_secs,
            usernames: view.chat_view.usernames,
            activity_scroll: view.activity_scroll,
            activity_rect_slot: view.activity_rect_slot,
        },
    );
    draw_horizontal_rule(frame, rule_area);
    draw_dashboard_chat_card(frame, chat_area, view.chat_view, terminal_images);
}

const TOP_STRIP_ROW_HEIGHT: u16 = 5;
const MAX_PINNED_HEIGHT: u16 = 6;
const CHAT_RULE_HEIGHT: u16 = 1;
const MIN_CHAT_HEIGHT_WITH_LOUNGE: u16 = 10;
const PINNED_GLYPH: &str = "● ";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DashboardChrome {
    top: bool,
    pinned_height: u16,
    pinned_top_rule: bool,
    chat_rule: bool,
}

fn dashboard_chrome(
    height: u16,
    width: u16,
    show_room_top_boxes: bool,
    pinned_messages: &[ChatMessage],
) -> DashboardChrome {
    let pinned_height = pinned_natural_height(pinned_messages, width);
    let mut top = show_room_top_boxes;

    if !dashboard_chrome_fits(height, top, pinned_height) {
        top = false;
    }

    DashboardChrome {
        top,
        pinned_height,
        pinned_top_rule: pinned_height > 0 && top,
        chat_rule: pinned_height > 0 || top,
    }
}

fn dashboard_chrome_fits(height: u16, top: bool, pinned_height: u16) -> bool {
    dashboard_chrome_height(top, pinned_height) + MIN_CHAT_HEIGHT_WITH_LOUNGE <= height
}

fn dashboard_chrome_height(top: bool, pinned_height: u16) -> u16 {
    let top_height = if top { TOP_STRIP_ROW_HEIGHT } else { 0 };
    let pinned_top_rule_height = if pinned_height > 0 && top {
        CHAT_RULE_HEIGHT
    } else {
        0
    };
    let rule_height = if pinned_height > 0 || top {
        CHAT_RULE_HEIGHT
    } else {
        0
    };
    top_height + pinned_top_rule_height + pinned_height + rule_height
}

/// Pre-wrap pinned messages to `width` and return the Lines, ready to render.
/// Same pattern chat uses: split into Lines, count Lines, render Lines.
fn pinned_lines(messages: &[ChatMessage], width: u16) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let prefix_w = PINNED_GLYPH.chars().count();
    let body_w = (width as usize).saturating_sub(prefix_w);
    if body_w == 0 {
        return Vec::new();
    }
    let indent = " ".repeat(prefix_w);
    let mut lines: Vec<Line<'static>> = Vec::new();
    for msg in messages {
        let flat: String = msg.body.split_whitespace().collect::<Vec<_>>().join(" ");
        let wraps = wrap_plain_line(&flat, body_w);
        let wraps = if wraps.is_empty() {
            vec![String::new()]
        } else {
            wraps
        };
        for (idx, chunk) in wraps.into_iter().enumerate() {
            let line = if idx == 0 {
                Line::from(vec![
                    Span::styled(PINNED_GLYPH, Style::default().fg(theme::AMBER())),
                    Span::styled(chunk, Style::default().fg(theme::TEXT())),
                ])
            } else {
                Line::from(vec![
                    Span::raw(indent.clone()),
                    Span::styled(chunk, Style::default().fg(theme::TEXT())),
                ])
            };
            lines.push(line);
        }
    }
    lines
}

fn pinned_natural_height(messages: &[ChatMessage], width: u16) -> u16 {
    (pinned_lines(messages, width).len() as u16).min(MAX_PINNED_HEIGHT)
}

fn draw_top_strip(frame: &mut Frame, area: Rect, data: TopStripData<'_>) {
    let cols = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Fill(1),
    ])
    .split(area);

    draw_box_activity(
        frame,
        cols[0],
        data.activity,
        data.online_count,
        data.active_friend_names,
        data.activity_scroll,
        data.activity_rect_slot,
    );
    draw_box_multiplayer_rooms(frame, cols[2], data.multiplayer_rooms, data.usernames);
    draw_box_daily_quest(frame, cols[4], data.quest_snapshot, data.cycle_secs);

    crate::app::common::sidebar::paint_vertical_separator(
        frame,
        cols[1].x + 1,
        cols[1].y,
        cols[1].height,
    );
    crate::app::common::sidebar::paint_vertical_separator(
        frame,
        cols[3].x + 1,
        cols[3].y,
        cols[3].height,
    );
}

fn draw_box_label_with_hint(frame: &mut Frame, area: Rect, label: &str, hint: &str) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                label.to_string(),
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::raw("  "),
            Span::styled(
                hint.to_string(),
                Style::default()
                    .fg(theme::BORDER_DIM())
                    .add_modifier(Modifier::ITALIC),
            ),
        ])),
        area,
    );
}

fn draw_box_multiplayer_rooms(
    frame: &mut Frame,
    area: Rect,
    multiplayer_rooms: &[DashboardRoomCard],
    usernames: &UsernameLookup<'_>,
) {
    crate::app::rooms::active_tables::draw_active_tables(
        frame,
        horizontal_padding(area, 1),
        multiplayer_rooms,
        usernames,
    );
}

fn draw_box_daily_quest(frame: &mut Frame, area: Rect, snapshot: &QuestSnapshot, cycle_secs: u64) {
    let area = horizontal_padding(area, 1);
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    let Some((index, total, item)) = selected_quest_card(snapshot, cycle_secs) else {
        let hint = if snapshot.user_id.is_some() {
            "(none assigned)"
        } else {
            "(loading)"
        };
        draw_box_label_with_hint(frame, rows[0], "quests", hint);
        return;
    };

    draw_quest_label(frame, rows[0], item, index + 1, total);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate(&item.title, rows[1].width as usize),
            quest_title_style(item),
        ))),
        rows[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate(&item.description, rows[2].width as usize),
            Style::default().fg(theme::TEXT_DIM()),
        ))),
        rows[2],
    );

    draw_quest_progress(frame, rows[3], item);
    draw_quest_meta(frame, rows[4], item);
}

fn selected_quest_card(
    snapshot: &QuestSnapshot,
    cycle_secs: u64,
) -> Option<(usize, usize, &QuestItem)> {
    let total = snapshot.daily.len() + snapshot.weekly.len();
    if total == 0 {
        return None;
    }
    let index = ((cycle_secs / QUEST_CARD_CYCLE_SECONDS) as usize) % total;
    let item = if index < snapshot.daily.len() {
        &snapshot.daily[index]
    } else {
        &snapshot.weekly[index - snapshot.daily.len()]
    };
    Some((index, total, item))
}

fn draw_quest_label(frame: &mut Frame, area: Rect, item: &QuestItem, index: usize, total: usize) {
    let status = if item.completed() { "done" } else { "open" };
    let status_style = if item.completed() {
        Style::default().fg(theme::SUCCESS())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("{} quest", item.cadence),
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{index}/{total}"),
                Style::default().fg(theme::BORDER_DIM()),
            ),
            Span::raw("  "),
            Span::styled(status, status_style),
        ])),
        area,
    );
}

fn quest_title_style(item: &QuestItem) -> Style {
    if item.completed() {
        Style::default()
            .fg(theme::SUCCESS())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    }
}

fn draw_quest_progress(frame: &mut Frame, area: Rect, item: &QuestItem) {
    if area.width == 0 {
        return;
    }
    let progress = item.visible_progress();
    let progress_text = format!("{progress}/{}", item.target);
    let bar_w = (area.width as usize).saturating_sub(progress_text.chars().count() + 1);
    let filled = if item.target <= 0 {
        0
    } else {
        (bar_w * progress.max(0) as usize / item.target as usize).min(bar_w)
    };
    let empty = bar_w.saturating_sub(filled);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("█".repeat(filled), Style::default().fg(theme::SUCCESS())),
            Span::styled("░".repeat(empty), Style::default().fg(theme::BORDER_DIM())),
            Span::raw(" "),
            Span::styled(progress_text, Style::default().fg(theme::TEXT_DIM())),
        ])),
        area,
    );
}

fn draw_quest_meta(frame: &mut Frame, area: Rect, item: &QuestItem) {
    let reward = if item.reward_chips > 0 {
        format!("+{} chips", item.reward_chips)
    } else {
        "no chips".to_string()
    };
    let meta = format!(
        "{} / {} / resets {}",
        item.difficulty, reward, item.period_end
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate(&meta, area.width as usize),
            Style::default().fg(theme::AMBER_DIM()),
        ))),
        area,
    );
}

fn draw_box_activity(
    frame: &mut Frame,
    area: Rect,
    activity: &VecDeque<ActivityEvent>,
    online_count: usize,
    active_friend_names: &[String],
    activity_scroll: u16,
    activity_rect_slot: Option<&std::cell::Cell<Option<Rect>>>,
) {
    let area = horizontal_padding(area, 1);
    if let Some(slot) = activity_rect_slot {
        slot.set(Some(area));
    }
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "online",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::raw("  "),
            Span::styled("● ", Style::default().fg(theme::SUCCESS())),
            Span::styled(
                online_count.to_string(),
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" here", Style::default().fg(theme::TEXT_DIM())),
        ])),
        rows[0],
    );

    let rows_without_friends = [rows[1], rows[2], rows[3], rows[4]];
    let rows_with_friends = [rows[2], rows[3], rows[4]];
    let event_rows = if active_friend_names.is_empty() {
        rows_without_friends.as_slice()
    } else {
        draw_active_friends_row(frame, rows[1], active_friend_names);
        rows_with_friends.as_slice()
    };
    // Clamp the scroll offset to the number of events that lie beyond the
    // visible window. Without this, trimming `activity` (which happens as
    // events age out) could leave the user stranded past the end.
    let visible = event_rows.len();
    let max_offset = activity.len().saturating_sub(visible);
    let offset = (activity_scroll as usize).min(max_offset);
    let mut drawn = 0;
    for (row, event) in event_rows
        .iter()
        .copied()
        .zip(activity.iter().rev().skip(offset))
    {
        let body_w = row.width as usize;
        let elapsed = event.at.elapsed().as_secs();
        let ago = if elapsed < 60 {
            format!("{}s", elapsed)
        } else if elapsed < 3600 {
            format!("{}m", elapsed / 60)
        } else {
            format!("{}h", elapsed / 3600)
        };
        let user = truncate(&event.username, 12);
        let user_part = format!("@{}", user);
        let action_w = body_w.saturating_sub(user_part.chars().count() + ago.chars().count() + 4);
        let action = truncate(&event.action, action_w);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(user_part, Style::default().fg(theme::TEXT())),
                Span::raw("  "),
                Span::styled(action, Style::default().fg(theme::TEXT_DIM())),
                Span::raw("  "),
                Span::styled(ago, Style::default().fg(theme::TEXT_FAINT())),
            ])),
            row,
        );
        drawn += 1;
    }
    if drawn == 0 {
        let empty_row = event_rows.first().copied().unwrap_or(rows[1]);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "the room is quiet",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ))),
            empty_row,
        );
    }
}

fn draw_active_friends_row(frame: &mut Frame, row: Rect, active_friend_names: &[String]) {
    let names = compact_friend_names(active_friend_names, row.width as usize);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                ACTIVE_FRIEND_MARKER,
                Style::default()
                    .fg(theme::BADGE_GOLD())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                names,
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
        ])),
        row,
    );
}

fn compact_friend_names(names: &[String], width: usize) -> String {
    let mut pieces: Vec<String> = names
        .iter()
        .take(ACTIVE_FRIEND_NAME_LIMIT)
        .map(|name| format!("@{}", truncate(name, 10)))
        .collect();
    if names.len() > ACTIVE_FRIEND_NAME_LIMIT {
        pieces.push(format!("+{}", names.len() - ACTIVE_FRIEND_NAME_LIMIT));
    }
    truncate(
        &pieces.join(" "),
        width.saturating_sub(ACTIVE_FRIEND_MARKER.chars().count() + 1),
    )
}

fn draw_pinned_messages(frame: &mut Frame, area: Rect, messages: &[ChatMessage]) {
    if area.width == 0 || area.height == 0 || messages.is_empty() {
        return;
    }
    let mut lines = pinned_lines(messages, area.width);
    let max_rows = area.height as usize;
    if lines.len() > max_rows {
        lines.truncate(max_rows);
        if let Some(last) = lines.last_mut() {
            *last = Line::from(Span::styled(
                "  …",
                Style::default()
                    .fg(theme::TEXT_FAINT())
                    .add_modifier(Modifier::ITALIC),
            ));
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_amber_rule(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(theme::AMBER_DIM()),
        ))),
        area,
    );
}

fn draw_horizontal_rule(frame: &mut Frame, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(area.width as usize),
            Style::default().fg(theme::BORDER_DIM()),
        ))),
        area,
    );
}

fn horizontal_padding(area: Rect, padding: u16) -> Rect {
    let padding = padding.min(area.width / 2);
    Rect {
        x: area.x + padding,
        y: area.y,
        width: area.width.saturating_sub(padding * 2),
        height: area.height,
    }
}

fn truncate(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    if max == 1 {
        return "…".to_string();
    }
    let mut out: String = chars.into_iter().take(max - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, Utc};
    use late_core::models::chat_message::ChatMessage;
    use uuid::Uuid;

    const TEST_WIDTH: u16 = 80;

    fn pin(body: &str) -> ChatMessage {
        let now = Utc::now();
        ChatMessage {
            id: Uuid::nil(),
            created: now,
            updated: now,
            pinned: true,
            reply_to_message_id: None,
            room_id: Uuid::nil(),
            user_id: Uuid::nil(),
            body: body.to_string(),
        }
    }

    fn quest(cadence: &str, title: &str) -> QuestItem {
        QuestItem {
            title: title.to_string(),
            description: format!("{title} description"),
            cadence: cadence.to_string(),
            domain: "puzzle".to_string(),
            difficulty: "medium".to_string(),
            progress: 1,
            target: 3,
            reward_chips: 100,
            completed_at: None,
            period_end: NaiveDate::from_ymd_opt(2026, 5, 25).unwrap(),
        }
    }

    #[test]
    fn compact_friend_names_keeps_four_names_before_overflow() {
        let names = vec![
            "alice".to_string(),
            "bob".to_string(),
            "cara".to_string(),
            "dana".to_string(),
            "erin".to_string(),
        ];

        assert_eq!(
            compact_friend_names(&names[..4], 80),
            "@alice @bob @cara @dana"
        );
        assert_eq!(
            compact_friend_names(&names, 80),
            "@alice @bob @cara @dana +1"
        );
    }

    #[test]
    fn dashboard_chrome_always_requests_pinned_row_when_present() {
        let pins = [pin("hello")];
        let chrome = dashboard_chrome(1, TEST_WIDTH, false, &pins);

        assert!(chrome.pinned_height > 0);
        assert!(chrome.chat_rule);
        assert!(!chrome.top);
    }

    #[test]
    fn dashboard_chrome_hides_top_boxes_when_space_is_tight() {
        let top_only_height = dashboard_chrome_height(true, 0) + MIN_CHAT_HEIGHT_WITH_LOUNGE;
        let chrome = dashboard_chrome(top_only_height - 1, TEST_WIDTH, true, &[]);

        assert!(!chrome.top);
    }

    #[test]
    fn dashboard_chrome_shows_top_when_space_allows() {
        let pins = [pin("hello")];
        let full_height = dashboard_chrome_height(true, 1) + MIN_CHAT_HEIGHT_WITH_LOUNGE;
        let chrome = dashboard_chrome(full_height, TEST_WIDTH, true, &pins);

        assert!(chrome.pinned_height > 0);
        assert!(chrome.top);
        assert!(chrome.pinned_top_rule);
    }

    #[test]
    fn pinned_natural_height_wraps_and_sums() {
        let pins = [
            pin("short"),
            pin(&"word ".repeat(40)), // forces multi-line wrap at width 80
        ];
        let height = pinned_natural_height(&pins, TEST_WIDTH);
        assert!(height >= 2, "expected wrapping to add rows, got {height}");
        assert!(height <= MAX_PINNED_HEIGHT);
    }

    #[test]
    fn pinned_natural_height_caps_at_max() {
        let pins: Vec<ChatMessage> = (0..20).map(|i| pin(&format!("pin {i}"))).collect();
        let height = pinned_natural_height(&pins, TEST_WIDTH);
        assert_eq!(height, MAX_PINNED_HEIGHT);
    }

    #[test]
    fn horizontal_padding_insets_left_and_right() {
        let area = Rect::new(10, 2, 20, 1);
        let padded = horizontal_padding(area, 1);

        assert_eq!(padded.x, 11);
        assert_eq!(padded.width, 18);
    }

    #[test]
    fn selected_quest_card_rotates_every_ten_seconds() {
        let snapshot = QuestSnapshot {
            user_id: Some(Uuid::nil()),
            daily: vec![quest("daily", "first"), quest("daily", "second")],
            weekly: vec![quest("weekly", "third")],
        };

        assert_eq!(
            selected_quest_card(&snapshot, 0).map(|(_, _, item)| item.title.as_str()),
            Some("first")
        );
        assert_eq!(
            selected_quest_card(&snapshot, 9).map(|(_, _, item)| item.title.as_str()),
            Some("first")
        );
        assert_eq!(
            selected_quest_card(&snapshot, 10).map(|(_, _, item)| item.title.as_str()),
            Some("second")
        );
        assert_eq!(
            selected_quest_card(&snapshot, 20).map(|(_, _, item)| item.title.as_str()),
            Some("third")
        );
        assert_eq!(
            selected_quest_card(&snapshot, 30).map(|(_, _, item)| item.title.as_str()),
            Some("first")
        );
    }
}
