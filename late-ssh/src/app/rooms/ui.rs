use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{
    chat::ui::EmbeddedRoomChatView,
    common::theme,
    rooms::{
        blackjack::{
            settings::{PACE_OPTIONS, STAKE_OPTIONS},
            state::{BlackjackSnapshot, State as BlackjackState},
        },
        filter::RoomsFilter,
        mock::{PLACEHOLDERS, PlaceholderKind, meta_for_real},
        svc::{RoomListItem, RoomsSnapshot, game_kind_label},
    },
};

const NARROW_WIDTH: u16 = 80;

pub struct RoomsPageView<'a> {
    pub add_form_open: bool,
    pub display_name: &'a str,
    pub create_focus_index: usize,
    pub create_pace_index: usize,
    pub create_stake_index: usize,
    pub snapshot: &'a RoomsSnapshot,
    pub selected_index: usize,
    pub active_room: Option<&'a RoomListItem>,
    pub blackjack_state: &'a BlackjackState,
    pub is_admin: bool,
    pub is_mod: bool,
    pub filter: RoomsFilter,
    pub search_active: bool,
    pub search_query: &'a str,
    pub usernames: &'a std::collections::HashMap<uuid::Uuid, String>,
    pub blackjack_snapshots: &'a std::collections::HashMap<uuid::Uuid, BlackjackSnapshot>,
    pub active_room_chat: Option<EmbeddedRoomChatView<'a>>,
}

#[derive(Clone, Copy)]
enum Row<'a> {
    Real(&'a RoomListItem),
    Placeholder(PlaceholderKind),
}

pub fn draw_rooms_page(frame: &mut Frame, area: Rect, mut view: RoomsPageView<'_>) {
    let block = Block::default()
        .title(rooms_title(&view))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 8 || inner.width < 36 {
        frame.render_widget(Paragraph::new("Terminal too small for Rooms"), inner);
        return;
    }

    if let Some(room) = view.active_room {
        draw_active_room(
            frame,
            inner,
            room,
            view.blackjack_state,
            view.usernames,
            view.active_room_chat.take(),
        );
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1), // filter pills
        Constraint::Length(1), // spacer
        Constraint::Min(3),    // list
        Constraint::Length(1), // footer hints
    ])
    .split(inner);

    draw_filter_bar(frame, layout[0], &view);

    let rows = build_rows(&view);
    if inner.width >= NARROW_WIDTH {
        draw_room_list_wide(frame, layout[2], &view, &rows);
    } else {
        draw_room_list_narrow(frame, layout[2], &view, &rows);
    }

    draw_footer(frame, layout[3], &view);

    if view.add_form_open {
        draw_create_blackjack_modal(frame, inner, &view);
    }
}

fn rooms_title(view: &RoomsPageView<'_>) -> String {
    if let Some(room) = view.active_room {
        return format!(
            " {} · {} · Esc back ",
            room.display_name,
            game_kind_label(room.game_kind)
        );
    }
    let real_count = view.snapshot.rooms.len();
    let open = view
        .snapshot
        .rooms
        .iter()
        .filter(|r| r.status == "open")
        .count();
    format!(" Rooms · {} live · {} open ", real_count, open)
}

fn build_rows<'a>(view: &'a RoomsPageView<'a>) -> Vec<Row<'a>> {
    let q = view.search_query.trim().to_lowercase();
    let mut rows: Vec<Row<'a>> = Vec::new();

    for room in &view.snapshot.rooms {
        if !view.filter.matches_real(room.game_kind) {
            continue;
        }
        if !q.is_empty() && !room.display_name.to_lowercase().contains(&q) {
            continue;
        }
        rows.push(Row::Real(room));
    }

    // Placeholders are not searchable — they're a static "what's coming" hint.
    if q.is_empty() {
        for kind in PLACEHOLDERS {
            if view.filter.matches_placeholder(*kind) {
                rows.push(Row::Placeholder(*kind));
            }
        }
    }

    rows
}

