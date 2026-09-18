//! City renderer: the street viewport (camera-follow over the map, the
//! runner as its one-cell mark) with the ambience painted on top every
//! animation tick: rain across the sky, the street and the drop, puddles
//! catching the nearest neon, signs that short out and drop a letter, steam
//! off the vents and the noodle bowls, the screen's static and its test
//! pattern, a blimp crossing behind the towers, the lower city twinkling
//! under the railing. Three overlays: a popover for the landmark within
//! reach, a pinned line when the street talks back, and a shop panel.
//!
//! Every color comes from the theme through the closed `Neon` palette, so
//! the city follows whatever palette the person picked. Single-width
//! glyphs only, like the map.

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
use super::map::{self, Landmark, Neon};
use super::state::{Enter, State};

/// Widest a floor label gets.
const LABEL_MAX: usize = 10;
/// The camera looks up the street: it centers this many rows north of
/// the runner, so a short terminal shows the shopfronts, not the drop.
const LOOK_NORTH: u16 = 8;
/// Rows between raindrops in one column.
const RAIN_PERIOD: u64 = 9;
/// The screen's cycle: static, then the test pattern for a moment.
const SCREEN_CYCLE: u64 = 120;
const SCREEN_PATTERN_TICKS: u64 = 8;
/// The static's shades, weighted toward the dark.
const STATIC_CHARS: [char; 6] = ['░', '▒', '▓', ' ', '░', '░'];
/// Steam rising off a vent.
const STEAM_CHARS: [char; 4] = ['~', '≈', '∙', '·'];
/// The blimp crossing the sky, behind the towers.
const BLIMP: &str = "<══╬══>";

pub(crate) struct CityView<'a> {
    pub state: &'a State,
    pub own_username: &'a str,
    /// The runner's look, for the mark and the tailor's mirror. `None` for
    /// a session without a runner row: the mark falls back to `@`.
    pub look: Option<&'a Look>,
}

type Cells = Vec<Vec<(char, Style)>>;

