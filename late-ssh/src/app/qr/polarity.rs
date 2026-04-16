use ratatui::style::{Color, Style};

use super::super::common::theme;

pub trait Polarity {
    fn style() -> Style;
}

/// Dark data modules on a light background (standard QR).
pub struct DarkOnLight;

impl Polarity for DarkOnLight {
    fn style() -> Style {
        Style::default().fg(theme::BG_HIGHLIGHT).bg(Color::White)
    }
}

/// Light data modules on a dark background (inverted).
pub struct LightOnDark;

impl Polarity for LightOnDark {
    fn style() -> Style {
        Style::default().fg(theme::TEXT_BRIGHT)
    }
}
