//! The fight scene: a panel over the street in the city's own palette.
//! Two portraits facing, the runner's and the glyph's, each losing cells
//! to static in proportion to its missing signal (GAME.md, "Signal
//! corrupts the look": health as something you see, not a number); what
//! the glyph means to do next, in its column; the exchange, line by line,
//! in the announcer's voice; and the hand along the bottom, five cards in
//! their slots with the turn's energy over them (GAME.md, "The round").
//! Pure: every frame is a function of the mirror, the scene, and the tick.
//!
//! The Old Signal gets the screen (GAME.md, "Diegetic spectacle": a boss
//! whose presence tears the frame). Its scene is the whole city area in
//! red, the empty rows full of static, the border losing cells to static
//! and the box shuddering a column from tick to tick while it
//! broadcasts; once the fight is over, everything holds still.
//!
//! Before the scene, the road (GAME.md, "The road"): the runner's sheet
//! on top, then the day's map, ten steps by three lanes, the lane walked
//! lit behind you and the road ahead fading into the dark; under it,
//! what waits on the lane the cursor is on, a glyph with its face, its
//! numbers, its moves, its pay, and the threat word the sim read for it
//! (the warning LoGD's master gave, before the fight instead of after),
//! or a rest, or a cache. Once the day's road is over the same panel is
//! the day's card.
//!
//! Also the sheet strip: level, signal, rations, bits, the crystals and
//! the day's glass when there are any, pinned top-right
//! while the runner walks the street, so the ritual's budget is always
//! in view.

use ratatui::{
    Frame,
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::cards::{Board, Card, Draft, ENERGY, HAND};
use super::data::{self, FOES, FoeKind, FoeTier, Intent, NOISE_CARDS, OLD_SIGNAL, RATIONS_PER_DAY};
use super::road::{LANES, Mark, Node, STEPS};
use super::session::{Picker, Scene};
use super::sim::Threat;
use super::state::{Fight, Powers, Quarry, Sheet, Shut, Slot};
use crate::app::arcade::share::puzzle_number;
use crate::app::deadchannel::city::map::Neon;
use crate::app::deadchannel::city::ui::{
    INK, INK_BRIGHT, INK_DIM, INK_MUTED, dim, glow, ink, lit, mix, neon_rgb, scale, tint_rgb,
};
use crate::app::deadchannel::runner::state::{Look, PORTRAIT_HEIGHT, PORTRAIT_WIDTH};

/// Cells of a signal bar.
const BAR_CELLS: usize = 12;
/// The static a lost cell turns into.
const STATIC: [char; 3] = ['░', '▒', '▓'];

pub(crate) struct SceneView<'a> {
    pub sheet: Option<&'a Sheet>,
    pub scene: &'a Scene,
    pub look: Option<&'a Look>,
    pub own_username: &'a str,
    /// The city's animation clock (`city/state.rs::anim_tick`): the Old
    /// Signal's tear and shudder run on it. A glyph's scene ignores it.
    pub tick: u64,
}

/// How the scene is dressed: a glyph's way, or the Old Signal's.
struct Dress {
    /// The foe's portrait, bar, and name.
    foe: Style,
    foe_name: Style,
    /// The foe's cells gone to static.
    foe_lost: Style,
    frame: Style,
    title: Style,
    title_text: &'static str,
    rule: Style,
}

/// A glyph: a cyan box over the street.
fn glyph_dress() -> Dress {
    Dress {
        foe: glow(Neon::Cyan),
        foe_name: lit(Neon::Cyan),
        foe_lost: dim(Neon::Red),
        frame: glow(Neon::Cyan),
        title: lit(Neon::Cyan),
        title_text: " the end of the row ",
        rule: dim(Neon::Cyan),
    }
}

/// A bright glyph: the same box, burning amber.
fn bright_dress() -> Dress {
    Dress {
        foe: glow(Neon::Amber),
        foe_name: lit(Neon::Amber),
        foe_lost: dim(Neon::Red),
        frame: glow(Neon::Amber),
        title: lit(Neon::Amber),
        title_text: " something bright ",
        rule: dim(Neon::Amber),
    }
}

/// The Old Signal: red, and once it is over, the red burns down.
fn old_signal_dress(over: bool) -> Dress {
    let frame = match over {
        true => dim(Neon::Red),
        false => glow(Neon::Red),
    };
    Dress {
        foe: glow(Neon::Red),
        foe_name: lit(Neon::Red),
        foe_lost: dim(Neon::Magenta),
        frame,
        title: lit(Neon::Red),
        title_text: " the bottom of the city ",
        rule: dim(Neon::Red),
    }
}

/// A portrait row with some of its cells gone to static: `missing` is the
/// fraction of signal lost, `seed` keeps the same cells gone from frame
/// to frame (a wound looks the same all fight, not a shimmer).
pub(crate) fn corrupt(
    rows: [&str; PORTRAIT_HEIGHT],
    missing: f32,
    seed: u64,
) -> Vec<Vec<(char, bool)>> {
    let total = PORTRAIT_WIDTH * PORTRAIT_HEIGHT;
    let gone = ((total as f32) * missing.clamp(0.0, 1.0)).round() as usize;
    // Rank every cell by a seeded hash; the lowest `gone` are lost.
    let mut order: Vec<(u64, usize)> = (0..total)
        .map(|i| {
            (
                mix(seed ^ (i as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15)),
                i,
            )
        })
        .collect();
    order.sort_unstable();
    let lost: std::collections::HashSet<usize> = order.iter().take(gone).map(|(_, i)| *i).collect();
    rows.iter()
        .enumerate()
        .map(|(y, row)| {
            row.chars()
                .enumerate()
                .map(|(x, ch)| {
                    let i = y * PORTRAIT_WIDTH + x;
                    match lost.contains(&i) {
                        true => (STATIC[(mix(seed ^ (i as u64 + 77)) % 3) as usize], true),
                        false => (ch, false),
                    }
                })
                .collect()
        })
        .collect()
}

fn missing(current: i32, max: i32) -> f32 {
    1.0 - (current.clamp(0, max.max(1)) as f32 / max.max(1) as f32)
}

/// The runner's portrait, corrupted by the sheet's missing signal, one
/// span per cell run. A session with no look wears a plain `@` face.
fn runner_rows(sheet: &Sheet, look: Option<&Look>) -> Vec<Vec<Span<'static>>> {
    let missing = missing(sheet.signal, sheet.max_signal());
    let seed = sheet.user_id.as_u128() as u64
        ^ sheet
            .day
            .and_hms_opt(0, 0, 0)
            .map(|t| t.and_utc().timestamp() as u64)
            .unwrap_or(0);
    match look {
        Some(look) => {
            let worn = look.rows();
            let rows = [worn[0].piece.row, worn[1].piece.row, worn[2].piece.row];
            corrupt(rows, missing, seed)
                .into_iter()
                .zip(worn)
                .map(|(cells, worn)| {
                    cells
                        .into_iter()
                        .map(|(ch, lost)| {
                            let style = match lost {
                                true => ink(INK_DIM),
                                false => ink(tint_rgb(worn.tint)),
                            };
                            Span::styled(ch.to_string(), style)
                        })
                        .collect()
                })
                .collect()
        }
        None => vec![
            vec![Span::styled("     ", ink(INK))],
            vec![Span::styled("  @  ", lit(Neon::Amber))],
            vec![Span::styled("     ", ink(INK))],
        ],
    }
}

