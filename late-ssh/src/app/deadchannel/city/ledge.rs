//! Over the ledge: the lower city, all the way down. A perspective picture
//! at twice the terminal's vertical resolution (half-block cells, a color
//! per half), built fresh for the size it is shown at and the tick: a
//! haze on the horizon, towers in four depths from the far grey ones to
//! the near black ones, windows lit at random and flickering, the city's
//! script in neon on the nearest towers, antenna lights, the spinner's
//! beam crossing, rain in the sky. Pure in the size and the tick; nothing
//! is kept between frames.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::deadchannel::glyphs::GLYPH_ALPHABET;

use super::map::Neon;
use super::ui::{INK, INK_DIM, NIGHT, Rgb, ink, lit, mix, neon_rgb, rgb_color, scale};

/// Where the horizon sits, as a fraction of the height from the top.
const HORIZON: f32 = 0.28;
/// The haze the far city dissolves into, and the sky above it.
const HAZE: Rgb = [0.09, 0.10, 0.16];
const SKY: Rgb = [0.05, 0.05, 0.09];
/// Tower faces, near to far: the near ones are black against the glow.
const TOWER_NEAR: Rgb = [0.06, 0.06, 0.09];
const TOWER_FAR: Rgb = [0.16, 0.17, 0.24];
/// Window colors.
const WINDOWS: [Rgb; 3] = [[0.95, 0.70, 0.35], [0.45, 0.80, 0.95], [0.70, 0.70, 0.78]];
/// Rows between raindrops in one column of sky.
const RAIN_PERIOD: u64 = 10;

/// The depths, far to near. Each is drawn over the one before it.
const DEPTHS: [f32; 4] = [0.15, 0.35, 0.60, 1.0];

/// A tower in the picture, in pixel units (a pixel is half a cell tall).
struct Tower {
    x0: usize,
    x1: usize,
    top: usize,
    depth: f32,
    /// Which window color this tower's tenants use.
    tint: usize,
    /// A neon band of the city's script across the face, at this pixel
    /// row, for the near towers.
    neon: Option<(usize, Neon)>,
    /// A blinking antenna light on the roof.
    antenna: bool,
}

/// The towers for a picture this wide and this tall, the same every time
/// for the same size.
fn towers(w: usize, ph: usize) -> Vec<Tower> {
    let horizon = (ph as f32 * HORIZON) as usize;
    let mut out = Vec::new();
    let mut seed = 977u64;
    let mut next = || {
        seed = mix(seed + 1);
        seed
    };
    for (layer, &depth) in DEPTHS.iter().enumerate() {
        let count = (w / (3 + (depth * 14.0) as usize)).max(2);
        let width_min = 2 + (depth * 6.0) as usize;
        let width_max = 3 + (depth * 16.0) as usize;
        // The base of a tower sinks with depth: the near ones stand below
        // the frame, the far ones on the horizon.
        let base = horizon + ((ph - horizon) as f32 * depth * 1.15) as usize;
        let tallest = (ph as f32 * (0.18 + 0.55 * depth)) as usize;
        for i in 0..count {
            let r = next();
            let width = width_min + (r % (width_max - width_min + 1) as u64) as usize;
            let slot = w * i / count;
            let x0 = (slot + (r / 7 % (w / count).max(1) as u64) as usize).min(w.saturating_sub(1));
            let x1 = (x0 + width).min(w);
            let height = (tallest / 2 + (r / 13 % (tallest / 2).max(1) as u64) as usize).max(2);
            let top = base.saturating_sub(height);
            let neon = if layer >= 2 && r / 31 % 3 == 0 {
                let neon = match r / 97 % 5 {
                    0 => Neon::Cyan,
                    1 => Neon::Magenta,
                    2 => Neon::Amber,
                    3 => Neon::Green,
                    _ => Neon::Red,
                };
                Some((top + 2 + (r / 17 % 3) as usize, neon))
            } else {
                None
            };
            out.push(Tower {
                x0,
                x1,
                top,
                depth,
                tint: (r / 5 % 3) as usize,
                neon,
                antenna: r / 11 % 4 == 0,
            });
        }
    }
    out
}

