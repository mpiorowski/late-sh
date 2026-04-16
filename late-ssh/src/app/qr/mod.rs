use qrcodegen::QrCode;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::common::theme;

#[derive(Copy, Clone, Debug)]
enum HalfBlock {
    Empty, // ' '
    Upper, // ▀
    Lower, // ▄
    Full,  // █
}

impl HalfBlock {
    const fn from_modules(top: bool, bot: bool) -> Self {
        match (top, bot) {
            (false, false) => Self::Empty,
            (true, false) => Self::Upper,
            (false, true) => Self::Lower,
            (true, true) => Self::Full,
        }
    }

    const fn glyph(self) -> char {
        match self {
            Self::Empty => ' ',
            Self::Upper => '\u{2580}',
            Self::Lower => '\u{2584}',
            Self::Full => '\u{2588}',
        }
    }
}

const QUIET_ZONE: i32 = 4;

pub fn generate_qr_braille<'a>(qr: &QrCode) -> Vec<Line<'a>> {
    let size = qr.size();
    let qr_style = Style::default().fg(theme::TEXT_BRIGHT);
    let full_width = (size + QUIET_ZONE * 2) as usize;
    let pad_rows = ((QUIET_ZONE + 1) / 2) as usize;
    let data_rows = (size as usize / 2) + 1;

    let mut lines: Vec<Line<'a>> = Vec::with_capacity(pad_rows * 2 + data_rows);
    let pad_row = row_string(std::iter::repeat_n(HalfBlock::Empty, full_width));

    for _ in 0..pad_rows {
        lines.push(Line::from(Span::styled(pad_row.clone(), qr_style)));
    }

    let get_module = |x: i32, y: i32| -> bool {
        x >= 0 && x < size && y >= 0 && y < size && qr.get_module(x, y)
    };

    let mut i = 0;
    while i <= size {
        let left = std::iter::repeat_n(HalfBlock::Empty, QUIET_ZONE as usize);
        let data =
            (0..=size).map(|j| HalfBlock::from_modules(get_module(j, i), get_module(j, i + 1)));
        let right = std::iter::repeat_n(HalfBlock::Empty, (QUIET_ZONE - 1) as usize);
        let row = row_string(left.chain(data).chain(right));
        lines.push(Line::from(Span::styled(row, qr_style)));
        i += 2;
    }

    for _ in 0..pad_rows {
        lines.push(Line::from(Span::styled(pad_row.clone(), qr_style)));
    }

    lines
}

fn row_string(blocks: impl Iterator<Item = HalfBlock>) -> String {
    let (lo, hi) = blocks.size_hint();
    let cap = hi.unwrap_or(lo) * 3;
    let mut s = String::with_capacity(cap);
    for b in blocks {
        s.push(b.glyph());
    }
    s
}

pub fn draw_qr_overlay(frame: &mut Frame, area: Rect, url: &str, title: &str, subtitle: &str) {
    use qrcodegen::QrCodeEcc;

    let Ok(qr) = QrCode::encode_text(url, QrCodeEcc::Low) else {
        return;
    };

    let dim = Style::default().fg(theme::TEXT_DIM);
    let green = Style::default().fg(theme::SUCCESS);

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(format!("  {subtitle}"), dim)),
        Line::from(Span::styled("  URL copied to clipboard", green)),
        Line::from(""),
    ];
    lines.extend(generate_qr_braille(&qr));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("  Press any key to close.", dim)));
    lines.push(Line::from(""));

    let content_w = lines.iter().map(|l| l.width() as u16).max().unwrap_or(0);
    let content_h = lines.len() as u16;
    let h = (content_h + 2).min(area.height.saturating_sub(4));
    let w = (content_w + 4).max(h * 2).min(area.width.saturating_sub(4));
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let popup_area = Rect::new(x, y, w, h);

    frame.render_widget(Clear, popup_area);
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);
    frame.render_widget(
        Paragraph::new(lines).centered().wrap(Wrap { trim: false }),
        inner,
    );
}
