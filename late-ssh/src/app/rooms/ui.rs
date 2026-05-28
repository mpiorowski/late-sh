use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::{
    chat::ui::EmbeddedRoomChatView,
    common::theme,
    files::terminal_image::TerminalImageFrame,
    rooms::{
        backend::{ActiveRoomBackend, CreateRoomFlow, GameDrawCtx},
        filter::RoomsFilter,
        registry::RoomGameRegistry,
        svc::{RoomListItem, RoomsSnapshot},
    },
};
use crate::usernames::UsernameLookup;

const WIDE_LIST_MIN_WIDTH: u16 = 96;
const WIDE_LIST_BASE_WIDTH: usize = 96;
const ROOM_FILTER_PADDING_X: u16 = 4;

pub struct RoomsPageView<'a> {
    pub create_flow: Option<&'a CreateRoomFlow>,
    pub snapshot: &'a RoomsSnapshot,
    pub selected_index: usize,
    pub active_room: Option<&'a RoomListItem>,
    pub active_room_game: Option<&'a dyn ActiveRoomBackend>,
    pub room_game_registry: &'a RoomGameRegistry,
    pub is_admin: bool,
    pub is_moderator: bool,
    pub filter: RoomsFilter,
    pub search_active: bool,
    pub search_query: &'a str,
    pub usernames: &'a UsernameLookup<'a>,
    pub active_room_chat: Option<EmbeddedRoomChatView<'a>>,
}

#[derive(Clone, Copy)]
enum Row<'a> {
    Real(&'a RoomListItem),
}

pub fn draw_rooms_page(
    frame: &mut Frame,
    area: Rect,
    mut view: RoomsPageView<'_>,
    terminal_images: &mut TerminalImageFrame,
) {
    if area.height < 8 || area.width < 36 {
        frame.render_widget(Paragraph::new("Terminal too small for Rooms"), area);
        return;
    }

    if view.active_room.is_some() {
        if let Some(active_room_game) = view.active_room_game {
            draw_active_room(
                frame,
                area,
                active_room_game,
                view.usernames,
                view.active_room_chat.take(),
                terminal_images,
            );
        } else {
            frame.render_widget(Paragraph::new("Loading table..."), area);
        }
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1), // top padding
        Constraint::Length(1), // filter pills
        Constraint::Length(1), // spacer
        Constraint::Min(3),    // list
        Constraint::Length(1), // footer hints
    ])
    .split(area);

    draw_filter_bar(frame, layout[1], &view);

    let rows = build_rows(&view);
    if area.width >= WIDE_LIST_MIN_WIDTH {
        draw_room_list_wide(frame, layout[3], &view, &rows);
    } else {
        draw_room_list_narrow(frame, layout[3], &view, &rows);
    }

    draw_footer(frame, layout[4], &view);

    if let Some(flow) = view.create_flow {
        match flow {
            CreateRoomFlow::Picker { kind_index } => {
                draw_create_picker_modal(frame, area, &view, *kind_index);
            }
            CreateRoomFlow::Game { modal, .. } => modal.draw(frame, area),
        }
    }
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

    rows
}