fn draw_filter_bar(frame: &mut Frame, area: Rect, view: &RoomsPageView<'_>) {
    if area.height == 0 {
        return;
    }

    if view.search_active {
        let line = Line::from(vec![
            Span::styled("/ ", Style::default().fg(theme::AMBER())),
            Span::styled(view.search_query, Style::default().fg(theme::TEXT_BRIGHT())),
            Span::styled("█", Style::default().fg(theme::AMBER())),
            Span::raw("   "),
            Span::styled(
                "Enter apply · Esc cancel",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }

    let mut spans: Vec<Span> = Vec::new();
    for (i, filter) in RoomsFilter::ALL.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        let selected = *filter == view.filter;
        let style = if selected {
            Style::default()
                .fg(theme::BG_SELECTION())
                .bg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(format!(" {} ", filter.label()), style));
    }

    if !view.search_query.is_empty() {
        spans.push(Span::raw("   "));
        spans.push(Span::styled(
            format!("/ {}", view.search_query),
            Style::default().fg(theme::AMBER_DIM()),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

const CREATE_MODAL_WIDTH: u16 = 64;
const CREATE_MODAL_HEIGHT: u16 = 16;
const CREATE_LABEL_WIDTH: usize = 14;

fn draw_create_blackjack_modal(frame: &mut Frame, area: Rect, view: &RoomsPageView<'_>) {
    let modal_area = centered_rect(
        area,
        CREATE_MODAL_WIDTH.min(area.width),
        CREATE_MODAL_HEIGHT.min(area.height),
    );
    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .title(" New Blackjack Table ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    let layout = Layout::vertical([
        Constraint::Length(1), // breathing
        Constraint::Length(1), // section: Table
        Constraint::Length(1), // breathing
        Constraint::Length(1), // Name row
        Constraint::Length(1), // breathing
        Constraint::Length(1), // section: Game
        Constraint::Length(1), // breathing
        Constraint::Length(1), // Pace row
        Constraint::Length(1), // Stake row
        Constraint::Min(0),    // flex spacer
        Constraint::Length(1), // footer
    ])
    .split(inner);

    let width = inner.width as usize;

    frame.render_widget(Paragraph::new(create_section_heading("Table")), layout[1]);
    frame.render_widget(Paragraph::new(create_name_row(view, width)), layout[3]);

    frame.render_widget(Paragraph::new(create_section_heading("Game")), layout[5]);
    frame.render_widget(
        Paragraph::new(create_option_row(
            view.create_focus_index == 1,
            "Pace",
            PACE_OPTIONS
                .iter()
                .map(|pace| pace.label().to_string())
                .collect::<Vec<_>>(),
            view.create_pace_index,
            width,
        )),
        layout[7],
    );
    frame.render_widget(
        Paragraph::new(create_option_row(
            view.create_focus_index == 2,
            "Stake",
            STAKE_OPTIONS
                .iter()
                .map(|stake| stake.to_string())
                .collect::<Vec<_>>(),
            view.create_stake_index,
            width,
        )),
        layout[8],
    );

    frame.render_widget(Paragraph::new(create_footer_line()), layout[10]);
}

fn create_section_heading(title: &str) -> Line<'static> {
    let dim = Style::default().fg(theme::BORDER());
    let accent = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::styled("  ── ", dim),
        Span::styled(title.to_string(), accent),
        Span::styled(" ──", dim),
    ])
}

fn create_name_row(view: &RoomsPageView<'_>, width: usize) -> Line<'static> {
    let focused = view.create_focus_index == 0;
    let (value_text, value_color) = if focused {
        (format!("{}█", view.display_name), theme::AMBER())
    } else if view.display_name.trim().is_empty() {
        ("not set".to_string(), theme::TEXT_FAINT())
    } else {
        (view.display_name.to_string(), theme::TEXT_BRIGHT())
    };

    let (prefix_style, label_style, mut value_style, trailing_style) = row_styles(focused);
    value_style = value_style.fg(value_color);

    let marker = if focused { "›" } else { " " };
    let prefix = format!(" {marker} ");
    let label_text = format!("{:<width$}", "Name", width = CREATE_LABEL_WIDTH);
    let used = prefix.chars().count() + label_text.chars().count() + value_text.chars().count();
    let padding = width.saturating_sub(used);

    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(label_text, label_style),
        Span::styled(value_text, value_style),
        Span::styled(" ".repeat(padding), trailing_style),
    ])
}

fn create_option_row(
    focused: bool,
    label: &str,
    options: Vec<String>,
    selected_index: usize,
    width: usize,
) -> Line<'static> {
    let (prefix_style, label_style, _, trailing_style) = row_styles(focused);

    let marker = if focused { "›" } else { " " };
    let prefix = format!(" {marker} ");
    let label_text = format!("{:<width$}", label, width = CREATE_LABEL_WIDTH);

    let mut spans = vec![
        Span::styled(prefix.clone(), prefix_style),
        Span::styled(label_text.clone(), label_style),
    ];
    let mut used = prefix.chars().count() + label_text.chars().count();

    for (index, option) in options.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ", trailing_style));
            used += 2;
        }
        let pill = format!(" {} ", option);
        used += pill.chars().count();
        let selected = index == selected_index;
        let style = if selected {
            Style::default()
                .fg(theme::BG_SELECTION())
                .bg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else if focused {
            Style::default()
                .fg(theme::TEXT_DIM())
                .bg(theme::BG_SELECTION())
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(pill, style));
    }

    let padding = width.saturating_sub(used);
    spans.push(Span::styled(" ".repeat(padding), trailing_style));
    Line::from(spans)
}