fn foe_rows(fight: &Fight, dress: &Dress) -> Vec<Vec<Span<'static>>> {
    let missing = missing(fight.foe_signal, fight.foe_max_signal);
    let kind = match fight.quarry {
        Quarry::Glyph(kind) => kind as u64,
        Quarry::OldSignal => FOES.len() as u64,
    };
    let seed = (kind + 1).wrapping_mul(0x51_7cc1_b727_220a);
    corrupt(fight.foe().portrait, missing, seed)
        .into_iter()
        .map(|cells| {
            cells
                .into_iter()
                .map(|(ch, lost)| {
                    let style = match lost {
                        true => dress.foe_lost,
                        false => dress.foe,
                    };
                    Span::styled(ch.to_string(), style)
                })
                .collect()
        })
        .collect()
}

/// The panel's inner width: fixed, so the scene never resizes while the
/// exchange runs (a box that grows a row per hit is a box that jumps).
const INNER: usize = 78;
/// Rows of the exchange the scene shows; older lines scroll off the top.
const LOG_ROWS: usize = 5;
/// The fewest log rows a short terminal may squeeze the scene to.
const LOG_ROWS_MIN: usize = 2;
/// Width of each stat column beside a portrait.
const COL: usize = 28;

/// A signal bar as two spans, the filled run and the empty run. `mirror`
/// fills from the right, so the two bars face each other across the box.
fn bar_spans(current: i32, max: i32, filled_style: Style, mirror: bool) -> Vec<Span<'static>> {
    let max = max.max(1);
    let filled = ((current.clamp(0, max) as f32 / max as f32) * BAR_CELLS as f32).round() as usize;
    let full = Span::styled("█".repeat(filled), filled_style);
    let empty = Span::styled("░".repeat(BAR_CELLS - filled), ink(INK_MUTED));
    match mirror {
        true => vec![empty, full],
        false => vec![full, empty],
    }
}

fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.width()).sum()
}

/// Pad a run of spans to `width`, on the right or (for the foe's column)
/// on the left.
fn pad(mut spans: Vec<Span<'static>>, width: usize, left: bool) -> Vec<Span<'static>> {
    let used = spans_width(&spans);
    let fill = Span::styled(" ".repeat(width.saturating_sub(used)), ink(INK));
    match left {
        true => spans.insert(0, fill),
        false => spans.push(fill),
    }
    spans
}

/// The carried weapon's name, `bare hands` at tier 0.
pub(crate) fn weapon_name(sheet: &Sheet) -> &'static str {
    sheet.gear_name(Slot::Weapon).unwrap_or("bare hands")
}

/// The carried armor's name, `street clothes` at tier 0.
pub(crate) fn armor_name(sheet: &Sheet) -> &'static str {
    sheet.gear_name(Slot::Armor).unwrap_or("street clothes")
}

/// The three header rows: your face and stats on the left, the glyph's
/// on the right, the two bars facing each other, and under the glyph's
/// bar `intent`, what it means to do next (empty once the fight is over:
/// its numbers go there instead).
fn header(
    sheet: &Sheet,
    look: Option<&Look>,
    username: &str,
    dress: &Dress,
    intent: Vec<Span<'static>>,
) -> Vec<Line<'static>> {
    let head = ink(INK_BRIGHT).add_modifier(Modifier::BOLD);
    let dim_text = ink(INK_DIM);
    let text = ink(INK);
    let mine = runner_rows(sheet, look);
    let theirs = sheet.fight.as_ref().map(|fight| foe_rows(fight, dress));
    let left: [Vec<Span<'static>>; 3] = [
        vec![
            Span::styled(username.to_string(), head),
            Span::styled(format!("  lv {}", sheet.level), text),
        ],
        {
            let mut row = vec![Span::styled("signal ", dim_text)];
            row.extend(bar_spans(
                sheet.signal,
                sheet.max_signal(),
                glow(Neon::Green),
                false,
            ));
            row.push(Span::styled(
                format!(" {}/{}", sheet.signal, sheet.max_signal()),
                text,
            ));
            row
        },
        vec![Span::styled(
            format!("attack {}  defense {}", sheet.attack(), sheet.defense()),
            dim_text,
        )],
    ];
    let right: [Vec<Span<'static>>; 3] = match &sheet.fight {
        Some(fight) => [
            vec![
                match fight.foe_level() {
                    Some(level) => Span::styled(format!("lv {level}  "), text),
                    None => Span::styled("", text),
                },
                Span::styled(fight.name(), dress.foe_name),
            ],
            {
                let mut row = vec![Span::styled(
                    format!("{}/{} ", fight.foe_signal, fight.foe_max_signal),
                    text,
                )];
                row.extend(bar_spans(
                    fight.foe_signal,
                    fight.foe_max_signal,
                    dress.foe,
                    true,
                ));
                row.push(Span::styled(" signal", dim_text));
                row
            },
            match intent.is_empty() {
                true => vec![Span::styled(
                    format!("attack {}  defense {}", fight.foe_attack, fight.foe_defense),
                    dim_text,
                )],
                false => intent,
            },
        ],
        None => [Vec::new(), Vec::new(), Vec::new()],
    };
    (0..PORTRAIT_HEIGHT)
        .map(|row| {
            let mut spans: Vec<Span<'static>> = vec![Span::styled("  ", text)];
            spans.extend(mine[row].iter().cloned());
            spans.push(Span::styled("   ", text));
            spans.extend(pad(left[row].clone(), COL, false));
            spans.push(Span::styled("  ", text));
            spans.extend(pad(right[row].clone(), COL, true));
            spans.push(Span::styled("   ", text));
            match &theirs {
                Some(theirs) => spans.extend(theirs[row].iter().cloned()),
                None => spans.push(Span::styled("     ", text)),
            }
            spans.push(Span::styled("  ", text));
            Line::from(spans)
        })
        .collect()
}

fn truncate(line: &str, width: usize) -> String {
    match line.chars().count() > width {
        true => {
            let mut out: String = line.chars().take(width.saturating_sub(1)).collect();
            out.push('…');
            out
        }
        false => line.to_string(),
    }
}

/// One in this many border cells is static on the Old Signal's frame,
/// a different set every tick.
const TEAR_ONE_IN: u64 = 6;
/// One in this many cells of an empty row is static on the Old Signal's
/// screen.
const NOISE_ONE_IN: u64 = 9;

/// A row of the Old Signal's screen with nothing printed on it: sparse
/// static, `seed` deciding which cells. Live, the seed moves with the
/// tick; over, it holds, and the row goes quiet grey.
fn noise_line(width: usize, seed: u64, quiet: bool) -> Line<'static> {
    let style = match quiet {
        true => ink(INK_MUTED),
        false => dim(Neon::Red),
    };
    let row: String = (0..width)
        .map(|x| {
            let roll = mix(seed ^ (x as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15));
            match roll % NOISE_ONE_IN {
                0 => STATIC[(roll / NOISE_ONE_IN % 3) as usize],
                _ => ' ',
            }
        })
        .collect();
    Line::from(Span::styled(row, style))
}

