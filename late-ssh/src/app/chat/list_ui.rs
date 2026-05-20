use crate::app::common::theme;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

pub(crate) fn filtered_list_areas(area: Rect, mine_only: bool) -> (Option<Rect>, Rect) {
    if !mine_only || area.height < 2 {
        return (None, area);
    }
    let [status_area, list_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(area);
    (Some(status_area), list_area)
}

pub(crate) fn draw_mine_only_status(frame: &mut Frame, area: Rect, label: &str) {
    if area.width == 0 {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                "mine only",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(" · showing your {label}"),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
        ])),
        area,
    );
}
