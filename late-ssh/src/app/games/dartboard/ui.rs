use dartboard_tui::{CanvasStyle, CanvasWidget, CanvasWidgetState};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{
    common::theme,
    games::ui::{draw_game_frame, info_label_value, info_tagline, key_hint},
};

use super::state::State;

pub fn draw_game(frame: &mut Frame, area: Rect, state: &State) {
    let info = vec![
        info_tagline("Shared terminal artboard"),
        info_label_value(
            "Peers",
            state.snapshot.peers.len().to_string(),
            theme::SUCCESS(),
        ),
        info_label_value(
            "Cursor",
            format!("{},{}", state.cursor.x, state.cursor.y),
            theme::AMBER(),
        ),
        key_hint("Arrows", "move cursor"),
        key_hint("Type", "paint glyph"),
        key_hint("Bksp/Del", "clear glyph"),
        key_hint("Mouse", "hover / place cursor"),
        key_hint("^Q", "leave artboard"),
    ];
    let canvas_area = draw_game_frame(frame, area, "Artboard", info);
    draw_canvas(frame, canvas_area, state);
}

pub fn canvas_area_for_screen(screen_size: (u16, u16)) -> Rect {
    let area = Rect::new(0, 0, screen_size.0, screen_size.1);
    let root_inner = Block::default().borders(Borders::ALL).inner(area);
    let cols = Layout::horizontal([Constraint::Fill(1), Constraint::Length(24)]).split(root_inner);
    let game_inner = Block::default().borders(Borders::ALL).inner(cols[0]);
    Layout::horizontal([Constraint::Fill(1), Constraint::Length(28)]).split(game_inner)[0]
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

fn draw_canvas(frame: &mut Frame, area: Rect, state: &State) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let canvas_state = CanvasWidgetState::new(&state.snapshot.canvas, state.viewport_origin);
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
