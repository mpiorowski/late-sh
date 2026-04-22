use dartboard_core::CellValue;
use dartboard_editor::{Clipboard, SWATCH_CAPACITY, Swatch};
use dartboard_tui::{CanvasStyle, CanvasWidget, CanvasWidgetState};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::app::{common::theme, games::ui::info_label_value};

use super::data::lines_for;
use super::state::{BrushMode, HelpTab, State};

const INFO_WIDTH: u16 = 28;
const SWATCH_BOX_WIDTH: u16 = 16;
const SWATCH_BOX_HEIGHT: u16 = 8;
const SWATCH_BOTTOM_CLEARANCE: u16 = 1;
const SWATCH_NOTICE_CLEARANCE: u16 = 1;
const PIN_UNPINNED: char = '📌';
const PIN_PINNED: char = '📍';

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SwatchHit {
    Body(usize),
    Pin(usize),
}

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State, interacting: bool) {
    let info = artboard_info_lines(state, interacting);
    let layout = artboard_layout(area);
    let info_area = info_block_area(layout.info_anchor, info.len());
    draw_canvas(frame, area, layout.canvas, info_area, state);
    draw_artboard_sidebar(frame, info_area, &info);
    if state.is_help_open() {
        draw_help(frame, area, state);
    }
    if state.is_glyph_picker_open()
        && let Some(catalog) = state.glyph_catalog()
    {
        crate::app::icon_picker::picker::render(frame, area, state.glyph_picker_state(), catalog);
    }
}

pub fn canvas_area_for_screen(screen_size: (u16, u16)) -> Rect {
    artboard_game_area_for_screen(screen_size)
}

fn dartboard_canvas_style() -> CanvasStyle {
    // Defer to dartboard-tui defaults for selection/floating colors; only
    // override the out-of-bounds background so it blends with the arcade
    // chrome and the default glyph fg so unpainted areas read as panel text.
    CanvasStyle {
        oob_bg: theme::BG_CANVAS(),
        default_glyph_fg: theme::TEXT(),
        ..CanvasStyle::default()
    }
}

fn draw_artboard_sidebar(frame: &mut Frame, info_area: Option<Rect>, info_lines: &[Line<'_>]) {
    if let Some(info_area) = info_area {
        let info_block = Block::default()
            .title(" Info ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme::BORDER()));
        let info_inner = info_block.inner(info_area);
        frame.render_widget(Clear, info_area);
        frame.render_widget(info_block, info_area);
        if info_inner.width > 0 && info_inner.height > 0 {
            frame.render_widget(Paragraph::new(info_lines.to_vec()), info_inner);
        }
    }
}

fn artboard_info_lines(state: &State, interacting: bool) -> Vec<Line<'static>> {
    let mut lines = vec![info_label_value(
        "Mode",
        if interacting {
            "active".to_string()
        } else {
            "view".to_string()
        },
        if interacting {
            theme::AMBER()
        } else {
            theme::TEXT_BRIGHT()
        },
    )];
    lines.push(info_label_value(
        "Cursor",
        format!("{},{}", state.cursor().x, state.cursor().y),
        theme::AMBER(),
    ));
    let owner_pos = state.owner_subject_pos();
    let owner_value = state.owner_username().unwrap_or("?").to_string();
    let owner_color = if state.owner_username().is_some() {
        theme::TEXT_BRIGHT()
    } else {
        theme::TEXT_FAINT()
    };
    lines.push(info_label_value("Owner", owner_value, owner_color));
    lines.push(info_label_value(
        "Cell",
        format!("{},{}", owner_pos.x, owner_pos.y),
        theme::AMBER(),
    ));
    lines.push(pan_indicator_line(state));

    let (brush, brush_color) = match state.brush_mode() {
        BrushMode::None => ("none".to_string(), theme::TEXT_FAINT()),
        BrushMode::Swatch => ("swatch".to_string(), theme::TEXT_BRIGHT()),
        BrushMode::Glyph(ch) => (ch.to_string(), theme::TEXT_BRIGHT()),
    };
    lines.push(info_label_value("Brush", brush, brush_color));
    let (selection_value, selection_color) = if let Some(selection) = state.selection_view() {
        let width = selection.anchor.x.abs_diff(selection.cursor.x) + 1;
        let height = selection.anchor.y.abs_diff(selection.cursor.y) + 1;
        (format!("{width}x{height}"), theme::SUCCESS())
    } else {
        ("none".to_string(), theme::TEXT_FAINT())
    };
    lines.push(info_label_value(
        "Selection",
        selection_value,
        selection_color,
    ));
    let mut peers = state.snapshot.peers.clone();
    peers.sort_by_key(|peer| {
        (
            peer.user_id != state.snapshot.your_user_id.unwrap_or_default(),
            peer.name.to_ascii_lowercase(),
        )
    });
    if !peers.is_empty() {
        lines.push(Line::from(""));
        lines.push(section_label("Peers"));
        for peer in peers {
            let suffix = if Some(peer.user_id) == state.snapshot.your_user_id {
                " (you)"
            } else {
                ""
            };
            lines.push(Line::from(vec![
                Span::styled("• ", Style::default().fg(theme::TEXT_DIM())),
                Span::styled(peer.name, Style::default().fg(rgb(peer.color))),
                Span::styled(suffix, Style::default().fg(theme::TEXT_FAINT())),
            ]));
        }
    }

    lines
}

fn pan_indicator_line(state: &State) -> Line<'static> {
    let [can_left, can_up, can_down, can_right] = pan_indicator_enabled(state);
    Line::from(vec![
        Span::styled(
            format!("{:<11}", "Pan"),
            Style::default().fg(theme::TEXT_DIM()),
        ),
        pan_indicator_span('◀', can_left),
        Span::raw(" "),
        pan_indicator_span('▲', can_up),
        Span::raw(" "),
        pan_indicator_span('▼', can_down),
        Span::raw(" "),
        pan_indicator_span('▶', can_right),
    ])
}