/// The Old Signal's presence on the frame: border cells gone to static,
/// a different set every tick. Only the box-drawing cells tear; the
/// titles stay legible.
fn tear(buf: &mut Buffer, rect: Rect, tick: u64) {
    let style = dim(Neon::Red);
    let top = rect.y;
    let bottom = rect.y + rect.height.saturating_sub(1);
    let left = rect.x;
    let right = rect.x + rect.width.saturating_sub(1);
    let mut perimeter: Vec<(u16, u16)> = Vec::new();
    for x in left..=right {
        perimeter.push((x, top));
        perimeter.push((x, bottom));
    }
    for y in top + 1..bottom {
        perimeter.push((left, y));
        perimeter.push((right, y));
    }
    for (i, (x, y)) in perimeter.into_iter().enumerate() {
        let roll = mix(tick.wrapping_mul(0x9e37_79b9_7f4a_7c15)
            ^ (i as u64 + 1).wrapping_mul(0x51_7cc1_b727_220a));
        if !roll.is_multiple_of(TEAR_ONE_IN) {
            continue;
        }
        let Some(cell) = buf.cell_mut((x, y)) else {
            continue;
        };
        if !"─│┌┐└┘".contains(cell.symbol()) {
            continue;
        }
        let mut encoded = [0u8; 4];
        cell.set_symbol(STATIC[(roll / TEAR_ONE_IN % 3) as usize].encode_utf8(&mut encoded))
            .set_style(style);
    }
}

/// One card of the hand, border and all.
const CARD_W: usize = 13;
/// Rows of the hand drawn as cards: the readout row, then the cards.
const HAND_ROWS: usize = 5;
/// Rows of the hand on a terminal too short for cards: the readout row,
/// then the hand as one row of chips.
const HAND_ROWS_COMPACT: usize = 2;

/// The neon a card burns in.
/// By what it is for: red hits, amber is the big turn, cyan guards, green
/// cleans and mends, magenta is static put to work, white is the cut-out.
fn card_neon(card: Card) -> Option<Neon> {
    match card {
        Card::Strike | Card::Jab | Card::Sever => Some(Neon::Red),
        Card::Surge | Card::Burn => Some(Neon::Amber),
        Card::Block | Card::Bulwark | Card::Riposte => Some(Neon::Cyan),
        Card::Wipe | Card::Siphon => Some(Neon::Green),
        Card::Ground => Some(Neon::Magenta),
        Card::Mute => Some(Neon::White),
        Card::Static => None,
    }
}

/// What a card does, in the two rows under its name: the number it would
/// land for played now, then the rule nobody would guess. The numbers are
/// `Card::effect`'s, so the card never prints one the machine would not
/// play.
fn card_text(card: Card, powers: &Powers, board: &Board) -> [String; 2] {
    let effect = card.effect(powers, board);
    let hit = format!("hit {}", effect.damage);
    let hold = format!("hold {}", effect.block);
    match card {
        Card::Strike | Card::Surge => [hit, String::new()],
        Card::Block | Card::Bulwark => [hold, "till hit".to_string()],
        Card::Wipe => [hold, "-static".to_string()],
        Card::Jab => [hit, "free".to_string()],
        Card::Siphon => [hit, format!("mend {}", effect.mend)],
        Card::Riposte => [hit, "+block up".to_string()],
        Card::Burn => [format!("+{} energy", effect.energy), "-static".to_string()],
        Card::Ground => [hit, "x static".to_string()],
        Card::Sever => [hit, "x2.5<half".to_string()],
        Card::Mute => ["its move".to_string(), "= nothing".to_string()],
        Card::Static => ["dead card".to_string(), "play=gone".to_string()],
    }
}

/// The one number a chip has room for.
fn chip_number(card: Card, powers: &Powers, board: &Board) -> String {
    let effect = card.effect(powers, board);
    match card {
        Card::Strike
        | Card::Surge
        | Card::Jab
        | Card::Siphon
        | Card::Riposte
        | Card::Ground
        | Card::Sever => format!(" {}", effect.damage),
        Card::Block | Card::Wipe | Card::Bulwark => format!(" {}", effect.block),
        Card::Burn | Card::Mute | Card::Static => String::new(),
    }
}

/// The turn's budget, above the hand: energy as pips, the piles, the
/// static riding the deck.
fn hand_readout(fight: &Fight) -> Line<'static> {
    let dim_text = ink(INK_DIM);
    let mut spans = vec![Span::styled("    energy ", dim_text)];
    // A burn carries the turn past its three: the row grows to show it.
    for pip in 0..ENERGY.max(fight.energy) {
        // Spent energy is a different shape, not only a darker one: the
        // count reads without color.
        spans.push(match pip < fight.energy {
            true => Span::styled("██", lit(Neon::Amber)),
            false => Span::styled("░░", ink(INK_MUTED)),
        });
        spans.push(Span::styled(" ", ink(INK)));
    }
    spans.push(Span::styled(
        format!(
            "   deck {} · discard {}",
            fight.piles.draw.len(),
            fight.piles.discard.len()
        ),
        dim_text,
    ));
    match fight.piles.static_cards() {
        0 => {}
        cards => spans.push(Span::styled(
            format!(" · static {cards}"),
            dim(Neon::Magenta),
        )),
    }
    Line::from(spans)
}

/// The hand as five cards side by side, each in its slot: the number to
/// press in the corner, the name on the frame, what it does inside, its
/// cost as pips on the bottom edge. A card the turn cannot pay for is
/// drawn dark; a slot already played keeps its corners and nothing else,
/// so the numbers never move under the fingers.
fn hand_cards(sheet: &Sheet, fight: &Fight) -> Vec<Line<'static>> {
    let powers = sheet.powers(fight);
    let board = fight.board();
    let text = ink(INK);
    let muted = ink(INK_MUTED);
    let mut rows: [Vec<Span<'static>>; 4] = [
        vec![Span::styled("    ", text)],
        vec![Span::styled("    ", text)],
        vec![Span::styled("    ", text)],
        vec![Span::styled("    ", text)],
    ];
    let inside = CARD_W - 4;
    for slot in 0..HAND {
        match fight.piles.card(slot) {
            None => {
                let gap = " ".repeat(CARD_W - 2);
                rows[0].push(Span::styled(format!("╭{gap}╮"), muted));
                rows[1].push(Span::styled(" ".repeat(CARD_W), text));
                rows[2].push(Span::styled(" ".repeat(CARD_W), text));
                rows[3].push(Span::styled(format!("╰{gap}╯"), muted));
            }
            Some(card) => {
                let affordable = card.cost() <= fight.energy;
                let (frame, name, body, key) = match (card_neon(card), affordable) {
                    (Some(neon), true) => (dim(neon), lit(neon), ink(INK_BRIGHT), lit(Neon::Amber)),
                    (Some(_), false) | (None, false) => (muted, muted, muted, muted),
                    (None, true) => (ink(INK_DIM), ink(INK_DIM), muted, lit(Neon::Amber)),
                };
                let [first, second] = card_text(card, &powers, &board);
                let label = card.name();
                rows[0].extend([
                    Span::styled("╭ ", frame),
                    Span::styled((slot + 1).to_string(), key),
                    Span::styled(" ", frame),
                    Span::styled(label.to_string(), name),
                    Span::styled(format!(" {}╮", "─".repeat(CARD_W - 6 - label.len())), frame),
                ]);
                for (row, line) in [(1, first), (2, second)] {
                    rows[row].extend([
                        Span::styled("│ ", frame),
                        Span::styled(format!("{line:<inside$}"), body),
                        Span::styled(" │", frame),
                    ]);
                }
                let pips = match card.cost() {
                    0 => "free".to_string(),
                    cost => vec!["█"; usize::from(cost)].join(" "),
                };
                rows[3].extend([
                    Span::styled("╰ ", frame),
                    Span::styled(
                        pips.clone(),
                        match affordable {
                            true => glow(Neon::Amber),
                            false => muted,
                        },
                    ),
                    Span::styled(
                        format!(" {}╯", "─".repeat(CARD_W - 4 - pips.chars().count())),
                        frame,
                    ),
                ]);
            }
        }
        for row in &mut rows {
            row.push(Span::styled(" ", text));
        }
    }
    rows.into_iter().map(Line::from).collect()
}

