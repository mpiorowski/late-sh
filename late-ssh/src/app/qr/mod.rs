use qrcodegen::QrCode;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

mod barcode;
mod polarity;
mod widget;

pub use barcode::{Barcode, Braille, FullBlock, HalfBlock};
pub use polarity::{DarkOnLight, LightOnDark, Polarity};
pub use widget::{AspectRatio, QrWidget, QuietZone, Scaling};

use super::common::theme;

pub fn draw_qr_overlay(frame: &mut Frame, area: Rect, url: &str, title: &str, subtitle: &str) {
    use qrcodegen::QrCodeEcc;

    let Ok(qr) = QrCode::encode_text(url, QrCodeEcc::Low) else {
        return;
    };

    let qr_widget = QrWidget::<HalfBlock, DarkOnLight>::new(&qr)
        .with_style(Style::default().fg(Color::Black).bg(Color::White));
    let qr_size = qr_widget.size(area);

    let header_h = 5u16;
    let footer_h = 3u16;
    let content_h = header_h + qr_size.height + footer_h;
    let content_w = qr_size.width.max(28);

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
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let [header_area, qr_area, footer_area] = Layout::vertical([
        Constraint::Length(header_h),
        Constraint::Length(qr_size.height),
        Constraint::Length(footer_h),
    ])
    .flex(Flex::Center)
    .areas(inner);

    let [qr_area] = Layout::horizontal([Constraint::Length(qr_size.width)])
        .flex(Flex::Center)
        .areas(qr_area);

    let dim = Style::default().fg(theme::TEXT_DIM());
    let amber = Style::default().fg(theme::AMBER());
    let green = Style::default().fg(theme::SUCCESS());

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(format!("  {subtitle}"), dim)),
            Line::from(Span::styled("  URL copied to clipboard", green)),
            Line::from(Span::styled(format!("  {url}"), amber)),
            Line::from(""),
        ])
        .centered(),
        header_area,
    );

    frame.render_widget(qr_widget, qr_area);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled("  Press any key to close.", dim)),
            Line::from(""),
        ])
        .centered(),
        footer_area,
    );
}