fn pan_indicator_span(ch: char, enabled: bool) -> Span<'static> {
    let style = if enabled {
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::BORDER_DIM())
    };
    Span::styled(ch.to_string(), style)
}

fn info_block_height(line_count: usize) -> u16 {
    line_count.max(1).saturating_add(2) as u16
}

fn info_block_area(sidebar_area: Rect, line_count: usize) -> Option<Rect> {
    if sidebar_area.width == 0 || sidebar_area.height == 0 {
        return None;
    }
    let info_height = info_block_height(line_count).min(sidebar_area.height);
    if info_height < 3 {
        return None;
    }
    Some(
        Layout::vertical([Constraint::Length(info_height), Constraint::Min(0)]).split(sidebar_area)
            [0],
    )
}

fn pan_indicator_enabled(state: &State) -> [bool; 4] {
    let viewport = state.viewport_origin();
    let viewport_width = state.editor.viewport.width as usize;
    let viewport_height = state.editor.viewport.height as usize;
    let can_left = viewport.x > 0;
    let can_up = viewport.y > 0;
    let can_right = viewport.x + viewport_width < state.snapshot.canvas.width;
    let can_down = viewport.y + viewport_height < state.snapshot.canvas.height;

    [can_left, can_up, can_down, can_right]
}

fn section_label(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    ))
}

fn rgb(color: dartboard_core::RgbColor) -> ratatui::style::Color {
    ratatui::style::Color::Rgb(color.r, color.g, color.b)
}

fn artboard_layout(area: Rect) -> ArtboardLayout {
    let info_width = area.width.min(INFO_WIDTH);
    let info_anchor = Rect::new(
        area.right().saturating_sub(info_width),
        area.y,
        info_width,
        area.height,
    );
    ArtboardLayout {
        canvas: area,
        info_anchor,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ArtboardLayout {
    canvas: Rect,
    info_anchor: Rect,
}

fn draw_canvas(
    frame: &mut Frame,
    game_area: Rect,
    canvas_area: Rect,
    info_area: Option<Rect>,
    state: &State,
) {
    if canvas_area.width == 0 || canvas_area.height == 0 {
        return;
    }

    let render_canvas = state.canvas_for_render();
    let canvas = render_canvas.as_ref().unwrap_or(&state.snapshot.canvas);
    let mut canvas_state = CanvasWidgetState::new(canvas, state.viewport_origin());
    if let Some(selection) = state.selection_view() {
        canvas_state = canvas_state.selection(selection);
    }
    if let Some(floating) = state.floating_view() {
        canvas_state = canvas_state.floating(floating);
    }
    frame.render_widget(
        CanvasWidget::new(&canvas_state).style(dartboard_canvas_style()),
        canvas_area,
    );

    if let Some(notice) = &state.private_notice {
        let overlay = Rect {
            x: canvas_area.x,
            y: canvas_area.bottom().saturating_sub(1),
            width: canvas_area.width,
            height: 1,
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                notice.as_str(),
                Style::default()
                    .fg(theme::AMBER_DIM())
                    .add_modifier(Modifier::ITALIC),
            ))),
            overlay,
        );
    }

    let swatch_boxes = render_swatch_strip(frame, game_area, info_area, state);

    // The widget renders cells; the frame owns the cursor position so the
    // terminal's native cursor lands on the active cell without the widget
    // needing to repaint a highlight.
    let cursor = state.cursor();
    let viewport_origin = state.viewport_origin();
    if state.should_show_canvas_cursor()
        && cursor.x >= viewport_origin.x
        && cursor.y >= viewport_origin.y
        && cursor.x < viewport_origin.x + canvas_area.width as usize
        && cursor.y < viewport_origin.y + canvas_area.height as usize
    {
        let cx = canvas_area.x + (cursor.x - viewport_origin.x) as u16;
        let cy = canvas_area.y + (cursor.y - viewport_origin.y) as u16;
        if !info_area.is_some_and(|rect| rect_contains(rect, cx, cy))
            && !swatch_boxes
                .iter()
                .flatten()
                .any(|rect| rect_contains(*rect, cx, cy))
        {
            frame.set_cursor_position((cx, cy));
        }
    }
}

