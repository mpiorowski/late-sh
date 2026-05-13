//! Combined "install CLI + pair browser" modal opened with the global `P` shortcut.
//!
//! The top half lists the three install paths (curl, nix run, build-from-source);
//! the bottom half renders a QR for the user's session pairing URL. The URL is
//! also staged into the clipboard via `pending_clipboard` at open time.

use qrcodegen::{QrCode, QrCodeEcc};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::qr::{DarkOnLight, HalfBlock, QrWidget};
use super::theme;

pub const INSTALL_COMMAND: &str = "curl -fsSL https://cli.late.sh/install.sh | bash";
pub const NIX_COMMAND: &str = "nix run github:mpiorowski/late-sh#late";
pub const SOURCE_URL: &str = "https://github.com/mpiorowski/late-sh";

const BUILD_STEPS: &[&str] = &[
    "git clone https://github.com/mpiorowski/late-sh",
    "cd late-sh",
    "cargo build --release --bin late",
];

const MODAL_WIDTH: u16 = 80;
const MODAL_HEIGHT: u16 = 36;

pub fn draw(frame: &mut Frame, area: Rect, pair_url: &str) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Install CLI & Pair Browser ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let qr_code = QrCode::encode_text(pair_url, QrCodeEcc::Low).ok();
    let qr_widget = qr_code.as_ref().map(|qr| {
        QrWidget::<HalfBlock, DarkOnLight>::new(qr)
            .with_style(Style::default().fg(Color::Black).bg(Color::White))
    });
    let qr_size = qr_widget
        .as_ref()
        .map(|w| w.size(inner))
        .unwrap_or_else(|| ratatui::layout::Size::new(0, 0));

    let footer_h: u16 = 2;
    let qr_block_h: u16 = qr_size.height.saturating_add(3); // qr + url + copied note

    let [install_area, divider_area, pair_area, footer_area] = Layout::vertical([
        Constraint::Min(11),
        Constraint::Length(1),
        Constraint::Length(qr_block_h.max(1)),
        Constraint::Length(footer_h),
    ])
    .areas(inner);

    draw_install_section(frame, install_area);
    draw_divider(frame, divider_area, "── alternatively pair browser ──");
    draw_pair_section(frame, pair_area, qr_widget.as_ref(), qr_size, pair_url);
    draw_footer(frame, footer_area);
}

fn draw_install_section(frame: &mut Frame, area: Rect) {
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let amber = Style::default().fg(theme::AMBER());
    let code = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .bg(theme::BG_HIGHLIGHT());

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled("linux / macos / windows", faint)).centered());
    lines.push(Line::from(Span::styled(pill(INSTALL_COMMAND), code)).centered());
    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled("nixos", faint)]).centered());
    lines.push(Line::from(Span::styled(pill(NIX_COMMAND), code)).centered());
    lines.push(Line::from(""));
    lines.push(
        Line::from(vec![
            Span::styled("or build from source · ", faint),
            Span::styled(SOURCE_URL, amber),
        ])
        .centered(),
    );
    for step in BUILD_STEPS {
        lines.push(Line::from(Span::styled(pill(step), code)).centered());
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_pair_section(
    frame: &mut Frame,
    area: Rect,
    qr_widget: Option<&QrWidget<'_, HalfBlock, DarkOnLight>>,
    qr_size: ratatui::layout::Size,
    pair_url: &str,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }

    let [qr_area, url_area] = Layout::vertical([
        Constraint::Length(qr_size.height.max(1)),
        Constraint::Length(1),
    ])
    .areas(area);

    if let Some(qr) = qr_widget {
        let [qr_centered] = Layout::horizontal([Constraint::Length(qr_size.width.max(1))])
            .flex(Flex::Center)
            .areas(qr_area);
        frame.render_widget(qr, qr_centered);
    }

    let amber = Style::default().fg(theme::AMBER());
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(pair_url.to_string(), amber))).centered(),
        url_area,
    );
}

fn draw_divider(frame: &mut Frame, area: Rect, label: &'static str) {
    if area.height == 0 {
        return;
    }
    let dim = Style::default().fg(theme::TEXT_FAINT());
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(label, dim))).centered(),
        area,
    );
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    if area.height == 0 {
        return;
    }
    let dim = Style::default().fg(theme::TEXT_DIM());
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled("Press any key to close", dim)).centered(),
    ];
    frame.render_widget(Paragraph::new(lines), area);
}

fn pill(text: &str) -> String {
    format!("  {text}  ")
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
