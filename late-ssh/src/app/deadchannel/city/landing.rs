//! Night City's card in the Games hub (page `3`), drawn for runners only
//! (`door/hub`, [`crate::app::door::hub::state::HubGame::roster`]): the
//! night city's front door. A skyline in the city's own palette (two rows
//! of towers, lit windows that come and go, signs that short out, an
//! antenna blinking red, rain, the monorail passing over, the street wet
//! under it all), the name in neon over it, a billboard ticker, then the
//! runner's sheet and today's road, who else is down there, and Enter to
//! go down. Pure render: the clock and everything else arrive in
//! [`LandingView`].
//!
//! Like the street, the card ignores the theme: every cell is painted on
//! the city's night (`ui::NIGHT`), so a light theme cannot bleed in.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Wrap},
};
use uuid::Uuid;

use crate::app::deadchannel::fight::road::STEPS;
use crate::app::deadchannel::fight::state::Sheet;
use crate::app::deadchannel::street::state::StreetView;

use super::map::Neon;
use super::ui::{
    INK, INK_BRIGHT, INK_DIM, INK_MUTED, NIGHT, Rgb, ink, neon_rgb, night, rgb_color, scale,
};

/// The ambience runs at half the animation edge, like the street's.
const SLOW: u64 = 2;
/// Rows above the name: the monorail's track, then a row of sky.
const BANNER_TOP: usize = 2;
/// Rows between the name and the street: the towers stand in them, the
/// tallest reaching up behind the name.
const SKYLINE_ROWS: usize = 10;
/// Rows between raindrops in one column.
const RAIN_PERIOD: u64 = 6;
/// The monorail: the train, its speed in cells per tick, and the gap after
/// it leaves before the next one comes in.
const TRAIN: &str = "╞▬▬▪▬▬▪▬▬▪▬▬▪▬▬╡";
const TRAIN_SPEED: u64 = 2;
const TRAIN_GAP: u64 = 140;
/// Words the signs on the towers spell, top to bottom.
const SIGNS: [&str; 7] = ["BAR", "VIDS", "BITS", "TEK", "INK", "PAWN", "SLEEP"];
/// The ticker under the skyline, scrolling one cell a tick.
const TICKER: &str = "TUNED TO A DEAD CHANNEL   ///   DEAD AIR POURS CRYSTALS, ONE A NIGHT   ///   NOODLES, TWO BOWLS OR NONE   ///   BITS: IT HUMS. IT HAS NEVER ONCE PAID OUT   ///   THE LOWER CITY, ALL THE WAY DOWN   ///   ";
/// The name's letters, five rows tall in full blocks. A space between
/// letters, three between the words.
const GLYPH_ROWS: usize = 5;

/// The skyline's surfaces.
const SKY_FAR: Rgb = [0.08, 0.08, 0.13];
const FACE: Rgb = [0.13, 0.12, 0.18];
const WINDOW_WARM: Rgb = [1.0, 0.76, 0.36];
const WINDOW_TV: Rgb = [0.45, 0.70, 1.0];
const WINDOW_DARK: Rgb = [0.22, 0.22, 0.28];
const RAIN: Rgb = [0.42, 0.48, 0.62];
const TRACK: Rgb = [0.30, 0.30, 0.36];
const STREET: Rgb = [0.20, 0.21, 0.26];
const BEACON: Rgb = [1.0, 0.18, 0.18];

pub(crate) struct LandingView<'a> {
    /// The runner's sheet mirror (`fight/session.rs`); `None` until the
    /// reload lands.
    pub sheet: Option<&'a Sheet>,
    /// Every runner on the street (`deadchannel/street`).
    pub street: &'a StreetView,
    /// This session's user, left out of the street count.
    pub own_user_id: Uuid,
    /// `marquee_tick`, the city's animation clock.
    pub tick: usize,
}

/// The Night City row in the hub sidebar, selected: magenta neon.
pub(crate) fn sidebar_selected() -> Style {
    Style::default()
        .fg(rgb_color(NIGHT))
        .bg(rgb_color(neon_rgb(Neon::Magenta)))
        .add_modifier(Modifier::BOLD)
}

/// The Night City row in the hub sidebar, not selected: the sign still
/// burns, on whatever the theme's background is.
pub(crate) fn sidebar_idle() -> Style {
    Style::default()
        .fg(rgb_color(neon_rgb(Neon::Magenta)))
        .add_modifier(Modifier::BOLD)
}