pub(crate) fn swatch_hit(
    screen_size: (u16, u16),
    state: &State,
    sgr_x: u16,
    sgr_y: u16,
) -> Option<SwatchHit> {
    let col = sgr_x.checked_sub(1)?;
    let row = sgr_y.checked_sub(1)?;
    let boxes = swatch_box_rects(screen_size, state);

    for (idx, maybe_rect) in boxes.iter().enumerate() {
        let Some(rect) = maybe_rect else { continue };
        if state.swatches()[idx].is_some() && rect_contains(swatch_pin_rect(*rect), col, row) {
            return Some(SwatchHit::Pin(idx));
        }
    }

    for (idx, maybe_rect) in boxes.iter().enumerate() {
        let Some(rect) = maybe_rect else { continue };
        if rect_contains(swatch_body_rect(*rect), col, row) {
            return Some(SwatchHit::Body(idx));
        }
    }

    None
}

pub(crate) fn swatch_box_rects(
    screen_size: (u16, u16),
    state: &State,
) -> [Option<Rect>; SWATCH_CAPACITY] {
    let game_area = artboard_game_area_for_screen(screen_size);
    let info_area = artboard_info_area_for_screen(screen_size, state);
    swatch_box_rects_in_game_area(game_area, info_area, state.private_notice.is_some())
}

fn render_swatch_strip(
    frame: &mut Frame,
    game_area: Rect,
    info_area: Option<Rect>,
    state: &State,
) -> [Option<Rect>; SWATCH_CAPACITY] {
    let rects = swatch_box_rects_in_game_area(game_area, info_area, state.private_notice.is_some());
    let active_idx = state.active_swatch_index();
    let is_transparent = state.floating_is_transparent();
    let Some(strip_rect) = swatch_strip_rect(&rects) else {
        return rects;
    };

    frame.render_widget(Clear, strip_rect);
    render_swatch_strip_frame(frame.buffer_mut(), &rects, state, active_idx);
    for (idx, maybe_rect) in rects.iter().enumerate() {
        let Some(rect) = maybe_rect else {
            continue;
        };
        render_swatch_box_contents(
            frame.buffer_mut(),
            *rect,
            state.swatches()[idx].as_ref(),
            active_idx == Some(idx),
            active_idx == Some(idx) && is_transparent,
        );
    }

    rects
}

fn swatch_box_rects_in_game_area(
    game_area: Rect,
    info_area: Option<Rect>,
    has_notice: bool,
) -> [Option<Rect>; SWATCH_CAPACITY] {
    let mut rects = [None; SWATCH_CAPACITY];
    let margin_bottom = swatch_margin_bottom(has_notice);
    if game_area.width < SWATCH_BOX_WIDTH || game_area.height < SWATCH_BOX_HEIGHT + margin_bottom {
        return rects;
    }

    let box_y = game_area
        .bottom()
        .saturating_sub(margin_bottom + SWATCH_BOX_HEIGHT);
    let box_bottom = box_y + SWATCH_BOX_HEIGHT;
    let right_edge =
        if info_area.is_some_and(|info| ranges_overlap(box_y, box_bottom, info.y, info.bottom())) {
            info_area.expect("checked is_some").x
        } else {
            game_area.right()
        };
    let available_width = right_edge.saturating_sub(game_area.x);
    if available_width < SWATCH_BOX_WIDTH {
        return rects;
    }

    let n_visible = ((available_width - 1) / (SWATCH_BOX_WIDTH - 1)).min(SWATCH_CAPACITY as u16);
    if n_visible == 0 {
        return rects;
    }

    let strip_width = 1 + n_visible * (SWATCH_BOX_WIDTH - 1);
    let strip_x = right_edge - strip_width;
    for (idx, rect) in rects.iter_mut().enumerate() {
        if (idx as u16) >= n_visible {
            continue;
        }
        let box_x = strip_x + idx as u16 * (SWATCH_BOX_WIDTH - 1);
        *rect = Some(Rect::new(box_x, box_y, SWATCH_BOX_WIDTH, SWATCH_BOX_HEIGHT));
    }

    rects
}

