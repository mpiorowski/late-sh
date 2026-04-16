use std::marker::PhantomData;

use qrcodegen::QrCode;
use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    style::{Style, Styled},
    text::Text,
    widgets::Widget,
};

use super::barcode::{Barcode, HalfBlock};
use super::polarity::{DarkOnLight, Polarity};

/// Quiet zone (border) around the QR code.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum QuietZone {
    #[default]
    Enabled,
    Disabled,
}

/// How the QR code should be scaled relative to the render area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scaling {
    /// Each QR module maps to exactly `(width, height)` sub-modules.
    Exact(u16, u16),
    /// Scale up to at most fill the render area.
    Max,
    /// Scale up to at least fill the render area.
    Min,
}

impl Default for Scaling {
    fn default() -> Self {
        Self::Exact(1, 1)
    }
}

/// Visual aspect ratio of the rendered QR code.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AspectRatio {
    /// Auto-pad to visually square.
    #[default]
    Square,
    /// Natural aspect from barcode encoding, no correction.
    Computed,
    /// Custom width:height ratio via horizontal padding.
    Custom(u16, u16),
}

/// A ratatui widget that renders a QR code.
///
/// Generic over barcode encoding (`B`) and color polarity (`P`).
///
/// ```ignore
/// let qr = QrCode::encode_text("https://example.com", QrCodeEcc::Low).unwrap();
/// let widget = QrWidget::new(&qr);
/// frame.render_widget(widget, area);
/// ```
#[derive(Clone, Copy)]
pub struct QrWidget<'a, B = HalfBlock, P = DarkOnLight> {
    qr: &'a QrCode,
    quiet_zone: QuietZone,
    scaling: Scaling,
    aspect_ratio: AspectRatio,
    style: Style,
    _phantom: PhantomData<(B, P)>,
}

impl<'a, B: Barcode, P: Polarity> QrWidget<'a, B, P> {
    pub fn new(qr: &'a QrCode) -> Self {
        Self {
            qr,
            quiet_zone: QuietZone::default(),
            scaling: Scaling::default(),
            aspect_ratio: AspectRatio::default(),
            style: P::style(),
            _phantom: PhantomData,
        }
    }

    #[must_use]
    pub const fn with_quiet_zone(mut self, qz: QuietZone) -> Self {
        self.quiet_zone = qz;
        self
    }

    #[must_use]
    pub const fn with_scaling(mut self, scaling: Scaling) -> Self {
        self.scaling = scaling;
        self
    }

    #[must_use]
    pub const fn with_aspect_ratio(mut self, aspect_ratio: AspectRatio) -> Self {
        self.aspect_ratio = aspect_ratio;
        self
    }

    #[must_use]
    pub fn with_style(mut self, style: impl Into<Style>) -> Self {
        self.style = style.into();
        self
    }

    /// Rendered size in terminal cells for a given area.
    ///
    /// For [`Scaling::Exact`] the result is independent of `area`.
    /// For [`Scaling::Max`] / [`Scaling::Min`] the result may exceed `area`.
    pub fn size(&self, area: Rect) -> Size {
        let qr_w = self.total_modules();
        let (sx, sy) = self.resolve_scaling(area, qr_w);
        let raw_w = ((qr_w * sx + B::MODULES_W - 1) / B::MODULES_W) as u16;
        let raw_h = ((qr_w * sy + B::MODULES_H - 1) / B::MODULES_H) as u16;
        let (h_pad, v_pad) = self.aspect_pad(raw_w, raw_h);
        Size::new(raw_w + h_pad * 2, raw_h + v_pad * 2)
    }

    /// Total QR modules across one axis (data + quiet zone on both sides).
    fn total_modules(&self) -> i32 {
        self.qr.size()
            + match self.quiet_zone {
                QuietZone::Enabled => 8,
                QuietZone::Disabled => 0,
            }
    }