fn draw_filter_bar(frame: &mut Frame, area: Rect, view: &RoomsPageView<'_>) {
    if area.height == 0 {
        return;
    }

    let area = area.inner(Margin {
        horizontal: ROOM_FILTER_PADDING_X,
        vertical: 0,
    });

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
    let mut filters = Vec::with_capacity(view.room_game_registry.ordered_kinds().len() + 1);
    filters.push(RoomsFilter::All);
    filters.extend(
        view.room_game_registry
            .ordered_kinds()
            .iter()
            .copied()
            .map(RoomsFilter::Kind),
    );
    for (i, filter) in filters.iter().enumerate() {
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

const PICKER_MODAL_WIDTH: u16 = 56;

fn draw_create_picker_modal(
    frame: &mut Frame,
    area: Rect,
    view: &RoomsPageView<'_>,
    kind_index: usize,
) {
    let kinds = view.room_game_registry.ordered_kinds();
    // 2 borders + 1 breathing + 1 heading + 1 breathing + N rows + 1 flex + 1 footer
    let height = (kinds.len() as u16).saturating_add(7).max(9);
    let modal_area = picker_centered_rect(
        area,
        PICKER_MODAL_WIDTH.min(area.width),
        height.min(area.height),
    );
    frame.render_widget(Clear, modal_area);

    let block = Block::default()
        .title(" New Room ")
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
        Constraint::Length(1),                  // breathing
        Constraint::Length(1),                  // heading
        Constraint::Length(1),                  // breathing
        Constraint::Length(kinds.len() as u16), // rows
        Constraint::Min(0),                     // flex
        Constraint::Length(1),                  // footer
    ])
    .split(inner);

    frame.render_widget(
        Paragraph::new(picker_section_heading("Choose a game")),
        layout[1],
    );

    let body_width = layout[3].width as usize;
    let mut rows: Vec<Line> = Vec::with_capacity(kinds.len());
    for (index, kind) in kinds.iter().enumerate() {
        rows.push(picker_row(
            view.room_game_registry.label(*kind),
            view.room_game_registry.slug_prefix(*kind),
            index == kind_index,
            body_width,
        ));
    }
    frame.render_widget(Paragraph::new(rows), layout[3]);

    let footer = Line::from(vec![
        Span::raw("  "),
        Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" choose  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("↵", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" open  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" cancel", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer), layout[5]);
}

fn picker_section_heading(title: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("  ── ", Style::default().fg(theme::BORDER())),
        Span::styled(
            title.to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ──", Style::default().fg(theme::BORDER())),
    ])
}

fn picker_row(label: &str, slug: &str, selected: bool, width: usize) -> Line<'static> {
    let marker = if selected { "›" } else { " " };
    let prefix_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let label_style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .bg(theme::BG_SELECTION())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_BRIGHT())
    };
    let slug_style = if selected {
        Style::default()
            .fg(theme::TEXT_DIM())
            .bg(theme::BG_SELECTION())
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let trailing_style = if selected {
        Style::default().bg(theme::BG_SELECTION())
    } else {
        Style::default()
    };

    let prefix = format!(" {marker} ");
    let label_text = label.to_string();
    let slug_text = format!("   ({slug})");
    let used = prefix.chars().count() + label_text.chars().count() + slug_text.chars().count();
    let padding = width.saturating_sub(used.min(width));

    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(label_text, label_style),
        Span::styled(slug_text, slug_style),
        Span::styled(" ".repeat(padding), trailing_style),
    ])
}