fn render_swatch_strip_frame(
    buf: &mut Buffer,
    rects: &[Option<Rect>; SWATCH_CAPACITY],
    state: &State,
    active_idx: Option<usize>,
) {
    let Some(strip_rect) = swatch_strip_rect(rects) else {
        return;
    };
    let Some(last_idx) = rects.iter().rposition(Option::is_some) else {
        return;
    };
    let top_row = strip_rect.y;
    let bottom_row = strip_rect.bottom() - 1;

    for (idx, maybe_rect) in rects.iter().enumerate() {
        let Some(rect) = maybe_rect else {
            continue;
        };
        let style = swatch_border_style(state.swatches()[idx].as_ref(), active_idx == Some(idx));
        let divider_style = if idx == 0 {
            style
        } else {
            swatch_divider_style(
                state.swatches()[idx - 1].as_ref(),
                state.swatches()[idx].as_ref(),
                active_idx == Some(idx - 1),
                active_idx == Some(idx),
            )
        };
        let left = rect.x;
        let right = rect.right() - 1;
        let top_left = if idx == 0 { '┌' } else { '┬' };
        let bottom_left = if idx == 0 { '└' } else { '┴' };
        buf[(left, top_row)]
            .set_char(top_left)
            .set_style(divider_style);
        buf[(left, bottom_row)]
            .set_char(bottom_left)
            .set_style(divider_style);
        for x in (left + 1)..right {
            buf[(x, top_row)].set_char('─').set_style(style);
            buf[(x, bottom_row)].set_char('─').set_style(style);
        }
        for y in (top_row + 1)..bottom_row {
            buf[(left, y)].set_char('│').set_style(divider_style);
        }
        if idx == last_idx {
            buf[(right, top_row)].set_char('┐').set_style(style);
            buf[(right, bottom_row)].set_char('┘').set_style(style);
            for y in (top_row + 1)..bottom_row {
                buf[(right, y)].set_char('│').set_style(style);
            }
        }
    }
}

fn render_swatch_box_contents(
    buf: &mut Buffer,
    rect: Rect,
    swatch: Option<&Swatch>,
    _is_active: bool,
    is_transparent: bool,
) {
    let inner = Rect::new(rect.x + 1, rect.y + 1, rect.width - 2, rect.height - 2);
    for dy in 0..inner.height {
        for dx in 0..inner.width {
            buf[(inner.x + dx, inner.y + dy)]
                .set_char(' ')
                .set_bg(theme::BG_CANVAS())
                .set_fg(theme::TEXT());
        }
    }

    if let Some(swatch) = swatch {
        render_swatch_preview(buf, inner, &swatch.clipboard);
        let pin_rect = swatch_pin_rect(rect);
        let pin_char = if swatch.pinned {
            PIN_PINNED
        } else {
            PIN_UNPINNED
        };
        let pin_style = Style::default()
            .bg(theme::BG_CANVAS())
            .fg(if swatch.pinned {
                theme::BORDER_ACTIVE()
            } else {
                theme::TEXT_FAINT()
            });
        buf[(pin_rect.x, pin_rect.y)]
            .set_char(pin_char)
            .set_style(pin_style);
        buf[(pin_rect.x + 1, pin_rect.y)]
            .set_char(' ')
            .set_style(pin_style);
    }

    if is_transparent {
        buf[(rect.right() - 2, inner.y)].set_char('◌').set_style(
            Style::default()
                .fg(theme::BORDER_ACTIVE())
                .bg(theme::BG_CANVAS()),
        );
    }
}

fn swatch_border_style(swatch: Option<&Swatch>, is_active: bool) -> Style {
    if is_active {
        Style::default().fg(theme::BORDER_ACTIVE())
    } else if swatch.is_some() {
        Style::default().fg(theme::AMBER_DIM())
    } else {
        Style::default().fg(theme::BORDER_DIM())
    }
}

fn swatch_divider_style(
    left_swatch: Option<&Swatch>,
    right_swatch: Option<&Swatch>,
    left_active: bool,
    right_active: bool,
) -> Style {
    if left_active || right_active {
        Style::default().fg(theme::BORDER_ACTIVE())
    } else if left_swatch.is_some() || right_swatch.is_some() {
        Style::default().fg(theme::AMBER_DIM())
    } else {
        Style::default().fg(theme::BORDER_DIM())
    }
}

