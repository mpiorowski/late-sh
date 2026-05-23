//! Combined "install CLI + pair browser" modal opened with the global `Ctrl+R` shortcut.
//!
//! Renders one big scrollable paragraph: hero pitch -> install paths -> what `late`
//! unlocks (audio, youtube, clipboard read + copy, controls) -> terminal-FAQ pointer
//! -> browser-pair QR and link. The QR is rendered as plain HalfBlock text so it
//! lives in the same scroll stream as the rest of the content. Press j/k or arrows
//! to scroll down to it. Phones scan the standard `▀`/`▄`/`█` rendering fine.

use qrcodegen::{QrCode, QrCodeEcc};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::qr::{Barcode, HalfBlock};
use super::theme;

pub const SHELL_INSTALL_COMMAND: &str = "curl -fsSL https://cli.late.sh/install.sh | bash";
pub const WINDOWS_INSTALL_COMMAND: &str = "irm https://cli.late.sh/install.ps1 | iex";
pub const NIX_COMMAND: &str = "nix run github:mpiorowski/late-sh#late";
pub const SOURCE_URL: &str = "https://github.com/mpiorowski/late-sh";

const MODAL_WIDTH: u16 = 82;
const MODAL_HEIGHT: u16 = 32;
const QUIET_ZONE: i32 = 4;

pub fn draw(frame: &mut Frame, area: Rect, pair_url: &str, scroll: u16) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Install `late` · Pair Browser ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let [body_area, footer_area] =
        Layout::vertical([Constraint::Min(8), Constraint::Length(1)]).areas(inner);

    let lines = build_lines(pair_url);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0)),
        body_area,
    );

    draw_footer(frame, footer_area);
}

fn build_lines(pair_url: &str) -> Vec<Line<'static>> {
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let text = Style::default().fg(theme::TEXT());
    let bright = Style::default().fg(theme::TEXT_BRIGHT());
    let amber = Style::default().fg(theme::AMBER());
    let amber_bold = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);
    let code = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .bg(theme::BG_HIGHLIGHT());
    let key = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);

    let mut lines: Vec<Line<'static>> = vec![
        Line::from(vec![
            Span::styled(" one process. ", amber_bold),
            Span::styled("ssh + local audio + youtube webview + os clipboard.", text),
        ]),
        Line::from(vec![
            Span::styled(" run ", dim),
            Span::styled("`late`", bright),
            Span::styled(" instead of ", dim),
            Span::styled("`ssh late.sh`", bright),
            Span::styled(". no browser tab required.", dim),
        ]),
        Line::from(""),
        Line::from(Span::styled(" install", amber_bold)),
        Line::from(vec![
            Span::styled("   linux · macos · termux   ", faint),
            Span::styled(format!(" {SHELL_INSTALL_COMMAND} "), code),
        ]),
        Line::from(vec![
            Span::styled("   windows powershell       ", faint),
            Span::styled(format!(" {WINDOWS_INSTALL_COMMAND} "), code),
        ]),
        Line::from(vec![
            Span::styled("   nixos                    ", faint),
            Span::styled(format!(" {NIX_COMMAND} "), code),
        ]),
        Line::from(vec![
            Span::styled("   build from source        ", faint),
            Span::styled(format!(" git clone {SOURCE_URL} "), code),
        ]),
        Line::from(vec![
            Span::styled("                            ", faint),
            Span::styled(" cargo build --release --bin late ", code),
        ]),
        Line::from(""),
        Line::from(Span::styled(" what `late` unlocks", amber_bold)),
        Line::from(vec![
            Span::styled("   audio       ", faint),
            Span::styled("icecast playback + visualizer on your machine", text),
        ]),
        Line::from(vec![
            Span::styled("   youtube     ", faint),
            Span::styled("embedded webview hosts the shared queue locally", text),
        ]),
        Line::from(vec![
            Span::styled("   clipboard   ", faint),
            Span::styled("/paste-image", amber),
            Span::styled(" drops your OS clipboard image into chat", text),
        ]),
        Line::from(vec![
            Span::styled("   controls    ", faint),
            Span::styled("m", key),
            Span::styled(" mute · ", text),
            Span::styled("+/-", key),
            Span::styled(" volume · ", text),
            Span::styled("v+x", key),
            Span::styled(" swap source · ", text),
            Span::styled("v+v", key),
            Span::styled(" music booth", text),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(" something off? press ", dim),
            Span::styled("Ctrl+L", key),
            Span::styled(" for the terminal FAQ:", dim),
        ]),
        Line::from(Span::styled(
            "   copy · clickable links · images · selection · notifications · cli youtube",
            faint,
        )),
        Line::from(""),
        Line::from(Span::styled(
            "──────── alternatively pair browser ────────",
            faint,
        ))
        .centered(),
        Line::from(""),
    ];

    if let Ok(qr) = QrCode::encode_text(pair_url, QrCodeEcc::Low) {
        for row in qr_lines(&qr) {
            lines.push(row);
        }
    }

    lines.push(Line::from(""));
    lines.push(
        Line::from(Span::styled(
            pair_url.to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ))
        .centered(),
    );
    lines.push(
        Line::from(Span::styled(
            "scan with your phone or open the link on any device",
            Style::default().fg(theme::TEXT_DIM()),
        ))
        .centered(),
    );

    lines
}

fn qr_lines(qr: &QrCode) -> Vec<Line<'static>> {
    let size = qr.size();
    let total = size + QUIET_ZONE * 2;
    let style = Style::default().fg(Color::Black).bg(Color::White);

    let module = |x: i32, y: i32| -> bool {
        let mx = x - QUIET_ZONE;
        let my = y - QUIET_ZONE;
        if mx < 0 || my < 0 || mx >= size || my >= size {
            return false;
        }
        qr.get_module(mx, my)
    };

    let mut out = Vec::with_capacity(((total + 1) / 2) as usize);
    let mut y = 0i32;
    while y < total {
        let mut s = String::with_capacity(total as usize);
        for x in 0..total {
            let top = module(x, y);
            let bot = module(x, y + 1);
            let bits = (top as u32) | ((bot as u32) << 1);
            s.push(HalfBlock::glyph(bits));
        }
        out.push(Line::from(Span::styled(s, style)).centered());
        y += 2;
    }
    out
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    if area.height == 0 {
        return;
    }
    let dim = Style::default().fg(theme::TEXT_DIM());
    let key = Style::default().fg(theme::AMBER_DIM());
    let footer = Line::from(vec![
        Span::styled("  ↑↓ j/k", key),
        Span::styled(" scroll   ", dim),
        Span::styled("PgUp/PgDn", key),
        Span::styled(" jump   ", dim),
        Span::styled("Esc/q", key),
        Span::styled(" close", dim),
    ]);
    frame.render_widget(Paragraph::new(footer), area);
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