/// The hand as one row of chips, for a terminal with no room for cards.
fn hand_chips(sheet: &Sheet, fight: &Fight) -> Line<'static> {
    let powers = sheet.powers(fight);
    let board = fight.board();
    let text = ink(INK);
    let muted = ink(INK_MUTED);
    let mut spans = vec![Span::styled("    ", text)];
    for slot in 0..HAND {
        match fight.piles.card(slot) {
            None => spans.push(Span::styled(format!("[{} ·] ", slot + 1), muted)),
            Some(card) => {
                let affordable = card.cost() <= fight.energy;
                let name = match (card_neon(card), affordable) {
                    (Some(neon), true) => lit(neon),
                    (None, true) => ink(INK_DIM),
                    (_, false) => muted,
                };
                let number = chip_number(card, &powers, &board);
                spans.push(Span::styled(
                    format!("[{} ", slot + 1),
                    match affordable {
                        true => lit(Neon::Amber),
                        false => muted,
                    },
                ));
                spans.push(Span::styled(format!("{}{number}", card.name()), name));
                spans.push(Span::styled("] ", muted));
            }
        }
    }
    Line::from(spans)
}

/// An intent as the word the road's preview uses for it.
fn intent_word(intent: Intent) -> &'static str {
    match intent {
        Intent::Hit => "hits",
        Intent::Charge => "gathers",
        Intent::Heavy => "heavy",
        Intent::Noise => "noise",
    }
}

/// What the glyph means to do at the end of this turn, in the foe's
/// column: the number the hand is played against. A heavy pulses with the
/// tick, the one thing on a glyph's scene that moves.
fn intent_spans(fight: &Fight, powers: &Powers, live: bool, tick: u64) -> Vec<Span<'static>> {
    if fight.muted && fight.intent() != Intent::Charge {
        return vec![Span::styled("▸ muted. nothing lands", lit(Neon::White))];
    }
    let (line, style) = match fight.intent() {
        Intent::Hit => (format!("▸ hits for {}", powers.hit), glow(Neon::Red)),
        Intent::Charge => (
            format!("▸ gathering. {} next turn", powers.hit * 2),
            glow(Neon::Amber),
        ),
        Intent::Heavy => (
            format!("▸ comes down for {}", powers.hit * 2),
            match live && (tick / 3).is_multiple_of(2) {
                true => glow(Neon::Red),
                false => lit(Neon::Red),
            },
        ),
        Intent::Noise => (
            format!("▸ noise: {NOISE_CARDS} static cards"),
            glow(Neon::Magenta),
        ),
    };
    vec![Span::styled(line, style)]
}

/// The row under the header: what you carry and the block standing on
/// the left, the glyph's move after this one on the right.
fn stance_row(sheet: &Sheet, fight: Option<&Fight>, inner: usize) -> Line<'static> {
    let dim_text = ink(INK_DIM);
    let text = ink(INK);
    let mut left = vec![
        Span::styled(" ".repeat(2 + PORTRAIT_WIDTH + 3), text),
        Span::styled(weapon_name(sheet).to_string(), text),
        Span::styled(" · ", dim_text),
        Span::styled(armor_name(sheet).to_string(), text),
    ];
    let Some(fight) = fight else {
        return Line::from(left);
    };
    if fight.block > 0 {
        left.push(Span::styled("   block ", dim_text));
        left.push(Span::styled(fight.block.to_string(), lit(Neon::Cyan)));
    }
    let right = format!("then {}", intent_word(fight.intent_in(1)));
    let used = spans_width(&left) + right.chars().count() + 2 + PORTRAIT_WIDTH + 3;
    left.push(Span::styled(" ".repeat(inner.saturating_sub(used)), text));
    left.push(Span::styled(right, ink(INK_MUTED)));
    Line::from(left)
}

/// The scene: a glyph's is centered over the street at a fixed size, the
/// Old Signal's is the whole area.
pub(crate) fn draw_scene(frame: &mut Frame, area: Rect, view: SceneView<'_>) {
    let text = ink(INK);
    let bright = ink(INK_BRIGHT);
    let dim_text = ink(INK_DIM);
    let muted = ink(INK_MUTED);
    let key = lit(Neon::Amber);
    let boss = view.scene.old_signal;
    let live = !view.scene.over;
    let fight = view.sheet.and_then(|sheet| sheet.fight.as_ref());
    let dress = match (boss, fight.is_some_and(|fight| fight.bright)) {
        (true, _) => old_signal_dress(view.scene.over),
        (false, true) => bright_dress(),
        (false, false) => glyph_dress(),
    };

    // Fixed height: the header, the stance row, the rule, the log, the
    // hand, the keys. A glyph's box gives up log rows on a short
    // terminal, then the cards themselves (the hand becomes one row of
    // chips); the Old Signal's takes every row there is.
    let frame_rows = 1 + PORTRAIT_HEIGHT + 1 + 1 + 1 + 1 + 1;
    let room = usize::from(area.height.saturating_sub(3));
    let cards = room >= frame_rows + HAND_ROWS + LOG_ROWS_MIN;
    let hand_rows = match cards {
        true => HAND_ROWS,
        false => HAND_ROWS_COMPACT,
    };
    let fixed_rows = frame_rows + hand_rows;
    let rect = match boss {
        true => {
            // One column narrower than the area, leaning left or right
            // from tick to tick while it broadcasts: the shudder.
            let lean = match live {
                true => (mix(view.tick) % 2) as u16,
                false => 0,
            };
            Rect {
                x: area.x + lean,
                y: area.y,
                width: area.width.saturating_sub(1),
                height: area.height,
            }
        }
        false => {
            let log_rows = LOG_ROWS
                .min(room.saturating_sub(fixed_rows))
                .max(LOG_ROWS_MIN);
            let width = (INNER + 2).min(usize::from(area.width).saturating_sub(2)) as u16;
            let height = ((fixed_rows + log_rows) as u16 + 2).min(area.height.saturating_sub(1));
            Rect {
                x: area.x + (area.width.saturating_sub(width)) / 2,
                y: area.y + (area.height.saturating_sub(height)) / 2,
                width,
                height,
            }
        }
    };
    let inner = usize::from(rect.width).saturating_sub(2);
    let log_rows = usize::from(rect.height)
        .saturating_sub(2)
        .saturating_sub(fixed_rows)
        .max(LOG_ROWS_MIN);

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(fixed_rows + log_rows);
    lines.push(Line::default());
    match view.sheet {
        Some(sheet) => {
            let intent = match (fight, live) {
                (Some(fight), true) => {
                    intent_spans(fight, &sheet.powers(fight), !view.scene.waiting, view.tick)
                }
                (Some(_), false) | (None, _) => Vec::new(),
            };
            lines.extend(header(sheet, view.look, view.own_username, &dress, intent));
            lines.push(stance_row(sheet, fight.filter(|_| live), inner));
        }
        None => {
            lines.push(Line::from(Span::styled("  the static parts.", dim_text)));
            lines.push(Line::default());
            lines.push(Line::default());
            lines.push(Line::default());
        }
    }
    lines.push(Line::from(Span::styled(
        format!("  {}", "─".repeat(inner.saturating_sub(4))),
        dress.rule,
    )));

    // The exchange, newest at the bottom: the latest answer bright with a
    // marker, everything before it dimmed. The Old Signal's empty rows
    // are static, not blank.
    let shown: Vec<&String> = view.scene.lines.iter().rev().take(log_rows).collect();
    let shown_count = shown.len();
    for row in shown_count..log_rows {
        match boss {
            true => {
                let seed = match live {
                    true => mix(view.tick) ^ row as u64,
                    false => row as u64,
                };
                lines.push(noise_line(inner, seed, !live));
            }
            false => lines.push(Line::default()),
        }
    }
    let total = view.scene.lines.len();
    for (i, line) in shown.into_iter().rev().enumerate() {
        let index = total - shown_count + i;
        let latest = index + view.scene.latest >= total;
        let body = truncate(line, inner.saturating_sub(6));
        let spans = match latest {
            true => vec![Span::styled("  ▸ ", key), Span::styled(body, bright)],
            false => vec![Span::styled("    ", text), Span::styled(body, dim_text)],
        };
        lines.push(Line::from(spans));
    }
    lines.push(Line::default());

    // The hand, while there is a fight to play it in. Once it is over
    // the rows stay (the box never jumps) and go quiet.
    match (view.sheet, fight.filter(|_| live), cards) {
        (Some(sheet), Some(fight), true) => {
            lines.push(hand_readout(fight));
            lines.extend(hand_cards(sheet, fight));
        }
        (Some(sheet), Some(fight), false) => {
            lines.push(hand_readout(fight));
            lines.push(hand_chips(sheet, fight));
        }
        (None, _, _) | (_, None, _) => {
            for _ in 0..hand_rows {
                lines.push(Line::default());
            }
        }
    }

    let keys = match (view.scene.over, view.scene.waiting) {
        (true, _) => vec![
            Span::styled("    [Enter] ", key),
            Span::styled("back to the road", text),
        ],
        (false, true) => vec![Span::styled("    the static is deciding.", muted)],
        (false, false) => vec![
            Span::styled("    [1-5] ", key),
            Span::styled("play", text),
            Span::styled("   [e] ", key),
            Span::styled("end turn", text),
            Span::styled("   [a] ", key),
            Span::styled("auto turn", text),
            Span::styled("   [r] ", key),
            Span::styled("run", text),
            Span::styled("  esc runs too", dim_text),
        ],
    };
    lines.push(Line::from(keys));

    let budget = match view.sheet {
        Some(sheet) => format!(
            " step {}/{} · bits {} ",
            RATIONS_PER_DAY - sheet.rations_left,
            RATIONS_PER_DAY,
            sheet.bits
        ),
        None => String::new(),
    };
    // The Old Signal's box shifts from under the street: the area is
    // painted night whole, so the column it leans away from is sky, not
    // a strip of street showing through.
    match boss {
        true => frame.render_widget(Block::default().style(text), area),
        false => frame.render_widget(Clear, rect),
    }
    frame.render_widget(
        Paragraph::new(lines).style(text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(dress.frame)
                .title(Span::styled(dress.title_text, dress.title))
                .title_bottom(Span::styled(budget, dim_text)),
        ),
        rect,
    );
    if boss && live {
        tear(frame.buffer_mut(), rect, view.tick);
    }
}