fn render_swatch_preview(buf: &mut Buffer, inner: Rect, clipboard: &Clipboard) {
    let (crop_x, crop_y) = clipboard_preview_offset(clipboard);
    let preview_style = Style::default().fg(theme::TEXT()).bg(theme::BG_HIGHLIGHT());

    for dy in 0..inner.height {
        let cy = crop_y + dy as usize;
        if cy >= clipboard.height {
            break;
        }

        let mut dx: u16 = 0;
        while dx < inner.width {
            let cx = crop_x + dx as usize;
            if cx >= clipboard.width {
                break;
            }

            match clipboard.get(cx, cy) {
                Some(CellValue::Narrow(ch)) => {
                    buf[(inner.x + dx, inner.y + dy)]
                        .set_char(ch)
                        .set_style(preview_style);
                    dx += 1;
                }
                Some(CellValue::Wide(ch)) => {
                    buf[(inner.x + dx, inner.y + dy)]
                        .set_char(ch)
                        .set_style(preview_style);
                    if dx + 1 < inner.width {
                        buf[(inner.x + dx + 1, inner.y + dy)]
                            .set_char(' ')
                            .set_style(preview_style);
                    }
                    dx += 2;
                }
                Some(CellValue::WideCont) | None => {
                    buf[(inner.x + dx, inner.y + dy)]
                        .set_char(' ')
                        .set_style(preview_style);
                    dx += 1;
                }
            }
        }
    }
}

fn clipboard_preview_offset(clipboard: &Clipboard) -> (usize, usize) {
    let has_visible = (0..clipboard.height)
        .any(|y| (0..clipboard.width).any(|x| cell_is_visible(clipboard.get(x, y))));
    if !has_visible {
        return (0, 0);
    }

    let mut first_row = 0;
    'outer_row: for y in 0..clipboard.height {
        for x in 0..clipboard.width {
            if cell_is_visible(clipboard.get(x, y)) {
                first_row = y;
                break 'outer_row;
            }
        }
    }

    let mut first_col = 0;
    'outer_col: for x in 0..clipboard.width {
        for y in 0..clipboard.height {
            if cell_is_visible(clipboard.get(x, y)) {
                first_col = x;
                break 'outer_col;
            }
        }
    }

    (first_col, first_row)
}

fn cell_is_visible(cell: Option<CellValue>) -> bool {
    match cell {
        Some(CellValue::Narrow(ch) | CellValue::Wide(ch)) => ch != ' ',
        Some(CellValue::WideCont) => true,
        None => false,
    }
}

fn swatch_pin_rect(rect: Rect) -> Rect {
    Rect::new(rect.x + rect.width - 3, rect.y + rect.height - 2, 2, 1)
}

fn swatch_body_rect(rect: Rect) -> Rect {
    Rect::new(rect.x + 1, rect.y + 1, rect.width - 2, rect.height - 2)
}

fn swatch_margin_bottom(has_notice: bool) -> u16 {
    SWATCH_BOTTOM_CLEARANCE
        + if has_notice {
            SWATCH_NOTICE_CLEARANCE
        } else {
            0
        }
}

fn rect_contains(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x && y >= rect.y && x < rect.right() && y < rect.bottom()
}

fn ranges_overlap(a_start: u16, a_end: u16, b_start: u16, b_end: u16) -> bool {
    a_start < b_end && b_start < a_end
}

fn swatch_strip_rect(rects: &[Option<Rect>; SWATCH_CAPACITY]) -> Option<Rect> {
    let first = rects.iter().flatten().next().copied()?;
    let last = rects.iter().flatten().last().copied()?;
    Some(Rect::new(
        first.x,
        first.y,
        last.right() - first.x,
        first.height,
    ))
}

fn artboard_game_area_for_screen(screen_size: (u16, u16)) -> Rect {
    let screen = Rect::new(0, 0, screen_size.0, screen_size.1);
    let app_inner = Block::default().borders(Borders::ALL).inner(screen);
    Layout::horizontal([Constraint::Fill(1), Constraint::Length(24)]).split(app_inner)[0]
}

fn artboard_info_area_for_screen(screen_size: (u16, u16), state: &State) -> Option<Rect> {
    let info_lines = artboard_info_lines(state, false);
    let layout = artboard_layout(artboard_game_area_for_screen(screen_size));
    info_block_area(layout.info_anchor, info_lines.len())
}

fn help_popup_area(area: Rect) -> Rect {
    centered_rect(96, 34, area)
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}

fn help_layout(popup: Rect) -> Option<[Rect; 5]> {
    let inner = Block::default().borders(Borders::ALL).inner(popup);
    if inner.height < 5 || inner.width < 10 {
        return None;
    }
    let layout = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(8),
        Constraint::Length(1),
    ])
    .split(inner);
    Some([layout[0], layout[1], layout[2], layout[3], layout[4]])
}