fn row_styles(focused: bool) -> (Style, Style, Style, Style) {
    if focused {
        (
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_SELECTION())
                .add_modifier(Modifier::BOLD),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .bg(theme::BG_SELECTION())
                .add_modifier(Modifier::BOLD),
            Style::default().bg(theme::BG_SELECTION()),
            Style::default().bg(theme::BG_SELECTION()),
        )
    } else {
        (
            Style::default().fg(theme::TEXT_FAINT()),
            Style::default().fg(theme::TEXT_DIM()),
            Style::default(),
            Style::default(),
        )
    }
}

fn create_footer_line() -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled("Tab", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" field  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("←→", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" select  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("↵", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" create  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
    ])
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

fn draw_room_list_wide(frame: &mut Frame, area: Rect, view: &RoomsPageView<'_>, rows: &[Row<'_>]) {
    if area.height == 0 {
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if rows.is_empty() {
        draw_empty_state(frame, inner, view);
        return;
    }

    let mut lines: Vec<Line> = Vec::with_capacity(rows.len() + 2);
    lines.push(header_line());
    lines.push(divider_line(inner.width));

    let visible = (inner.height as usize).saturating_sub(2);
    let mut real_index: usize = 0;
    let mut placeholder_intro_drawn = false;

    for row in rows.iter().take(visible) {
        match row {
            Row::Real(room) => {
                let selected = real_index == view.selected_index;
                lines.push(real_row_wide(room, selected, view));
                real_index += 1;
            }
            Row::Placeholder(kind) => {
                if !placeholder_intro_drawn {
                    lines.push(placeholder_intro_line());
                    placeholder_intro_drawn = true;
                }
                lines.push(placeholder_row_wide(*kind));
            }
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn header_line() -> Line<'static> {
    let style = Style::default()
        .fg(theme::TEXT_DIM())
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{:<28}", "Name"), style),
        Span::styled(format!("{:<12}", "Game"), style),
        Span::styled(format!("{:<8}", "Seats"), style),
        Span::styled(format!("{:<14}", "Pace"), style),
        Span::styled(format!("{:<10}", "Stakes"), style),
        Span::styled("Status", style),
    ])
}

fn divider_line(width: u16) -> Line<'static> {
    let len = width.saturating_sub(2) as usize;
    Line::from(Span::styled(
        "─".repeat(len),
        Style::default().fg(theme::BORDER_DIM()),
    ))
}

