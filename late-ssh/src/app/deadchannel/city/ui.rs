//! City renderer: the street viewport (camera-follow over the map, the
//! runner as its one-cell mark), lit.
//!
//! Every frame builds a `Scene`: a light map (every light on the street,
//! `map::LIGHTS`, spread across open floor with falloff and stopped by
//! walls, plus the spinner's searchlight passing over) and a visibility
//! map (the street fades to black with distance from the runner, and a
//! room stays dark until you are at its door). Each map cell is then a
//! `Surface`, either lit (its color times ambient plus the light reaching
//! it, times visibility, halved in the shadow under a wall) or emissive
//! (a neon letter, a lamp, a window: it burns on its own and only fades
//! with distance). The ambience is painted on top: rain where there is
//! light to see it by, puddles catching what shines on them, signs that
//! short out (and their light with them), steam, the screen's static.
//! Walkers pace the street under the same light. Three overlays: a
//! popover for the landmark within reach, a pinned line when the street
//! talks back, and a shop panel.
//!
//! The city has its own palette (fixed RGB, `neon_rgb` and the surface
//! constants) and does not follow the theme: one look, tuned once. The
//! overlays are chrome and keep the theme.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::common::theme;
use crate::app::deadchannel::glyphs::GLYPH_ALPHABET;
use crate::app::deadchannel::runner::state::Tint;
use crate::app::deadchannel::runner::state::{Look, Slot, pieces_for};
use crate::app::deadchannel::runner::ui::{portrait_spans, tint_color};

use super::data;
use super::map::{self, Landmark, LightKind, Neon};
use super::state::{Enter, State};

/// Widest a floor label gets.
const LABEL_MAX: usize = 10;
/// The camera looks up the street: it centers this many rows north of
/// the runner, so a short terminal shows the shopfronts, not the drop.
const LOOK_NORTH: u16 = 8;
/// Rows between raindrops in one column.
const RAIN_PERIOD: u64 = 12;
/// The ambience runs at half the animation edge, so the street breathes
/// instead of flickering. Walkers keep the full edge; their `period` is
/// the pacing.
const SLOW: u64 = 2;
/// The screen's cycle: static, then the test pattern for a moment.
const SCREEN_CYCLE: u64 = 120;
const SCREEN_PATTERN_TICKS: u64 = 8;
/// The static's shades, weighted toward the dark.
const STATIC_CHARS: [char; 6] = ['░', '▒', '▓', ' ', '░', '░'];
/// Steam rising off a vent.
const STEAM_CHARS: [char; 4] = ['~', '≈', '∙', '·'];

// Lighting. The night is dark: an unlit surface shows at `AMBIENT` of its
// color, a lit one adds the light that reaches it.
const AMBIENT: f32 = 0.16;
/// Columns from the runner within which everything shows in full.
const SEE_FULL: f32 = 22.0;
/// Columns from the runner beyond which the street is at its darkest.
const SEE_END: f32 = 72.0;
/// How much of a lit surface survives at the far end.
const SEE_FLOOR: f32 = 0.18;
/// How much of an emissive surface survives at the far end: neon carries.
const EMISSIVE_FLOOR: f32 = 0.45;
/// The shadow under a wall.
const SHADOW: f32 = 0.45;
/// A room you are not at the door of.
const INSIDE_DARK: f32 = 0.3;
/// How close to a building counts as at its door.
const REVEAL_REACH: u16 = 3;
/// Color channels are rounded to this many steps so neighbouring cells
/// under the same light share a style (the SSH frame is runs of spans).
const LEVELS: f32 = 24.0;

pub(crate) struct CityView<'a> {
    pub state: &'a State,
    pub own_username: &'a str,
    /// The runner's look, for the mark and the tailor's mirror. `None` for
    /// a session without a runner row: the mark falls back to `@`.
    pub look: Option<&'a Look>,
}

type Cells = Vec<Vec<(char, Style)>>;
type Rgb = [f32; 3];