/// Draw the card into `area`, scrolled down `scroll` rows, and return how
/// far it can scroll (`door::landing::render_scrolled`).
pub(crate) fn draw(frame: &mut Frame, area: Rect, view: &LandingView<'_>, scroll: u16) -> u16 {
    frame.render_widget(Block::default().style(Style::default().bg(night())), area);
    let t = view.tick as u64 / SLOW;
    let width = usize::from(area.width);
    let mut lines = skyline(width, t);
    lines.push(ticker(width, view.tick as u64));
    lines.extend(body(view));
    crate::app::door::landing::render_scrolled(
        frame,
        area,
        Paragraph::new(lines)
            .style(Style::default().bg(night()))
            .wrap(Wrap { trim: false }),
        scroll,
    )
}

// ------------------------------------------------------------- the skyline

type Cells = Vec<Vec<(char, Style)>>;

/// What a cell of the skyline is, before the rain decides whether to fall
/// in front of it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    Sky,
    Far,
    Near,
    Name,
}

fn on(fg: Rgb, bg: Rgb) -> Style {
    Style::default().fg(rgb_color(fg)).bg(rgb_color(bg))
}

/// The skyline, one `Line` per row: track, sky, the name, the towers, the
/// street.
fn skyline(width: usize, t: u64) -> Vec<Line<'static>> {
    let name = name_rows(width);
    let banner_rows = name.len();
    let street_row = BANNER_TOP + banner_rows + SKYLINE_ROWS;
    let height = street_row + 1;
    let sky = ink(INK_MUTED);
    let mut cells: Cells = vec![vec![(' ', sky); width]; height];
    let mut layer = vec![vec![Layer::Sky; width]; height];

    far_towers(&mut cells, &mut layer, street_row, t);
    let signs = near_towers(&mut cells, &mut layer, street_row, t);
    monorail(&mut cells, t);
    name_sign(&mut cells, &mut layer, &name, t);
    rain(&mut cells, &layer, street_row, t);
    street(&mut cells, street_row, &signs, t);

    cells.into_iter().map(row_line).collect()
}

/// The far row of towers: flat silhouettes against the sky, a faint
/// window here and there. Stepped every few columns.
fn far_towers(cells: &mut Cells, layer: &mut [Vec<Layer>], street_row: usize, t: u64) {
    let width = cells[0].len();
    let mut x = 0;
    let mut block = 0u64;
    while x < width {
        let w = 3 + (hash(block, 11) % 5) as usize;
        let h = 4 + (hash(block, 12) % 8) as usize;
        for col in x..(x + w).min(width) {
            for row in street_row.saturating_sub(h)..street_row {
                cells[row][col] = (' ', on(SKY_FAR, SKY_FAR));
                layer[row][col] = Layer::Far;
                let lit = hash(col as u64 * 31 + row as u64, 13 + t / 97) % 17 == 0;
                if lit {
                    cells[row][col] = ('·', on(scale(WINDOW_WARM, 0.45), SKY_FAR));
                }
            }
        }
        x += w;
        block += 1;
    }
}

/// A sign hung on a near tower: its column, its color, and whether it is
/// burning this tick (the street's puddles reflect only what burns).
struct HungSign {
    col: usize,
    neon: Neon,
    burning: bool,
}