fn real_row_wide<'a>(room: &'a RoomListItem, selected: bool, view: &RoomsPageView<'_>) -> Line<'a> {
    let meta = meta_for_real(room.game_kind);
    let (status_text, status_color) = real_status(&room.status);

    let pointer_style = if selected {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let name_style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    let dim = Style::default().fg(theme::TEXT_DIM());

    Line::from(vec![
        Span::styled(if selected { "▸ " } else { "  " }, pointer_style),
        Span::styled(
            format!("{:<28}", truncate(&room.display_name, 28)),
            name_style,
        ),
        Span::styled(
            format!("{:<12}", game_kind_label(room.game_kind)),
            Style::default().fg(theme::AMBER()),
        ),
        Span::styled(format!("{:<8}", seats_label(room, meta.seats, view)), dim),
        Span::styled(format!("{:<14}", room.blackjack_settings.pace_label()), dim),
        Span::styled(
            format!("{:<10}", room.blackjack_settings.stake_label()),
            dim,
        ),
        Span::styled(status_text, Style::default().fg(status_color)),
    ])
}

fn placeholder_row_wide(kind: PlaceholderKind) -> Line<'static> {
    let meta = kind.meta();
    let dim = Style::default().fg(theme::TEXT_DIM());
    let faint = Style::default().fg(theme::TEXT_FAINT());

    Line::from(vec![
        Span::styled("  ", faint),
        Span::styled(format!("{:<28}", kind.label()), faint),
        Span::styled(format!("{:<12}", kind.label()), faint),
        Span::styled(format!("{:<8}", format!("{} seats", meta.seats)), faint),
        Span::styled(format!("{:<14}", meta.pace), faint),
        Span::styled(format!("{:<10}", stakes_label()), faint),
        Span::styled("Coming soon", dim),
    ])
}

fn placeholder_intro_line() -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "· soon ·",
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        ),
    ])
}

