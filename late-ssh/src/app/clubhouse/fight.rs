//! The first-visit tour's taste of the dungeon: one scripted fight, drawn as
//! the whole DCSS screen (the view of the level centred on the hero, the
//! character panel down the right, the message window underneath) and
//! worded the way crawl words it.
//! Nothing here is a game: every key press plays the next beat of the same
//! short scene, so nothing is simulated or saved.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

/// The explored level, far bigger than the fight: the view is centred on
/// the hero the way crawl centres it, so a wide terminal shows more of it.
/// `LANE` is the row the hero walks in along, through the hall's west door.
/// Glyphs are crawl's own: `≈` lava, `~` shallow water, `⌠` a fountain, `_`
/// an altar, `'` and `+` doors.
const MAP: [&str; 22] = [
    "  ###########                      #########################",
    "  #.........#                      #.......................#",
    "  #..<......#                      #.##############...._...#",
    "  #.........#                      #.#            #........#",
    "  #.........#                      #.#            ##########",
    "  #######.###                      #.#                ##########",
    "        #.#               ##########'##########       #........#",
    "        #.#               #...............≈≈≈≈#       #.....%..#",
    "        #.#               #....#.._......#≈≈≈≈#########........#",
    "        #.#               #.~~~............≈≈.'................#",
    "        #.#################.~⌠~...............#########....>...#",
    "        #.................'...................#       #........#",
    "        #########.#########...................#       #........#",
    "                #.#       #........?......$$≈≈#       #........#",
    "                #.#       #....#.........#≈≈≈≈#       #####.####",
    "                #.#       #...!...........≈≈≈≈#           #.#",
    "                #.#       ##########+##########           #.#",
    "           ######.#######                                 #.#",
    "           #............#                   ###############.#",
    "           #...)....!...#                   ................#",
    "           #............#                   #################",
    "           ##############",
];
const LANE: i32 = 11;
const DRAGON_X: i32 = 38;
/// The hall the fight happens in, walls included: columns, then rows. Only
/// what is inside it and within `SIGHT` of the hero is in view.
const HALL: ((i32, i32), (i32, i32)) = ((26, 46), (6, 16));
const SIGHT: i32 = 8;

const PANEL_WIDTH: u16 = 38;
const GAP: u16 = 1;
const HERO_HP: u16 = 118;
const HERO_MP: u16 = 11;
/// Cells in the health and magic bars.
const BAR_CELLS: u16 = 22;
/// Cells in the noise meter.
const NOISE_CELLS: u16 = 9;
/// The game clock at the first beat, in tenths of a turn.
const CLOCK: u32 = 312_047;

/// Crawl's sixteen colours, by its own names, as the terminal's palette.
/// The scene is painted in these and not the house theme: that is what a
/// crawl session looks like here.
mod crawl {
    use ratatui::style::Color;
    pub(super) const BLACK: Color = Color::Black;
    pub(super) const BLUE: Color = Color::Blue;
    pub(super) const GREEN: Color = Color::Green;
    pub(super) const CYAN: Color = Color::Cyan;
    pub(super) const RED: Color = Color::Red;
    pub(super) const MAGENTA: Color = Color::Magenta;
    pub(super) const BROWN: Color = Color::Yellow;
    pub(super) const LIGHTGREY: Color = Color::Gray;
    pub(super) const DARKGREY: Color = Color::DarkGray;
    pub(super) const LIGHTBLUE: Color = Color::LightBlue;
    pub(super) const LIGHTGREEN: Color = Color::LightGreen;
    pub(super) const LIGHTRED: Color = Color::LightRed;
    pub(super) const LIGHTMAGENTA: Color = Color::LightMagenta;
    pub(super) const YELLOW: Color = Color::LightYellow;
    pub(super) const WHITE: Color = Color::White;
}

/// How hurt the dragon is, which colours its entry in the monster list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dragon {
    Unhurt,
    SeverelyWounded,
    Dead,
}