pub(crate) struct PickerView<'a> {
    pub sheet: Option<&'a Sheet>,
    pub picker: &'a Picker,
    pub look: Option<&'a Look>,
    pub own_username: &'a str,
    /// The city's animation clock: the bright nodes flicker on it.
    pub tick: u64,
    /// The last step's word (`FightSession::till`): what the rest mended,
    /// what the cache held, why the row refused.
    pub word: Option<&'a str>,
}

fn threat_span(threat: Option<Threat>) -> Span<'static> {
    match threat {
        Some(threat) => {
            let style = match threat {
                Threat::Easy => lit(Neon::Green),
                Threat::Even => lit(Neon::Cyan),
                Threat::Risky => lit(Neon::Amber),
                Threat::Grim => lit(Neon::Red),
            };
            Span::styled(threat.word().to_string(), style)
        }
        None => Span::styled("…", ink(INK_MUTED)),
    }
}

/// A fight waiting on the road, as the panel shows it under the map: the
/// face, and three rows of numbers beside it.
struct Offer {
    kind: &'static FoeKind,
    tier: FoeTier,
    /// `None` for the Old Signal, which has no level.
    level: Option<i32>,
    threat: Option<Threat>,
    note: &'static str,
    boss: bool,
    /// The bright glyph: named so, in amber, and it leaves a crystal.
    bright: bool,
}

/// A glyph's whole mind, as the road previews it: its pattern, once
/// round, in the order it comes.
fn pattern_words(kind: &FoeKind) -> String {
    kind.pattern
        .iter()
        .map(|intent| intent_word(*intent))
        .collect::<Vec<_>>()
        .join(" > ")
}

fn offer_lines(offer: &Offer, sheet: &Sheet) -> Vec<Line<'static>> {
    let text = ink(INK);
    let dim_text = ink(INK_DIM);
    let key = lit(Neon::Amber);
    let (face, name) = match (offer.boss, offer.bright) {
        (true, _) => (glow(Neon::Red), lit(Neon::Red)),
        (false, true) => (glow(Neon::Amber), lit(Neon::Amber)),
        (false, false) => (glow(Neon::Cyan), lit(Neon::Cyan)),
    };
    let pay = match (offer.boss, offer.bright) {
        (true, _) => "pays nothing on the sheet. a mark, and the climb over".to_string(),
        (false, true) => format!(
            "pays {} bits · {} exp · a crystal",
            offer.tier.bits, offer.tier.exp
        ),
        (false, false) => format!("pays {} bits · {} exp", offer.tier.bits, offer.tier.exp),
    };
    let head = {
        let label = match offer.bright {
            true => format!("bright {}", offer.kind.name),
            false => offer.kind.name.to_string(),
        };
        let mut row = vec![Span::styled(label, name)];
        if let Some(level) = offer.level {
            row.push(Span::styled(format!("  lv {level}"), text));
        }
        row
    };
    let width = INNER.saturating_sub(4 + 5 + 3 + 8);
    // The note takes what the name leaves, so the threat word keeps its
    // column whatever the glyph is called.
    let head = {
        let mut row = head;
        let room = width.saturating_sub(spans_width(&row) + 3);
        row.push(Span::styled(
            format!("   {}", truncate(offer.note, room)),
            dim_text,
        ));
        row
    };
    let hit = data::RULES.hit(offer.tier.attack, sheet.defense());
    let numbers = format!("signal {} · hits for {hit} · it ", offer.tier.signal);
    let moves = truncate(
        &pattern_words(offer.kind),
        width.saturating_sub(numbers.chars().count()),
    );
    let rows: [Vec<Span<'static>>; 3] = [
        head,
        vec![
            Span::styled(numbers, dim_text),
            Span::styled(moves, ink(INK_MUTED)),
        ],
        vec![Span::styled(pay, text)],
    ];
    rows.into_iter()
        .enumerate()
        .map(|(i, row)| {
            let mut spans = vec![match i {
                0 => Span::styled("  ▸ ", key),
                _ => Span::styled("    ", text),
            }];
            spans.push(Span::styled(offer.kind.portrait[i].to_string(), face));
            spans.push(Span::styled("   ", text));
            let threat = match i {
                0 => vec![threat_span(offer.threat)],
                _ => Vec::new(),
            };
            spans.extend(pad(row, width, false));
            spans.extend(threat);
            Line::from(spans)
        })
        .collect()
}

/// Why the road is over for the day, in the words the row would refuse a
/// step with; `None` while there is road to walk.
fn shut_reason(sheet: &Sheet) -> Option<&'static str> {
    match sheet.shut() {
        Some(Shut::SignalDown) => {
            Some("your signal is down. nothing in there can see you until tomorrow.")
        }
        Some(Shut::NoRations) => Some("the road is walked. the static will keep until the roll."),
        None => None,
    }
}

