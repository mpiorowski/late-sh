use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use super::common::theme;

pub fn generate_qr_braille<'a>(url: &str) -> Vec<Line<'a>> {
    use qrcodegen::{QrCode, QrCodeEcc};

    let qr = match QrCode::encode_text(url, QrCodeEcc::Low) {
        Ok(qr) => qr,
        Err(_) => return vec![],
    };

    let size = qr.size();
    let qr_style = Style::default().fg(theme::TEXT_BRIGHT);
    let quiet_zone: i32 = 4;

    let ww = ' ';
    let bb = '\u{2588}'; // █
    let wb = '\u{2584}'; // ▄
    let bw = '\u{2580}'; // ▀

    let mut lines = Vec::new();
    let full_width = (size + quiet_zone * 2) as usize;
    let pad_row: String = std::iter::repeat_n(ww, full_width).collect();

    let get_module = |x: i32, y: i32| -> bool {
        x >= 0 && x < size && y >= 0 && y < size && qr.get_module(x, y)
    };

    for _ in 0..(quiet_zone + 1) / 2 {
        lines.push(Line::from(vec![Span::styled(pad_row.clone(), qr_style)]));
    }

    let mut i = 0;
    while i <= size {
        let mut row = String::with_capacity(full_width);
        for _ in 0..quiet_zone {
            row.push(ww);
        }
        for j in 0..=size {
            let curr = get_module(j, i);
            let next = get_module(j, i + 1);
            let c = match (curr, next) {
                (true, true) => bb,
                (true, false) => bw,
                (false, false) => ww,
                (false, true) => wb,
            };
            row.push(c);
        }
        for _ in 0..quiet_zone - 1 {
            row.push(ww);
        }
        lines.push(Line::from(vec![Span::styled(row, qr_style)]));
        i += 2;
    }

    for _ in 0..(quiet_zone + 1) / 2 {
        lines.push(Line::from(vec![Span::styled(pad_row.clone(), qr_style)]));
    }

    lines
}

pub fn draw_qr_overlay(frame: &mut Frame, area: Rect, url: &str, title: &str, subtitle: &str) {
    let dim = Style::default().fg(theme::TEXT_DIM);
    let green = Style::default().fg(theme::SUCCESS);

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(format!("  {subtitle}"), dim)),
        Line::from(Span::styled("  URL copied to clipboard", green)),
        Line::from(""),
    ];
    lines.extend(generate_qr_braille(url));
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