    /// Extra (horizontal, vertical) padding per side to achieve target aspect ratio.
    fn aspect_pad(&self, chars_w: u16, rows_h: u16) -> (u16, u16) {
        let (target_w, target_h) = match self.aspect_ratio {
            AspectRatio::Square => (rows_h as u32 * 2, chars_w as u32 / 2),
            AspectRatio::Computed => return (0, 0),
            AspectRatio::Custom(w, h) => {
                if w == 0 || h == 0 {
                    return (0, 0);
                }
                let tw = (rows_h as u32 * w as u32) / h as u32;
                let th = (chars_w as u32 * h as u32) / w as u32;
                (tw, th)
            }
        };
        let cw = chars_w as u32;
        let ch = rows_h as u32;
        let h_pad = if target_w > cw {
            (target_w - cw).div_ceil(2)
        } else {
            0
        };
        let v_pad = if target_h > ch {
            (target_h - ch).div_ceil(2)
        } else {
            0
        };
        (h_pad as u16, v_pad as u16)
    }

    fn resolve_scaling(&self, area: Rect, qr_w: i32) -> (i32, i32) {
        match self.scaling {
            Scaling::Exact(x, y) => (x.max(1) as i32, y.max(1) as i32),
            Scaling::Max => {
                let sx = (area.width as i32 * B::MODULES_W) / qr_w;
                let sy = (area.height as i32 * B::MODULES_H) / qr_w;
                (sx.max(1), sy.max(1))
            }
            Scaling::Min => {
                let sx = (area.width as i32 * B::MODULES_W + qr_w - 1) / qr_w;
                let sy = (area.height as i32 * B::MODULES_H + qr_w - 1) / qr_w;
                (sx.max(1), sy.max(1))
            }
        }
    }

    fn build_text(&self, area: Rect) -> String {
        let qr = self.qr;
        let size = qr.size();
        let qz = match self.quiet_zone {
            QuietZone::Enabled => 4i32,
            QuietZone::Disabled => 0,
        };
        let qr_w = self.total_modules();
        let (sx, sy) = self.resolve_scaling(area, qr_w);

        let total_mx = qr_w * sx;
        let total_my = qr_w * sy;

        let glyphs_w = (total_mx + B::MODULES_W - 1) / B::MODULES_W;
        let raw_rows = (total_my + B::MODULES_H - 1) / B::MODULES_H;
        let (h_pad, v_pad) = self.aspect_pad(glyphs_w as u16, raw_rows as u16);
        let h_pad = h_pad as usize;
        let v_pad = v_pad as usize;
        let total_chars_w = glyphs_w as usize + h_pad * 2;
        let total_rows = raw_rows as usize + v_pad * 2;
        let off = B::glyph(0);

        let pad_row: String = std::iter::repeat_n(off, total_chars_w).collect();
        let mut out = String::with_capacity((total_chars_w * 3 + 1) * total_rows);

        for row in 0..total_rows {
            if row > 0 {
                out.push('\n');
            }

            if row < v_pad || row >= total_rows - v_pad {
                out.push_str(&pad_row);
                continue;
            }

            let gy = (row - v_pad) as i32;
            for _ in 0..h_pad {
                out.push(off);
            }
            for gx in 0..glyphs_w {
                let mut modules: u32 = 0;
                for dy in 0..B::MODULES_H {
                    for dx in 0..B::MODULES_W {
                        let smx = gx * B::MODULES_W + dx;
                        let smy = gy * B::MODULES_H + dy;
                        let orig_x = smx / sx - qz;
                        let orig_y = smy / sy - qz;
                        if orig_x >= 0
                            && orig_x < size
                            && orig_y >= 0
                            && orig_y < size
                            && qr.get_module(orig_x, orig_y)
                        {
                            modules |= 1 << (dy * B::MODULES_W + dx);
                        }
                    }
                }
                out.push(B::glyph(modules));
            }
            for _ in 0..h_pad {
                out.push(off);
            }
        }

        out
    }
}

impl<B: Barcode, P: Polarity> Widget for QrWidget<'_, B, P> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        (&self).render(area, buf);
    }
}

impl<B: Barcode, P: Polarity> Widget for &QrWidget<'_, B, P> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let text = self.build_text(area);
        Text::raw(text).style(self.style).render(area, buf);
    }
}

impl<B: Barcode, P: Polarity> Styled for QrWidget<'_, B, P> {
    type Item = Self;

    fn style(&self) -> Style {
        self.style
    }

    fn set_style<S: Into<Style>>(mut self, style: S) -> Self::Item {
        self.style = style.into();
        self
    }
}
