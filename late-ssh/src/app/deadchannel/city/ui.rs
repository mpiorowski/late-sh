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
//! Walkers pace the street under the same light, a car crosses it with
//! its headlights ahead of it, the monorail passes overhead, the signs
//! smear into the wet ground below them, the billboards cycle through
//! the city's script. Three overlays: a popover for the landmark within
//! reach, a pinned line when the street talks back, and a shop panel.
//! At the railing, Enter swaps the street for `ledge`: the lower city.
//!
//! The city has its own palette (fixed RGB: `NIGHT` under every cell,
//! `neon_rgb`, the surface constants, the `INK_*` greys of the overlays)
//! and does not follow the theme at all: one look, tuned once. Nothing on
//! this screen reads the theme module, so a light theme cannot bleed in.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::app::deadchannel::fight::data::TRADE_IN_PERCENT;
use crate::app::deadchannel::fight::session::Scene as FightScene;
use crate::app::deadchannel::fight::state::{Sheet, Slot as GearSlot, gear_name};
use crate::app::deadchannel::fight::ui as fight_ui;
use crate::app::deadchannel::glyphs::GLYPH_ALPHABET;
use crate::app::deadchannel::runner::state::{Look, Tint};
use crate::app::deadchannel::tailor::ui as tailor_ui;

use super::data;
use super::ledge;
use super::map::{self, Landmark, LightKind, Neon};
use super::state::{Enter, State};

/// Widest a floor label gets.
const LABEL_MAX: usize = 10;
/// The camera looks up the street: it centers this many rows north of
/// the runner, so a short terminal shows the shopfronts, not the drop.
const LOOK_NORTH: u16 = 8;
/// Rows between raindrops in one column.
const RAIN_PERIOD: u64 = 7;
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
const AMBIENT: f32 = 0.30;
/// Columns from the runner within which everything shows in full.
const SEE_FULL: f32 = 30.0;
/// Columns from the runner beyond which the street is at its darkest.
const SEE_END: f32 = 110.0;
/// How much of a lit surface survives at the far end.
const SEE_FLOOR: f32 = 0.30;
/// How much of an emissive surface survives at the far end: neon carries.
const EMISSIVE_FLOOR: f32 = 0.55;
/// The shadow under a wall.
const SHADOW: f32 = 0.5;
/// A room you are not at the door of.
const INSIDE_DARK: f32 = 0.45;
/// The light the runner carries: a pool around you, so where you stand is
/// always the best lit place on the street.
const CARRY_RADIUS: u16 = 7;
const CARRY: f32 = 0.7;
/// How many rows a sign smears into the wet ground in front of it.
const REFLECT_ROWS: u16 = 6;
const REFLECT: f32 = 0.5;
/// The car: one cell every `CAR_STEP` ticks along its route, then a gap
/// of `CAR_GAP` cells before the next one.
const CAR_STEP: u64 = 1;
const CAR_GAP: u64 = 240;
/// The monorail: the track row (sky), the train, its speed in cells per
/// tick, and the run it makes (the map plus room to leave the frame).
const TRACK_Y: u16 = 1;
const TRAIN: &str = "╞▬▬▪▬▬▪▬▬▪▬▬▪▬▬╡";
const TRAIN_SPEED: u64 = 2;
const TRAIN_RUN: u64 = 560;
/// A billboard scrolls one line for this many ticks, then goes dark for
/// a moment and starts the next.
const BILLBOARD_CYCLE: u64 = 120;
const BILLBOARD_DARK: u64 = 6;
/// What the boards say: street copy, one line at a time, scrolling.
const BILLBOARD_LINES: [&str; 8] = [
    "DEAD AIR   the signal is warm in here",
    "TEK   repairs, when there is something to repair",
    "VIDS   tuned to a dead channel",
    "NOODLES   two bowls or none",
    "THE LOWER CITY   all the way down",
    "BITS   it hums. it has never once paid out",
    "OFF-STREET   rooms by the hour, by the night",
    "THE WIRE   back up to #deadchannel",
];
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
    /// The runner's sheet mirror (`fight/session.rs`), for the strip and
    /// the scene's header; `None` until the descent's reload lands.
    pub sheet: Option<&'a Sheet>,
    /// The fight scene, when one is open over the street.
    pub scene: Option<&'a FightScene>,
    /// The armorer's last word (`fight/session.rs`), for its panel.
    pub till: Option<&'a str>,
    /// The tailor's mirror (`tailor/session.rs`), for its panel.
    pub tailor: tailor_ui::MirrorView<'a>,
}