fn tab_rects(area: Rect) -> Vec<(HelpTab, Rect)> {
    let mut hits = Vec::with_capacity(HelpTab::ALL.len());
    let mut x = area.x.saturating_add(2);
    for tab in HelpTab::ALL {
        let label = tab.label();
        let width = label.chars().count() as u16 + 2;
        if x.saturating_add(width) > area.right() {
            break;
        }
        hits.push((tab, Rect::new(x, area.y, width, 1)));
        x = x.saturating_add(width).saturating_add(1);
    }
    hits
}

pub(crate) fn help_tab_hit(
    screen_size: (u16, u16),
    state: &State,
    sgr_x: u16,
    sgr_y: u16,
) -> Option<HelpTab> {
    if !state.is_help_open() {
        return None;
    }
    let col = sgr_x.checked_sub(1)?;
    let row = sgr_y.checked_sub(1)?;
    let area = artboard_game_area_for_screen(screen_size);
    let popup = help_popup_area(area);
    let layout = help_layout(popup)?;
    tab_rects(layout[1])
        .into_iter()
        .find_map(|(tab, rect)| rect_contains(rect, col, row).then_some(tab))
}

pub(crate) fn info_hit(screen_size: (u16, u16), state: &State, sgr_x: u16, sgr_y: u16) -> bool {
    let Some(info_area) = artboard_info_area_for_screen(screen_size, state) else {
        return false;
    };
    let Some(col) = sgr_x.checked_sub(1) else {
        return false;
    };
    let Some(row) = sgr_y.checked_sub(1) else {
        return false;
    };
    rect_contains(info_area, col, row)
}

