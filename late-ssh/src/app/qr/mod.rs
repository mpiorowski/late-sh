use std::marker::PhantomData;

use qrcodegen::QrCode;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

mod barcode;
mod polarity;

pub use barcode::{Barcode, Braille, FullBlock, HalfBlock};
pub use polarity::{DarkOnLight, LightOnDark, Polarity};

use super::common::theme;

// Here we define the default, which is HalfBlock with LoD
// but for testing we can just switch any of the zst defaults
// Maybe can expose this in future as a config option?
pub struct QrGenerator<B = Braille, P = DarkOnLight>(PhantomData<(B, P)>);
type DefaultQrGenerator = QrGenerator;

impl<B: Barcode, P: Polarity> QrGenerator<B, P> {
    pub fn generate_lines<'a>(qr: &QrCode) -> Vec<Line<'a>> {
        barcode::render::<B, P>(qr)
    }
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
    lines.extend(DefaultQrGenerator::generate_lines(&qr));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled("  Press any key to close.", dim)));
    lines.push(Line::from(""));

    let content_w = lines.iter().map(|l| l.width() as u16).max().unwrap_or(0);
    let content_h = lines.len() as u16;
    let h = (content_h + 2).min(area.height.saturating_sub(4));
    let w = (content_w + 4).max(h * 2).min(area.width.saturating_sub(4));

    let [popup_area] = Layout::vertical([Constraint::Length(h)])
        .flex(Flex::Center)
        .areas(area);
    let [popup_area] = Layout::horizontal([Constraint::Length(w)])
        .flex(Flex::Center)
        .areas(popup_area);

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