/// The message channel a line is printed on, which is what colours it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Channel {
    Plain,
    Warning,
    Danger,
    God,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Beat {
    hero_x: i32,
    /// Whether the dragon's breath fills the lane this turn.
    breath: bool,
    hero_hp: u16,
    /// Filled cells of the noise meter.
    noise: u16,
    dragon: Dragon,
    /// What crawl would print for this turn.
    log: &'static [(Channel, &'static str)],
}

const BEATS: [Beat; 4] = [
    Beat {
        hero_x: 31,
        breath: false,
        hero_hp: 118,
        noise: 0,
        dragon: Dragon::Unhurt,
        log: &[(Channel::Warning, "A fire dragon comes into view.")],
    },
    Beat {
        hero_x: 33,
        breath: true,
        hero_hp: 41,
        noise: 8,
        dragon: Dragon::Unhurt,
        log: &[
            (Channel::Plain, "The fire dragon breathes flames at you."),
            (
                Channel::Danger,
                "The blast of flame engulfs you!! You are burned terribly!",
            ),
        ],
    },
    Beat {
        hero_x: 37,
        breath: false,
        hero_hp: 41,
        noise: 5,
        dragon: Dragon::SeverelyWounded,
        log: &[
            (Channel::Plain, "You slash the fire dragon!!!"),
            (Channel::Plain, "The fire dragon is severely wounded."),
        ],
    },
    Beat {
        hero_x: 37,
        breath: false,
        hero_hp: 41,
        noise: 4,
        dragon: Dragon::Dead,
        log: &[
            (Channel::Plain, "You slice the fire dragon!!!"),
            (Channel::Plain, "You kill the fire dragon!"),
            (Channel::God, "Okawaru is honoured by your kill."),
        ],
    },
];

/// How far into the scene the newcomer has fought.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fight {
    beat: usize,
}

impl Fight {
    pub fn new() -> Self {
        Self { beat: 0 }
    }

    /// Play the next beat. Does nothing once the dragon is down.
    pub fn strike(&mut self) {
        if !self.won() {
            self.beat += 1;
        }
    }

    pub fn won(&self) -> bool {
        self.beat == BEATS.len() - 1
    }

    fn now(&self) -> Beat {
        BEATS[self.beat]
    }
}

impl Default for Fight {
    fn default() -> Self {
        Self::new()
    }
}

/// One cell of the level at this beat: its glyph and colour. What the
/// hero can see has crawl's colours, what they only remember is dark grey
/// (stairs and items keep theirs), and what was never explored is blank.
fn cell(beat: Beat, x: i32, y: i32) -> (char, Style) {
    if y == LANE && x == beat.hero_x {
        return ('@', Style::default().fg(crawl::BLACK).bg(crawl::LIGHTGREY));
    }
    if y == LANE && x == DRAGON_X {
        return match beat.dragon {
            Dragon::Unhurt | Dragon::SeverelyWounded => ('D', Style::default().fg(crawl::GREEN)),
            // A corpse, as crawl draws one.
            Dragon::Dead => ('†', Style::default().fg(crawl::GREEN)),
        };
    }
    let in_lane = y == LANE && x > beat.hero_x && x < DRAGON_X;
    let around_hero = (x - beat.hero_x).abs() <= 1 && (y - LANE).abs() == 1;
    if beat.breath && (in_lane || around_hero) {
        // A cloud of flame flickers between these three.
        let color = match (x + 2 * y).rem_euclid(3) {
            0 => crawl::RED,
            1 => crawl::YELLOW,
            _ => crawl::LIGHTRED,
        };
        return ('§', Style::default().fg(color));
    }

    let row = match usize::try_from(y) {
        Ok(y) if y < MAP.len() => MAP[y],
        Ok(_) | Err(_) => return (' ', Style::default()),
    };
    let glyph = match usize::try_from(x) {
        Ok(x) => match row.chars().nth(x) {
            Some(glyph) => glyph,
            None => return (' ', Style::default()),
        },
        Err(_) => return (' ', Style::default()),
    };
    let ((left, right), (top, bottom)) = HALL;
    let in_hall = (left..=right).contains(&x) && (top..=bottom).contains(&y);
    let (dx, dy) = (x - beat.hero_x, y - LANE);
    let in_view = in_hall && dx * dx + dy * dy <= SIGHT * SIGHT;
    let color = match (glyph, in_view) {
        // Stairs and items look the same remembered as seen.
        ('>', _) => crawl::RED,
        ('<', _) => crawl::GREEN,
        ('$', _) => crawl::YELLOW,
        ('?', _) => crawl::WHITE,
        ('!', _) => crawl::LIGHTMAGENTA,
        (')', _) => crawl::CYAN,
        ('%', _) => crawl::BROWN,
        (_, false) => crawl::DARKGREY,
        ('#', true) => crawl::BROWN,
        ('≈', true) => crawl::RED,
        ('~', true) => crawl::CYAN,
        ('⌠', true) => crawl::BLUE,
        ('_', true) => crawl::CYAN,
        (_, true) => crawl::LIGHTGREY,
    };
    (glyph, Style::default().fg(color))
}

