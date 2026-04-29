use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::theme;

pub const INSTALL_COMMAND: &str = "curl -fsSL https://cli.late.sh/install.sh | bash";
pub const SOURCE_URL: &str = "https://github.com/mpiorowski/late-sh";
pub const SOURCE_BUILD_COMMAND: &str = "git clone https://github.com/mpiorowski/late-sh && cd late-sh && cargo build --release --bin late";

pub fn draw(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(82, 14, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" CLI Install ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let dim = Style::default().fg(theme::TEXT_DIM());
    let bright = Style::default().fg(theme::TEXT_BRIGHT());
    let amber = Style::default().fg(theme::AMBER());
    let code = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .bg(theme::BG_HIGHLIGHT());
    let heading = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(" Installer", heading),
            Span::styled("  paste this in your local terminal", dim),
        ]),
        Line::from(Span::styled(format!("  {INSTALL_COMMAND}"), code)),
        Line::from(""),
        Line::from(vec![
            Span::styled(" BUILD SOURCE", heading),
            Span::styled("  if clipboard or installer is awkward", dim),
        ]),
        Line::from(vec![
            Span::styled("  Source: ", bright),
            Span::styled(SOURCE_URL, amber),
        ]),
        Line::from(Span::styled(format!("  {SOURCE_BUILD_COMMAND}"), code)),
        Line::from(""),
        Line::from(Span::styled("  Press any key to close.", dim)),
    ];

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width.saturating_sub(4)).max(1);
    let height = height.min(area.height.saturating_sub(4)).max(1);
    let [popup] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(area);
    let [popup] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(popup);
    popup
}
