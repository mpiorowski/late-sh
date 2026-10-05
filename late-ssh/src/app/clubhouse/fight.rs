//! The first-visit tour's taste of the dungeon: one scripted fight, drawn as
//! the whole DCSS screen (the view of the level centred on the hero, the
//! character panel down the right, the message window underneath) and
//! worded the way crawl words it.
//! Nothing here is a game: every key press plays the next beat of the same
//! short scene, so nothing is simulated or saved.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::theme;

/// The explored level, far bigger than the fight: the view is centred on
/// the hero the way crawl centres it, so a wide terminal shows more of it.
/// `LANE` is the row the hero walks in along, through the hall's west door.
const MAP: [&str; 22] = [
    "  ###########                      #########################",
    "  #.........#                      #.......................#",
    "  #..<......#                      #.##############...._...#",
    "  #.........#                      #.#            #........#",
    "  #.........#                      #.#            ##########",
    "  #######.###                      #.#                ##########",
    "        #.#               ##########'##########       #........#",
    "        #.#               #...................#       #.....%..#",
    "        #.#               #....#.........#..?.#########........#",
    "        #.#               #...................'................#",
    "        #.#################...................#########....>...#",
    "        #.................'...................#       #........#",
    "        #########.#########...................#       #........#",
    "                #.#       #...............$$..#       #........#",
    "                #.#       #....#.........#....#       #####.####",
    "                #.#       #...................#           #.#",
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
/// what is inside it and within `SIGHT` of the hero is lit.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Effect {
    None,
    /// The dragon's breath, from its mouth to the hero.
    Fire,
    /// The hero's wand of acid, from the hero to the dragon.
    Acid,
}

/// How hurt the dragon is, which colours its entry in the monster list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dragon {
    Unhurt,
    HeavilyWounded,
    SeverelyWounded,
    Dead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Beat {
    hero_x: i32,
    effect: Effect,
    hero_hp: u16,
    /// Filled cells of the noise meter.
    noise: u16,
    dragon: Dragon,
    /// What crawl would print for this turn.
    log: &'static [&'static str],
}