/// Columns between one step of the map and the next.
const STEP_PITCH: usize = 7;
/// The first step's column: the map sits centered in the panel.
const MAP_X: usize = (INNER - (STEPS - 1) * STEP_PITCH) / 2;
/// Steps ahead of the runner that still show their color before the road
/// fades into the dark, the way the street does.
const SEE_AHEAD: usize = 3;

type MapCell = (char, Style);

fn node_x(step: usize) -> usize {
    MAP_X + step * STEP_PITCH
}

/// A node's mark on the map. The bright one burns unevenly: it is the
/// only thing on the road that moves.
fn node_char(node: Node, tick: u64) -> char {
    match node {
        Node::Glyph => '▚',
        Node::Bright => match (tick / 4).is_multiple_of(2) {
            true => '▓',
            false => '▒',
        },
        Node::Rest => '+',
        Node::Cache => '$',
    }
}

fn node_neon(node: Node) -> Neon {
    match node {
        Node::Glyph => Neon::Cyan,
        Node::Bright => Neon::Amber,
        Node::Rest => Neon::Green,
        Node::Cache => Neon::White,
    }
}

/// A step already taken, lit by how it went.
fn walked_cell(node: Node, mark: Mark, tick: u64) -> MapCell {
    match mark {
        Mark::Fighting => (node_char(node, tick), lit(Neon::White)),
        Mark::Won => (node_char(node, tick), lit(Neon::Green)),
        Mark::BrightWon => ('▓', lit(Neon::Amber)),
        Mark::Ran => (node_char(node, tick), dim(Neon::Amber)),
        Mark::Fell => ('░', lit(Neon::Red)),
        Mark::Cleared => ('+', lit(Neon::Cyan)),
        Mark::Cached => ('$', lit(Neon::White)),
    }
}

/// The run of road between `step` and the one after it, from `from` to
/// `to`: straight along a lane, or bent through the gap to the lane
/// beside it. `pitch` is rows between lanes.
fn connect(
    grid: &mut [Vec<MapCell>],
    step: usize,
    from: usize,
    to: usize,
    pitch: usize,
    style: Style,
    dashed: bool,
) {
    let (flat, upright) = match dashed {
        true => ('╌', '╎'),
        false => ('─', '│'),
    };
    let x0 = node_x(step);
    let bend = x0 + STEP_PITCH / 2 + 1;
    let (ya, yb) = (from * pitch, to * pitch);
    if from == to {
        for cell in &mut grid[ya][x0 + 1..x0 + STEP_PITCH] {
            *cell = (flat, style);
        }
        return;
    }
    let (out, back) = match to > from {
        true => ('╮', '╰'),
        false => ('╯', '╭'),
    };
    for cell in &mut grid[ya][x0 + 1..bend] {
        *cell = (flat, style);
    }
    grid[ya][bend] = (out, style);
    for row in grid.iter_mut().take(ya.max(yb)).skip(ya.min(yb) + 1) {
        row[bend] = (upright, style);
    }
    grid[yb][bend] = (back, style);
    for cell in &mut grid[yb][bend + 1..x0 + STEP_PITCH] {
        *cell = (flat, style);
    }
}

/// A row of map cells as one line, same-style runs as one span each.
fn map_line(row: Vec<MapCell>) -> Line<'static> {
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

/// The day's road as a map: ten steps left to right, three lanes top to
/// bottom, the same picture for every runner. What is behind you is the
/// lane you walked, lit by how each step went, joined like a line on a
/// transit map; the step in front is lit, the lanes you can reach from
/// where you stand brighter than the one you cannot, the cursor's in
/// brackets with the way to it dashed in; past that the road fades into
/// the dark the way the street does, except the bright ones, which carry.
/// `pitch` is rows between lanes: two when the terminal has them.
fn road_map(sheet: &Sheet, cursor: u8, tick: u64, pitch: usize) -> Vec<Line<'static>> {
    let road = sheet.todays_road();
    let walked = sheet.road.path.len();
    let over = sheet.road_over();
    let blank = (' ', ink(INK));
    let mut grid = vec![vec![blank; INNER]; (LANES - 1) * pitch + 1];
    // The lanes themselves, faint: three rails for the nodes to sit on.
    let rail = ink(scale(INK_MUTED, 0.6));
    for lane in 0..LANES {
        for cell in &mut grid[lane * pitch][node_x(0)..=node_x(STEPS - 1)] {
            *cell = ('·', rail);
        }
    }
    let wire = dim(Neon::Cyan);
    for step in 1..walked {
        connect(
            &mut grid,
            step - 1,
            usize::from(sheet.road.path[step - 1].lane),
            usize::from(sheet.road.path[step].lane),
            pitch,
            wire,
            false,
        );
    }
    for (step, lanes) in road.steps.iter().enumerate() {
        for (lane, node) in lanes.iter().enumerate() {
            let cell = match sheet.road.path.get(step) {
                Some(trace) if usize::from(trace.lane) == lane => {
                    walked_cell(*node, trace.mark, tick)
                }
                // A lane walked past: still on the map, out of the light.
                Some(_) => (node_char(*node, 0), ink(INK_MUTED)),
                None => {
                    let ahead = step - walked;
                    let neon = node_neon(*node);
                    let style = match (ahead, *node) {
                        (0, _) if !over && sheet.road.reaches(lane as u8) => lit(neon),
                        (0, _) => dim(neon),
                        (_, Node::Bright) => glow(neon),
                        (ahead, _) if ahead <= SEE_AHEAD => dim(neon),
                        (_, Node::Glyph | Node::Rest | Node::Cache) => {
                            ink(scale(neon_rgb(neon), 0.3))
                        }
                    };
                    (node_char(*node, tick), style)
                }
            };
            grid[lane * pitch][node_x(step)] = cell;
        }
    }
    if !over && walked < STEPS {
        let way = dim(Neon::Amber);
        let x = node_x(walked);
        let y = usize::from(cursor) * pitch;
        match sheet.road.lane() {
            Some(from) => connect(
                &mut grid,
                walked - 1,
                usize::from(from),
                usize::from(cursor),
                pitch,
                way,
                true,
            ),
            // Nothing walked yet: the way in is the screen itself.
            None => {
                grid[y][x - 3] = ('▸', way);
            }
        }
        grid[y][x - 1] = ('[', lit(Neon::Amber));
        grid[y][x + 1] = (']', lit(Neon::Amber));
    }
    grid.into_iter().map(map_line).collect()
}

/// The step numbers over the map: behind you muted, the one in front lit.
fn step_numbers(sheet: &Sheet) -> Line<'static> {
    let walked = sheet.road.path.len();
    let over = sheet.road_over();
    let mut spans = Vec::new();
    let mut at = 0;
    for step in 0..STEPS {
        let label = (step + 1).to_string();
        let x = node_x(step) + 1 - label.len();
        spans.push(Span::styled(" ".repeat(x - at), ink(INK)));
        at = x + label.len();
        let style = match step.cmp(&walked) {
            std::cmp::Ordering::Less => ink(INK_MUTED),
            std::cmp::Ordering::Equal if !over => lit(Neon::Amber),
            std::cmp::Ordering::Equal | std::cmp::Ordering::Greater => ink(INK_DIM),
        };
        spans.push(Span::styled(label, style));
    }
    Line::from(spans)
}

