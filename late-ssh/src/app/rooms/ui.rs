use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{
    common::theme,
    rooms::{
        blackjack::{
            settings::{PACE_OPTIONS, STAKE_OPTIONS},
            state::State as BlackjackState,
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
}

#[derive(Clone, Copy)]
enum Row<'a> {
    Real(&'a RoomListItem),
    Placeholder(PlaceholderKind),
}

pub fn draw_rooms_page(frame: &mut Frame, area: Rect, view: &RoomsPageView<'_>) {
    let block = Block::default()
        .title(rooms_title(view))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 8 || inner.width < 36 {
        frame.render_widget(Paragraph::new("Terminal too small for Rooms"), inner);
        return;
    }

    if let Some(room) = view.active_room {
        draw_active_room(frame, inner, room, view.blackjack_state);
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1), // filter pills
        Constraint::Length(1), // spacer
        Constraint::Min(3),    // list
        Constraint::Length(1), // footer hints
    ])
    .split(inner);

    draw_filter_bar(frame, layout[0], view);

    let rows = build_rows(view);
    if inner.width >= NARROW_WIDTH {
        draw_room_list_wide(frame, layout[2], view, &rows);
    } else {
        draw_room_list_narrow(frame, layout[2], view, &rows);
    }

    draw_footer(frame, layout[3], view);

    if view.add_form_open {
        draw_create_blackjack_modal(frame, inner, view);
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

fn draw_create_blackjack_modal(frame: &mut Frame, area: Rect, view: &RoomsPageView<'_>) {
    let modal_area = centered_rect(area, 56.min(area.width), 12.min(area.height));
    let block = Block::default()
        .title(" New Blackjack Table ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(modal_area);
    frame.render_widget(Clear, modal_area);
    frame.render_widget(block, modal_area);

    let pace = PACE_OPTIONS
        .get(view.create_pace_index)
        .copied()
        .unwrap_or_default();
    let stake = STAKE_OPTIONS
        .get(view.create_stake_index)
        .copied()
        .unwrap_or(STAKE_OPTIONS[0]);

    let name_value = format!("{}█", view.display_name);
    let stake_value = format!("{stake} chips");
    let lines = vec![
        form_line("Name", &name_value, view.create_focus_index == 0),
        form_line("Pace", pace.table_label(), view.create_focus_index == 1),
        option_line(
            PACE_OPTIONS
                .iter()
                .map(|pace| pace.label())
                .collect::<Vec<_>>(),
            view.create_pace_index,
        ),
        form_line("Stake", &stake_value, view.create_focus_index == 2),
        option_line(
            STAKE_OPTIONS
                .iter()
                .map(|stake| format!("{stake}"))
                .collect::<Vec<_>>(),
            view.create_stake_index,
        ),
        Line::from(""),
        Line::from(Span::styled(
            "Tab field · ←/→ select · Enter create · Esc cancel",
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ];

    frame.render_widget(Paragraph::new(lines), inner);
}

fn form_line<'a>(label: &'static str, value: &'a str, focused: bool) -> Line<'a> {
    let label_style = if focused {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let value_style = if focused {
        Style::default().fg(theme::TEXT_BRIGHT())
    } else {
        Style::default().fg(theme::TEXT())
    };
    Line::from(vec![
        Span::styled(format!("{label:<7}"), label_style),
        Span::styled(value.to_string(), value_style),
    ])
}

fn option_line<T: ToString>(options: Vec<T>, selected_index: usize) -> Line<'static> {
    let mut spans = vec![Span::raw("        ")];
    for (index, option) in options.into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw(" "));
        }
        let selected = index == selected_index;
        let style = if selected {
            Style::default()
                .fg(theme::BG_SELECTION())
                .bg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(format!(" {} ", option.to_string()), style));
    }
    Line::from(spans)
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
                lines.push(real_row_wide(room, selected));
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

fn real_row_wide(room: &RoomListItem, selected: bool) -> Line<'_> {
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
        Span::styled(format!("{:<8}", format!("?/{}", meta.seats)), dim),
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
                let (a, b) = real_card_narrow(room, selected);
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

fn real_card_narrow(room: &RoomListItem, selected: bool) -> (Line<'_>, Line<'_>) {
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
                "?/{} seats · {} · {}",
                meta.seats,
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
        hint_pair("Tab", "filter"),
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
) {
    let layout = Layout::vertical([
        Constraint::Percentage(70),
        Constraint::Length(1),
        Constraint::Percentage(30),
    ])
    .split(area);

    draw_game_area(frame, layout[0], room, blackjack_state);
    draw_chat_placeholder(frame, layout[2], room);
}

fn draw_game_area(
    frame: &mut Frame,
    area: Rect,
    room: &RoomListItem,
    blackjack_state: &BlackjackState,
) {
    match room.game_kind {
        crate::app::rooms::svc::GameKind::Blackjack => {
            crate::app::rooms::blackjack::ui::draw_game(frame, area, blackjack_state, false);
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