fn lerp(a: Rgb, b: Rgb, k: f32) -> Rgb {
    [
        a[0] + (b[0] - a[0]) * k,
        a[1] + (b[1] - a[1]) * k,
        a[2] + (b[2] - a[2]) * k,
    ]
}

/// The picture as pixels: `w` wide, `ph` tall (twice the rows), a color
/// each. Towers are painted far to near so the near ones stand in front.
fn pixels(w: usize, ph: usize, t: u64) -> Vec<Rgb> {
    let horizon = (ph as f32 * HORIZON) as usize;
    let mut px = vec![SKY; w * ph];
    // The haze: brightest on the horizon, thinning up into the sky and
    // down into the dark.
    for y in 0..ph {
        let d = (y as f32 - horizon as f32).abs() / ph as f32;
        let k = (1.0 - d * 3.0).clamp(0.0, 1.0);
        let color = lerp(SKY, HAZE, k);
        for x in 0..w {
            px[y * w + x] = color;
        }
    }
    for tower in towers(w, ph) {
        let face = lerp(TOWER_FAR, TOWER_NEAR, tower.depth);
        let haze = 1.0 - tower.depth;
        let face = lerp(face, HAZE, haze * 0.6);
        let density = 0.10 + 0.14 * tower.depth;
        for y in tower.top..ph {
            for x in tower.x0..tower.x1 {
                let h = mix(u64::from(x as u32) * 131 + u64::from(y as u32) * 17 + 5);
                let window = (h % 100) as f32 / 100.0 < density
                    && x > tower.x0
                    && x + 1 < tower.x1
                    && y > tower.top;
                let color = if window {
                    let off = mix(u64::from(x as u32) * 7 + u64::from(y as u32) * 3 + t / 6)
                        .is_multiple_of(19);
                    let level = if off { 0.25 } else { 0.55 + 0.45 * tower.depth };
                    lerp(scale(WINDOWS[tower.tint], level), HAZE, haze * 0.5)
                } else {
                    face
                };
                px[y * w + x] = color;
            }
        }
        if tower.antenna && tower.top >= 2 {
            let x = (tower.x0 + tower.x1) / 2;
            let on = (t / 4 + u64::from(x as u32)).is_multiple_of(2);
            let level = if on { 1.0 } else { 0.2 };
            px[(tower.top - 1) * w + x] =
                scale(neon_rgb(Neon::Red), level * (0.4 + 0.6 * tower.depth));
            px[(tower.top - 2) * w + x] = lerp(face, HAZE, 0.3);
        }
    }
    // The spinner: a white point crossing the far city, its beam angled
    // down, slow.
    let span = w as u64 + 30;
    let sx = (t / 3 % span) as i64 - 15;
    let sy = horizon.saturating_sub(ph / 10);
    for k in 0..4i64 {
        let x = sx - k;
        let y = sy as i64 + k;
        if x >= 0 && (x as usize) < w && (y as usize) < ph {
            let level = if k == 0 { 1.0 } else { 0.35 / k as f32 };
            px[y as usize * w + x as usize] = lerp(
                px[y as usize * w + x as usize],
                neon_rgb(Neon::White),
                level,
            );
        }
    }
    px
}