pub(crate) fn draw(frame: &mut Frame, area: Rect, view: CityView<'_>) {
    if area.width < 4 || area.height < 4 {
        return;
    }
    let t = view.state.anim_tick;
    let scene = Scene::build(t, view.state.player_x, view.state.player_y);
    let mut cells = compose_grid(&scene);
    animate(&mut cells, t, &scene);
    draw_runner(&mut cells, &view);

    let vw = usize::from(area.width);
    let vh = usize::from(area.height);
    let map_w = usize::from(map::MAP_W);
    let map_h = usize::from(map::MAP_H);
    let cam_x = camera_origin(usize::from(view.state.player_x), vw, map_w);
    let cam_y = camera_origin(
        usize::from(view.state.player_y.saturating_sub(LOOK_NORTH)),
        vh,
        map_h,
    );
    let pad_x = vw.saturating_sub(map_w) / 2;
    let pad_y = vh.saturating_sub(map_h) / 2;

    let mut lines: Vec<Line> = Vec::with_capacity(vh);
    for _ in 0..pad_y {
        lines.push(Line::default());
    }
    for row in cells.iter().skip(cam_y).take(vh.saturating_sub(pad_y)) {
        let mut spans: Vec<Span> = Vec::new();
        if pad_x > 0 {
            spans.push(Span::raw(" ".repeat(pad_x)));
        }
        // Same-style runs become one span each; the street is long runs.
        let mut run = String::new();
        let mut run_style: Option<Style> = None;
        for &(ch, style) in row.iter().skip(cam_x).take(vw.saturating_sub(pad_x)) {
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
    frame.render_widget(Paragraph::new(lines), area);

    draw_street_line(frame, area, &view);
    draw_popover(frame, area, &view);
    draw_panel(frame, area, &view);
}

fn camera_origin(player: usize, viewport: usize, map_len: usize) -> usize {
    if viewport >= map_len {
        return 0;
    }
    player.saturating_sub(viewport / 2).min(map_len - viewport)
}

// ------------------------------------------------------------- palette

/// The neon colors, fixed: the city's own palette.
fn neon_rgb(neon: Neon) -> Rgb {
    match neon {
        Neon::Cyan => [0.15, 0.85, 1.0],
        Neon::Magenta => [1.0, 0.30, 0.80],
        Neon::Red => [1.0, 0.25, 0.22],
        Neon::Amber => [1.0, 0.66, 0.18],
        Neon::Green => [0.35, 1.0, 0.50],
        Neon::White => [0.90, 0.92, 1.0],
    }
}

const WALL: Rgb = [0.55, 0.52, 0.50];
const FLOOR: Rgb = [0.62, 0.62, 0.68];
const PUDDLE: Rgb = [0.75, 0.80, 0.95];
const WATER: Rgb = [0.35, 0.50, 0.65];
const CRATE: Rgb = [0.52, 0.44, 0.36];
const PERSON: Rgb = [0.72, 0.70, 0.68];
const CAT: Rgb = [0.94, 0.94, 0.92];
const RAT: Rgb = [0.48, 0.45, 0.42];
const PLANT: Rgb = [0.38, 0.68, 0.38];
const RAIN: Rgb = [0.70, 0.76, 0.92];
const STEAM: Rgb = [0.80, 0.80, 0.84];
const FRAME: Rgb = [0.30, 0.27, 0.25];

fn rgb_color(c: Rgb) -> Color {
    let q = |v: f32| ((v.clamp(0.0, 1.0) * LEVELS).round() / LEVELS * 255.0) as u8;
    Color::Rgb(q(c[0]), q(c[1]), q(c[2]))
}

fn scale(c: Rgb, k: f32) -> Rgb {
    [c[0] * k, c[1] * k, c[2] * k]
}

fn luma(c: Rgb) -> f32 {
    0.3 * c[0] + 0.55 * c[1] + 0.15 * c[2]
}

/// A neon at full burn, for the overlays and the runner's mark.
fn lit(neon: Neon) -> Style {
    Style::default()
        .fg(rgb_color(neon_rgb(neon)))
        .add_modifier(Modifier::BOLD)
}

fn glow(neon: Neon) -> Style {
    Style::default().fg(rgb_color(neon_rgb(neon)))
}

fn dim(neon: Neon) -> Style {
    Style::default().fg(rgb_color(scale(neon_rgb(neon), 0.55)))
}

/// The neon a landmark burns in: its sign's color, or the street's grey
/// for the things that have no sign.
fn landmark_neon(landmark: Landmark) -> Neon {
    match landmark {
        Landmark::Armorer => Neon::Red,
        Landmark::Tailor => Neon::Magenta,
        Landmark::Lockers => Neon::Cyan,
        Landmark::Bands => Neon::Green,
        Landmark::Bar => Neon::Amber,
        Landmark::Screen => Neon::White,
        Landmark::Repairs => Neon::White,
        Landmark::Board => Neon::White,
        Landmark::Bits => Neon::Green,
        Landmark::Noodles => Neon::Amber,
        Landmark::Umbrellas => Neon::Cyan,
        Landmark::Blades => Neon::Red,
        Landmark::Reader => Neon::Magenta,
        Landmark::Stairs => Neon::Red,
        Landmark::Wire => Neon::Amber,
    }
}

const NEON_CYCLE: [Neon; 5] = [
    Neon::Cyan,
    Neon::Magenta,
    Neon::Amber,
    Neon::Green,
    Neon::Red,
];

fn hashed_neon(x: u16, y: u16) -> Neon {
    let h = mix(u64::from(x) * 31 + u64::from(y) * 131);
    NEON_CYCLE[(h % NEON_CYCLE.len() as u64) as usize]
}

fn sign_at(x: u16, y: u16) -> Option<Neon> {
    map::SIGNS
        .iter()
        .chain(map::BANNERS.iter())
        .chain(map::CART_SIGNS.iter())
        .find(|sign| sign.zone.contains(x, y))
        .map(|sign| sign.color)
}

fn building_at(x: u16, y: u16) -> Option<&'static map::Building> {
    map::BUILDINGS.iter().find(|b| b.zone.contains(x, y))
}

// --------------------------------------------------------------- scene

/// What this frame's light and sight look like, cell by cell.
struct Scene {
    light: Vec<Rgb>,
    vis: Vec<f32>,
}

impl Scene {
    fn build(t: u64, player_x: u16, player_y: u16) -> Scene {
        Scene {
            light: light_map(t / SLOW),
            vis: visibility_map(player_x, player_y),
        }
    }

    fn light(&self, x: u16, y: u16) -> Rgb {
        self.light[index(x, y)]
    }

    fn vis(&self, x: u16, y: u16) -> f32 {
        self.vis[index(x, y)]
    }
}

fn index(x: u16, y: u16) -> usize {
    usize::from(y) * usize::from(map::MAP_W) + usize::from(x)
}

/// Every light spread over the floor it reaches. A column is one unit of
/// distance, a row two (the cells are twice as tall as wide); light
/// crosses open floor and doorways, lands on walls, and stops there.
fn light_map(t: u64) -> Vec<Rgb> {
    let mut out = vec![[0.0f32; 3]; usize::from(map::MAP_W) * usize::from(map::MAP_H)];
    for (i, light) in map::LIGHTS.iter().enumerate() {
        let level = light_level(i, light, t);
        if level <= 0.0 {
            continue;
        }
        spread(
            &mut out,
            light.x,
            light.y,
            light.radius,
            scale(neon_rgb(light.color), level),
        );
    }
    // The spinner overhead: its searchlight sweeps the length of the
    // street, a pool of white on the wet ground.
    let span = i64::from(map::MAP_W) + 80;
    let sx = (t / 2 % span as u64) as i64 - 40;
    let sy = i64::from(map::STREET.y0 + map::STREET.y1) / 2;
    if sx >= 1 && sx < i64::from(map::MAP_W) - 1 {
        spread(
            &mut out,
            sx as u16,
            sy as u16,
            9,
            scale(neon_rgb(Neon::White), 1.4),
        );
    }
    out
}

/// One light, flooded out to its radius: Dial's buckets by cost, a
/// column costing one and a row two.
fn spread(out: &mut [Rgb], x0: u16, y0: u16, radius: u16, color: Rgb) {
    let radius = usize::from(radius);
    let mut buckets: Vec<Vec<(u16, u16)>> = vec![Vec::new(); radius + 1];
    let mut best: std::collections::HashMap<(u16, u16), usize> = std::collections::HashMap::new();
    buckets[0].push((x0, y0));
    best.insert((x0, y0), 0);
    for cost in 0..=radius {
        let cells = std::mem::take(&mut buckets[cost]);
        for (x, y) in cells {
            if best.get(&(x, y)) != Some(&cost) {
                continue;
            }
            let fall = 1.0 - cost as f32 / (radius as f32 + 1.0);
            let k = fall * fall;
            let cell = &mut out[index(x, y)];
            cell[0] += color[0] * k;
            cell[1] += color[1] * k;
            cell[2] += color[2] * k;
            // Light leaves the source whatever it is set in, then only
            // crosses open ground.
            if (x, y) != (x0, y0) && !map::walkable(x, y) {
                continue;
            }
            for (dx, dy, step) in [(1i32, 0i32, 1usize), (-1, 0, 1), (0, 1, 2), (0, -1, 2)] {
                let nx = x.saturating_add_signed(dx as i16);
                let ny = y.saturating_add_signed(dy as i16);
                if nx == 0 || ny == 0 || nx >= map::MAP_W - 1 || ny >= map::MAP_H - 1 {
                    continue;
                }
                let next = cost + step;
                if next > radius {
                    continue;
                }
                let entry = best.entry((nx, ny)).or_insert(usize::MAX);
                if next < *entry {
                    *entry = next;
                    buckets[next].push((nx, ny));
                }
            }
        }
    }
}

/// How bright a light burns this tick: lamps flicker, signs short out,
/// windows go dark for a while, the screen pulses with its static.
fn light_level(i: usize, light: &map::Light, t: u64) -> f32 {
    let h = mix(i as u64 * 977 + t / 8);
    match light.kind {
        LightKind::Lamp => {
            if h % 11 == 0 {
                0.35
            } else {
                1.0
            }
        }
        LightKind::Lantern => {
            if mix(i as u64 * 31 + t / 3) % 4 == 0 {
                0.7
            } else {
                1.0
            }
        }
        LightKind::Machine => {
            if (t / 4 + i as u64) % 2 == 0 {
                1.0
            } else {
                0.7
            }
        }
        LightKind::Candle => 0.5 + (mix(i as u64 * 7 + t / 2) % 3) as f32 * 0.15,
        LightKind::Stairs => {
            if (t / 5) % 2 == 0 {
                1.0
            } else {
                0.6
            }
        }
        LightKind::Sign => match sign_state(light.x, light.y, t) {
            SignState::Burning => 1.0,
            SignState::Dropped(_) => 0.75,
            SignState::Short => 0.1,
        },
        LightKind::Door => 0.7,
        LightKind::Window => {
            if window_dark(light.x, light.y, t) {
                0.0
            } else {
                0.45
            }
        }
        LightKind::Screen => {
            let phase = t % SCREEN_CYCLE;
            if phase < SCREEN_PATTERN_TICKS {
                1.3
            } else {
                0.7 + (mix(t) % 5) as f32 * 0.08
            }
        }
    }
}

/// How far the runner sees: everything within `SEE_FULL` columns, then a
/// fade to `SEE_FLOOR` by `SEE_END`. Rooms show only from their door.
fn visibility_map(player_x: u16, player_y: u16) -> Vec<f32> {
    let revealed: Vec<bool> = map::BUILDINGS
        .iter()
        .map(|b| b.zone.distance(player_x, player_y) <= REVEAL_REACH)
        .collect();
    let mut out = vec![1.0f32; usize::from(map::MAP_W) * usize::from(map::MAP_H)];
    for y in 0..map::MAP_H {
        for x in 0..map::MAP_W {
            let dx = f32::from(x.abs_diff(player_x));
            let dy = f32::from(y.abs_diff(player_y)) * 2.0;
            let d = (dx * dx + dy * dy).sqrt();
            let mut v = if d <= SEE_FULL {
                1.0
            } else {
                let k = ((d - SEE_FULL) / (SEE_END - SEE_FULL)).min(1.0);
                1.0 - k * (1.0 - SEE_FLOOR)
            };
            if let Some(i) = map::BUILDINGS.iter().position(|b| {
                b.zone.contains(x, y)
                    && x > b.zone.x0
                    && x < b.zone.x1
                    && y > b.zone.y0
                    && y < b.zone.y1
            }) && !revealed[i]
            {
                v *= INSIDE_DARK;
            }
            out[index(x, y)] = v;
        }
    }
    out
}

// ------------------------------------------------------------ surfaces

/// What a map cell is made of, before light.
#[derive(Clone, Copy)]
struct Surface {
    color: Rgb,
    /// Burns on its own: neon, lamps, windows. Light adds nothing, only
    /// distance takes away.
    emissive: bool,
    /// How much of the light reaching it a lit surface shows: wet things
    /// more than one.
    reflect: f32,
}

fn lit_surface(color: Rgb) -> Surface {
    Surface {
        color,
        emissive: false,
        reflect: 1.0,
    }
}

fn wet(color: Rgb, reflect: f32) -> Surface {
    Surface {
        color,
        emissive: false,
        reflect,
    }
}

fn emissive(color: Rgb) -> Surface {
    Surface {
        color,
        emissive: true,
        reflect: 0.0,
    }
}

/// The surface of the map char at `(x, y)`.
fn surface(ch: char, x: u16, y: u16) -> Surface {
    // The frame and the two signs let into it.
    if y == 0 || y == map::MAP_H - 1 || x == 0 || x == map::MAP_W - 1 {
        if map::TITLE.contains(x, y) {
            return match ch {
                '▚' | '▞' => emissive(neon_rgb(Neon::Amber)),
                '╡' | '╞' => emissive(FRAME),
                _ => emissive(neon_rgb(Neon::White)),
            };
        }
        if y == map::MAP_H - 1 && !matches!(ch, '═' | '╚' | '╝' | '╡' | '╞') {
            return emissive(scale(neon_rgb(Neon::White), 0.5));
        }
        return emissive(FRAME);
    }
    if (x, y) == map::DEAD_LETTER {
        return lit_surface(scale(WALL, 0.6));
    }
    if let Some(neon) = sign_at(x, y) {
        return emissive(neon_rgb(neon));
    }
    if map::SCREEN_FACE.contains(x, y) {
        return emissive(scale(neon_rgb(Neon::White), 0.4));
    }
    if (x, y) == map::STAIRS {
        return emissive(neon_rgb(Neon::Red));
    }
    if map::WIRE.contains(x, y) && ch == '>' {
        return emissive(neon_rgb(Neon::Amber));
    }
    if map::DROP.contains(x, y) {
        return match ch {
            '▪' => emissive(scale(neon_rgb(hashed_neon(x, y)), 0.6)),
            '·' | '∙' => emissive(scale(neon_rgb(Neon::White), 0.35)),
            _ => lit_surface(scale(WALL, 0.3)),
        };
    }
    if let Some(building) = building_at(x, y) {
        let neon = building.color.map(neon_rgb);
        return match ch {
            '#' => lit_surface(WALL),
            '+' => match neon {
                Some(c) => emissive(scale(c, 0.8)),
                None => lit_surface(scale(WALL, 0.8)),
            },
            '╬' => emissive(window_color(x, y)),
            '=' | '∩' => lit_surface(scale(WALL, 0.9)),
            '@' | '&' => lit_surface(PERSON),
            ')' | '[' | '"' | '!' | '/' | '\\' | 'x' | 'Y' | 'o' | '▌' => match neon {
                Some(c) => lit_surface([0.5 + c[0] * 0.5, 0.5 + c[1] * 0.5, 0.5 + c[2] * 0.5]),
                None => lit_surface(WALL),
            },
            '$' => emissive(neon_rgb(Neon::Green)),
            '♪' => emissive(neon_rgb(Neon::Amber)),
            '_' => emissive(scale(neon_rgb(Neon::White), 0.8)),
            '°' => emissive(scale(neon_rgb(Neon::Amber), 0.8)),
            '≈' => wet(WATER, 1.6),
            '▬' | '▪' => lit_surface(CRATE),
            '▓' => lit_surface(scale(WALL, 0.7)),
            '♣' => lit_surface(PLANT),
            '≡' => lit_surface(scale(WALL, 0.8)),
            _ => lit_surface(FLOOR),
        };
    }
    // The street, the alleys, the court, the canal, the yard.
    match ch {
        '≈' | '~' if map::PUDDLES.contains(&(x, y)) => wet(PUDDLE, 1.6),
        '≈' | '~' => wet(WATER, 1.5),
        '≡' | '▒' | '=' | '═' | '╪' | '#' | '░' | '▓' => lit_surface(WALL),
        '▪' => lit_surface(CRATE),
        '%' => lit_surface([1.0, 0.85, 0.6]),
        'T' => lit_surface([0.5, 0.9, 1.0]),
        ')' => lit_surface([1.0, 0.6, 0.6]),
        '*' => emissive(neon_rgb(Neon::Amber)),
        '°' => emissive(scale(neon_rgb(Neon::Amber), 0.8)),
        '$' => emissive(neon_rgb(Neon::Green)),
        '?' => emissive(neon_rgb(Neon::White)),
        '_' => emissive(scale(neon_rgb(Neon::White), 0.8)),
        '♣' => lit_surface(PLANT),
        'c' => lit_surface(CAT),
        'r' => lit_surface(RAT),
        '@' => lit_surface(PERSON),
        _ => lit_surface(FLOOR),
    }
}

/// A lit window: amber, a cold blue, or a grey glow, per pane.
fn window_color(x: u16, y: u16) -> Rgb {
    match mix(u64::from(x) * 53 + u64::from(y) * 97) % 3 {
        0 => [0.85, 0.60, 0.25],
        1 => [0.35, 0.65, 0.85],
        _ => [0.55, 0.55, 0.60],
    }
}

/// A window goes dark for a while now and then.
fn window_dark(x: u16, y: u16, t: u64) -> bool {
    mix(u64::from(x) * 53 + u64::from(y) * 97 + t / 12) % 13 == 0
}

enum SignState {
    Burning,
    /// One letter dark, at this column.
    Dropped(u16),
    /// The whole sign out for a frame.
    Short,
}

/// Neon shorts out for a frame now and then, or drops one letter. Keyed
/// on the sign's light cell so the letters and the light agree.
fn sign_state(light_x: u16, light_y: u16, t: u64) -> SignState {
    let h = mix(u64::from(light_x) * 131 + u64::from(light_y) * 17 + t / 3);
    if h % 23 == 0 {
        SignState::Short
    } else if h % 11 == 0 {
        SignState::Dropped((h / 11 % 9) as u16)
    } else {
        SignState::Burning
    }
}

/// A wall glyph throws a shadow on the floor south of it.
fn casts_shadow(x: u16, y: u16) -> bool {
    if y == 0 {
        return false;
    }
    let above = map::char_at(x, y - 1);
    matches!(above, '#' | '╬' | '▓' | '+') || sign_at(x, y - 1).is_some()
}

/// The color a surface shows under this frame's light, at this distance.
fn shade(surface: Surface, light: Rgb, vis: f32, shadow: bool) -> Rgb {
    if surface.emissive {
        let k = EMISSIVE_FLOOR + (1.0 - EMISSIVE_FLOOR) * vis;
        return scale(surface.color, k);
    }
    let k = if shadow { SHADOW } else { 1.0 };
    let c = surface.color;
    [
        c[0] * (AMBIENT + light[0] * surface.reflect) * vis * k,
        c[1] * (AMBIENT + light[1] * surface.reflect) * vis * k,
        c[2] * (AMBIENT + light[2] * surface.reflect) * vis * k,
    ]
}

fn styled(surface: Surface, scene: &Scene, x: u16, y: u16, bold: bool) -> Style {
    let shadow = !surface.emissive && casts_shadow(x, y);
    let color = shade(surface, scene.light(x, y), scene.vis(x, y), shadow);
    let style = Style::default().fg(rgb_color(color));
    if bold {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

/// Every map cell lit: the base frame before the ambience.
fn compose_grid(scene: &Scene) -> Cells {
    map::grid()
        .iter()
        .enumerate()
        .map(|(y, row)| {
            let y = y as u16;
            row.iter()
                .enumerate()
                .map(|(x, &ch)| {
                    let x = x as u16;
                    let surface = surface(ch, x, y);
                    let glyph = roof_edge(ch, x, y);
                    let bold = surface.emissive && ch.is_alphabetic();
                    (glyph, styled(surface, scene, x, y, bold))
                })
                .collect()
        })
        .collect()
}

/// The top wall of a building against the dark behind it reads as a roof
/// edge, so the block stands up off the street.
fn roof_edge(ch: char, x: u16, y: u16) -> char {
    if ch == '#' && y > 0 && map::char_at(x, y - 1) == ' ' && !map::walkable(x, y - 1) {
        '▀'
    } else {
        ch
    }
}

// ------------------------------------------------------------ animation

fn animate(cells: &mut Cells, t: u64, scene: &Scene) {
    let slow = t / SLOW;
    rain(cells, slow, scene);
    puddles(cells, slow, scene);
    signs(cells, slow, scene);
    windows(cells, slow, scene);
    screen(cells, slow, scene);
    steam(cells, slow, scene);
    drop_lights(cells, slow, scene);
    wire_pulse(cells, slow, scene);
    walkers(cells, t, scene);
}

/// The street's people, cats and rats pace their stretch of floor, back
/// and forth, one step every `period` ticks. Pure in the tick: no state.
fn walkers(cells: &mut Cells, t: u64, scene: &Scene) {
    for walker in map::WALKERS.iter() {
        let len = u64::from(walker.x1 - walker.x0);
        if len == 0 {
            continue;
        }
        let k = (t / walker.period + walker.phase) % (2 * len);
        let offset = if k <= len { k } else { 2 * len - k };
        let x = walker.x0 + offset as u16;
        let color = match walker.glyph {
            'c' => CAT,
            'r' => RAT,
            _ => PERSON,
        };
        if map::walkable(x, walker.y) {
            let style = styled(lit_surface(color), scene, x, walker.y, false);
            set(cells, x, walker.y, walker.glyph, style);
        }
    }
}

/// Rain on every open cell there is light to see it by: one drop per
/// column every `RAIN_PERIOD` rows, falling one row a tick, in the color
/// of the light it falls through.
fn rain(cells: &mut Cells, t: u64, scene: &Scene) {
    for y in 1..map::MAP_H - 1 {
        for x in 1..map::MAP_W - 1 {
            if !is_floor(map::char_at(x, y)) {
                continue;
            }
            if !map::walkable(x, y) && !map::DROP.contains(x, y) {
                continue;
            }
            let light = scene.light(x, y);
            if luma(light) < 0.10 {
                continue;
            }
            let column = mix(u64::from(x) * 7919);
            if column % 2 == 0 {
                continue;
            }
            if (u64::from(y) + t / 2 + column) % RAIN_PERIOD != 0 {
                continue;
            }
            let ch = if column % 5 == 0 { '|' } else { '\'' };
            let style = styled(wet(RAIN, 1.3), scene, x, y, false);
            set(cells, x, y, ch, style);
        }
    }
}

/// Puddles and the canal shimmer: the light on them comes and goes.
fn puddles(cells: &mut Cells, t: u64, scene: &Scene) {
    for &(x, y) in map::PUDDLES.iter() {
        let h = mix(u64::from(x) * 3 + u64::from(y) * 7 + t / 6);
        let ch = if h % 2 == 0 { '≈' } else { '~' };
        let reflect = if h % 7 == 0 { 2.4 } else { 1.4 };
        let style = styled(wet(PUDDLE, reflect), scene, x, y, false);
        set(cells, x, y, ch, style);
    }
}

/// Neon shorts out for a frame now and then, or drops one letter; the
/// light it throws follows (`light_level`).
fn signs(cells: &mut Cells, t: u64, scene: &Scene) {
    for sign in map::SIGNS.iter().chain(map::BANNERS.iter()) {
        let z = sign.zone;
        let (lx, ly) = ((z.x0 + z.x1) / 2, z.y0);
        let dark = lit_surface(scale(WALL, 0.6));
        match sign_state(lx, ly, t) {
            SignState::Burning => {}
            SignState::Short => {
                for x in z.x0..=z.x1 {
                    let ch = map::char_at(x, z.y0);
                    set(cells, x, z.y0, ch, styled(dark, scene, x, z.y0, false));
                }
            }
            SignState::Dropped(k) => {
                let x = z.x0 + k % (z.x1 - z.x0 + 1);
                let ch = map::char_at(x, z.y0);
                set(cells, x, z.y0, ch, styled(dark, scene, x, z.y0, false));
            }
        }
    }
    // The dead letter stays dead whatever the sign does.
    let (dx, dy) = map::DEAD_LETTER;
    set(
        cells,
        dx,
        dy,
        map::char_at(dx, dy),
        styled(lit_surface(scale(WALL, 0.6)), scene, dx, dy, false),
    );
}

/// A lit window goes dark for a while now and then.
fn windows(cells: &mut Cells, t: u64, scene: &Scene) {
    for light in map::LIGHTS.iter() {
        if light.kind != LightKind::Window || !window_dark(light.x, light.y, t) {
            continue;
        }
        let style = styled(
            lit_surface(scale(WALL, 0.7)),
            scene,
            light.x,
            light.y,
            false,
        );
        set(cells, light.x, light.y, '╬', style);
    }
}

/// The screen at the end of the street: static, torn now and then, and
/// for a moment every cycle the test pattern. Rarely a glyph surfaces in
/// the noise (the fauna's alphabet, foreshadowing).
fn screen(cells: &mut Cells, t: u64, scene: &Scene) {
    let face = map::SCREEN_FACE;
    let phase = t % SCREEN_CYCLE;
    let pattern = phase < SCREEN_PATTERN_TICKS;
    let glyph_frame = (t / SCREEN_CYCLE) % 5 == 3
        && (SCREEN_PATTERN_TICKS..SCREEN_PATTERN_TICKS + 3).contains(&phase);
    let width = u64::from(face.x1 - face.x0 + 1);
    let bars: [Neon; 6] = [
        Neon::White,
        Neon::Amber,
        Neon::Cyan,
        Neon::Green,
        Neon::Magenta,
        Neon::Red,
    ];
    for y in face.y0..=face.y1 {
        let torn = mix(u64::from(y) * 17 + t / 2) % 17 == 0;
        for x in face.x0..=face.x1 {
            let h = mix(u64::from(x) * 97 + u64::from(y) * 53 + t);
            let vis = scene.vis(x, y);
            if pattern {
                let bar = (u64::from(x - face.x0) * 6 / width) as usize;
                let color = shade(emissive(neon_rgb(bars[bar])), [0.0; 3], vis, false);
                set(cells, x, y, '█', Style::default().fg(rgb_color(color)));
                continue;
            }
            let ch = if torn {
                if h % 2 == 0 { '▀' } else { '▄' }
            } else if glyph_frame && h % 9 == 0 {
                GLYPH_ALPHABET[(h / 9 % GLYPH_ALPHABET.len() as u64) as usize]
            } else {
                STATIC_CHARS[(h % STATIC_CHARS.len() as u64) as usize]
            };
            let level = match h / 7 % 4 {
                0 => 0.75,
                1 => 0.55,
                _ => 0.35,
            };
            let level = if glyph_frame && GLYPH_ALPHABET.contains(&ch) {
                1.0
            } else {
                level
            };
            let color = shade(
                emissive(scale(neon_rgb(Neon::White), level)),
                [0.0; 3],
                vis,
                false,
            );
            set(cells, x, y, ch, Style::default().fg(rgb_color(color)));
        }
    }
}

/// Steam rises three cells off every vent, drifting, lit by what it
/// drifts through.
fn steam(cells: &mut Cells, t: u64, scene: &Scene) {
    for &(vx, vy) in map::VENTS.iter() {
        for k in 1..=3u16 {
            let y = vy.saturating_sub(k);
            let h = mix(u64::from(vx) * 31 + u64::from(k) * 17 + t / 2);
            if h % 3 == 0 {
                continue;
            }
            let drift = (h / 3 % 3) as i32 - 1;
            let x = vx.saturating_add_signed(drift as i16);
            let ch = STEAM_CHARS[((u64::from(k) + t / 4) % STEAM_CHARS.len() as u64) as usize];
            let reflect = if k == 1 { 1.6 } else { 1.1 };
            let style = styled(wet(STEAM, reflect), scene, x, y, false);
            put_if_floor(cells, x, y, ch, style);
        }
    }
}

/// The lower city twinkles.
fn drop_lights(cells: &mut Cells, t: u64, scene: &Scene) {
    for &(x, y) in map::DROP_LIGHTS.iter() {
        let ch = map::char_at(x, y);
        let h = mix(u64::from(x) * 31 + u64::from(y) * 131 + t / 10);
        let base = surface(ch, x, y);
        let level = match ch {
            '▪' if h % 5 == 0 => 0.3,
            '▪' => 1.0,
            _ if h % 6 == 0 => 0.4,
            _ => 1.0,
        };
        let style = styled(emissive(scale(base.color, level)), scene, x, y, false);
        set(cells, x, y, ch, style);
    }
}

/// The way up to the wire pulses, a signal climbing: the railing either
/// side of the gap.
fn wire_pulse(cells: &mut Cells, t: u64, scene: &Scene) {
    let z = map::WIRE;
    let level = if (t / 5) % 2 == 0 { 1.0 } else { 0.5 };
    for x in [z.x0, z.x1] {
        let style = styled(
            emissive(scale(neon_rgb(Neon::Amber), level)),
            scene,
            x,
            z.y0,
            false,
        );
        set(cells, x, z.y0, map::char_at(x, z.y0), style);
    }
}

// --------------------------------------------------------------- runner

fn draw_runner(cells: &mut Cells, view: &CityView<'_>) {
    let (x, y) = (view.state.player_x, view.state.player_y);
    let mark = view.look.map(|look| look.mark).unwrap_or('@');
    set(cells, x, y, mark, lit(Neon::Amber));
    let label = truncate_name(view.own_username);
    put_label(
        cells,
        x,
        y.saturating_sub(1),
        &label,
        Style::default().fg(theme::TEXT_BRIGHT()),
    );
}

// ------------------------------------------------------------- overlays

/// The street's answer, pinned top-left like the bartender's banner.
fn draw_street_line(frame: &mut Frame, area: Rect, view: &CityView<'_>) {
    let Some((landmark, index)) = view.state.line() else {
        return;
    };
    let pool = data::lines(landmark);
    let Some(text) = pool.get(index) else {
        return;
    };
    let neon = landmark_neon(landmark);
    let width = (text.chars().count() + 4).min(usize::from(area.width).saturating_sub(2)) as u16;
    let rect = Rect {
        x: area.x + 1,
        y: area.y,
        width,
        height: 3.min(area.height),
    };
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            *text,
            Style::default().fg(theme::TEXT()),
        )))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(dim(neon))
                .title(Span::styled(
                    format!(" {} ", data::title(landmark)),
                    lit(neon),
                )),
        ),
        rect,
    );
}