pub(crate) fn draw(frame: &mut Frame, area: Rect, view: CityView<'_>) {
    if area.width < 4 || area.height < 4 {
        return;
    }
    let t = view.state.anim_tick;
    let mut cells = styled_base_grid();
    animate(&mut cells, t);
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

/// The color a neon burns with, from the theme.
fn neon_color(neon: Neon) -> Color {
    match neon {
        Neon::Cyan => theme::CHAT_AUTHOR(),
        Neon::Magenta => theme::BOT(),
        Neon::Red => theme::ERROR(),
        Neon::Amber => theme::AMBER_GLOW(),
        Neon::Green => theme::SUCCESS(),
        Neon::White => theme::TEXT_BRIGHT(),
    }
}

fn lit(neon: Neon) -> Style {
    Style::default()
        .fg(neon_color(neon))
        .add_modifier(Modifier::BOLD)
}

fn glow(neon: Neon) -> Style {
    Style::default().fg(neon_color(neon))
}

fn dim(neon: Neon) -> Style {
    Style::default()
        .fg(neon_color(neon))
        .add_modifier(Modifier::DIM)
}

fn faint() -> Style {
    Style::default().fg(theme::TEXT_FAINT())
}

fn concrete() -> Style {
    Style::default().fg(theme::TEXT_DIM())
}

fn muted() -> Style {
    Style::default().fg(theme::TEXT_MUTED())
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

/// The sign nearest a street cell, by column: what a puddle reflects.
fn nearest_neon(x: u16) -> Neon {
    map::SIGNS
        .iter()
        .chain(map::CART_SIGNS.iter())
        .min_by_key(|sign| {
            let center = u32::from(sign.zone.x0 + sign.zone.x1) / 2;
            center.abs_diff(u32::from(x))
        })
        .map(|sign| sign.color)
        .unwrap_or(Neon::Cyan)
}

fn sign_at(x: u16, y: u16) -> Option<Neon> {
    map::SIGNS
        .iter()
        .chain(map::BANNERS.iter())
        .chain(map::CART_SIGNS.iter())
        .find(|sign| sign.zone.contains(x, y))
        .map(|sign| sign.color)
}

// ------------------------------------------------------------ base grid

fn styled_base_grid() -> Cells {
    map::grid()
        .iter()
        .enumerate()
        .map(|(y, row)| {
            row.iter()
                .enumerate()
                .map(|(x, &ch)| (ch, base_style(ch, x as u16, y as u16)))
                .collect()
        })
        .collect()
}

fn base_style(ch: char, x: u16, y: u16) -> Style {
    let frame_style = Style::default().fg(theme::BORDER_DIM());
    // The walls, and the two signs let into them.
    if y == 0 || y == map::MAP_H - 1 || x == 0 || x == map::MAP_W - 1 {
        if map::TITLE.contains(x, y) {
            return match ch {
                '▚' | '▞' => lit(Neon::Amber),
                '╡' | '╞' => frame_style,
                _ => Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            };
        }
        if y == map::MAP_H - 1 && !matches!(ch, '═' | '╚' | '╝') {
            return match ch {
                '╡' | '╞' => frame_style,
                _ => muted(),
            };
        }
        return frame_style;
    }
    // The far skyline: silhouettes, a few lit windows, the mast.
    if map::SKY.contains(x, y) {
        return match ch {
            '▪' => dim(hashed_neon(x, y)),
            '·' => faint(),
            '╫' => concrete(),
            _ => faint(),
        };
    }
    // Neon burns, one letter on the lockers is dead.
    if (x, y) == map::DEAD_LETTER {
        return faint();
    }
    if let Some(neon) = sign_at(x, y) {
        return lit(neon);
    }
    if map::BOARD_SIGN.contains(x, y) {
        return lit(Neon::White);
    }
    if map::WIRE_SIGN.contains(x, y) {
        return match ch {
            '▲' => lit(Neon::Amber),
            _ => Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        };
    }
    if map::STAIRS_SIGNS.iter().any(|z| z.contains(x, y)) {
        return match ch {
            '▼' => glow(Neon::Red),
            _ => muted(),
        };
    }
    // Awnings wear their shop's color, dimmed.
    if let Some(index) = map::AWNINGS.iter().position(|z| z.contains(x, y)) {
        return dim(map::SIGNS[index].color);
    }
    if map::WINDOWS.iter().any(|z| z.contains(x, y)) {
        return match ch {
            '▪' => window_glow(x, y),
            _ => faint(),
        };
    }
    if map::SCREEN_FACE.contains(x, y) {
        return concrete();
    }
    // The facades: concrete lines, lit doorways, what is in the windows.
    let facade_band = y > map::SKY.y1 && y < map::STREET.y0;
    if facade_band {
        return match ch {
            '┃' => muted(),
            '▒' => dim(Neon::Amber),
            '≡' | '╪' => concrete(),
            '│' | '─' | '╭' | '╮' | '╰' | '╯' | '├' | '┤' | '╔' | '╗' | '╚' | '╝' | '═' | '║'
            | '╓' | '╖' | '╙' | '╜' => faint(),
            '[' | ']' => faint(),
            '†' | '/' | '\\' | '|' => Style::default().fg(theme::TEXT_BRIGHT()),
            '(' | ')' | 'o' => dim(Neon::Green),
            '▁' | '▂' | '▃' | '▅' | '▆' => glow(Neon::Green),
            '♪' => glow(Neon::Amber),
            '░' | '▓' => dim(Neon::Amber),
            '◈' | '●' | '■' | '▚' | '▞' => glow(Neon::Magenta),
            '≋' => Style::default().fg(theme::TEXT_BRIGHT()),
            '╳' => muted(),
            '▼' => glow(Neon::Red),
            '█' => concrete(),
            _ => muted(),
        };
    }
    // The street.
    if y >= map::STREET.y0 && y <= map::STREET.y1 {
        return match ch {
            '╌' => faint(),
            '≈' | '~' => dim(nearest_neon(x)),
            '▒' if map::BOARD.contains(x, y) => faint(),
            '▒' => faint(),
            '╥' => glow(Neon::Amber),
            '@' => muted(),
            '╷' | '┴' => glow(Neon::Cyan),
            '†' | '/' => Style::default().fg(theme::TEXT_BRIGHT()),
            '╪' => Style::default().fg(theme::TEXT_BRIGHT()),
            '◌' => lit(Neon::Magenta),
            '▪' => dim(Neon::Amber),
            '≡' => faint(),
            '▓' => glow(Neon::Green),
            '(' | ')' => muted(),
            '╱' | '╲' | '▔' => concrete(),
            'o' => concrete(),
            _ => concrete(),
        };
    }
    // The railing.
    if y == map::RAIL_Y {
        return match ch {
            '╪' => concrete(),
            '╡' | '╞' => glow(Neon::Amber),
            _ => faint(),
        };
    }
    // The drop, and the stairwell down to the wire hanging into it.
    if map::WIRE.contains(x, y) {
        return match ch {
            '▓' => glow(Neon::Amber),
            '▒' => dim(Neon::Amber),
            '░' => faint(),
            _ => concrete(),
        };
    }
    match ch {
        '▪' => dim(hashed_neon(x, y)),
        '·' | '∙' => faint(),
        _ => faint(),
    }
}

/// A lit window: amber, a cold blue, or a grey glow, per pane.
fn window_glow(x: u16, y: u16) -> Style {
    match mix(u64::from(x) * 53 + u64::from(y) * 97) % 3 {
        0 => Style::default().fg(theme::AMBER_DIM()),
        1 => dim(Neon::Cyan),
        _ => muted(),
    }
}

// ------------------------------------------------------------ animation

fn animate(cells: &mut Cells, t: u64) {
    rain(cells, t);
    puddles(cells, t);
    signs(cells, t);
    windows(cells, t);
    screen(cells, t);
    steam(cells, t);
    lamps(cells, t);
    drop_lights(cells, t);
    blimp(cells, t);
    mast(cells, t);
    bits_screen(cells, t);
    wire_pulse(cells, t);
}

/// Rain across the sky, the street and the drop: one drop per column every
/// `RAIN_PERIOD` rows, falling one row a tick, only on open cells and never
/// under an awning.
fn rain(cells: &mut Cells, t: u64) {
    for zone in [map::SKY, map::STREET, map::DROP] {
        for y in zone.y0..=zone.y1 {
            for x in zone.x0..=zone.x1 {
                if map::char_at(x, y) != ' ' {
                    continue;
                }
                if y <= map::STREET.y0 + 1 && map::AWNINGS.iter().any(|a| a.x0 <= x && x <= a.x1) {
                    continue;
                }
                let column = mix(u64::from(x) * 7919);
                if column % 3 == 0 {
                    continue;
                }
                if (u64::from(y) + t + column) % RAIN_PERIOD != 0 {
                    continue;
                }
                let ch = if column % 5 == 0 { '|' } else { '\'' };
                let style = if zone == map::SKY {
                    concrete()
                } else {
                    faint()
                };
                set(cells, x, y, ch, style);
            }
        }
    }
}

/// Puddles shimmer, and now and then catch the full neon.
fn puddles(cells: &mut Cells, t: u64) {
    for &(x, y) in map::PUDDLES.iter() {
        let h = mix(u64::from(x) * 3 + u64::from(y) * 7 + t / 6);
        let ch = if h % 2 == 0 { '≈' } else { '~' };
        let neon = nearest_neon(x);
        let style = if h % 7 == 0 { glow(neon) } else { dim(neon) };
        set(cells, x, y, ch, style);
    }
}

/// Neon shorts out for a frame now and then, or drops one letter.
fn signs(cells: &mut Cells, t: u64) {
    let all = map::SIGNS
        .iter()
        .chain(map::BANNERS.iter())
        .chain(map::CART_SIGNS.iter());
    for (index, sign) in all.enumerate() {
        let h = mix(index as u64 * 131 + t / 3);
        let short = h % 23 == 0;
        let dropped = if h % 11 == 0 {
            Some(sign.zone.x0 + (h / 11 % u64::from(sign.zone.x1 - sign.zone.x0 + 1)) as u16)
        } else {
            None
        };
        if !short && dropped.is_none() {
            continue;
        }
        for y in sign.zone.y0..=sign.zone.y1 {
            for x in sign.zone.x0..=sign.zone.x1 {
                let ch = map::char_at(x, y);
                if ch == ' ' {
                    continue;
                }
                if short || dropped == Some(x) {
                    set(cells, x, y, ch, faint());
                }
            }
        }
    }
    // The dead letter stays dead whatever the sign does.
    let (dx, dy) = map::DEAD_LETTER;
    set(cells, dx, dy, map::char_at(dx, dy), faint());
}

/// A lit window goes dark for a while now and then.
fn windows(cells: &mut Cells, t: u64) {
    for zone in map::WINDOWS.iter() {
        for y in zone.y0..=zone.y1 {
            for x in zone.x0..=zone.x1 {
                if map::char_at(x, y) != '▪' {
                    continue;
                }
                let h = mix(u64::from(x) * 53 + u64::from(y) * 97 + t / 12);
                if h % 13 == 0 {
                    set(cells, x, y, '·', faint());
                }
            }
        }
    }
}

/// The screen at the end of the street: static, torn now and then, and
/// for a moment every cycle the test pattern. Rarely a glyph surfaces in
/// the noise (the fauna's alphabet, foreshadowing).
fn screen(cells: &mut Cells, t: u64) {
    let face = map::SCREEN_FACE;
    let phase = t % SCREEN_CYCLE;
    let pattern = phase < SCREEN_PATTERN_TICKS;
    let glyph_frame = (t / SCREEN_CYCLE) % 5 == 3
        && (SCREEN_PATTERN_TICKS..SCREEN_PATTERN_TICKS + 3).contains(&phase);
    let width = u64::from(face.x1 - face.x0 + 1);
    let bars: [Style; 7] = [
        glow(Neon::White),
        glow(Neon::Amber),
        glow(Neon::Cyan),
        glow(Neon::Green),
        glow(Neon::Magenta),
        glow(Neon::Red),
        concrete(),
    ];
    for y in face.y0..=face.y1 {
        let torn = mix(u64::from(y) * 17 + t / 2) % 17 == 0;
        for x in face.x0..=face.x1 {
            let h = mix(u64::from(x) * 97 + u64::from(y) * 53 + t);
            if pattern {
                let bar = (u64::from(x - face.x0) * 7 / width) as usize;
                set(cells, x, y, '█', bars[bar]);
                continue;
            }
            let ch = if torn {
                if h % 2 == 0 { '▀' } else { '▄' }
            } else if glyph_frame && h % 9 == 0 {
                GLYPH_ALPHABET[(h / 9 % GLYPH_ALPHABET.len() as u64) as usize]
            } else {
                STATIC_CHARS[(h % STATIC_CHARS.len() as u64) as usize]
            };
            let style = match h / 7 % 4 {
                0 => muted(),
                1 => concrete(),
                _ => faint(),
            };
            let style = if glyph_frame && GLYPH_ALPHABET.contains(&ch) {
                Style::default().fg(theme::TEXT_BRIGHT())
            } else {
                style
            };
            set(cells, x, y, ch, style);
        }
    }
}

/// Steam rises three cells off every vent, drifting.
fn steam(cells: &mut Cells, t: u64) {
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
            let style = if k == 1 { concrete() } else { faint() };
            put_if_floor(cells, x, y, ch, style);
        }
    }
}