/// The rule under the map, with the legend set into it: four marks, and
/// that is the whole road.
fn legend_rule() -> Line<'static> {
    let rule = dim(Neon::Cyan);
    let dim_text = ink(INK_DIM);
    let entries = [
        (Node::Glyph, "glyph"),
        (Node::Bright, "bright"),
        (Node::Rest, "rest"),
        (Node::Cache, "cache"),
    ];
    let mut spans = vec![Span::styled("  ── ", rule)];
    for (node, name) in entries {
        spans.push(Span::styled(
            node_char(node, 0).to_string(),
            glow(node_neon(node)),
        ));
        spans.push(Span::styled(format!(" {name}  "), dim_text));
    }
    let used = spans_width(&spans);
    spans.push(Span::styled(
        "─".repeat((INNER - 2).saturating_sub(used)),
        rule,
    ));
    Line::from(spans)
}

/// Three rows on what waits under the cursor, and the key row for it.
fn node_lines(
    sheet: &Sheet,
    picker: &Picker,
    node: Node,
) -> (Vec<Line<'static>>, Vec<Span<'static>>) {
    let text = ink(INK);
    let dim_text = ink(INK_DIM);
    let muted = ink(INK_MUTED);
    let key = lit(Neon::Amber);
    let mut keys = vec![Span::styled("    [↑↓] ", key), Span::styled("lane", text)];
    let lines = match node {
        Node::Glyph => {
            let offer = match sheet.signal_hears() {
                true => Offer {
                    kind: &OLD_SIGNAL,
                    tier: data::RULES.old_signal,
                    level: None,
                    threat: picker.fair,
                    note: "the bottom of the city",
                    boss: true,
                    bright: false,
                },
                false => {
                    let (index, kind, tier) = data::foe_for_level(sheet.level);
                    Offer {
                        kind,
                        tier,
                        level: Some(index as i32 + 1),
                        threat: picker.fair,
                        note: "the glyph of your level",
                        boss: false,
                        bright: false,
                    }
                }
            };
            keys.extend([
                Span::styled("   [Enter] ", key),
                Span::styled("fight", text),
            ]);
            if let Some((_, kind, _)) = data::lower_foe_for_level(sheet.level) {
                keys.extend([
                    Span::styled("   [g] ", key),
                    Span::styled(format!("{}, half pay ", kind.name), text),
                    threat_span(picker.lower),
                ]);
            }
            offer_lines(&offer, sheet)
        }
        Node::Bright => {
            let (index, kind, tier) = data::bright_foe_for_level(sheet.level);
            keys.extend([
                Span::styled("   [Enter] ", key),
                Span::styled("fight", text),
            ]);
            offer_lines(
                &Offer {
                    kind,
                    tier,
                    level: Some(index as i32 + 1),
                    threat: picker.bright,
                    note: "it carries a crystal",
                    boss: false,
                    bright: true,
                },
                sheet,
            )
        }
        Node::Rest => {
            keys.extend([
                Span::styled("   [Enter] ", key),
                Span::styled("clear the deck", text),
            ]);
            vec![
                Line::from(vec![
                    Span::styled("  ▸ ", key),
                    Span::styled("a doorway out of the rain", lit(Neon::Green)),
                    Span::styled("   shake the static out", dim_text),
                ]),
                Line::from(vec![
                    Span::styled("      clear   ", text),
                    match sheet.road.static_cards {
                        0 => Span::styled("your deck is clean", muted),
                        1 => Span::styled("1 static card out of your deck", dim_text),
                        n => Span::styled(format!("{n} static cards out of your deck"), dim_text),
                    },
                ]),
                Line::default(),
            ]
        }
        Node::Cache => {
            keys.extend([
                Span::styled("   [Enter] ", key),
                Span::styled("take it", text),
            ]);
            vec![
                Line::from(vec![
                    Span::styled("  ▸ ", key),
                    Span::styled("a cache", lit(Neon::White)),
                    Span::styled("   somebody left it, or lost it", dim_text),
                ]),
                Line::from(vec![
                    Span::styled("      take    ", text),
                    Span::styled(format!("{} bits", data::RULES.cache(sheet.level)), dim_text),
                    match sheet.debt {
                        0 => Span::styled("", dim_text),
                        _ => Span::styled("   the machine takes its half", dim(Neon::Red)),
                    },
                ]),
                Line::default(),
            ]
        }
    };
    keys.push(Span::styled("  esc back", dim_text));
    (lines, keys)
}

/// A draft owed, in the three rows the node under the cursor would have:
/// the level and what the pick replaces, then the two cards, a key each.
fn draft_lines(draft: &Draft) -> (Vec<Line<'static>>, Vec<Span<'static>>) {
    let text = ink(INK);
    let dim_text = ink(INK_DIM);
    let key = lit(Neon::Amber);
    let mut lines = vec![Line::from(vec![
        Span::styled("  ▸ ", key),
        Span::styled("a new card", lit(Neon::Amber)),
        Span::styled(
            format!(
                "   level {}. it takes the place of a {}, until the mark",
                draft.level,
                draft.replaces.name()
            ),
            dim_text,
        ),
    ])];
    for (option, card) in draft.options.iter().enumerate() {
        let name = match card_neon(*card) {
            Some(neon) => lit(neon),
            None => text,
        };
        lines.push(Line::from(vec![
            Span::styled(format!("      [{}] ", option + 1), key),
            Span::styled(format!("{:<8}", card.name()), name),
            Span::styled(card.rule().to_string(), text),
        ]));
    }
    let keys = vec![
        Span::styled("    [1] [2] ", key),
        Span::styled("take one", text),
        Span::styled("   the road waits on it", dim_text),
        Span::styled("  esc back", dim_text),
    ];
    (lines, keys)
}