/// The picture as cells: a half block where the two pixels differ, a
/// plain space with the one color where they agree, the city's script
/// over the near towers' neon bands, rain in the sky.
fn compose(w: usize, h: usize, t: u64) -> Vec<Line<'static>> {
    let ph = h * 2;
    let px = pixels(w, ph, t);
    let bands: Vec<(usize, usize, usize, Neon)> = towers(w, ph)
        .into_iter()
        .filter_map(|tower| {
            tower
                .neon
                .map(|(row, neon)| (tower.x0 + 1, tower.x1.saturating_sub(1), row / 2, neon))
        })
        .collect();
    let mut lines = Vec::with_capacity(h);
    for row in 0..h {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut run = String::new();
        let mut run_style: Option<Style> = None;
        for x in 0..w {
            let top = px[(row * 2) * w + x];
            let bottom = px[(row * 2 + 1) * w + x];
            let sky = top == SKY || row * 2 < (ph as f32 * HORIZON) as usize;
            let (ch, style) = if let Some(&(_, _, _, neon)) = bands
                .iter()
                .find(|&&(x0, x1, band_row, _)| row == band_row && x >= x0 && x < x1)
            {
                let h = mix(u64::from(x as u32) * 7 + t / BILLBOARD_TICKS * 31);
                let glyph = GLYPH_ALPHABET[(h % GLYPH_ALPHABET.len() as u64) as usize];
                let flicker = mix(u64::from(x as u32) * 3 + t / 2).is_multiple_of(11);
                let level = if flicker { 0.4 } else { 1.0 };
                (
                    glyph,
                    Style::default()
                        .fg(rgb_color(scale(neon_rgb(neon), level)))
                        .bg(rgb_color(bottom))
                        .add_modifier(Modifier::BOLD),
                )
            } else if sky
                && mix(u64::from(x as u32) * 7919).is_multiple_of(3)
                && (row as u64 + t / 2 + u64::from(x as u32)).is_multiple_of(RAIN_PERIOD)
            {
                (
                    '\'',
                    Style::default()
                        .fg(rgb_color(lerp(top, [0.6, 0.65, 0.8], 0.35)))
                        .bg(rgb_color(top)),
                )
            } else if top == bottom {
                (' ', Style::default().fg(rgb_color(top)).bg(rgb_color(top)))
            } else {
                (
                    '▀',
                    Style::default().fg(rgb_color(top)).bg(rgb_color(bottom)),
                )
            };
            match run_style {
                Some(current) if current == style => run.push(ch),
                Some(current) => {
                    spans.push(Span::styled(std::mem::take(&mut run), current));
                    run.push(ch);
                    run_style = Some(style);
                }
                None => {
                    run.push(ch);
                    run_style = Some(style);
                }
            }
        }
        if let Some(style) = run_style {
            spans.push(Span::styled(run, style));
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// A neon band shows one text for this many ticks.
const BILLBOARD_TICKS: u64 = 40;

/// The view: the picture over the whole area, a title top-left and the
/// way back bottom-right.
pub(super) fn draw(frame: &mut Frame, area: Rect, t: u64) {
    frame.render_widget(Block::default().style(ink(NIGHT)), area);
    let lines = compose(usize::from(area.width), usize::from(area.height), t);
    frame.render_widget(Paragraph::new(lines).style(ink(INK)), area);

    let pitch = "the lower city, all the way down";
    let title_rect = Rect {
        x: area.x + 1,
        y: area.y,
        width: (pitch.chars().count() as u16 + 4).min(area.width.saturating_sub(2)),
        height: 3.min(area.height),
    };
    frame.render_widget(Clear, title_rect);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(pitch, ink(INK_DIM))))
            .style(ink(INK))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(ink(INK_DIM))
                    .title(Span::styled(" the drop ", lit(Neon::White))),
            ),
        title_rect,
    );

    let back = vec![Line::from(vec![
        Span::styled("[Enter] ", lit(Neon::Amber)),
        Span::styled("step back", ink(INK)),
    ])];
    let width = 21u16.min(area.width.saturating_sub(2));
    let height = 3u16.min(area.height.saturating_sub(1));
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width + 1),
        y: area.y + area.height.saturating_sub(height),
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(back).style(ink(INK)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(ink(INK_DIM))
                .title(Span::styled(" the ledge ", lit(Neon::White))),
        ),
        rect,
    );
}

#[cfg(test)]
#[path = "ledge_test.rs"]
mod ledge_test;