/// The near row of towers: faces with windows in a grid, some lit warm,
/// some lit by a screen, some dark, a few changing their minds. Tall ones
/// carry a vertical sign or an antenna. Returns the signs, for the street.
fn near_towers(
    cells: &mut Cells,
    layer: &mut [Vec<Layer>],
    street_row: usize,
    t: u64,
) -> Vec<HungSign> {
    let width = cells[0].len();
    let mut signs = Vec::new();
    let mut x = (hash(7, 7) % 3) as usize;
    let mut tower = 0u64;
    while x < width {
        let w = 5 + (hash(tower, 1) % 6) as usize;
        let h = 3 + (hash(tower, 2) % (SKYLINE_ROWS as u64 + 1)) as usize;
        let top = street_row.saturating_sub(h);
        let right = (x + w).min(width);
        for col in x..right {
            for row in top..street_row {
                let edge = col == x || col + 1 == x + w;
                let (ch, style) = match (edge, row == top) {
                    (_, true) => ('▄', on(FACE, NIGHT)),
                    (true, false) => ('▐', on(scale(FACE, 0.6), FACE)),
                    (false, false) => window(col - x, row - top, col, row, t),
                };
                cells[row][col] = (ch, style);
                layer[row][col] = Layer::Near;
            }
        }
        // The tall ones carry something: a sign down the face, or an
        // antenna with a light on it.
        if h >= 6 && w >= 6 {
            match hash(tower, 3) % 3 {
                0 | 1 => {
                    let word = SIGNS[(hash(tower, 4) % SIGNS.len() as u64) as usize];
                    let neon = [
                        Neon::Magenta,
                        Neon::Cyan,
                        Neon::Amber,
                        Neon::Green,
                        Neon::Red,
                    ][(hash(tower, 5) % 5) as usize];
                    let col = x + 1 + (hash(tower, 6) % (w as u64 - 3)) as usize;
                    // A sign shorts out now and then, a few ticks at a time.
                    let burning = hash(tower, 8 + t / 3) % 13 != 0;
                    let style = match burning {
                        true => on(neon_rgb(neon), FACE).add_modifier(Modifier::BOLD),
                        false => on(scale(neon_rgb(neon), 0.22), FACE),
                    };
                    for (i, ch) in word.chars().enumerate() {
                        let row = top + 1 + i;
                        if row < street_row && col < width {
                            cells[row][col] = (ch, style);
                        }
                    }
                    if col < width {
                        signs.push(HungSign { col, neon, burning });
                    }
                }
                _ => {
                    let col = x + w / 2;
                    if col < width && top >= 2 {
                        cells[top - 1][col] = ('│', on(scale(FACE, 1.6), NIGHT));
                        layer[top - 1][col] = Layer::Near;
                        let blink = (t / 6 + tower).is_multiple_of(2);
                        let beacon = match blink {
                            true => on(BEACON, NIGHT).add_modifier(Modifier::BOLD),
                            false => on(scale(BEACON, 0.3), NIGHT),
                        };
                        cells[top - 2][col] = ('•', beacon);
                        layer[top - 2][col] = Layer::Near;
                    }
                }
            }
        }
        x += w + (hash(tower, 9) % 2) as usize;
        tower += 1;
    }
    signs
}

/// One cell of a tower face: a window on every other column and row, lit
/// or not, the rest bare face.
fn window(dx: usize, dy: usize, col: usize, row: usize, t: u64) -> (char, Style) {
    if dx % 2 == 0 || dy % 2 == 0 {
        return (' ', on(FACE, FACE));
    }
    let seed = hash(col as u64, row as u64);
    // Some windows change their minds, slowly.
    let flip = hash(seed, t / 41) % 9 == 0;
    let kind = (seed % 10) as u8;
    let rgb = match (kind, flip) {
        (0..=3, false) | (8, true) => WINDOW_WARM,
        (4, false) | (9, true) => WINDOW_TV,
        _ => WINDOW_DARK,
    };
    // A screen behind a window flickers on its own clock.
    let rgb = match rgb == WINDOW_TV && hash(seed, t).is_multiple_of(3) {
        true => scale(rgb, 0.7),
        false => rgb,
    };
    ('▪', on(rgb, FACE))
}

/// The monorail on the top row: the track all the way across, a train
/// crossing it now and then.
fn monorail(cells: &mut Cells, t: u64) {
    let width = cells[0].len();
    for cell in cells[0].iter_mut() {
        *cell = ('═', ink(TRACK));
    }
    let len = TRAIN.chars().count() as u64;
    let run = width as u64 + len + TRAIN_GAP;
    let head = (t * TRAIN_SPEED) % run;
    for (i, ch) in TRAIN.chars().enumerate() {
        let x = head as i64 - len as i64 + i as i64;
        if x >= 0 && (x as usize) < width {
            let style = match ch {
                '▪' => ink(WINDOW_WARM).add_modifier(Modifier::BOLD),
                _ => ink(INK_DIM),
            };
            cells[0][x as usize] = (ch, style);
        }
    }
}