/// What you can do where you stand, bottom-right.
fn draw_popover(frame: &mut Frame, area: Rect, view: &CityView<'_>) {
    if view.state.panel().is_some() {
        return;
    }
    let Some(landmark) = view.state.nearby() else {
        return;
    };
    let neon = landmark_neon(landmark);
    let key = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(theme::TEXT());
    let verb = match landmark.on_enter() {
        Enter::Panel(_) => "step in",
        Enter::Line(_) => "look closer",
        Enter::Leave => "back up the wire",
    };
    let title = format!(" {} ", data::title(landmark));
    let lines = vec![
        Line::from(vec![
            Span::styled("[Enter] ", key),
            Span::styled(verb, text),
        ]),
        Line::from(Span::styled(
            data::pitch(landmark),
            Style::default().fg(theme::TEXT_DIM()),
        )),
    ];
    let width = (lines
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0)
        .max(title.chars().count())
        + 4)
    .min(usize::from(area.width).saturating_sub(2)) as u16;
    let height = (lines.len() as u16 + 2).min(area.height.saturating_sub(1));
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width + 1),
        y: area.y + area.height.saturating_sub(height),
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(glow(neon))
                .title(Span::styled(title, lit(neon))),
        ),
        rect,
    );
}

/// A shop's panel, centered over the street. Content only: the catalogs
/// are on the counter, the tills are not open.
fn draw_panel(frame: &mut Frame, area: Rect, view: &CityView<'_>) {
    let Some(landmark) = view.state.panel() else {
        return;
    };
    let neon = landmark_neon(landmark);
    let lines = panel_lines(landmark, view);
    let width = (lines.iter().map(Line::width).max().unwrap_or(0) + 4)
        .min(usize::from(area.width).saturating_sub(2)) as u16;
    let height = (lines.len() as u16 + 2).min(area.height.saturating_sub(1));
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(glow(neon))
                .title(Span::styled(
                    format!(" {} ", data::title(landmark)),
                    lit(neon),
                ))
                .title_bottom(Span::styled(
                    " Esc closes ",
                    Style::default().fg(theme::TEXT_DIM()),
                )),
        ),
        rect,
    );
}