/// One row of the view, `cols` wide, with the hero in its middle column.
fn map_row(beat: Beat, y: i32, cols: u16) -> Vec<Span<'static>> {
    let left = beat.hero_x - i32::from(cols / 2);
    (0..i32::from(cols))
        .map(|column| {
            let (glyph, style) = cell(beat, left + column, y);
            Span::styled(glyph.to_string(), style)
        })
        .collect()
}

fn caption(text: &'static str) -> Span<'static> {
    Span::styled(text, Style::default().fg(crawl::BROWN))
}

fn value(text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(crawl::LIGHTGREY))
}

/// `Health: 41/118  ========--------------`, crawl's own bar: what is left
/// in `full`, what this turn took (`was` down to `now`) in red, the rest
/// empty. The number yellows under half and reddens under a quarter.
fn bar(label: &'static str, now: u16, was: u16, max: u16, full: Color) -> Vec<Span<'static>> {
    let cells = |points: u16| (points * BAR_CELLS).div_ceil(max);
    let (filled, lost) = (cells(now), cells(was) - cells(now));
    let number = if now * 4 <= max {
        crawl::RED
    } else if now * 2 <= max {
        crawl::YELLOW
    } else {
        crawl::LIGHTGREY
    };
    vec![
        caption(label),
        Span::styled(
            format!("{:<8}", format!("{now}/{max}")),
            Style::default().fg(number),
        ),
        Span::styled("=".repeat(usize::from(filled)), Style::default().fg(full)),
        Span::styled(
            "=".repeat(usize::from(lost)),
            Style::default().fg(crawl::RED),
        ),
        Span::styled(
            "-".repeat(usize::from(BAR_CELLS - filled - lost)),
            Style::default().fg(crawl::DARKGREY),
        ),
    ]
}

/// Two labelled numbers on one panel row, crawl's two columns.
fn stats(
    left: (&'static str, &'static str),
    right: (&'static str, &'static str),
) -> Vec<Span<'static>> {
    vec![
        caption(left.0),
        value(format!("{:<13}", left.1)),
        caption(right.0),
        value(right.1),
    ]
}