const BEATS: [Beat; 7] = [
    Beat {
        hero_x: 31,
        effect: Effect::None,
        hero_hp: 118,
        noise: 0,
        dragon: Dragon::Unhurt,
        log: &["A fire dragon comes into view."],
    },
    Beat {
        hero_x: 33,
        effect: Effect::None,
        hero_hp: 118,
        noise: 9,
        dragon: Dragon::Unhurt,
        log: &["The fire dragon roars deafeningly!"],
    },
    Beat {
        hero_x: 33,
        effect: Effect::Fire,
        hero_hp: 41,
        noise: 7,
        dragon: Dragon::Unhurt,
        log: &[
            "The fire dragon breathes flames at you.",
            "The blast of flame engulfs you!! You are burned terribly!",
        ],
    },
    Beat {
        hero_x: 33,
        effect: Effect::None,
        hero_hp: 97,
        noise: 1,
        dragon: Dragon::Unhurt,
        log: &["You drink the potion of heal wounds. You feel much better."],
    },
    Beat {
        hero_x: 33,
        effect: Effect::Acid,
        hero_hp: 97,
        noise: 5,
        dragon: Dragon::HeavilyWounded,
        log: &[
            "You zap the wand. The bolt of acid hits the fire dragon!!",
            "The fire dragon is heavily wounded.",
        ],
    },
    Beat {
        hero_x: 37,
        effect: Effect::None,
        hero_hp: 68,
        noise: 4,
        dragon: Dragon::SeverelyWounded,
        log: &[
            "You slash the fire dragon!! The fire dragon claws you!",
            "The fire dragon is severely wounded.",
        ],
    },
    Beat {
        hero_x: 37,
        effect: Effect::None,
        hero_hp: 68,
        noise: 4,
        dragon: Dragon::Dead,
        log: &[
            "You slice the fire dragon!!! You kill the fire dragon!",
            "Okawaru is honoured by your kill.",
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

/// One cell of the level at this beat: its glyph and how it is lit. What
/// the hero can see is drawn in full colour, what they only remember is
/// faint, and what was never explored is blank, the way crawl draws a level.
fn cell(beat: Beat, x: i32, y: i32) -> (char, Style) {
    let bold = Modifier::BOLD;
    if y == LANE && x == beat.hero_x {
        return (
            '@',
            Style::default()
                .fg(theme::BG_CANVAS())
                .bg(theme::TEXT_BRIGHT())
                .add_modifier(bold),
        );
    }
    if y == LANE && x == DRAGON_X {
        return match beat.dragon {
            Dragon::Unhurt | Dragon::HeavilyWounded | Dragon::SeverelyWounded => {
                ('D', Style::default().fg(theme::ERROR()).add_modifier(bold))
            }
            // A corpse, as crawl draws one.
            Dragon::Dead => ('†', Style::default().fg(theme::ERROR())),
        };
    }
    let between = y == LANE && x > beat.hero_x && x < DRAGON_X;
    match beat.effect {
        Effect::Fire if between || ((x - beat.hero_x).abs() <= 1 && (y - LANE).abs() == 1) => {
            let color = if (x + y) % 2 == 0 {
                theme::ERROR()
            } else {
                theme::AMBER_GLOW()
            };
            return ('§', Style::default().fg(color).add_modifier(bold));
        }
        Effect::Acid if between => {
            return (
                '*',
                Style::default().fg(theme::SUCCESS()).add_modifier(bold),
            );
        }
        Effect::None | Effect::Fire | Effect::Acid => {}
    }

    let row = match usize::try_from(y) {
        Ok(y) if y < MAP.len() => MAP[y].as_bytes(),
        Ok(_) | Err(_) => return (' ', Style::default()),
    };
    let glyph = match usize::try_from(x) {
        Ok(x) if x < row.len() => char::from(row[x]),
        Ok(_) | Err(_) => return (' ', Style::default()),
    };
    let ((left, right), (top, bottom)) = HALL;
    let in_hall = (left..=right).contains(&x) && (top..=bottom).contains(&y);
    let (dx, dy) = (x - beat.hero_x, y - LANE);
    if !in_hall || dx * dx + dy * dy > SIGHT * SIGHT {
        return (glyph, Style::default().fg(theme::TEXT_FAINT()));
    }
    let color = match glyph {
        '#' => theme::AMBER_DIM(),
        '\'' | '+' => theme::AMBER(),
        '$' => theme::AMBER_GLOW(),
        '?' => theme::MENTION(),
        _ => theme::TEXT(),
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

/// `Health: 41/118  ========--------------`, crawl's own bar.
fn bar(label: &'static str, now: u16, max: u16, color: Color) -> Vec<Span<'static>> {
    let filled = (now * BAR_CELLS).div_ceil(max);
    vec![
        Span::styled(label, Style::default().fg(theme::AMBER())),
        Span::styled(
            format!("{:<8}", format!("{now}/{max}")),
            Style::default().fg(theme::TEXT_BRIGHT()),
        ),
        Span::styled("=".repeat(usize::from(filled)), Style::default().fg(color)),
        Span::styled(
            "-".repeat(usize::from(BAR_CELLS - filled)),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ]
}

/// Two labelled numbers on one panel row, crawl's two columns.
fn stats(
    left: (&'static str, &'static str),
    right: (&'static str, &'static str),
) -> Vec<Span<'static>> {
    let label = Style::default().fg(theme::AMBER());
    let value = Style::default().fg(theme::TEXT());
    vec![
        Span::styled(left.0, label),
        Span::styled(format!("{:<13}", left.1), value),
        Span::styled(right.0, label),
        Span::styled(right.1, value),
    ]
}

/// The character panel, top to bottom, one entry per row.
fn panel(fight: &Fight, hero: &str) -> Vec<Vec<Span<'static>>> {
    let beat = fight.now();
    let bright = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .add_modifier(Modifier::BOLD);
    let label = Style::default().fg(theme::AMBER());
    let text = Style::default().fg(theme::TEXT());
    let hp_color = if beat.hero_hp * 2 <= HERO_HP {
        theme::ERROR()
    } else {
        theme::SUCCESS()
    };
    let noise_color = if beat.noise * 2 > NOISE_CELLS {
        theme::ERROR()
    } else {
        theme::AMBER_GLOW()
    };
    let clock = CLOCK + 10 * fight.beat as u32;
    let monster = match beat.dragon {
        Dragon::Unhurt => Some(theme::SUCCESS()),
        Dragon::HeavilyWounded => Some(theme::AMBER_GLOW()),
        Dragon::SeverelyWounded => Some(theme::ERROR()),
        Dragon::Dead => None,
    };
    vec![
        vec![Span::styled(format!("{hero} the Slayer"), bright)],
        vec![Span::styled("Minotaur of Okawaru ****..", text)],
        bar("Health: ", beat.hero_hp, HERO_HP, hp_color),
        bar("Magic:  ", HERO_MP, HERO_MP, theme::MENTION()),
        stats(("AC: ", "21"), ("Str: ", "27")),
        stats(("EV: ", "12"), ("Int: ", " 6")),
        stats(("SH: ", " 9"), ("Dex: ", "13")),
        stats(("XL: ", "14 Next: 62%"), ("Place: ", "Dungeon:13")),
        vec![
            Span::styled("Noise: ", label),
            Span::styled(
                "=".repeat(usize::from(beat.noise)),
                Style::default().fg(noise_color),
            ),
            Span::styled(
                "-".repeat(usize::from(NOISE_CELLS - beat.noise)),
                Style::default().fg(theme::TEXT_FAINT()),
            ),
            Span::styled("  Time: ", label),
            Span::styled(format!("{}.{} (1.0)", clock / 10, clock % 10), text),
        ],
        vec![
            Span::styled("a) ", label),
            Span::styled(
                "+3 battleaxe (flame)",
                Style::default().fg(theme::SUCCESS()),
            ),
        ],
        vec![
            Span::styled("Throw: ", label),
            Span::styled("7 javelins", text),
        ],
        match monster {
            Some(health) => vec![
                Span::styled("D ", Style::default().fg(theme::ERROR())),
                Span::styled("█ ", Style::default().fg(health)),
                Span::styled("fire dragon", text),
            ],
            None => Vec::new(),
        },
    ]
}

/// The whole screen as `height` lines of `width`: the view of the level
/// with the panel down its right edge, then the message window, which
/// fills from its top and keeps the newest message brightest.
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

    let fresh = beat.log.len();
    let said: Vec<&'static str> = BEATS[..=fight.beat]
        .iter()
        .flat_map(|beat| beat.log.iter().copied())
        .collect();
    let shown = said.len().min(usize::from(log_rows));
    for (index, message) in said[said.len() - shown..].iter().enumerate() {
        let color = if index + fresh >= shown {
            theme::TEXT_BRIGHT()
        } else {
            theme::TEXT_DIM()
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
