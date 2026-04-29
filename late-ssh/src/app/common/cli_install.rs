use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::theme;

pub const INSTALL_COMMAND: &str = "curl -fsSL https://cli.late.sh/install.sh | bash";
pub const SOURCE_URL: &str = "https://github.com/mpiorowski/late-sh";

const BUILD_STEPS: &[&str] = &[
    "git clone https://github.com/mpiorowski/late-sh",
    "cd late-sh",
    "cargo build --release --bin late",
];

const MODAL_WIDTH: u16 = 72;
const MODAL_HEIGHT: u16 = 17;

pub fn draw(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Install CLI ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let dim = Style::default().fg(theme::TEXT_DIM());
    let amber = Style::default().fg(theme::AMBER());
    let code = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .bg(theme::BG_HIGHLIGHT());

    let install_pill = format!("  {INSTALL_COMMAND}  ");
    let divider = "── BUILD SOURCE ──";

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Copied when supported. Paste in your local terminal",
            dim,
        ))
        .centered(),
        Line::from(""),
        Line::from(Span::styled(install_pill, code)).centered(),
        Line::from(""),
        Line::from(Span::styled(divider, dim)).centered(),
        Line::from(""),
        Line::from(Span::styled(SOURCE_URL, amber)).centered(),
        Line::from(""),
    ];
    for step in BUILD_STEPS {
        let pill = format!("  {step}  ");
        lines.push(Line::from(Span::styled(pill, code)).centered());
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("Press any key to close", dim)).centered());

    frame.render_widget(Paragraph::new(lines), inner);
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