/// The character panel, top to bottom, one entry per row.
fn panel(fight: &Fight, hero: &str) -> Vec<Vec<Span<'static>>> {
    let beat = fight.now();
    let was = BEATS[fight.beat.saturating_sub(1)].hero_hp;
    let title = Style::default().fg(crawl::YELLOW);
    let noise_color = if beat.noise * 3 <= NOISE_CELLS {
        crawl::LIGHTGREY
    } else if beat.noise * 3 <= NOISE_CELLS * 2 {
        crawl::YELLOW
    } else {
        crawl::RED
    };
    let clock = CLOCK + 10 * fight.beat as u32;
    let monster = match beat.dragon {
        Dragon::Unhurt => Some(crawl::GREEN),
        Dragon::SeverelyWounded => Some(crawl::MAGENTA),
        Dragon::Dead => None,
    };
    vec![
        vec![Span::styled(format!("{hero} the Slayer"), title)],
        vec![Span::styled("Minotaur of Okawaru ****..", title)],
        bar(
            "Health: ",
            beat.hero_hp,
            was.max(beat.hero_hp),
            HERO_HP,
            crawl::LIGHTGREEN,
        ),
        bar("Magic:  ", HERO_MP, HERO_MP, HERO_MP, crawl::LIGHTBLUE),
        stats(("AC: ", "21"), ("Str: ", "27")),
        stats(("EV: ", "12"), ("Int: ", " 6")),
        stats(("SH: ", " 9"), ("Dex: ", "13")),
        stats(("XL: ", "14 Next: 62%"), ("Place: ", "Dungeon:13")),
        vec![
            caption("Noise: "),
            Span::styled(
                "=".repeat(usize::from(beat.noise)),
                Style::default().fg(noise_color),
            ),
            Span::styled(
                "-".repeat(usize::from(NOISE_CELLS - beat.noise)),
                Style::default().fg(crawl::DARKGREY),
            ),
            caption("  Time: "),
            value(format!("{}.{} (1.0)", clock / 10, clock % 10)),
        ],
        vec![
            caption("a) "),
            Span::styled("+3 battleaxe (flame)", Style::default().fg(crawl::LIGHTRED)),
        ],
        vec![caption("Throw: "), value("7 javelins")],
        match monster {
            Some(health) => vec![
                Span::styled("D ", Style::default().fg(crawl::GREEN)),
                Span::styled(" ", Style::default().bg(health)),
                value(" fire dragon"),
            ],
            None => Vec::new(),
        },
    ]
}

/// The whole screen as `height` lines of `width`: the view of the level
/// with the panel down its right edge, then the message window, which
/// fills from its top, each line in its channel's colour.
fn lines(fight: &Fight, hero: &str, width: u16, height: u16) -> Vec<Line<'static>> {
    let beat = fight.now();
    let log_rows = (height / 4).clamp(3, 7).min(height);
    let map_rows = height - log_rows;
    let map_cols = width.saturating_sub(PANEL_WIDTH + GAP);
    let top = LANE - i32::from(map_rows / 2);
    let mut panel = panel(fight, hero).into_iter();
    let mut lines: Vec<Line<'static>> = (0..i32::from(map_rows))
        .map(|row| {
            let mut spans = map_row(beat, top + row, map_cols);
            spans.push(Span::raw(" ".repeat(usize::from(GAP))));
            spans.extend(panel.next().unwrap_or_default());
            Line::from(spans)
        })
        .collect();

    let said: Vec<(Channel, &'static str)> = BEATS[..=fight.beat]
        .iter()
        .flat_map(|beat| beat.log.iter().copied())
        .collect();
    let shown = said.len().min(usize::from(log_rows));
    for (channel, message) in &said[said.len() - shown..] {
        let color = match channel {
            Channel::Plain => crawl::LIGHTGREY,
            Channel::Warning => crawl::LIGHTRED,
            Channel::Danger => crawl::RED,
            Channel::God => crawl::CYAN,
        };
        lines.push(Line::from(Span::styled(
            *message,
            Style::default().fg(color),
        )));
    }
    lines
}

/// Fills `area`. `hero` is the newcomer's own name, for the panel's first
/// line.
pub(crate) fn draw(frame: &mut Frame, area: Rect, fight: &Fight, hero: &str) {
    frame.render_widget(
        Paragraph::new(lines(fight, hero, area.width, area.height)),
        area,
    );
}

#[cfg(test)]
#[path = "fight_test.rs"]
mod fight_test;