fn draw_help(frame: &mut Frame, area: Rect, state: &State) {
    let popup = help_popup_area(area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Artboard Help ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));

    frame.render_widget(block, popup);

    let Some(layout) = help_layout(popup) else {
        return;
    };

    draw_tabs(frame, layout[1], state.help_tab());

    let body = layout[3].inner(Margin::new(2, 0));
    let lines: Vec<Line> = lines_for(state.help_tab())
        .into_iter()
        .map(|line| Line::from(Span::styled(line, Style::default().fg(theme::TEXT()))))
        .collect();
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .scroll((state.help_scroll(), 0)),
        body,
    );

    let footer = Line::from(vec![
        Span::styled("  Tab/S+Tab", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" switch tabs  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("↑↓ j/k", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" scroll  ", Style::default().fg(theme::TEXT_DIM())),
        Span::styled("Esc/q", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" close", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(footer), layout[4]);
}

fn draw_tabs(frame: &mut Frame, area: Rect, selected: HelpTab) {
    let mut spans = vec![Span::raw("  ")];
    for tab in HelpTab::ALL {
        let active = tab == selected;
        let style = if active {
            Style::default()
                .fg(theme::AMBER_GLOW())
                .bg(theme::BG_HIGHLIGHT())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(format!(" {} ", tab.label()), style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::artboard::provenance::ArtboardProvenance;
    use crate::app::artboard::state::State;
    use dartboard_core::{CellValue, RgbColor};
    use dartboard_editor::Clipboard;
    use ratatui::buffer::Buffer;

    use super::super::svc::{DartboardService, DartboardSnapshot};

    #[test]
    fn canvas_area_matches_artboard_frame_layout() {
        assert_eq!(canvas_area_for_screen((80, 24)), Rect::new(1, 1, 54, 22));
    }

    #[test]
    fn info_box_overlays_top_right_of_full_canvas_width() {
        let state = test_state();
        assert_eq!(
            artboard_info_area_for_screen((80, 24), &state),
            Some(Rect::new(27, 1, 28, 9))
        );
    }

    #[test]
    fn help_lines_cover_all_tabs_with_title_headings() {
        for tab in HelpTab::ALL {
            let lines = lines_for(tab);
            assert!(!lines.is_empty(), "{:?} should have content", tab);
            assert!(!lines[0].is_empty(), "{:?} should lead with a heading", tab);
        }
        let drawing = lines_for(HelpTab::Drawing).join("\n");
        assert!(drawing.contains("move cursor"));
        assert!(drawing.contains("Shift+arrows"));
    }

    #[test]
    fn clipboard_preview_offset_skips_leading_blank_rows_and_columns() {
        let clipboard = Clipboard::new(
            4,
            3,
            vec![
                Some(CellValue::Narrow(' ')),
                Some(CellValue::Narrow(' ')),
                Some(CellValue::Narrow(' ')),
                Some(CellValue::Narrow(' ')),
                Some(CellValue::Narrow(' ')),
                Some(CellValue::Narrow(' ')),
                Some(CellValue::Narrow('A')),
                Some(CellValue::Narrow(' ')),
                Some(CellValue::Narrow(' ')),
                Some(CellValue::Narrow(' ')),
                Some(CellValue::Narrow(' ')),
                Some(CellValue::Narrow(' ')),
            ],
        );

        assert_eq!(clipboard_preview_offset(&clipboard), (2, 1));
    }

    #[test]
    fn help_tab_hit_uses_overlay_tab_rects() {
        let mut state = test_state();
        state.toggle_help();
        let area = artboard_game_area_for_screen((80, 24));
        let popup = help_popup_area(area);
        let layout = help_layout(popup).expect("help layout");
        let drawing = tab_rects(layout[1])
            .into_iter()
            .find(|(tab, _)| *tab == HelpTab::Drawing)
            .expect("drawing tab hit rect");
        let rect = drawing.1;

        assert_eq!(
            help_tab_hit((80, 24), &state, rect.x + 1, rect.y + 1),
            Some(HelpTab::Drawing)
        );
    }

    #[test]
    fn help_scroll_is_preserved_per_tab() {
        let mut state = test_state();
        state.toggle_help();
        state.scroll_help(3);
        assert_eq!(state.help_scroll(), 3);
        state.select_next_help_tab();
        assert_eq!(state.help_scroll(), 0);
        state.scroll_help(7);
        assert_eq!(state.help_scroll(), 7);
        state.select_prev_help_tab();
        assert_eq!(state.help_scroll(), 3);
    }

    #[test]
    fn info_block_height_tracks_visible_lines() {
        assert_eq!(info_block_height(0), 3);
        assert_eq!(info_block_height(1), 3);
        assert_eq!(info_block_height(2), 4);
    }

    #[test]
    fn info_lines_include_mode_and_pan_rows_before_brush() {
        let state = test_state();
        let lines = artboard_info_lines(&state, false);

        assert_eq!(lines[0].to_string(), "Mode       view");
        assert_eq!(lines[1].to_string(), "Cursor     0,0");
        assert_eq!(lines[2].to_string(), "Owner      ?");
        assert_eq!(lines[3].to_string(), "Cell       0,0");
        assert_eq!(lines[4].to_string(), "Pan        ◀ ▲ ▼ ▶");
        assert_eq!(lines[5].to_string(), "Brush      none");
        assert_eq!(lines[6].to_string(), "Selection  none");
    }

    #[test]
    fn info_lines_show_selection_dimensions() {
        let mut state = test_state();
        state.begin_selection_from_cursor();
        state.move_right((80, 24));
        state.move_right((80, 24));
        state.move_down((80, 24));
        assert!(state.update_selection_to_cursor());

        let lines = artboard_info_lines(&state, true);
        assert_eq!(lines[0].to_string(), "Mode       active");
        assert_eq!(lines[6].to_string(), "Selection  3x2");
    }

    #[test]
    fn swatch_boxes_use_full_artboard_width_below_short_info_block() {
        let state = test_state();
        let rects = swatch_box_rects((80, 24), &state);
        assert_eq!(rects[0], Some(Rect::new(9, 14, 16, 8)));
        assert_eq!(rects[1], Some(Rect::new(24, 14, 16, 8)));
        assert_eq!(rects[2], Some(Rect::new(39, 14, 16, 8)));
        assert!(rects[3].is_none());
    }

    #[test]
    fn swatch_boxes_fall_back_to_canvas_edge_when_info_block_reaches_them() {
        let mut state = test_state();
        state.snapshot.peers = (0..10)
            .map(|idx| dartboard_core::Peer {
                user_id: idx as u64,
                name: format!("user{idx}"),
                color: RgbColor::new(120, 120, 120),
            })
            .collect();
        let rects = swatch_box_rects((80, 24), &state);
        assert_eq!(rects[0], Some(Rect::new(11, 14, 16, 8)));
        assert!(rects[1].is_none());
    }

    #[test]
    fn swatch_boxes_raise_above_notice_row() {
        let mut state = test_state();
        state.private_notice = Some("Heads up".to_string());
        let rects = swatch_box_rects((80, 24), &state);
        assert_eq!(rects[0], Some(Rect::new(9, 13, 16, 8)));
    }

    #[test]
    fn swatch_boxes_leave_bottom_canvas_row_visible() {
        let state = test_state();
        let rects = swatch_box_rects((80, 24), &state);
        let canvas = canvas_area_for_screen((80, 24));

        assert!(
            !rects
                .iter()
                .flatten()
                .any(|rect| rect_contains(*rect, 10, canvas.bottom() - 1))
        );
    }

    #[test]
    fn swatch_hit_uses_sgr_coordinates_and_prefers_pin() {
        let mut state = test_state();
        state.editor.swatches[0] = Some(dartboard_editor::Swatch {
            clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
            pinned: false,
        });

        assert_eq!(
            swatch_hit((80, 24), &state, 11, 16),
            Some(SwatchHit::Body(0))
        );
        assert_eq!(
            swatch_hit((80, 24), &state, 23, 21),
            Some(SwatchHit::Pin(0))
        );
    }

    #[test]
    fn active_swatch_brightens_both_shared_dividers() {
        let mut state = test_state();
        for swatch in state.editor.swatches.iter_mut().take(3) {
            *swatch = Some(dartboard_editor::Swatch {
                clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
                pinned: false,
            });
        }
        state.activate_swatch(1);

        let rects = swatch_box_rects((120, 24), &state);
        let area = Rect::new(0, 0, 120, 24);
        let mut buf = Buffer::empty(area);
        render_swatch_strip_frame(&mut buf, &rects, &state, state.active_swatch_index());

        let middle = rects[1].expect("middle swatch visible");
        let right = rects[2].expect("right swatch visible");
        let divider_y = middle.y + 1;
        let top_y = middle.y;

        assert_eq!(buf[(middle.x, divider_y)].fg, theme::BORDER_ACTIVE());
        assert_eq!(buf[(right.x, divider_y)].fg, theme::BORDER_ACTIVE());
        assert_eq!(buf[(middle.x, top_y)].symbol(), "┬");
        assert_eq!(buf[(right.x, top_y)].symbol(), "┬");
    }

    #[test]
    fn filled_swatch_divider_beats_empty_neighbor() {
        let mut state = test_state();
        state.editor.swatches[0] = Some(dartboard_editor::Swatch {
            clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
            pinned: false,
        });

        let rects = swatch_box_rects((120, 24), &state);
        let area = Rect::new(0, 0, 120, 24);
        let mut buf = Buffer::empty(area);
        render_swatch_strip_frame(&mut buf, &rects, &state, state.active_swatch_index());

        let divider_x = rects[1].expect("second swatch visible").x;
        let divider_y = rects[1].expect("second swatch visible").y + 1;

        assert_eq!(buf[(divider_x, divider_y)].fg, theme::AMBER_DIM());
    }

    #[test]
    fn divider_priority_is_selected_then_filled_then_empty() {
        let mut state = test_state();
        for swatch in state.editor.swatches.iter_mut().take(2) {
            *swatch = Some(dartboard_editor::Swatch {
                clipboard: Clipboard::new(1, 1, vec![Some(CellValue::Narrow('A'))]),
                pinned: false,
            });
        }
        state.activate_swatch(0);

        let rects = swatch_box_rects((160, 24), &state);
        let area = Rect::new(0, 0, 160, 24);
        let mut buf = Buffer::empty(area);
        render_swatch_strip_frame(&mut buf, &rects, &state, state.active_swatch_index());

        let divider_12_x = rects[1].expect("second swatch visible").x;
        let divider_23_x = rects[2].expect("third swatch visible").x;
        let divider_34_x = rects[3].expect("fourth swatch visible").x;
        let _divider_45_x = rects[4].expect("fifth swatch visible").x;
        let divider_y = rects[1].expect("second swatch visible").y + 1;

        assert_eq!(buf[(divider_12_x, divider_y)].fg, theme::BORDER_ACTIVE());
        assert_eq!(buf[(divider_23_x, divider_y)].fg, theme::AMBER_DIM());
        assert_eq!(buf[(divider_34_x, divider_y)].fg, theme::BORDER_DIM());
    }

    #[test]
    fn pan_indicator_reflects_available_viewport_directions() {
        let mut state = test_state();
        state.snapshot.canvas = dartboard_core::Canvas::with_size(80, 60);
        state.editor.viewport.width = 26;
        state.editor.viewport.height = 22;
        state.editor.viewport_origin = dartboard_core::Pos { x: 5, y: 7 };
        let enabled = pan_indicator_enabled(&state);
        assert_eq!(enabled, [true, true, true, true]);

        state.editor.viewport_origin = dartboard_core::Pos { x: 0, y: 0 };
        state.snapshot.canvas = dartboard_core::Canvas::with_size(26, 22);
        let enabled = pan_indicator_enabled(&state);
        assert_eq!(enabled, [false, false, false, false]);
    }

    fn test_state() -> State {
        let shared_provenance = ArtboardProvenance::default().shared();
        let snapshot = DartboardSnapshot {
            provenance: ArtboardProvenance::default(),
            your_user_id: Some(1),
            your_color: Some(RgbColor::new(255, 196, 64)),
            ..Default::default()
        };
        let svc = DartboardService::disconnected_for_tests(snapshot);
        State::new(svc, "painter".to_string(), shared_provenance)
    }
}