/// The name, laid over the sky and the tallest towers: NIGHT in magenta,
/// CITY in cyan, each letter its own tube that shorts out on its own
/// clock, with a dim echo one cell down and to the right.
fn name_sign(cells: &mut Cells, layer: &mut [Vec<Layer>], name: &[Vec<NameCell>], t: u64) {
    let width = cells[0].len();
    let name_width = name.first().map_or(0, Vec::len);
    let left = width.saturating_sub(name_width) / 2;
    for (dy, row) in name.iter().enumerate() {
        for (dx, cell) in row.iter().enumerate() {
            let Some((letter, neon)) = cell else {
                continue;
            };
            let (x, y) = (left + dx, BANNER_TOP + dy);
            if x >= width {
                continue;
            }
            let shorted = hash(*letter as u64, t / 4) % 29 == 0;
            let rgb = match shorted {
                true => scale(neon_rgb(*neon), 0.25),
                false => neon_rgb(*neon),
            };
            cells[y][x] = ('█', ink(rgb));
            layer[y][x] = Layer::Name;
            // The echo: only where the sign itself is not.
            let (ex, ey) = (x + 1, y + 1);
            let echo_free = name
                .get(dy + 1)
                .and_then(|r| r.get(dx + 1))
                .is_none_or(|c| c.is_none());
            if ex < width && ey < cells.len() && echo_free && !shorted {
                let bg = cells[ey][ex].1.bg.unwrap_or(night());
                cells[ey][ex] = (
                    '░',
                    Style::default()
                        .fg(rgb_color(scale(neon_rgb(*neon), 0.4)))
                        .bg(bg),
                );
                layer[ey][ex] = Layer::Name;
            }
        }
    }
}

/// Rain falls through the sky and in front of the far towers; the near
/// faces and the name stand in front of it.
fn rain(cells: &mut Cells, layer: &[Vec<Layer>], street_row: usize, t: u64) {
    let width = cells[0].len();
    for col in 0..width {
        if !hash(col as u64, 21).is_multiple_of(3) {
            continue;
        }
        // A drop sits on the rows where `row` matches the clock, so it
        // moves down one row a tick.
        let drop = (t + hash(col as u64, 22)) % RAIN_PERIOD;
        for row in 1..street_row {
            if row as u64 % RAIN_PERIOD != drop {
                continue;
            }
            match layer[row][col] {
                Layer::Sky => cells[row][col] = ('╎', ink(RAIN)),
                Layer::Far => cells[row][col] = ('╎', on(scale(RAIN, 0.8), SKY_FAR)),
                Layer::Near | Layer::Name => {}
            }
        }
    }
}

/// The street: wet ground, the signs smeared into it where they burn,
/// rings where the rain lands.
fn street(cells: &mut Cells, street_row: usize, signs: &[HungSign], t: u64) {
    let width = cells[0].len();
    for col in 0..width {
        let ring = hash(col as u64, t / 2) % 23 == 0;
        cells[street_row][col] = match ring {
            true => ('∘', ink(RAIN)),
            false => ('▁', ink(STREET)),
        };
    }
    for sign in signs.iter().filter(|sign| sign.burning) {
        for (offset, k) in [(-1i64, 0.3), (0, 0.6), (1, 0.3)] {
            let col = sign.col as i64 + offset;
            if col < 0 || col as usize >= width {
                continue;
            }
            let col = col as usize;
            let ch = ['≈', '~', '≈'][(hash(col as u64, t / 3) % 3) as usize];
            cells[street_row][col] = (ch, ink(scale(neon_rgb(sign.neon), k)));
        }
    }
}

/// One row of cells as a line, runs of one style as one span.
fn row_line(row: Vec<(char, Style)>) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run = String::new();
    let mut run_style: Option<Style> = None;
    for (ch, style) in row {
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
    Line::from(spans)
}

// ------------------------------------------------------------- the name

/// A cell of the name: the letter it belongs to (its index, the clock a
/// tube shorts out on) and its neon, or nothing.
type NameCell = Option<(usize, Neon)>;

fn glyph(letter: char) -> [&'static str; GLYPH_ROWS] {
    match letter {
        'N' => ["█   █", "██  █", "█ █ █", "█  ██", "█   █"],
        'I' => ["███", " █ ", " █ ", " █ ", "███"],
        'G' => [" ████", "█    ", "█  ██", "█   █", " ███ "],
        'H' => ["█   █", "█   █", "█████", "█   █", "█   █"],
        'T' => ["█████", "  █  ", "  █  ", "  █  ", "  █  "],
        'C' => [" ████", "█    ", "█    ", "█    ", " ████"],
        'Y' => ["█   █", " █ █ ", "  █  ", "  █  ", "  █  "],
        other => unreachable!("the name has no {other}"),
    }
}

/// A word in the block font: rows of cells, letters a space apart.
fn word_rows(word: &str, neon: Neon, first_letter: usize) -> Vec<Vec<NameCell>> {
    let mut rows: Vec<Vec<NameCell>> = vec![Vec::new(); GLYPH_ROWS];
    for (i, letter) in word.chars().enumerate() {
        let shape = glyph(letter);
        for (row, line) in rows.iter_mut().zip(shape) {
            if i > 0 {
                row.push(None);
            }
            row.extend(
                line.chars()
                    .map(|c| (c == '█').then_some((first_letter + i, neon))),
            );
        }
    }
    rows
}

