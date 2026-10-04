//! The fight scene: a panel over the street in the city's own palette.
//! Two portraits facing, the runner's and the glyph's, each losing cells
//! to static in proportion to its missing signal (GAME.md, "Signal
//! corrupts the look": health as something you see, not a number); the
//! exchange, line by line, in the announcer's voice; three keys. Pure:
//! every frame is a function of the mirror, the scene, and the tick.
//!
//! The Old Signal gets the screen (GAME.md, "Diegetic spectacle": a boss
//! whose presence tears the frame). Its scene is the whole city area in
//! red, the empty rows full of static, the border losing cells to static
//! and the box shuddering a column from tick to tick while it
//! broadcasts; once the fight is over, everything holds still.
//!
//! Before the scene, the picker: the runner's sheet on top, then the
//! glyphs on offer, the bright one on a step it waits behind, the one of
//! your level, and the one below, each with its face, its numbers, its
//! pay, and the threat word the sim read for it. The warning LoGD's master gave, before the fight instead of after.
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

use super::data::{self, FOES, FoeKind, FoeTier, OLD_SIGNAL, OLD_SIGNAL_TIER, RATIONS_PER_DAY};
use super::session::{Picker, Scene};
use super::sim::Threat;
use super::state::{Fight, Pick, Quarry, Sheet, Shut, Slot};
use crate::app::deadchannel::city::map::Neon;
use crate::app::deadchannel::city::ui::{
    INK, INK_BRIGHT, INK_DIM, INK_MUTED, dim, glow, ink, lit, mix, tint_rgb,
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
const LOG_ROWS: usize = 8;
/// The fewest log rows a short terminal may squeeze the scene to.
const LOG_ROWS_MIN: usize = 3;
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

/// What the runner carries, in words: the row under the header, aligned
/// under the stats column. Bare hands and street clothes are named as
/// such, so a fresh runner reads what the armorer would change.
fn gear_row(sheet: &Sheet) -> Line<'static> {
    let dim_text = ink(INK_DIM);
    let text = ink(INK);
    Line::from(vec![
        Span::styled(" ".repeat(2 + PORTRAIT_WIDTH + 3), text),
        Span::styled(weapon_name(sheet).to_string(), text),
        Span::styled(" · ", dim_text),
        Span::styled(armor_name(sheet).to_string(), text),
    ])
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
/// on the right, the two bars facing each other.
fn header(sheet: &Sheet, look: Option<&Look>, username: &str, dress: &Dress) -> Vec<Line<'static>> {
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
            vec![Span::styled(
                format!("attack {}  defense {}", fight.foe_attack, fight.foe_defense),
                dim_text,
            )],
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

/// The scene: a glyph's is centered over the street at a fixed size, the
/// Old Signal's is the whole area.
pub(crate) fn draw_scene(frame: &mut Frame, area: Rect, view: SceneView<'_>) {
    let text = ink(INK);
    let bright = ink(INK_BRIGHT);
    let dim_text = ink(INK_DIM);
    let muted = ink(INK_MUTED);
    let key = lit(Neon::Amber);
    let boss = view.scene.old_signal;
    let live = boss && !view.scene.over;
    let dress = match boss {
        true => old_signal_dress(view.scene.over),
        false => glyph_dress(),
    };

    // Fixed height: the header, the rule, the log, the keys, each with
    // room to breathe. A glyph's box gives up log rows on a short
    // terminal, nothing else; the Old Signal's takes every row there is.
    let fixed_rows = 1 + PORTRAIT_HEIGHT + 1 + 1 + 1 + 1 + 1 + 1;
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
            let room = usize::from(area.height.saturating_sub(3));
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
            lines.extend(header(sheet, view.look, view.own_username, &dress));
            lines.push(gear_row(sheet));
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
    lines.push(Line::default());

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

    let keys = match (view.scene.over, view.scene.waiting) {
        (true, _) => vec![
            Span::styled("    [Enter] ", key),
            Span::styled("back to the street", text),
        ],
        (false, true) => vec![Span::styled("    the static is deciding.", muted)],
        (false, false) => vec![
            Span::styled("    [a] ", key),
            Span::styled("attack", text),
            Span::styled("        [r] ", key),
            Span::styled("run", text),
            Span::styled("  esc runs too", dim_text),
        ],
    };
    lines.push(Line::from(keys));
    lines.push(Line::default());

    let budget = match view.sheet {
        Some(sheet) => format!(
            " rations {}/{} · bits {} ",
            sheet.rations_left,
            super::data::RATIONS_PER_DAY,
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
    if live {
        tear(frame.buffer_mut(), rect, view.tick);
    }
}

pub(crate) struct PickerView<'a> {
    pub sheet: Option<&'a Sheet>,
    pub picker: &'a Picker,
    pub look: Option<&'a Look>,
    pub own_username: &'a str,
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

/// One offer in the picker: the key, the face, and three rows of numbers
/// beside it.
struct Offer {
    key: &'static str,
    pick: Pick,
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

fn offer_lines(offer: &Offer, cursor: Pick) -> Vec<Line<'static>> {
    let text = ink(INK);
    let dim_text = ink(INK_DIM);
    let key = lit(Neon::Amber);
    let (face, name) = match (offer.boss, offer.bright) {
        (true, _) => (glow(Neon::Red), lit(Neon::Red)),
        (false, true) => (glow(Neon::Amber), lit(Neon::Amber)),
        (false, false) => (glow(Neon::Cyan), lit(Neon::Cyan)),
    };
    let marker = match offer.pick == cursor {
        true => Span::styled("  ▸ ", key),
        false => Span::styled("    ", text),
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
    let rows: [Vec<Span<'static>>; 3] = [
        head,
        vec![Span::styled(
            format!(
                "attack {}  defense {}  signal {}",
                offer.tier.attack, offer.tier.defense, offer.tier.signal
            ),
            dim_text,
        )],
        vec![
            Span::styled(pay, text),
            Span::styled(format!("   {}", offer.note), dim_text),
        ],
    ];
    rows.into_iter()
        .enumerate()
        .map(|(i, row)| {
            let mut spans = vec![match i {
                0 => marker.clone(),
                _ => Span::styled("    ", text),
            }];
            spans.push(match i {
                0 => Span::styled(format!("[{}]  ", offer.key), key),
                _ => Span::styled("     ", text),
            });
            spans.push(Span::styled(offer.kind.portrait[i].to_string(), face));
            spans.push(Span::styled("   ", text));
            let threat = match i {
                0 => vec![threat_span(offer.threat)],
                _ => Vec::new(),
            };
            spans.extend(pad(row, INNER.saturating_sub(4 + 5 + 5 + 3 + 8), false));
            spans.extend(threat);
            Line::from(spans)
        })
        .collect()
}

/// Why the picker has nothing on offer, in the words the row would
/// refuse a step in with; `None` while the static is open.
fn shut_reason(sheet: &Sheet) -> Option<&'static str> {
    match sheet.shut() {
        Some(Shut::SignalDown) => {
            Some("your signal is down. nothing in there can see you until tomorrow.")
        }
        Some(Shut::NoRations) => Some("you are spent for today. the static will keep."),
        None => None,
    }
}

/// The picker over the street: the sheet, the offers, the keys. Fixed
/// width like the scene; the height is what it holds.
pub(crate) fn draw_picker(frame: &mut Frame, area: Rect, view: PickerView<'_>) {
    let text = ink(INK);
    let dim_text = ink(INK_DIM);
    let muted = ink(INK_MUTED);
    let key = lit(Neon::Amber);
    let frame_style = glow(Neon::Cyan);
    let mut lines: Vec<Line<'static>> = vec![Line::default()];

    match view.sheet {
        None => {
            lines.push(Line::from(Span::styled(
                "  the static parts. your sheet has not come down the wire yet.",
                muted,
            )));
        }
        Some(sheet) => {
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
            lines.push(Line::default());

            match shut_reason(sheet) {
                Some(reason) => {
                    lines.push(Line::from(Span::styled(format!("    {reason}"), text)));
                }
                None => {
                    // Three offers stack without the blank rows between
                    // them, so the box still fits a short terminal.
                    let bright = sheet.bright_waits();
                    if bright {
                        let (index, kind, tier) = data::bright_foe_for_level(sheet.level);
                        lines.extend(offer_lines(
                            &Offer {
                                key: "b",
                                pick: Pick::Bright,
                                kind,
                                tier,
                                level: Some(index as i32 + 1),
                                threat: view.picker.bright,
                                note: "this step only",
                                boss: false,
                                bright: true,
                            },
                            view.picker.cursor,
                        ));
                    }
                    let fair = match sheet.signal_hears() {
                        true => Offer {
                            key: "f",
                            pick: Pick::Fair,
                            kind: &OLD_SIGNAL,
                            tier: OLD_SIGNAL_TIER,
                            level: None,
                            threat: view.picker.fair,
                            note: "the bottom of the city",
                            boss: true,
                            bright: false,
                        },
                        false => {
                            let (index, kind, tier) = data::foe_for_level(sheet.level);
                            Offer {
                                key: "f",
                                pick: Pick::Fair,
                                kind,
                                tier,
                                level: Some(index as i32 + 1),
                                threat: view.picker.fair,
                                note: "the glyph of your level",
                                boss: false,
                                bright: false,
                            }
                        }
                    };
                    lines.extend(offer_lines(&fair, view.picker.cursor));
                    if let Some((index, kind, tier)) = data::lower_foe_for_level(sheet.level) {
                        if !bright {
                            lines.push(Line::default());
                        }
                        lines.extend(offer_lines(
                            &Offer {
                                key: "g",
                                pick: Pick::Lower,
                                kind,
                                tier,
                                level: Some(index as i32 + 1),
                                threat: view.picker.lower,
                                note: "a step down, half pay",
                                boss: false,
                                bright: false,
                            },
                            view.picker.cursor,
                        ));
                    }
                }
            }
        }
    }
    lines.push(Line::default());
    // With nothing on offer the step-in keys close the picker
    // (`FightSession::choose`), and the key row says so.
    let shut = view.sheet.is_some_and(|sheet| shut_reason(sheet).is_some());
    lines.push(Line::from(match shut {
        true => vec![
            Span::styled("    [Enter] ", key),
            Span::styled("back to the street", text),
        ],
        false => vec![
            Span::styled("    [↑↓] ", key),
            Span::styled("pick", text),
            Span::styled("   [Enter] ", key),
            Span::styled("step in", text),
            Span::styled("   esc back to the street", dim_text),
        ],
    }));
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
                .title(Span::styled(" the static ", lit(Neon::Cyan))),
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