/// Each street lamp throws a little light on the sidewalk below it, and
/// flickers now and then.
fn lamps(cells: &mut Cells, t: u64) {
    for (index, &(lx, ly)) in map::LAMPS.iter().enumerate() {
        let h = mix(index as u64 * 977 + t / 8);
        if h % 9 == 0 {
            set(cells, lx, ly, '╥', dim(Neon::Amber));
            continue;
        }
        set(cells, lx, ly, '╥', lit(Neon::Amber));
        for (dy, reach) in [(1u16, 1i32), (2, 2)] {
            for dx in -reach..=reach {
                let x = lx.saturating_add_signed(dx as i16);
                put_if_floor(
                    cells,
                    x,
                    ly + dy,
                    '·',
                    Style::default().fg(theme::AMBER_DIM()),
                );
            }
        }
    }
}

/// The lower city twinkles.
fn drop_lights(cells: &mut Cells, t: u64) {
    for &(x, y) in map::DROP_LIGHTS.iter() {
        let ch = map::char_at(x, y);
        let h = mix(u64::from(x) * 31 + u64::from(y) * 131 + t / 10);
        let style = match ch {
            '▪' if h % 5 == 0 => faint(),
            '▪' => dim(hashed_neon(x, y)),
            _ if h % 6 == 0 => faint(),
            _ => Style::default().fg(theme::TEXT_DIM()),
        };
        set(cells, x, y, ch, style);
    }
}

