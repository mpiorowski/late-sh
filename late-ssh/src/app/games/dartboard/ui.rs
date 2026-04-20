use dartboard_tui::{CanvasStyle, CanvasWidget, CanvasWidgetState};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{common::theme, games::ui::info_label_value};

use super::state::{Brush, State};

const INFO_WIDTH: u16 = 28;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State) {
    let info = artboard_info_lines(state);
    let canvas_area = draw_artboard_frame(frame, area, &info);
    draw_canvas(frame, canvas_area, state);
}

pub fn canvas_area_for_screen(screen_size: (u16, u16)) -> Rect {
    let screen = Rect::new(0, 0, screen_size.0, screen_size.1);
    let app_inner = Block::default().borders(Borders::ALL).inner(screen);
    let content_area =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(24)]).split(app_inner)[0];
    artboard_layout(content_area).canvas
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

fn draw_artboard_frame(frame: &mut Frame, area: Rect, info_lines: &[Line<'_>]) -> Rect {
    let block = Block::default()
        .title(artboard_title())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER()));
    let layout = artboard_layout(area);
    frame.render_widget(block, area);

    if layout.sidebar.width > 0 && layout.sidebar.height > 0 {
        let info_height = info_block_height(info_lines.len()).min(layout.sidebar.height);
        if info_height >= 3 {
            let info_area = Layout::vertical([Constraint::Length(info_height), Constraint::Min(0)])
                .split(layout.sidebar)[0];
            let info_block = Block::default()
                .title(" Info ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::BORDER()));
            let info_inner = info_block.inner(info_area);
            frame.render_widget(info_block, info_area);
            if info_inner.width > 0 && info_inner.height > 0 {
                frame.render_widget(Paragraph::new(info_lines.to_vec()), info_inner);
            }
        }
    }

    layout.canvas
}

fn artboard_info_lines(state: &State) -> Vec<Line<'static>> {
    let mut lines = vec![info_label_value(
        "Cursor",
        format!("{},{}", state.cursor.x, state.cursor.y),
        theme::AMBER(),
    )];

    let brush = state
        .active_brush()
        .map(Brush::label)
        .unwrap_or_else(|| "sample".to_string());
    lines.push(info_label_value("Brush", brush, theme::TEXT_BRIGHT()));
    if let Some(selection) = state.selection_view() {
        lines.push(info_label_value(
            "Selection",
            format!(
                "{},{} → {},{}",
                selection.anchor.x, selection.anchor.y, selection.cursor.x, selection.cursor.y
            ),
            theme::SUCCESS(),
        ));
    }
    if let Some(floating) = state.floating_view() {
        lines.push(info_label_value(
            "Floating",
            format!(
                "{}x{} @ {},{}",
                floating.width, floating.height, floating.anchor.x, floating.anchor.y
            ),
            theme::AMBER_GLOW(),
        ));
    }

    let swatches: Vec<_> = state.recent_brushes().collect();
    if !swatches.is_empty() {
        lines.push(Line::from(""));
        lines.push(section_label("Swatches"));
        for brush in swatches {
            let is_active = state.active_brush() == Some(brush);
            lines.push(brush_line(brush, is_active));
        }
    }

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

fn artboard_title() -> Line<'static> {
    let mut spans = vec![Span::styled(
        " Artboard ",
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    )];

    for (key, desc) in [("^P", "Help"), ("^Q", "Quit")] {
        spans.push(Span::styled("· ", Style::default().fg(theme::BORDER_DIM())));
        spans.push(Span::styled(key, Style::default().fg(theme::AMBER_DIM())));
        spans.push(Span::styled(
            format!(" {desc} "),
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }

    spans.push(Span::styled(
        "───────────",
        Style::default().fg(theme::BORDER_DIM()),
    ));

    Line::from(spans)
}

fn info_block_height(line_count: usize) -> u16 {
    line_count.max(1).saturating_add(2) as u16
}

fn section_label(text: &str) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    ))
}

fn brush_line(brush: Brush, is_active: bool) -> Line<'static> {
    let prefix = if is_active { "> " } else { "  " };
    let style = if is_active {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    Line::from(vec![
        Span::styled(prefix, style),
        Span::styled(brush.label(), style),
    ])
}

fn rgb(color: dartboard_core::RgbColor) -> ratatui::style::Color {
    ratatui::style::Color::Rgb(color.r, color.g, color.b)
}

fn artboard_layout(area: Rect) -> ArtboardLayout {
    let inner = Block::default().borders(Borders::ALL).inner(area);
    let cols =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(INFO_WIDTH)]).split(inner);
    ArtboardLayout {
        canvas: cols[0],
        sidebar: cols[1],
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ArtboardLayout {
    canvas: Rect,
    sidebar: Rect,
}

fn draw_canvas(frame: &mut Frame, area: Rect, state: &State) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let render_canvas = state.canvas_for_render();
    let canvas = render_canvas.as_ref().unwrap_or(&state.snapshot.canvas);
    let mut canvas_state = CanvasWidgetState::new(canvas, state.viewport_origin);
    if let Some(selection) = state.selection_view() {
        canvas_state = canvas_state.selection(selection);
    }
    if let Some(floating) = state.floating_view() {
        canvas_state = canvas_state.floating(floating);
    }
    frame.render_widget(
        CanvasWidget::new(&canvas_state).style(dartboard_canvas_style()),
        area,
    );

    // The widget renders cells; the frame owns the cursor position so the
    // terminal's native cursor lands on the active cell without the widget
    // needing to repaint a highlight.
    if state.cursor.x >= state.viewport_origin.x
        && state.cursor.y >= state.viewport_origin.y
        && state.cursor.x < state.viewport_origin.x + area.width as usize
        && state.cursor.y < state.viewport_origin.y + area.height as usize
    {
        let cx = area.x + (state.cursor.x - state.viewport_origin.x) as u16;
        let cy = area.y + (state.cursor.y - state.viewport_origin.y) as u16;
        frame.set_cursor_position((cx, cy));
    }

    if let Some(notice) = &state.private_notice {
        let overlay = Rect {
            x: area.x,
            y: area.bottom().saturating_sub(1),
            width: area.width,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canvas_area_matches_artboard_frame_layout() {
        assert_eq!(canvas_area_for_screen((80, 24)), Rect::new(2, 2, 24, 20));
    }

    #[test]
    fn info_block_height_tracks_visible_lines() {
        assert_eq!(info_block_height(0), 3);
        assert_eq!(info_block_height(1), 3);
        assert_eq!(info_block_height(2), 4);
    }
}