/// NIGHT CITY on one line when the pane is wide enough, the two words
/// stacked when it is not.
fn name_rows(width: usize) -> Vec<Vec<NameCell>> {
    let night = word_rows("NIGHT", Neon::Magenta, 0);
    let city = word_rows("CITY", Neon::Cyan, 5);
    let one_line = night[0].len() + 3 + city[0].len();
    match width >= one_line + 4 {
        true => night
            .into_iter()
            .zip(city)
            .map(|(mut left, right)| {
                left.extend([None, None, None]);
                left.extend(right);
                left
            })
            .collect(),
        false => {
            let wide = night[0].len();
            let pad = (wide - city[0].len()) / 2;
            let mut rows = night;
            rows.push(vec![None; wide]);
            rows.extend(city.into_iter().map(|row| {
                let mut padded = vec![None; pad];
                padded.extend(row);
                padded.resize(wide, None);
                padded
            }));
            rows
        }
    }
}

// ------------------------------------------------------------- the copy

/// The billboard under the skyline: the ticker scrolling left, framed in
/// amber.
fn ticker(width: usize, tick: u64) -> Line<'static> {
    let chars: Vec<char> = TICKER.chars().collect();
    let inner = width.saturating_sub(4);
    let start = (tick as usize / 2) % chars.len();
    let text: String = (0..inner)
        .map(|i| chars[(start + i) % chars.len()])
        .collect();
    let amber = neon_rgb(Neon::Amber);
    Line::from(vec![
        Span::styled("▐ ", ink(scale(amber, 0.5))),
        Span::styled(text, ink(amber).add_modifier(Modifier::BOLD)),
        Span::styled(" ▌", ink(scale(amber, 0.5))),
    ])
}

fn heading(title: &str, neon: Neon) -> Line<'static> {
    Line::from(vec![
        Span::styled("  ▍", ink(neon_rgb(neon))),
        Span::styled(
            title.to_string(),
            ink(neon_rgb(neon)).add_modifier(Modifier::BOLD),
        ),
    ])
}

/// A `label  value` row in the street's greys.
fn stat(label: &str, value: Vec<Span<'static>>) -> Line<'static> {
    let mut spans = vec![Span::styled(format!("    {label:<9}"), ink(INK_DIM))];
    spans.extend(value);
    Line::from(spans)
}

fn number(value: impl ToString) -> Span<'static> {
    Span::styled(
        value.to_string(),
        ink(INK_BRIGHT).add_modifier(Modifier::BOLD),
    )
}

fn text(value: impl Into<String>) -> Span<'static> {
    Span::styled(value.into(), ink(INK_DIM))
}

/// A bar of `filled` out of `total` cells in `neon`, the rest dark.
fn bar(filled: usize, total: usize, neon: Neon) -> Vec<Span<'static>> {
    let filled = filled.min(total);
    vec![
        Span::styled("▰".repeat(filled), ink(neon_rgb(neon))),
        Span::styled("▱".repeat(total - filled), ink(INK_MUTED)),
        Span::raw(" "),
    ]
}

