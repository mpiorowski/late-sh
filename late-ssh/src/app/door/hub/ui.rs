use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::state::HubGame;
use crate::app::common::{primitives::hint_line, theme};
use crate::app::files::terminal_image::{TerminalImageFrame, TerminalImageProtocol};

/// View data the renderer needs for one frame of the Games hub.
pub struct HubView {
    pub selected: usize,
    pub delete_confirm: bool,
    pub rebels_enabled: bool,
    pub nethack_enabled: bool,
    pub terminal_image_protocol: Option<TerminalImageProtocol>,
}

pub fn draw_games_hub(
    frame: &mut Frame,
    area: Rect,
    view: &HubView,
    terminal_images: &mut TerminalImageFrame,
) {
    if area.height < 6 || area.width < 40 {
        frame.render_widget(
            Paragraph::new("Terminal too small for Games")
                .alignment(ratatui::layout::Alignment::Center),
            area,
        );
        return;
    }

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // breathing room under the top border
            Constraint::Length(1), // selector row
            Constraint::Length(1), // rule under the selector
            Constraint::Min(0),    // selected game's landing
            Constraint::Length(1), // footer hints
        ])
        .split(area);

    let selected = view.selected.min(HubGame::ALL.len() - 1);

    draw_selector_row(frame, layout[1], selected);
    frame.render_widget(full_rule(layout[2].width), layout[2]);

    // The selected game owns the body, rendered with its real two-column
    // landing (logo, stats, native banner/art) so it fills the width.
    match HubGame::ALL[selected] {
        HubGame::Lateania => crate::app::door::lateania::screen::draw_landing(
            frame,
            layout[3],
            view.delete_confirm,
            view.terminal_image_protocol,
            terminal_images,
        ),
        HubGame::Rebels => {
            crate::app::door::rebels::render::draw_landing(frame, layout[3], view.rebels_enabled);
        }
        HubGame::Nethack => {
            crate::app::door::nethack::render::draw_landing(frame, layout[3], view.nethack_enabled);
        }
    }

    draw_footer(frame, layout[4]);
}

fn draw_selector_row(frame: &mut Frame, area: Rect, selected: usize) {
    let mut spans = vec![Span::raw("  ")];
    for (i, game) in HubGame::ALL.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        let style = if i == selected {
            Style::default()
                .fg(theme::BG_SELECTION())
                .bg(theme::AMBER())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme::TEXT_DIM())
        };
        spans.push(Span::styled(format!(" {} ", game.label()), style));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let hints: &[(&str, &str)] = &[
        ("\u{2190} \u{2192}  or  j k", "switch game"),
        ("Enter", "play"),
    ];
    frame.render_widget(Paragraph::new(hint_line(hints)), area);
}

/// Faint full-width horizontal rule under the selector row.
fn full_rule(width: u16) -> Paragraph<'static> {
    let line = "\u{2500}".repeat(width as usize);
    Paragraph::new(Line::from(Span::styled(
        line,
        Style::default().fg(theme::BORDER_DIM()),
    )))
}

/// Which selector chip (if any) sits at terminal cell `(x, y)`. Mirrors the
/// layout in `draw_selector_row` (2-space lead, then `" {label} "` chips with a
/// 2-space gap), used for click-to-select.
pub fn selector_hit_test(area: Rect, x: u16, y: u16) -> Option<usize> {
    if y != area.y {
        return None;
    }
    let mut col = area.x + 2;
    for (i, game) in HubGame::ALL.iter().enumerate() {
        if i > 0 {
            col += 2;
        }
        let width = game.label().len() as u16 + 2; // surrounding spaces
        if x >= col && x < col + width {
            return Some(i);
        }
        col += width;
    }
    None
}