/// A blimp crosses the sky behind the towers, its light blinking.
fn blimp(cells: &mut Cells, t: u64) {
    let span = i64::from(map::MAP_W) + 40;
    let head = (t / 4 % span as u64) as i64 - 20;
    let y = map::SKY.y0 + 1;
    for (i, ch) in BLIMP.chars().enumerate() {
        let x = head + i as i64;
        if x < 1 || x >= i64::from(map::MAP_W) - 1 {
            continue;
        }
        let x = x as u16;
        if map::char_at(x, y) != ' ' {
            continue;
        }
        let style = if ch == '╬' {
            if (t / 6) % 2 == 0 {
                glow(Neon::Red)
            } else {
                faint()
            }
        } else {
            concrete()
        };
        set(cells, x, y, ch, style);
    }
}

/// The antenna mast's red light blinks.
fn mast(cells: &mut Cells, t: u64) {
    let (x, y) = map::MAST_LIGHT;
    if (t / 6) % 2 == 0 {
        set(cells, x, y, '*', lit(Neon::Red));
    }
}

/// The bits machine hums: its little display scrolls.
fn bits_screen(cells: &mut Cells, t: u64) {
    let z = map::BITS_SCREEN;
    const HUM: [char; 4] = ['▓', '▒', '░', '▒'];
    for x in z.x0..=z.x1 {
        let ch = HUM[((u64::from(x) + t / 4) % HUM.len() as u64) as usize];
        set(cells, x, z.y0, ch, glow(Neon::Green));
    }
}

/// The stairwell to the wire pulses upward, a signal climbing.
fn wire_pulse(cells: &mut Cells, t: u64) {
    let z = map::WIRE;
    let phase = (t / 5) % 3;
    for (i, y) in (z.y0 + 2..=z.y0 + 4).enumerate() {
        for x in z.x0 + 1..z.x1 {
            let ch = map::char_at(x, y);
            if ch == ' ' {
                continue;
            }
            let style = if i as u64 == 2 - phase {
                lit(Neon::Amber)
            } else {
                dim(Neon::Amber)
            };
            set(cells, x, y, ch, style);
        }
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

/// Paint only where the map is open, so ambience never eats a prop.
fn put_if_floor(cells: &mut Cells, x: u16, y: u16, ch: char, style: Style) {
    if x < map::MAP_W && y < map::MAP_H && map::char_at(x, y) == ' ' {
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