fn body(view: &LandingView<'_>) -> Vec<Line<'static>> {
    let mut lines = vec![
        Line::raw(""),
        Line::from(Span::styled(
            "  Under the clubhouse it is always night, and it is always raining.",
            ink(INK_BRIGHT),
        )),
        Line::from(Span::styled(
            "  Ten steps of road a day, a glyph on every lane, and a bar that pours crystals.",
            ink(INK),
        )),
        Line::raw(""),
        heading("YOUR RUNNER", Neon::Amber),
    ];
    lines.extend(runner(view.sheet));
    lines.push(Line::raw(""));
    lines.push(heading("THE STREET", Neon::Cyan));
    lines.push(street_count(view.street, view.own_user_id));
    lines.push(Line::raw(""));
    lines.push(heading("GO DOWN", Neon::Green));
    lines.push(Line::from(vec![
        Span::styled("    ▸ ", ink(neon_rgb(Neon::Green))),
        Span::styled(
            "Enter    ",
            ink(neon_rgb(Neon::Green)).add_modifier(Modifier::BOLD),
        ),
        Span::styled("down to Static Row", ink(INK_BRIGHT)),
    ]));
    for (key, label) in [
        (
            "0 0",
            "the way down from any page: the Lounge, then the stairs",
        ),
        ("0", "back up to the Lounge, from the street"),
        ("?", "the street's own guide, once you are down there"),
    ] {
        lines.push(Line::from(vec![
            Span::styled(
                format!("      {key:<9}"),
                ink(INK).add_modifier(Modifier::BOLD),
            ),
            Span::styled(label.to_string(), ink(INK_DIM)),
        ]));
    }
    lines.push(Line::raw(""));
    lines
}

/// The sheet, read off the mirror: level, signal, what the pockets hold,
/// today's road, and whether a glyph is waiting on it.
fn runner(sheet: Option<&Sheet>) -> Vec<Line<'static>> {
    let Some(sheet) = sheet else {
        return vec![Line::from(Span::styled(
            "    reading your sheet off the wire...",
            ink(INK_MUTED),
        ))];
    };
    let mut level = vec![number(sheet.level)];
    if sheet.peak_level > sheet.level {
        level.push(text(format!("  (peak {})", sheet.peak_level)));
    }
    if sheet.marks > 0 {
        level.push(text("  marks "));
        level.push(Span::styled(
            sheet.marks.to_string(),
            ink(neon_rgb(Neon::Red)).add_modifier(Modifier::BOLD),
        ));
    }
    let max_signal = sheet.max_signal().max(1);
    let signal_cells = (sheet.signal.max(0) as usize * 10).div_ceil(max_signal as usize);
    let mut signal = bar(signal_cells, 10, Neon::Cyan);
    signal.push(number(sheet.signal));
    signal.push(text(format!(" / {max_signal}")));

    let steps = sheet.road.path.len();
    let mut road = bar(steps, STEPS, Neon::Magenta);
    road.push(text(format!(
        "{steps} of {STEPS} steps, {} rations left",
        sheet.rations_left.max(0)
    )));

    let mut lines = vec![
        stat("level", level),
        stat("signal", signal),
        stat(
            "pockets",
            vec![
                number(sheet.bits),
                text(" bits   "),
                Span::styled(
                    sheet.crystals.to_string(),
                    ink(neon_rgb(Neon::Cyan)).add_modifier(Modifier::BOLD),
                ),
                text(" crystals   "),
                number(sheet.stash),
                text(" in the locker"),
            ],
        ),
        stat("road", road),
        stat(
            "glyphs",
            vec![
                number(sheet.kills),
                text(" put down, "),
                number(sheet.kills_today),
                text(" tonight"),
            ],
        ),
    ];
    if let Some(drink) = sheet.drink {
        lines.push(stat(
            "glass",
            vec![Span::styled(
                drink.name().to_string(),
                ink(neon_rgb(Neon::Amber)),
            )],
        ));
    }
    if sheet.fight.is_some() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "    ! a glyph is waiting for you on the road",
            ink(neon_rgb(Neon::Red)).add_modifier(Modifier::BOLD),
        )));
    }
    lines
}

/// Who else is down there, from presence: standing on the street, and how
/// many of them are looking at it right now.
fn street_count(street: &StreetView, own_user_id: Uuid) -> Line<'static> {
    let mut standing = 0usize;
    let mut looking = 0usize;
    for (user_id, runner) in street {
        if *user_id == own_user_id {
            continue;
        }
        standing += 1;
        looking += usize::from(runner.present);
    }
    match standing {
        0 => Line::from(Span::styled(
            "    nobody on the street tonight. be the first light on it.",
            ink(INK_DIM),
        )),
        1 => Line::from(vec![
            Span::raw("    "),
            number(1),
            text(" other runner on the street, "),
            Span::styled(
                match looking {
                    0 => "standing in the dark".to_string(),
                    _ => "looking around right now".to_string(),
                },
                ink(neon_rgb(Neon::Green)),
            ),
        ]),
        _ => Line::from(vec![
            Span::raw("    "),
            number(standing),
            text(" other runners on the street, "),
            Span::styled(
                looking.to_string(),
                ink(neon_rgb(Neon::Green)).add_modifier(Modifier::BOLD),
            ),
            text(" of them looking around right now"),
        ]),
    }
}

/// A cheap, stable mix: the same inputs paint the same skyline on every
/// replica and every frame.
fn hash(a: u64, b: u64) -> u64 {
    let mut x = a.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ b.wrapping_add(0x632B_E59B_D9B4_E019)
            .wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    x ^= x >> 29;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 32;
    x
}

#[cfg(test)]
#[path = "landing_test.rs"]
mod landing_test;
