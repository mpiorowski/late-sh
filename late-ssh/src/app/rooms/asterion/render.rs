use std::collections::HashMap;

use image::{Pixel, Rgba, RgbaImage};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

pub fn img_to_lines(
    img: &RgbaImage,
    overrides: &HashMap<(u32, u32), char>,
    background: Rgba<u8>,
) -> Vec<Line<'static>> {
    let width = img.width();
    let height = img.height();
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(height.div_ceil(2) as usize);
    let mut y = 0u32;
    while y + 1 < height {
        let mut row: Vec<Span<'static>> = Vec::with_capacity(width as usize);
        for x in 0..width {
            row.push(half_block(img, x, y, overrides, background));
        }
        lines.push(Line::from(row));
        y += 2;
    }
    if height % 2 == 1 {
        let mut row: Vec<Span<'static>> = Vec::with_capacity(width as usize);
        for x in 0..width {
            let pixel = *img.get_pixel(x, height - 1);
            row.push(if pixel[3] == 0 {
                Span::raw(" ")
            } else {
                Span::styled("▀", Style::default().fg(rgba_to_color(pixel)))
            });
        }
        lines.push(Line::from(row));
    }
    lines
}

fn half_block(
    img: &RgbaImage,
    x: u32,
    y: u32,
    overrides: &HashMap<(u32, u32), char>,
    background: Rgba<u8>,
) -> Span<'static> {
    let top = *img.get_pixel(x, y);
    let btm = *img.get_pixel(x, y + 1);

    if let (Some(&ch), Some(_)) = (overrides.get(&(x, y)), overrides.get(&(x, y + 1)))
        && top[3] < 255
        && btm[3] < 255
    {
        let shade = top[3];
        return Span::styled(
            ch.to_string(),
            Style::default().fg(Color::Rgb(shade, shade, shade)),
        );
    }

    let top_transparent = is_transparent(top, background);
    let btm_transparent = is_transparent(btm, background);
    match (top_transparent, btm_transparent) {
        (true, true) => Span::raw(" "),
        (false, true) => Span::styled("▀", Style::default().fg(rgba_to_color(top))),
        (true, false) => Span::styled("▄", Style::default().fg(rgba_to_color(btm))),
        (false, false) => Span::styled(
            "▀",
            Style::default()
                .fg(rgba_to_color(top))
                .bg(rgba_to_color(btm)),
        ),
    }
}

fn rgba_to_color(pixel: Rgba<u8>) -> Color {
    let [r, g, b, a] = pixel.0;
    let alpha = a as f32 / 255.0;
    Color::Rgb(
        (r as f32 * alpha) as u8,
        (g as f32 * alpha) as u8,
        (b as f32 * alpha) as u8,
    )
}

fn is_transparent(pixel: Rgba<u8>, background: Rgba<u8>) -> bool {
    pixel[3] == 0 || pixel.to_rgb() == background.to_rgb()
}
