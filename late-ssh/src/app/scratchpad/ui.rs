use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::common::theme;

use super::highlight::gutter_prefix_width;
use super::state::ScratchpadState;

/// A one-line "paired with @user" header, a gutter-and-syntax-highlighted
/// body (see `highlight.rs`), and a manually-placed real cursor. No mouse
/// click targets: keyboard-only editing (see module `CONTEXT.md`).
///
/// The body can no longer be `frame.render_widget(&state.editor, ..)`:
/// `ratatui-textarea`'s own `Widget` impl only supports one uniform `Style`
/// for the whole buffer, with no hook for per-token colors. `TextArea` still
/// owns all the editing state; this function only reads it (`lines()`,
/// `screen_cursor()`) to build the display by hand.
pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &ScratchpadState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(area);

    let language = state.language();
    let header = if state.partner_left() {
        Line::from(Span::styled(
            format!("@{} left the pairing. Esc to exit", state.partner_username),
            Style::default().fg(theme::TEXT_DIM()),
        ))
    } else {
        Line::from(vec![
            Span::styled("paired with ", Style::default().fg(theme::TEXT_DIM())),
            Span::styled(
                format!("@{}", state.partner_username),
                Style::default().fg(theme::AMBER()),
            ),
            Span::styled(
                format!(
                    "  (line {})  {}  Ctrl+L cycles language  Esc to leave",
                    state.partner_cursor.0.saturating_add(1),
                    language.label()
                ),
                Style::default().fg(theme::TEXT_DIM()),
            ),
        ])
    };
    frame.render_widget(Paragraph::new(header), chunks[0]);

    let total_lines = state.editor.lines().len();
    let gutter_width = gutter_prefix_width(total_lines);

    // The gutter gets its own fixed column, never scrolled horizontally.
    // `Paragraph::scroll`'s horizontal offset applies uniformly to every line
    // in the widget, so a gutter sharing the content's `Paragraph` would
    // scroll sideways right along with it -- see `highlight::gutter_lines`.
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(gutter_width as u16), Constraint::Min(0)])
        .split(chunks[1]);
    let gutter_area = body_chunks[0];
    let content_area = body_chunks[1];

    let cursor = state.editor.screen_cursor();
    let visible_height = content_area.height as usize;
    let vscroll = cursor.row.saturating_sub(visible_height.saturating_sub(1));
    let visible_width = content_area.width as usize;
    let hscroll = cursor.col.saturating_sub(visible_width.saturating_sub(1));

    frame.render_widget(
        Paragraph::new(super::highlight::gutter_lines(total_lines)).scroll((vscroll as u16, 0)),
        gutter_area,
    );
    // Only the lines down to the bottom of the viewport are worth styling, and
    // only when the text actually changed since the last frame -- syntect over
    // a full buffer costs more than the render loop's entire frame budget (see
    // `highlight::HighlightCache`).
    let visible_end = vscroll.saturating_add(visible_height);
    frame.render_widget(
        Paragraph::new(state.highlighted_body(language, visible_end))
            .scroll((vscroll as u16, hscroll as u16)),
        content_area,
    );

    state.record_viewport(content_area, vscroll, hscroll);

    let cursor_row_in_view = cursor.row.saturating_sub(vscroll);
    let cursor_col_in_view = cursor.col.saturating_sub(hscroll);
    if cursor_row_in_view < visible_height && cursor_col_in_view < visible_width {
        frame.set_cursor_position((
            content_area.x + cursor_col_in_view as u16,
            content_area.y + cursor_row_in_view as u16,
        ));
    }
}