/// The road over the street: the sheet, the day's map with the run on
/// it, what waits under the cursor (or the draft owed, ahead of any
/// step), the keys. Fixed width like the scene;
/// the lanes spread a row apart when the terminal has the rows.
pub(crate) fn draw_picker(frame: &mut Frame, area: Rect, view: PickerView<'_>) {
    let text = ink(INK);
    let dim_text = ink(INK_DIM);
    let muted = ink(INK_MUTED);
    let key = lit(Neon::Amber);
    let frame_style = glow(Neon::Cyan);
    // Everything but the map: the sheet, the rules, the numbers, three
    // rows on the node, the word, the keys.
    let fixed = 1 + PORTRAIT_HEIGHT + 1 + 1 + 1 + 1 + 3 + 1 + 1 + 1;
    let roomy = usize::from(area.height) >= fixed + ((LANES - 1) * 2 + 1) + 3 + 3;
    let pitch = match roomy {
        true => 2,
        false => 1,
    };
    let mut lines: Vec<Line<'static>> = vec![Line::default()];
    let mut title_right = String::new();
    let mut drafted = String::new();

    match view.sheet {
        None => {
            lines.push(Line::from(Span::styled(
                "  the static parts. your sheet has not come down the wire yet.",
                muted,
            )));
            lines.push(Line::default());
            lines.push(Line::from(vec![
                Span::styled("    [Enter] ", key),
                Span::styled("step in", text),
                Span::styled("   esc back to the street", dim_text),
            ]));
        }
        Some(sheet) => {
            title_right = format!(
                " {} · road #{} ",
                sheet.day.format("%a %-d %b").to_string().to_lowercase(),
                puzzle_number(sheet.day)
            );
            if !sheet.cards.is_empty() {
                let names: Vec<&str> = sheet.cards.iter().map(|card| card.name()).collect();
                drafted = format!(" drafted: {} ", names.join(" · "));
            }
            let face = runner_rows(sheet, view.look);
            let exp = match (
                sheet.signal_hears(),
                data::exp_to_advance(sheet.level, sheet.marks),
            ) {
                (true, _) => format!("exp {}  the Old Signal hears you", sheet.exp),
                (false, Some(need)) => format!("exp {}/{need}", sheet.exp),
                (false, None) => format!("exp {}/{}", sheet.exp, data::exp_to_seek(sheet.marks)),
            };
            let stats: [Vec<Span<'static>>; 3] = [
                vec![
                    Span::styled(
                        view.own_username.to_string(),
                        ink(INK_BRIGHT).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(format!("  lv {}", sheet.level), text),
                ],
                {
                    let mut row = vec![Span::styled("signal ", dim_text)];
                    row.extend(bar_spans(
                        sheet.signal,
                        sheet.max_signal(),
                        glow(Neon::Green),
                        false,
                    ));
                    row.push(Span::styled(
                        format!(" {}/{}", sheet.signal, sheet.max_signal()),
                        text,
                    ));
                    row
                },
                vec![Span::styled(
                    format!(
                        "attack {}  defense {}   {} · {}",
                        sheet.attack(),
                        sheet.defense(),
                        weapon_name(sheet),
                        armor_name(sheet)
                    ),
                    dim_text,
                )],
            ];
            for (row, stats) in face.into_iter().zip(stats) {
                let mut spans = vec![Span::styled("  ", text)];
                spans.extend(row);
                spans.push(Span::styled("   ", text));
                spans.extend(stats);
                lines.push(Line::from(spans));
            }
            let mut budget = vec![
                Span::styled(" ".repeat(2 + PORTRAIT_WIDTH + 3), text),
                Span::styled(
                    format!(
                        "rations {}/{RATIONS_PER_DAY} · bits {} · crystals {} · {exp}",
                        sheet.rations_left, sheet.bits, sheet.crystals
                    ),
                    dim_text,
                ),
            ];
            match sheet.road.static_cards {
                0 => {}
                cards => budget.push(Span::styled(
                    format!(" · static {cards}"),
                    dim(Neon::Magenta),
                )),
            }
            if sheet.debt > 0 {
                budget.push(Span::styled(
                    format!(" · owed {}", sheet.debt),
                    dim(Neon::Red),
                ));
            }
            lines.push(Line::from(budget));
            lines.push(Line::from(Span::styled(
                format!("  {}", "─".repeat(INNER.saturating_sub(4))),
                dim(Neon::Cyan),
            )));
            if roomy {
                lines.push(Line::default());
            }
            lines.push(step_numbers(sheet));
            if roomy {
                lines.push(Line::default());
            }
            lines.extend(road_map(sheet, view.picker.lane, view.tick, pitch));
            if roomy {
                lines.push(Line::default());
            }
            lines.push(legend_rule());

            let word = Line::from(match view.word {
                Some(word) => vec![
                    Span::styled("    ", text),
                    Span::styled(truncate(word, INNER.saturating_sub(6)), ink(INK_BRIGHT)),
                ],
                None => Vec::new(),
            });
            let reason = shut_reason(sheet);
            let ahead = match reason {
                Some(_) => None,
                None => sheet.node_ahead(view.picker.lane),
            };
            match (sheet.draft(), ahead) {
                (Some(draft), _) => {
                    let (detail, keys) = draft_lines(draft);
                    lines.extend(detail);
                    lines.push(word);
                    lines.push(Line::from(keys));
                }
                (None, None) => {
                    // The day's card, in words: the map above is the
                    // picture of it.
                    let glyphs = match sheet.kills_today {
                        1 => "1 glyph down".to_string(),
                        n => format!("{n} glyphs down"),
                    };
                    let reason = reason
                        .unwrap_or("the road is walked. the static will keep until the roll.");
                    lines.push(Line::from(Span::styled(format!("    {reason}"), text)));
                    lines.push(Line::from(Span::styled(
                        format!(
                            "    {glyphs} · signal {}/{} · step {} of {RATIONS_PER_DAY}",
                            sheet.signal,
                            sheet.max_signal(),
                            sheet.road.path.len()
                        ),
                        dim_text,
                    )));
                    lines.push(Line::default());
                    lines.push(word);
                    let mut keys = Vec::new();
                    if !sheet.road.path.is_empty() {
                        keys.extend([
                            Span::styled("    [s] ", key),
                            Span::styled("copy the day's card", text),
                        ]);
                    }
                    keys.extend([
                        Span::styled("    [Enter] ", key),
                        Span::styled("back to the street", text),
                    ]);
                    lines.push(Line::from(keys));
                }
                (None, Some(node)) => {
                    let (detail, keys) = node_lines(sheet, view.picker, node);
                    lines.extend(detail);
                    lines.push(word);
                    lines.push(Line::from(keys));
                }
            }
        }
    }
    lines.push(Line::default());

    let width = (INNER + 2).min(usize::from(area.width).saturating_sub(2)) as u16;
    let height = (lines.len() as u16 + 2).min(area.height.saturating_sub(1));
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).style(text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(frame_style)
                .title(Span::styled(" the road ", lit(Neon::Cyan)))
                .title(Line::from(Span::styled(title_right, dim_text)).right_aligned())
                .title_bottom(Span::styled(drafted, dim_text)),
        ),
        rect,
    );
}

/// The sheet strip, top-right: the day's budget and the kit at a glance.
pub(crate) fn draw_strip(frame: &mut Frame, area: Rect, sheet: &Sheet) {
    let text = ink(INK);
    let dim_text = ink(INK_DIM);
    let number = glow(Neon::Amber);
    let mut spans = vec![
        Span::styled("lv ", dim_text),
        Span::styled(sheet.level.to_string(), number),
        Span::styled("  signal ", dim_text),
    ];
    match sheet.is_down() {
        true => spans.push(Span::styled("down", dim(Neon::Red))),
        false => spans.push(Span::styled(
            format!("{}/{}", sheet.signal, sheet.max_signal()),
            text,
        )),
    }
    spans.push(Span::styled("  rations ", dim_text));
    spans.push(Span::styled(
        format!("{}/{}", sheet.rations_left, super::data::RATIONS_PER_DAY),
        text,
    ));
    spans.push(Span::styled("  bits ", dim_text));
    spans.push(Span::styled(sheet.bits.to_string(), number));
    if sheet.debt > 0 {
        spans.push(Span::styled("  owed ", dim_text));
        spans.push(Span::styled(sheet.debt.to_string(), dim(Neon::Red)));
    }
    if sheet.crystals > 0 {
        spans.push(Span::styled("  crystals ", dim_text));
        spans.push(Span::styled(sheet.crystals.to_string(), lit(Neon::Cyan)));
    }
    if let Some(drink) = sheet.drink {
        spans.push(Span::styled("  glass ", dim_text));
        spans.push(Span::styled(drink.name().to_string(), text));
    }
    spans.push(Span::styled("  weapon ", dim_text));
    spans.push(Span::styled(weapon_name(sheet).to_string(), text));
    spans.push(Span::styled("  armor ", dim_text));
    spans.push(Span::styled(armor_name(sheet).to_string(), text));
    let line = Line::from(spans);
    let width = (line.width() + 2).min(usize::from(area.width).saturating_sub(2)) as u16;
    if area.height < 2 {
        return;
    }
    let rect = Rect {
        x: area.x + area.width.saturating_sub(width + 1),
        y: area.y,
        width,
        height: 1,
    };
    frame.render_widget(Clear, rect);
    frame.render_widget(Paragraph::new(line).style(text), rect);
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod ui_test;