fn picker_centered_rect(area: Rect, width: u16, height: u16) -> Rect {
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

    if rows.is_empty() {
        draw_empty_state(frame, area, view);
        return;
    }

    let cols = wide_columns(area.width);
    let mut lines: Vec<Line> = Vec::with_capacity(rows.len() + 2);
    lines.push(header_line(cols));
    lines.push(divider_line(area.width));

    let visible = (area.height as usize).saturating_sub(2);

    for (real_index, row) in rows.iter().take(visible).enumerate() {
        let Row::Real(room) = row;
        let selected = real_index == view.selected_index;
        lines.push(real_row_wide(room, selected, view, cols, area.width));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

#[derive(Clone, Copy)]
struct WideColumns {
    name: usize,
    game: usize,
    creator: usize,
    seats: usize,
    pace: usize,
    stakes: usize,
}

fn wide_columns(width: u16) -> WideColumns {
    let mut cols = WideColumns {
        name: 22,
        game: 12,
        creator: 14,
        seats: 8,
        pace: 18,
        stakes: 12,
    };
    let mut extra = (width as usize).saturating_sub(WIDE_LIST_BASE_WIDTH);

    grow_col(&mut cols.name, &mut extra, 12);
    grow_col(&mut cols.pace, &mut extra, 6);
    grow_col(&mut cols.creator, &mut extra, 4);
    grow_col(&mut cols.stakes, &mut extra, 4);
    grow_col(&mut cols.game, &mut extra, 2);
    cols.name += extra;

    cols
}

fn grow_col(col: &mut usize, extra: &mut usize, max_growth: usize) {
    let growth = (*extra).min(max_growth);
    *col += growth;
    *extra -= growth;
}

fn header_line(cols: WideColumns) -> Line<'static> {
    let style = Style::default()
        .fg(theme::TEXT_DIM())
        .add_modifier(Modifier::BOLD);
    Line::from(vec![
        Span::raw("  "),
        Span::styled(pad_col("Name", cols.name), style),
        Span::styled(pad_col("Game", cols.game), style),
        Span::styled(pad_col("Creator", cols.creator), style),
        Span::styled(pad_col("Seats", cols.seats), style),
        Span::styled(pad_col("Pace", cols.pace), style),
        Span::styled(pad_col("Stakes", cols.stakes), style),
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

fn row_background_style(bg: Option<ratatui::style::Color>) -> Style {
    bg.map(|color| Style::default().bg(color))
        .unwrap_or_default()
}

fn real_row_wide(
    room: &RoomListItem,
    selected: bool,
    view: &RoomsPageView<'_>,
    cols: WideColumns,
    width: u16,
) -> Line<'static> {
    let meta = view.room_game_registry.directory_meta(room);
    let (status_text, status_color) = real_status(&room.status);
    let creator = creator_label(room, view);

    let pointer_style = if selected {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let name_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    let dim = if selected {
        Style::default().fg(theme::TEXT_BRIGHT())
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let game_style = if selected {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::AMBER())
    };
    let status_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(status_color)
    };
    let row_bg = selected.then(theme::BG_HIGHLIGHT);
    let row_len = 2
        + cols.name
        + cols.game
        + cols.creator
        + cols.seats
        + cols.pace
        + cols.stakes
        + status_text.chars().count();
    let trailing = " ".repeat((width as usize).saturating_sub(row_len));

    Line::from(vec![
        Span::styled(if selected { "▸ " } else { "  " }, pointer_style),
        Span::styled(pad_col(&room.display_name, cols.name), name_style),
        Span::styled(
            pad_col(view.room_game_registry.label(room.game_kind), cols.game),
            game_style,
        ),
        Span::styled(pad_col(&creator, cols.creator), dim),
        Span::styled(
            pad_col(&seats_label(room, meta.seats, view), cols.seats),
            dim,
        ),
        Span::styled(pad_col(&meta.pace, cols.pace), dim),
        Span::styled(pad_col(&meta.stakes, cols.stakes), dim),
        Span::styled(status_text, status_style),
        Span::styled(trailing, dim),
    ])
    .patch_style(row_background_style(row_bg))
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

    if rows.is_empty() {
        draw_empty_state(frame, area, view);
        return;
    }

    let mut lines: Vec<Line> = Vec::new();
    let visible_lines = area.height as usize;

    for (real_index, row) in rows.iter().enumerate() {
        if lines.len() + 2 > visible_lines {
            break;
        }
        let Row::Real(room) = row;
        let selected = real_index == view.selected_index;
        let (a, b) = real_card_narrow(room, selected, view, area.width);
        lines.push(a);
        lines.push(b);
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn real_card_narrow<'a>(
    room: &'a RoomListItem,
    selected: bool,
    view: &RoomsPageView<'_>,
    width: u16,
) -> (Line<'a>, Line<'a>) {
    let meta = view.room_game_registry.directory_meta(room);
    let (status_text, status_color) = real_status(&room.status);
    let creator = creator_label(room, view);
    let pointer = if selected { "▸ " } else { "  " };
    let name_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    let row_bg = selected.then(theme::BG_HIGHLIGHT);
    let marker_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    };
    let game_style = if selected {
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::AMBER())
    };
    let body_style = if selected {
        Style::default().fg(theme::TEXT_BRIGHT())
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let status_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(status_color)
    };
    let head_len = 2
        + room.display_name.chars().count()
        + 2
        + view
            .room_game_registry
            .label(room.game_kind)
            .chars()
            .count();
    let body_text = format!(
        "by {} · {} seats · {} · {}",
        creator,
        seats_label(room, meta.seats, view),
        meta.pace,
        meta.stakes
    );
    let body_len = 4 + body_text.chars().count() + 3 + status_text.chars().count();

    let head = Line::from(vec![
        Span::styled(pointer, marker_style),
        Span::styled(room.display_name.clone(), name_style),
        Span::raw("  "),
        Span::styled(view.room_game_registry.label(room.game_kind), game_style),
        Span::styled(
            " ".repeat((width as usize).saturating_sub(head_len)),
            body_style,
        ),
    ])
    .patch_style(row_background_style(row_bg));
    let body = Line::from(vec![
        Span::raw("    "),
        Span::styled(body_text, body_style),
        Span::raw("   "),
        Span::styled(status_text, status_style),
        Span::styled(
            " ".repeat((width as usize).saturating_sub(body_len)),
            body_style,
        ),
    ])
    .patch_style(row_background_style(row_bg));
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

    lines.push(Line::from(Span::styled(
        "Press n to create the first one.",
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
        Span::raw(" · "),
        hint_pair("n", "new"),
    ];

    if view.is_admin {
        spans.push(Span::raw(" · "));
        spans.push(hint_pair("d", "delete"));
    }

    if view.is_admin || view.is_moderator {
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

fn seats_label(room: &RoomListItem, fallback_total: u8, view: &RoomsPageView<'_>) -> String {
    let Some(hints) = view
        .room_game_registry
        .directory_hints(room.id, room.game_kind)
    else {
        return format!("?/{}", fallback_total);
    };
    format!("{}/{}", hints.occupied, hints.total)
}

fn creator_label(room: &RoomListItem, view: &RoomsPageView<'_>) -> String {
    if let Some(username) = room.created_by_username.as_deref().or_else(|| {
        room.created_by
            .and_then(|id| view.usernames.get(&id).map(String::as_str))
    }) {
        return format!("@{}", username);
    }

    room.created_by
        .map(short_user_id)
        .unwrap_or_else(|| "system".to_string())
}

fn short_user_id(user_id: uuid::Uuid) -> String {
    user_id.to_string().chars().take(8).collect()
}

fn pad_col(s: &str, width: usize) -> String {
    format!("{:<width$}", truncate(s, width), width = width)
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
    active_room_game: &dyn ActiveRoomBackend,
    usernames: &UsernameLookup<'_>,
    active_room_chat: Option<EmbeddedRoomChatView<'_>>,
    terminal_images: &mut TerminalImageFrame,
) {
    let game_area = active_room_game_area(active_room_game, area);
    let layout = Layout::vertical([
        Constraint::Length(game_area.height),
        Constraint::Length(1),
        Constraint::Min(5),
    ])
    .split(area);

    draw_game_area(frame, layout[0], active_room_game, usernames);
    draw_active_room_spacer(frame, layout[1]);
    if let Some(chat) = active_room_chat {
        crate::app::chat::ui::draw_embedded_room_chat(frame, layout[2], chat, terminal_images);
    }
}

pub(crate) fn active_room_game_area(active_room_game: &dyn ActiveRoomBackend, area: Rect) -> Rect {
    let game_height = preferred_game_height(active_room_game, area);
    Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: game_height,
    }
}

fn draw_active_room_spacer(frame: &mut Frame, area: Rect) {
    if area.height == 0 {
        return;
    }

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("`", Style::default().fg(theme::AMBER_DIM())),
            Span::styled(
                " toggle dashboard/game",
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ]))
        .alignment(Alignment::Right),
        area,
    );
}

fn preferred_game_height(active_room_game: &dyn ActiveRoomBackend, area: Rect) -> u16 {
    let chat_min: u16 = 8;
    let max_game = area.height.saturating_sub(chat_min + 1);
    let preferred = active_room_game.preferred_game_height(area);
    preferred.min(max_game).max(1)
}

fn draw_game_area(
    frame: &mut Frame,
    area: Rect,
    active_room_game: &dyn ActiveRoomBackend,
    usernames: &UsernameLookup<'_>,
) {
    active_room_game.draw(frame, area, GameDrawCtx { usernames });
}