type Cells = Vec<Vec<(char, Style)>>;
pub(super) type Rgb = [f32; 3];

pub(crate) fn draw(frame: &mut Frame, area: Rect, view: CityView<'_>) {
    if area.width < 4 || area.height < 4 {
        return;
    }
    let t = view.state.anim_tick;
    if view.state.at_ledge() {
        ledge::draw(frame, area, t);
        return;
    }
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

    frame.render_widget(Block::default().style(ink(INK)), area);
    let mut lines: Vec<Line> = Vec::with_capacity(vh);
    for _ in 0..pad_y {
        lines.push(Line::default());
    }
    for row in cells.iter().skip(cam_y).take(vh.saturating_sub(pad_y)) {
        let mut spans: Vec<Span> = Vec::new();
        if pad_x > 0 {
            spans.push(Span::styled(" ".repeat(pad_x), ink(INK)));
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
    frame.render_widget(Paragraph::new(lines).style(ink(INK)), area);

    draw_street_line(frame, area, &view);
    draw_popover(frame, area, &view);
    draw_panel(frame, area, &view);
    // The sheet strip and the fight scene (`fight/ui.rs`): the runner's
    // budget while walking, the scene over everything when one is open.
    match (view.sheet, view.scene) {
        (Some(sheet), None) => fight_ui::draw_strip(frame, area, sheet),
        (sheet, Some(scene)) => fight_ui::draw_scene(
            frame,
            area,
            fight_ui::SceneView {
                sheet,
                scene,
                look: view.look,
                own_username: view.own_username,
            },
        ),
        (None, None) => {}
    }
}

fn camera_origin(player: usize, viewport: usize, map_len: usize) -> usize {
    if viewport >= map_len {
        return 0;
    }
    player.saturating_sub(viewport / 2).min(map_len - viewport)
}

// ------------------------------------------------------------- palette

/// The neon colors, fixed: the city's own palette.
pub(super) fn neon_rgb(neon: Neon) -> Rgb {
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
/// The sky over the street: painted under every cell and every overlay,
/// so the terminal's canvas never shows through.
pub(super) const NIGHT: Rgb = [0.03, 0.03, 0.05];
/// The overlays' text, rain-grey on the night, bright to muted.
pub(crate) const INK_BRIGHT: Rgb = [0.92, 0.93, 0.96];
pub(crate) const INK: Rgb = [0.74, 0.76, 0.82];
pub(crate) const INK_DIM: Rgb = [0.50, 0.52, 0.58];
pub(crate) const INK_MUTED: Rgb = [0.34, 0.35, 0.40];

pub(super) fn rgb_color(c: Rgb) -> Color {
    let q = |v: f32| ((v.clamp(0.0, 1.0) * LEVELS).round() / LEVELS * 255.0) as u8;
    Color::Rgb(q(c[0]), q(c[1]), q(c[2]))
}

pub(super) fn night() -> Color {
    rgb_color(NIGHT)
}

/// A foreground on the night sky: every cell and every overlay span.
pub(crate) fn ink(c: Rgb) -> Style {
    Style::default().fg(rgb_color(c)).bg(night())
}

/// A tint of the tailor's rack in the city's own palette.
pub(crate) fn tint_rgb(tint: Tint) -> Rgb {
    match tint {
        Tint::Static => INK_DIM,
        Tint::Amber => neon_rgb(Neon::Amber),
        Tint::Phosphor => neon_rgb(Neon::Green),
        Tint::White => INK_BRIGHT,
        Tint::Red => neon_rgb(Neon::Red),
    }
}

pub(super) fn scale(c: Rgb, k: f32) -> Rgb {
    [c[0] * k, c[1] * k, c[2] * k]
}

#[cfg(test)]
fn luma(c: Rgb) -> f32 {
    0.3 * c[0] + 0.55 * c[1] + 0.15 * c[2]
}

/// A neon at full burn, for the overlays and the runner's mark.
pub(crate) fn lit(neon: Neon) -> Style {
    ink(neon_rgb(neon)).add_modifier(Modifier::BOLD)
}

pub(crate) fn glow(neon: Neon) -> Style {
    ink(neon_rgb(neon))
}

pub(crate) fn dim(neon: Neon) -> Style {
    ink(scale(neon_rgb(neon), 0.55))
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
        Landmark::Ledge => Neon::White,
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
            light: light_map(t, player_x, player_y),
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
/// crosses open floor, doorways and water, lands on walls, and stops
/// there. Takes the raw tick: the fixed lights run at the slow clock, the
/// car at the full one.
fn light_map(t: u64, player_x: u16, player_y: u16) -> Vec<Rgb> {
    let mut out = vec![[0.0f32; 3]; usize::from(map::MAP_W) * usize::from(map::MAP_H)];
    // What the runner carries.
    spread(
        &mut out,
        player_x,
        player_y,
        CARRY_RADIUS,
        scale([1.0, 0.95, 0.85], CARRY),
    );
    // The car's headlights: a pool ahead of it, and its own glow.
    if let Some(car) = car_at(t) {
        let route = car_route();
        let ahead = route[(car.index + 3).min(route.len() - 1)];
        spread(
            &mut out,
            ahead.0,
            ahead.1,
            6,
            scale(neon_rgb(Neon::White), 1.1),
        );
        let (cx, cy) = route[car.index];
        spread(&mut out, cx, cy, 2, scale(neon_rgb(Neon::Red), 0.5));
    }
    reflections(&mut out, t / SLOW);
    let t = t / SLOW;
    for (i, light) in map::LIGHTS.iter().enumerate() {
        let level = light_level(i, light, t);
        if level <= 0.0 {
            continue;
        }
        add(
            &mut out,
            &footprints()[i],
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

/// The cells one light reaches, and how much of it lands on each.
type Footprint = Vec<(usize, f32)>;

/// The fixed lights' footprints, computed once: the geometry never
/// changes, only the level each light burns at this tick.
fn footprints() -> &'static [Footprint] {
    static FOOTPRINTS: std::sync::OnceLock<Vec<Footprint>> = std::sync::OnceLock::new();
    FOOTPRINTS.get_or_init(|| {
        map::LIGHTS
            .iter()
            .map(|light| footprint(light.x, light.y, light.radius))
            .collect()
    })
}

fn add(out: &mut [Rgb], footprint: &Footprint, color: Rgb) {
    for &(i, k) in footprint {
        let cell = &mut out[i];
        cell[0] += color[0] * k;
        cell[1] += color[1] * k;
        cell[2] += color[2] * k;
    }
}

/// One moving light, flooded out to its radius and added in.
fn spread(out: &mut [Rgb], x0: u16, y0: u16, radius: u16, color: Rgb) {
    add(out, &footprint(x0, y0, radius), color);
}

/// One light, flooded out to its radius: Dial's buckets by cost, a
/// column costing one and a row two. The cost map is a local window
/// around the source, `radius` columns each way and half that in rows.
fn footprint(x0: u16, y0: u16, radius: u16) -> Footprint {
    let radius = usize::from(radius);
    let span_x = 2 * radius + 1;
    let span_y = radius + 1;
    let win_x0 = i64::from(x0) - radius as i64;
    let win_y0 = i64::from(y0) - (radius / 2) as i64;
    let slot = |x: u16, y: u16| -> usize {
        ((i64::from(y) - win_y0) as usize) * span_x + (i64::from(x) - win_x0) as usize
    };
    let mut best = vec![usize::MAX; span_x * span_y];
    let mut buckets: Vec<Vec<(u16, u16)>> = vec![Vec::new(); radius + 1];
    let mut out = Vec::new();
    buckets[0].push((x0, y0));
    best[slot(x0, y0)] = 0;
    for cost in 0..=radius {
        let cells = std::mem::take(&mut buckets[cost]);
        for (x, y) in cells {
            if best[slot(x, y)] != cost {
                continue;
            }
            let fall = 1.0 - cost as f32 / (radius as f32 + 1.0);
            out.push((index(x, y), fall * fall));
            // Light leaves the source whatever it is set in, then only
            // crosses open ground and water.
            if (x, y) != (x0, y0) && !transparent(x, y) {
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
                let entry = &mut best[slot(nx, ny)];
                if next < *entry {
                    *entry = next;
                    buckets[next].push((nx, ny));
                }
            }
        }
    }
    out
}

/// Light crosses open ground and flat water; everything else stops it.
fn transparent(x: u16, y: u16) -> bool {
    map::walkable(x, y) || matches!(map::char_at(x, y), '≈' | '~')
}

/// Every sign smears its color into the wet ground in front of it: down
/// the street when the sign faces south, up it when it faces north, a
/// few rows, fading, shimmering with the rain.
fn reflections(out: &mut [Rgb], t: u64) {
    for sign in map::SIGNS.iter() {
        let z = sign.zone;
        let dir: i16 = if transparent(z.x0, z.y0 + 1) {
            1
        } else if z.y0 > 0 && transparent(z.x0, z.y0 - 1) {
            -1
        } else {
            continue;
        };
        let color = neon_rgb(sign.color);
        for x in z.x0..=z.x1 {
            for k in 1..=REFLECT_ROWS {
                let y = z.y0.saturating_add_signed(dir * k as i16);
                if !transparent(x, y) {
                    break;
                }
                let shimmer =
                    0.6 + 0.1 * (mix(u64::from(x) * 13 + u64::from(y) * 29 + t / 2) % 5) as f32;
                let fall = 1.0 - f32::from(k) / (f32::from(REFLECT_ROWS) + 1.0);
                let k = REFLECT * fall * shimmer;
                let cell = &mut out[index(x, y)];
                cell[0] += color[0] * k;
                cell[1] += color[1] * k;
                cell[2] += color[2] * k;
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
            if h.is_multiple_of(11) {
                0.35
            } else {
                1.0
            }
        }
        LightKind::Lantern => {
            if mix(i as u64 * 31 + t / 3).is_multiple_of(4) {
                0.7
            } else {
                1.0
            }
        }
        LightKind::Machine => {
            if (t / 4 + i as u64).is_multiple_of(2) {
                1.0
            } else {
                0.7
            }
        }
        LightKind::Candle => 0.5 + (mix(i as u64 * 7 + t / 2) % 3) as f32 * 0.15,
        LightKind::Stairs => {
            if (t / 5).is_multiple_of(2) {
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
        LightKind::Billboard => match billboard_state(light.x, t) {
            BillboardState::Dark => 0.0,
            BillboardState::Showing(_) => 0.8,
        },
    }
}

enum BillboardState {
    /// Between two texts.
    Dark,
    /// Which text is up.
    Showing(u64),
}

/// A billboard cycles through texts, dark for a moment between them. Keyed
/// on its light column so the letters and the light agree.
fn billboard_state(light_x: u16, t: u64) -> BillboardState {
    let t = t + u64::from(light_x) * 7;
    if t % BILLBOARD_CYCLE < BILLBOARD_DARK {
        BillboardState::Dark
    } else {
        BillboardState::Showing(t / BILLBOARD_CYCLE)
    }
}

/// Where the car is on its route this tick, if it is on the street.
struct Car {
    index: usize,
}

/// The car's route: the length of the street, leg by leg, through both
/// doglegs. One cell per step.
fn car_route() -> &'static [(u16, u16)] {
    static ROUTE: std::sync::OnceLock<Vec<(u16, u16)>> = std::sync::OnceLock::new();
    ROUTE.get_or_init(|| {
        let waypoints: [(u16, u16); 7] = [
            (3, 19),
            (129, 19),
            (129, 21),
            (269, 21),
            (271, 21),
            (271, 18),
            (398, 18),
        ];
        let mut route = vec![waypoints[0]];
        for pair in waypoints.windows(2) {
            let (mut x, mut y) = pair[0];
            let (tx, ty) = pair[1];
            while (x, y) != (tx, ty) {
                x = if x < tx {
                    x + 1
                } else if x > tx {
                    x - 1
                } else {
                    x
                };
                y = if y < ty {
                    y + 1
                } else if y > ty {
                    y - 1
                } else {
                    y
                };
                route.push((x, y));
            }
        }
        route
    })
}

fn car_at(t: u64) -> Option<Car> {
    let len = car_route().len() as u64;
    let pos = t / CAR_STEP % (len + CAR_GAP);
    if pos < len {
        Some(Car {
            index: pos as usize,
        })
    } else {
        None
    }
}

/// Where the train's first cell is this tick, if it is crossing: it runs
/// east one time and west the next, and sits out every other run.
fn train_at(t: u64) -> Option<i64> {
    let run = t * TRAIN_SPEED / TRAIN_RUN;
    let along = (t * TRAIN_SPEED % TRAIN_RUN) as i64;
    let lead = TRAIN.chars().count() as i64;
    match run % 4 {
        0 => Some(along - lead - 40),
        2 => Some(i64::from(map::MAP_W) + 40 - along),
        _ => None,
    }
}

/// How far the runner sees: everything within `SEE_FULL` columns, then a
/// fade to `SEE_FLOOR` by `SEE_END`. Rooms show only from their door.
fn visibility_map(player_x: u16, player_y: u16) -> Vec<f32> {
    let revealed: Vec<bool> = map::BUILDINGS
        .iter()
        .map(|b| b.zone.distance(player_x, player_y) <= REVEAL_REACH)
        .collect();
    let inside = inside_map();
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
            if let Some(i) = inside[index(x, y)]
                && !revealed[i]
            {
                v *= INSIDE_DARK;
            }
            out[index(x, y)] = v;
        }
    }
    out
}

/// Which building each cell is strictly inside of, if any: computed once.
fn inside_map() -> &'static [Option<usize>] {
    static INSIDE: std::sync::OnceLock<Vec<Option<usize>>> = std::sync::OnceLock::new();
    INSIDE.get_or_init(|| {
        let mut out = vec![None; usize::from(map::MAP_W) * usize::from(map::MAP_H)];
        for (i, b) in map::BUILDINGS.iter().enumerate() {
            for y in b.zone.y0 + 1..b.zone.y1 {
                for x in b.zone.x0 + 1..b.zone.x1 {
                    out[index(x, y)] = Some(i);
                }
            }
        }
        out
    })
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
    mix(u64::from(x) * 53 + u64::from(y) * 97 + t / 12).is_multiple_of(13)
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
    if h.is_multiple_of(23) {
        SignState::Short
    } else if h.is_multiple_of(11) {
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
    let style = ink(color);
    if bold {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

/// What a map cell is before this frame's light: its glyph (a roof edge
/// where the wall meets the dark), its surface, whether it sits in a
/// wall's shadow, whether it is a neon letter. Computed once.
#[derive(Clone, Copy)]
struct Base {
    glyph: char,
    surface: Surface,
    shadow: bool,
    bold: bool,
}

fn base_map() -> &'static [Base] {
    static BASE: std::sync::OnceLock<Vec<Base>> = std::sync::OnceLock::new();
    BASE.get_or_init(|| {
        let mut out = Vec::with_capacity(usize::from(map::MAP_W) * usize::from(map::MAP_H));
        for (y, row) in map::grid().iter().enumerate() {
            let y = y as u16;
            for (x, &ch) in row.iter().enumerate() {
                let x = x as u16;
                let surface = surface(ch, x, y);
                out.push(Base {
                    glyph: roof_edge(ch, x, y),
                    surface,
                    shadow: !surface.emissive && casts_shadow(x, y),
                    bold: surface.emissive && ch.is_alphabetic(),
                });
            }
        }
        out
    })
}

/// Every map cell lit: the base frame before the ambience.
fn compose_grid(scene: &Scene) -> Cells {
    let base = base_map();
    (0..map::MAP_H)
        .map(|y| {
            (0..map::MAP_W)
                .map(|x| {
                    let cell = base[index(x, y)];
                    let color = shade(
                        cell.surface,
                        scene.light(x, y),
                        scene.vis(x, y),
                        cell.shadow,
                    );
                    let style = ink(color);
                    let style = if cell.bold {
                        style.add_modifier(Modifier::BOLD)
                    } else {
                        style
                    };
                    (cell.glyph, style)
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
    billboards(cells, slow, scene);
    walkers(cells, t, scene);
    car(cells, t, scene);
    monorail(cells, t, scene);
}

/// The billboards: a strip each, edged in dark steel, a line of street
/// copy scrolling across it in the board's neon, dark for a moment
/// between lines, a letter flickering now and then. The cells under them
/// are blank on the map.
fn billboards(cells: &mut Cells, t: u64, scene: &Scene) {
    let edge = lit_surface(scale(WALL, 0.7));
    let strip = lit_surface(scale(WALL, 0.35));
    for (i, sign) in map::BILLBOARDS.iter().enumerate() {
        let z = sign.zone;
        let y = z.y0;
        set(cells, z.x0, y, '▌', styled(edge, scene, z.x0, y, false));
        set(cells, z.x1, y, '▐', styled(edge, scene, z.x1, y, false));
        let inner = z.x0 + 1..z.x1;
        let light_x = (z.x0 + z.x1) / 2;
        let line = match billboard_state(light_x, t) {
            BillboardState::Dark => {
                for x in inner {
                    set(cells, x, y, ' ', styled(strip, scene, x, y, false));
                }
                continue;
            }
            BillboardState::Showing(n) => {
                BILLBOARD_LINES[((n + i as u64) % BILLBOARD_LINES.len() as u64) as usize]
            }
        };
        // The line enters from the right and runs off the left, then a
        // gap, then again.
        let width = u64::from(z.x1 - z.x0 - 1);
        let chars: Vec<char> = line.chars().collect();
        let run = chars.len() as u64 + width;
        let offset = (t + u64::from(light_x) * 7) % BILLBOARD_CYCLE % run;
        for (k, x) in inner.enumerate() {
            let pos = k as u64 + offset;
            let ch = if pos >= width && ((pos - width) as usize) < chars.len() {
                chars[(pos - width) as usize]
            } else {
                ' '
            };
            let flicker = mix(u64::from(x) * 3 + t / 2).is_multiple_of(17);
            let level = if flicker { 0.4 } else { 1.0 };
            let surface = if ch == ' ' {
                strip
            } else {
                emissive(scale(neon_rgb(sign.color), level))
            };
            set(
                cells,
                x,
                y,
                ch,
                styled(surface, scene, x, y, ch != ' ' && !flicker),
            );
        }
    }
}

/// The car: two cells on the street, tail light red, headlight white,
/// the pool of its headlights running ahead of it (`light_map`). Drawn
/// only on open floor, so it passes behind anything in the road.
fn car(cells: &mut Cells, t: u64, scene: &Scene) {
    let Some(car) = car_at(t) else {
        return;
    };
    let route = car_route();
    let (fx, fy) = route[car.index];
    let front = styled(
        emissive(scale(neon_rgb(Neon::White), 0.9)),
        scene,
        fx,
        fy,
        false,
    );
    put_if_floor(cells, fx, fy, '▪', front);
    if car.index > 0 {
        let (bx, by) = route[car.index - 1];
        let back = styled(
            emissive(scale(neon_rgb(Neon::Red), 0.8)),
            scene,
            bx,
            by,
            false,
        );
        put_if_floor(cells, bx, by, '▪', back);
    }
}

/// The monorail: a track across the sky, and now and then the train
/// crossing it, lit windows flickering.
fn monorail(cells: &mut Cells, t: u64, scene: &Scene) {
    let track = emissive(scale(FRAME, 0.9));
    for x in 1..map::MAP_W - 1 {
        set(
            cells,
            x,
            TRACK_Y,
            '╌',
            styled(track, scene, x, TRACK_Y, false),
        );
    }
    let Some(head) = train_at(t) else {
        return;
    };
    for (i, ch) in TRAIN.chars().enumerate() {
        let x = head + i as i64;
        if x < 1 || x >= i64::from(map::MAP_W) - 1 {
            continue;
        }
        let x = x as u16;
        let surface = match ch {
            '▪' => {
                let dark = mix(u64::from(x) * 17 + t / 6).is_multiple_of(7);
                let color = if i % 2 == 0 { Neon::Amber } else { Neon::Cyan };
                emissive(scale(neon_rgb(color), if dark { 0.3 } else { 1.0 }))
            }
            _ => emissive([0.40, 0.42, 0.48]),
        };
        set(
            cells,
            x,
            TRACK_Y,
            ch,
            styled(surface, scene, x, TRACK_Y, false),
        );
    }
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

/// Rain on every bare cell under the sky: the street, the alleys, the
/// drop, the void between the blocks, never a room and never a prop. Two
/// columns in three, a drop every `RAIN_PERIOD` rows, falling one row a
/// tick, in the color of the light it falls through (dim where the light
/// is dim).
fn rain(cells: &mut Cells, t: u64, scene: &Scene) {
    let inside = inside_map();
    for y in 1..map::MAP_H - 1 {
        for x in 1..map::MAP_W - 1 {
            if !is_floor(map::char_at(x, y)) {
                continue;
            }
            if inside[index(x, y)].is_some() {
                continue;
            }
            let column = mix(u64::from(x) * 7919);
            if column.is_multiple_of(3) {
                continue;
            }
            if !(u64::from(y) + t / 2 + column).is_multiple_of(RAIN_PERIOD) {
                continue;
            }
            let ch = if column.is_multiple_of(5) { '|' } else { '\'' };
            let style = styled(wet(RAIN, 1.3), scene, x, y, false);
            set(cells, x, y, ch, style);
        }
    }
}

/// Puddles and the canal shimmer: the light on them comes and goes.
fn puddles(cells: &mut Cells, t: u64, scene: &Scene) {
    for &(x, y) in map::PUDDLES.iter() {
        let h = mix(u64::from(x) * 3 + u64::from(y) * 7 + t / 6);
        let splash = mix(u64::from(x) * 5 + u64::from(y) * 11 + t).is_multiple_of(9);
        let ch = if splash {
            'o'
        } else if h.is_multiple_of(2) {
            '≈'
        } else {
            '~'
        };
        let reflect = if splash {
            2.8
        } else if h.is_multiple_of(7) {
            2.4
        } else {
            1.4
        };
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
        let torn = mix(u64::from(y) * 17 + t / 2).is_multiple_of(17);
        for x in face.x0..=face.x1 {
            let h = mix(u64::from(x) * 97 + u64::from(y) * 53 + t);
            let vis = scene.vis(x, y);
            if pattern {
                let bar = (u64::from(x - face.x0) * 6 / width) as usize;
                let color = shade(emissive(neon_rgb(bars[bar])), [0.0; 3], vis, false);
                set(cells, x, y, '█', ink(color));
                continue;
            }
            let ch = if torn {
                if h.is_multiple_of(2) { '▀' } else { '▄' }
            } else if glyph_frame && h.is_multiple_of(9) {
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
            set(cells, x, y, ch, ink(color));
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
            if h.is_multiple_of(3) {
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
            '▪' if h.is_multiple_of(5) => 0.3,
            '▪' => 1.0,
            _ if h.is_multiple_of(6) => 0.4,
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
    let level = if (t / 5).is_multiple_of(2) { 1.0 } else { 0.5 };
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
    put_label(cells, x, y.saturating_sub(1), &label, ink(INK_BRIGHT));
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
        Paragraph::new(Line::from(Span::styled(*text, ink(INK))))
            .style(ink(INK))
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
    if view.state.panel().is_some() || view.scene.is_some() {
        return;
    }
    let Some(landmark) = view.state.nearby() else {
        return;
    };
    let neon = landmark_neon(landmark);
    let key = lit(Neon::Amber);
    let text = ink(INK);
    let verb = match landmark.on_enter() {
        Enter::Panel(_) => "step in",
        Enter::Line(_) => "look closer",
        Enter::Leave => "back up the wire",
        Enter::Ledge => "look over",
        Enter::Fight => "step into the static",
    };
    let title = format!(" {} ", data::title(landmark));
    let lines = vec![
        Line::from(vec![
            Span::styled("[Enter] ", key),
            Span::styled(verb, text),
        ]),
        Line::from(Span::styled(data::pitch(landmark), ink(INK_DIM))),
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
        Paragraph::new(lines).style(ink(INK)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(glow(neon))
                .title(Span::styled(title, lit(neon))),
        ),
        rect,
    );
}

/// A shop's panel, centered over the street. The armorer trades; the
/// other catalogs are on the counter with their tills shut.
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
        Paragraph::new(lines).style(ink(INK)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(glow(neon))
                .title(Span::styled(
                    format!(" {} ", data::title(landmark)),
                    lit(neon),
                ))
                .title_bottom(Span::styled(" Esc closes ", ink(INK_DIM))),
        ),
        rect,
    );
}

fn panel_lines(landmark: Landmark, view: &CityView<'_>) -> Vec<Line<'static>> {
    let text = ink(INK);
    let dim_text = ink(INK_DIM);
    let muted_text = ink(INK_MUTED);
    let head = ink(INK_BRIGHT).add_modifier(Modifier::BOLD);
    let number = glow(Neon::Amber);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let blank = || Line::default();
    match landmark {
        Landmark::Armorer => lines.extend(armorer_lines(view)),
        Landmark::Tailor => lines.extend(tailor_ui::mirror_lines(&view.tailor)),
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
        | Landmark::Wire
        | Landmark::Ledge => {}
    }
    lines
}

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

pub(crate) fn mix(mut v: u64) -> u64 {
    v ^= v >> 33;
    v = v.wrapping_mul(0xff51_afd7_ed55_8ccd);
    v ^= v >> 33;
    v
}

/// The armorer's wall: the ladder with the cursor on it, what you carry
/// lit, the two keys priced for the picked row net of the trade-in, and
/// the last thing the armorer said.
fn armorer_lines(view: &CityView<'_>) -> Vec<Line<'static>> {
    let text = ink(INK);
    let dim_text = ink(INK_DIM);
    let muted_text = ink(INK_MUTED);
    let head = ink(INK_BRIGHT).add_modifier(Modifier::BOLD);
    let number = glow(Neon::Amber);
    let carried = lit(Neon::Amber);
    let key = lit(Neon::Amber);
    let short = dim(Neon::Red);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let picked = view.state.picked_tier();

    let Some(sheet) = view.sheet else {
        lines.push(Line::from(Span::styled(
            "the sheet has not come down the wire yet.",
            muted_text,
        )));
        return lines;
    };
    let name_or = |slot: GearSlot, bare: &'static str| sheet.gear_name(slot).unwrap_or(bare);
    lines.push(Line::from(vec![
        Span::styled("on hand ", dim_text),
        Span::styled(format!("{} bits", sheet.bits), number),
        Span::styled("      weapon ", dim_text),
        Span::styled(name_or(GearSlot::Weapon, "bare hands").to_string(), carried),
        Span::styled("      armor ", dim_text),
        Span::styled(name_or(GearSlot::Armor, "street clothes").to_string(), carried),
    ]));
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        format!("  {:>4}  {:<20}{:<21}{:>7}", "tier", "weapon", "armor", "bits"),
        head,
    )));
    for (index, price) in data::COST_LADDER.iter().enumerate() {
        let tier = index as i32 + 1;
        let cell = |slot: GearSlot, name: &'static str| {
            let style = match tier.cmp(&sheet.tier_of(slot)) {
                std::cmp::Ordering::Less => dim_text,
                std::cmp::Ordering::Equal => carried,
                std::cmp::Ordering::Greater => text,
            };
            Span::styled(name.to_string(), style)
        };
        let marker = match tier == picked {
            true => Span::styled("▸ ", key),
            false => Span::styled("  ", text),
        };
        lines.push(Line::from(vec![
            marker,
            Span::styled(format!("{tier:>4}  "), dim_text),
            cell(GearSlot::Weapon, data::WEAPONS[index]),
            Span::styled(" ".repeat(20usize.saturating_sub(data::WEAPONS[index].chars().count())), text),
            cell(GearSlot::Armor, data::ARMOR[index]),
            Span::styled(" ".repeat(21usize.saturating_sub(data::ARMOR[index].chars().count())), text),
            Span::styled(format!("{price:>7}"), number),
        ]));
    }
    lines.push(Line::default());
    let mut keys = vec![Span::styled("[↑↓] ", key), Span::styled("pick   ", text)];
    for (label, slot) in [("[w] ", GearSlot::Weapon), ("[a] ", GearSlot::Armor)] {
        keys.push(Span::styled(label, key));
        match picked > sheet.tier_of(slot) {
            true => {
                let price = sheet.outfit_price(slot, picked);
                keys.push(Span::styled(
                    format!("{} for ", gear_name(slot, picked).expect("a tier on the wall")),
                    text,
                ));
                let style = match price > sheet.bits {
                    true => short,
                    false => number,
                };
                keys.push(Span::styled(format!("{price} bits"), style));
            }
            false => keys.push(Span::styled("you carry that, or better", muted_text)),
        }
        keys.push(Span::styled("   ", text));
    }
    lines.push(Line::from(keys));
    lines.push(Line::from(Span::styled(
        format!(
            "power equals tier. {}% back on what you hand in. bits only, no credit.",
            TRADE_IN_PERCENT
        ),
        dim_text,
    )));
    if let Some(till) = view.till {
        lines.push(Line::from(Span::styled(till.to_string(), lit(Neon::Cyan))));
    }
    lines
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