fn draw_room_list_narrow(
    frame: &mut Frame,
    area: Rect,
    view: &RoomsPageView<'_>,
    rows: &[Row<'_>],
) {
    if area.height == 0 {
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if rows.is_empty() {
        draw_empty_state(frame, inner, view);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    let mut placeholder_intro_drawn = false;
    let mut real_index: usize = 0;
    let visible_lines = inner.height as usize;

    for row in rows {
        if lines.len() + 2 > visible_lines {
            break;
        }
        match row {
            Row::Real(room) => {
                let selected = real_index == view.selected_index;
                let (a, b) = real_card_narrow(room, selected, view);
                lines.push(a);
                lines.push(b);
                real_index += 1;
            }
            Row::Placeholder(kind) => {
                if !placeholder_intro_drawn {
                    if lines.len() + 1 > visible_lines {
                        break;
                    }
                    lines.push(placeholder_intro_line());
                    placeholder_intro_drawn = true;
                }
                let (a, b) = placeholder_card_narrow(*kind);
                lines.push(a);
                lines.push(b);
            }
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn real_card_narrow<'a>(
    room: &'a RoomListItem,
    selected: bool,
    view: &RoomsPageView<'_>,
) -> (Line<'a>, Line<'a>) {
    let meta = meta_for_real(room.game_kind);
    let (status_text, status_color) = real_status(&room.status);
    let pointer = if selected { "▸ " } else { "  " };
    let name_style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };

    let head = Line::from(vec![
        Span::styled(
            pointer,
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(room.display_name.clone(), name_style),
        Span::raw("  "),
        Span::styled(
            game_kind_label(room.game_kind),
            Style::default().fg(theme::AMBER()),
        ),
    ]);
    let body = Line::from(vec![
        Span::raw("    "),
        Span::styled(
            format!(
                "{} seats · {} · {}",
                seats_label(room, meta.seats, view),
                room.blackjack_settings.pace_label(),
                room.blackjack_settings.stake_label()
            ),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        Span::raw("   "),
        Span::styled(status_text, Style::default().fg(status_color)),
    ]);
    (head, body)
}

fn placeholder_card_narrow(kind: PlaceholderKind) -> (Line<'static>, Line<'static>) {
    let meta = kind.meta();
    let faint = Style::default().fg(theme::TEXT_FAINT());

    let head = Line::from(vec![
        Span::raw("  "),
        Span::styled(kind.label(), faint),
        Span::raw("  "),
        Span::styled("Coming soon", Style::default().fg(theme::TEXT_DIM())),
    ]);
    let body = Line::from(vec![
        Span::raw("    "),
        Span::styled(
            format!("{} seats · {} · {}", meta.seats, meta.pace, stakes_label()),
            faint,
        ),
    ]);
    (head, body)
}

fn draw_empty_state(frame: &mut Frame, area: Rect, view: &RoomsPageView<'_>) {
    let mut lines: Vec<Line> = Vec::new();
    let q_active = !view.search_query.is_empty();
    let primary = if q_active {
        format!("No rooms match \"{}\".", view.search_query)
    } else if view.filter == RoomsFilter::All {
        "No rooms yet.".to_string()
    } else {
        format!("No {} rooms yet.", view.filter.label())
    };
    lines.push(Line::from(Span::styled(
        primary,
        Style::default().fg(theme::TEXT_MUTED()),
    )));

    let hint = if view.is_admin {
        "Press n to create the first one."
    } else {
        "Ask an admin to spin one up."
    };
    lines.push(Line::from(Span::styled(
        hint,
        Style::default().fg(theme::TEXT_DIM()),
    )));

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, view: &RoomsPageView<'_>) {
    if area.height == 0 {
        return;
    }

    let mut spans: Vec<Span> = vec![
        hint_pair("j/k", "navigate"),
        Span::raw(" · "),
        hint_pair("Enter", "join"),
        Span::raw(" · "),
        hint_pair("h/l", "filter"),
        Span::raw(" · "),
        hint_pair("/", "search"),
    ];

    if view.is_admin {
        spans.push(Span::raw(" · "));
        spans.push(hint_pair("n", "new"));
    }

    if view.is_admin || view.is_mod {
        spans.push(Span::raw(" · "));
        spans.push(hint_pair("Esc", "back"));
    }

    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Left),
        area,
    );
}

fn hint_pair(key: &'static str, label: &'static str) -> Span<'static> {
    Span::styled(
        format!("{} {}", key, label),
        Style::default().fg(theme::TEXT_DIM()),
    )
}

fn real_status(status: &str) -> (&'static str, ratatui::style::Color) {
    match status {
        "open" => ("Open", theme::SUCCESS()),
        "in_round" => ("In round", theme::AMBER()),
        "paused" => ("Paused", theme::TEXT_DIM()),
        "closed" => ("Closed", theme::TEXT_DIM()),
        _ => ("—", theme::TEXT_DIM()),
    }
}

fn stakes_label() -> &'static str {
    "chips"
}

fn seats_label(room: &RoomListItem, fallback_total: u8, view: &RoomsPageView<'_>) -> String {
    let Some(snapshot) = view.blackjack_snapshots.get(&room.id) else {
        return format!("?/{}", fallback_total);
    };
    let occupied = snapshot
        .seats
        .iter()
        .filter(|seat| seat.user_id.is_some())
        .count();
    format!("{}/{}", occupied, snapshot.seats.len())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn draw_active_room(
    frame: &mut Frame,
    area: Rect,
    room: &RoomListItem,
    blackjack_state: &BlackjackState,
    usernames: &std::collections::HashMap<uuid::Uuid, String>,
    active_room_chat: Option<EmbeddedRoomChatView<'_>>,
) {
    let layout = Layout::vertical([
        Constraint::Percentage(70),
        Constraint::Length(1),
        Constraint::Percentage(30),
    ])
    .split(area);

    draw_game_area(frame, layout[0], room, blackjack_state, usernames);
    if let Some(chat) = active_room_chat {
        crate::app::chat::ui::draw_embedded_room_chat(frame, layout[2], chat);
    } else {
        draw_chat_placeholder(frame, layout[2], room);
    }
}

fn draw_game_area(
    frame: &mut Frame,
    area: Rect,
    room: &RoomListItem,
    blackjack_state: &BlackjackState,
    usernames: &std::collections::HashMap<uuid::Uuid, String>,
) {
    match room.game_kind {
        crate::app::rooms::svc::GameKind::Blackjack => {
            crate::app::rooms::blackjack::ui::draw_game(
                frame,
                area,
                blackjack_state,
                false,
                usernames,
            );
        }
    }
}

fn draw_chat_placeholder(frame: &mut Frame, area: Rect, room: &RoomListItem) {
    let block = Block::default()
        .title(" Chat ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(Span::styled(
            "Room chat will render here.",
            Style::default().fg(theme::TEXT_MUTED()),
        )),
        Line::from(Span::styled(
            room.chat_room_id.to_string(),
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}