fn panel_lines(landmark: Landmark, view: &CityView<'_>) -> Vec<Line<'static>> {
    let text = Style::default().fg(theme::TEXT());
    let dim_text = Style::default().fg(theme::TEXT_DIM());
    let muted_text = Style::default().fg(theme::TEXT_MUTED());
    let head = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .add_modifier(Modifier::BOLD);
    let number = Style::default().fg(theme::AMBER());
    let mut lines: Vec<Line<'static>> = Vec::new();
    let blank = || Line::default();
    match landmark {
        Landmark::Armorer => {
            lines.push(Line::from(vec![Span::styled(
                format!(
                    "{:>4}  {:<20}{:<21}{:>7}",
                    "tier", "weapon", "armor", "bits"
                ),
                head,
            )]));
            for tier in 0..15usize {
                lines.push(Line::from(vec![
                    Span::styled(format!("{:>4}  ", tier + 1), dim_text),
                    Span::styled(format!("{:<20}", data::WEAPONS[tier]), text),
                    Span::styled(format!("{:<21}", data::ARMOR[tier]), text),
                    Span::styled(format!("{:>7}", data::COST_LADDER[tier]), number),
                ]));
            }
            lines.push(blank());
            lines.push(Line::from(Span::styled(
                "bare hands and street clothes are tier 0. power equals tier; 75% back on what you trade in.",
                dim_text,
            )));
            lines.push(Line::from(Span::styled(
                "you carry nothing. the armorer does not extend credit, and the shutters are half down.",
                muted_text,
            )));
        }
        Landmark::Tailor => {
            lines.push(Line::from(Span::styled("the mirror", head)));
            match view.look {
                Some(look) => {
                    for span in portrait_spans(look) {
                        lines.push(Line::from(vec![Span::raw("   "), span]));
                    }
                    lines.push(Line::from(vec![
                        Span::raw("   "),
                        Span::styled("mark ", dim_text),
                        Span::styled(look.mark.to_string(), lit(Neon::Amber)),
                    ]));
                }
                None => lines.push(Line::from(Span::styled(
                    "   nothing looks back. you have no runner yet.",
                    muted_text,
                ))),
            }
            lines.push(blank());
            lines.push(Line::from(Span::styled(
                "the rack (starter set, free)",
                head,
            )));
            for (label, slot) in [
                ("hoods ", Slot::Hood),
                ("eyes  ", Slot::Eyes),
                ("coats ", Slot::Coat),
            ] {
                let mut spans = vec![Span::styled(label, dim_text)];
                for (i, piece) in pieces_for(slot).enumerate() {
                    let tint = TINT_CYCLE[i % TINT_CYCLE.len()];
                    spans.push(Span::styled(
                        piece.row,
                        Style::default().fg(tint_color(tint)),
                    ));
                    spans.push(Span::raw(" "));
                }
                lines.push(Line::from(spans));
            }
            let mut marks = vec![Span::styled("marks ", dim_text)];
            for glyph in GLYPH_ALPHABET {
                marks.push(Span::styled(format!("  {glyph}   "), glow(Neon::Amber)));
            }
            lines.push(Line::from(marks));
            let mut tints = vec![Span::styled("tints ", dim_text)];
            for tint in TINT_CYCLE {
                tints.push(Span::styled(
                    format!("{:?} ", tint).to_lowercase(),
                    Style::default().fg(tint_color(tint)),
                ));
            }
            lines.push(Line::from(tints));
            lines.push(blank());
            lines.push(Line::from(Span::styled("coming to the rack", head)));
            for (what, price) in data::TAILOR_PRICES {
                lines.push(Line::from(vec![
                    Span::styled(format!("  {what:<34}"), text),
                    Span::styled(price, number),
                ]));
            }
            lines.push(Line::from(Span::styled(
                "bought pieces are permanent and leave the rack with the season. earned ones are never sold.",
                dim_text,
            )));
        }
        Landmark::Lockers => {
            lines.push(Line::from(vec![
                Span::styled("on hand   ", dim_text),
                Span::styled("— bits", text),
            ]));
            lines.push(Line::from(vec![
                Span::styled("locker    ", dim_text),
                Span::styled("— bits", text),
            ]));
            lines.push(blank());
            lines.push(Line::from(Span::styled(
                "the locker keeps what you leave in it when your signal drops. the street takes the rest.",
                text,
            )));
            lines.push(Line::from(Span::styled(
                "[d] deposit   [w] withdraw        (the lockers are humming. not open yet)",
                muted_text,
            )));
        }
        Landmark::Bands => {
            for (i, band) in data::BANDS.iter().enumerate() {
                lines.push(Line::from(vec![
                    Span::styled(format!("[{}] ", i + 1), number),
                    Span::styled(band.name, head),
                ]));
                lines.push(Line::from(Span::styled(
                    format!("    {}", band.register),
                    text,
                )));
                lines.push(Line::from(vec![
                    Span::styled("    moves  ", dim_text),
                    Span::styled(band.moves.join(" · "), muted_text),
                ]));
                lines.push(blank());
            }
            lines.push(Line::from(Span::styled(
                "you have no band. the choice is made once, on the first descent; switching benches the other.",
                dim_text,
            )));
            lines.push(Line::from(Span::styled("(not open yet)", muted_text)));
        }
        Landmark::Bar => {
            lines.push(Line::from(Span::styled(
                "the signal is warm in here.",
                text,
            )));
            lines.push(blank());
            for drink in data::DRINKS {
                lines.push(Line::from(vec![
                    Span::styled("  ♪ ", glow(Neon::Amber)),
                    Span::styled(drink, text),
                ]));
            }
            lines.push(blank());
            lines.push(Line::from(Span::styled(
                "the bartender upstairs pours the real ones, for chips. down here the glasses are for looking at.",
                dim_text,
            )));
        }
        Landmark::Repairs => {
            lines.push(Line::from(Span::styled(
                "nothing on you is broken. yet.",
                text,
            )));
            lines.push(blank());
            lines.push(Line::from(Span::styled(
                "gear keeps when your signal drops. patch is for the day the static gets its hands on it.",
                dim_text,
            )));
            lines.push(Line::from(Span::styled("(not open yet)", muted_text)));
        }
        Landmark::Board => {
            lines.push(Line::from(Span::styled("standing orders", head)));
            for notice in data::NOTICES.iter() {
                lines.push(Line::from(vec![
                    Span::styled("  ▪ ", glow(Neon::Amber)),
                    Span::styled(format!("{:<40}", notice.text), text),
                    Span::styled(notice.reward, number),
                ]));
            }
            lines.push(blank());
            lines.push(Line::from(Span::styled(
                "nothing is posted yet. the pins are rusting.",
                dim_text,
            )));
        }
        Landmark::Bits => {
            lines.push(Line::from(Span::styled("the bits machine hums.", text)));
            lines.push(Line::from(Span::styled(
                "it has never once paid out. kicking it is free.",
                dim_text,
            )));
        }
        Landmark::Screen
        | Landmark::Noodles
        | Landmark::Umbrellas
        | Landmark::Blades
        | Landmark::Reader
        | Landmark::Stairs
        | Landmark::Wire => {}
    }
    lines
}

const TINT_CYCLE: [Tint; 5] = [
    Tint::Static,
    Tint::Amber,
    Tint::Phosphor,
    Tint::White,
    Tint::Red,
];

// -------------------------------------------------------------- helpers

fn set(cells: &mut Cells, x: u16, y: u16, ch: char, style: Style) {
    if x < map::MAP_W && y < map::MAP_H {
        cells[usize::from(y)][usize::from(x)] = (ch, style);
    }
}

/// Open ground: bare, or the tile register's wet-ground texture.
fn is_floor(ch: char) -> bool {
    matches!(ch, ' ' | '.' | ',' | '`')
}

/// Paint only where the map is open ground someone could stand on, so
/// ambience never eats a prop or leaks into a wall.
fn put_if_floor(cells: &mut Cells, x: u16, y: u16, ch: char, style: Style) {
    if is_floor(map::char_at(x, y)) && map::walkable(x, y) {
        cells[usize::from(y)][usize::from(x)] = (ch, style);
    }
}

fn put_label(cells: &mut Cells, x_center: u16, y: u16, label: &str, style: Style) {
    let len = label.chars().count() as u16;
    let start = x_center
        .saturating_sub(len / 2)
        .clamp(1, map::MAP_W.saturating_sub(len + 1));
    for (i, ch) in label.chars().enumerate() {
        set(cells, start + i as u16, y, ch, style);
    }
}

fn truncate_name(name: &str) -> String {
    let count = name.chars().count();
    if count <= LABEL_MAX {
        return name.to_string();
    }
    let mut out: String = name.chars().take(LABEL_MAX - 1).collect();
    out.push('…');
    out
}

fn mix(mut v: u64) -> u64 {
    v ^= v >> 33;
    v = v.wrapping_mul(0xff51_afd7_ed55_8ccd);
    v ^= v >> 33;
    v
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
