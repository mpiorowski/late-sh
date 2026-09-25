//! The fight scene: a panel over the street in the city's own palette.
//! Two portraits facing, the runner's and the glyph's, each losing cells
//! to static in proportion to its missing signal (GAME.md, "Signal
//! corrupts the look": health as something you see, not a number); the
//! exchange, line by line, in the announcer's voice; three keys. Pure:
//! every frame is a function of the mirror and the scene.
//!
//! Also the sheet strip: level, signal, rations, bits, pinned top-right
//! while the runner walks the street, so the ritual's budget is always
//! in view.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::session::Scene;
use super::data::FOES;
use super::state::{Fight, Quarry, Sheet, Slot};
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

fn foe_rows(fight: &Fight) -> Vec<Vec<Span<'static>>> {
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
                        true => dim(Neon::Red),
                        false => glow(Neon::Cyan),
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
fn header(sheet: &Sheet, look: Option<&Look>, username: &str) -> Vec<Line<'static>> {
    let head = ink(INK_BRIGHT).add_modifier(Modifier::BOLD);
    let dim_text = ink(INK_DIM);
    let text = ink(INK);
    let mine = runner_rows(sheet, look);
    let theirs = sheet.fight.as_ref().map(foe_rows);
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
                Span::styled(format!("lv {}  ", sheet.level), text),
                Span::styled(fight.foe().name.to_string(), lit(Neon::Cyan)),
            ],
            {
                let mut row = vec![Span::styled(
                    format!("{}/{} ", fight.foe_signal, fight.foe_max_signal),
                    text,
                )];
                row.extend(bar_spans(
                    fight.foe_signal,
                    fight.foe_max_signal,
                    glow(Neon::Cyan),
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

/// The scene, centered over the street, at a fixed size.
pub(crate) fn draw_scene(frame: &mut Frame, area: Rect, view: SceneView<'_>) {
    let text = ink(INK);
    let bright = ink(INK_BRIGHT);
    let dim_text = ink(INK_DIM);
    let muted = ink(INK_MUTED);
    let key = lit(Neon::Amber);
    let rule = dim(Neon::Cyan);

    // Fixed height: the header, the rule, the log, the keys, each with
    // room to breathe. A short terminal gives up log rows, nothing else.
    let fixed_rows = 1 + PORTRAIT_HEIGHT + 1 + 1 + 1 + 1 + 1 + 1;
    let room = usize::from(area.height.saturating_sub(3));
    let log_rows = LOG_ROWS
        .min(room.saturating_sub(fixed_rows))
        .max(LOG_ROWS_MIN);

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(fixed_rows + log_rows);
    lines.push(Line::default());
    match view.sheet {
        Some(sheet) => {
            lines.extend(header(sheet, view.look, view.own_username));
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
        format!("  {}", "─".repeat(INNER.saturating_sub(4))),
        rule,
    )));
    lines.push(Line::default());

    // The exchange, newest at the bottom: the latest answer bright with a
    // marker, everything before it dimmed.
    let shown: Vec<&String> = view.scene.lines.iter().rev().take(log_rows).collect();
    let shown_count = shown.len();
    for _ in shown_count..log_rows {
        lines.push(Line::default());
    }
    let total = view.scene.lines.len();
    for (i, line) in shown.into_iter().rev().enumerate() {
        let index = total - shown_count + i;
        let latest = index + view.scene.latest >= total;
        let body = truncate(line, INNER.saturating_sub(6));
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
            Span::styled("        [Esc] ", key),
            Span::styled("step back", dim_text),
        ],
    };
    lines.push(Line::from(keys));
    lines.push(Line::default());

    let width = (INNER + 2).min(usize::from(area.width).saturating_sub(2)) as u16;
    let height = (lines.len() as u16 + 2).min(area.height.saturating_sub(1));
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    let budget = match view.sheet {
        Some(sheet) => format!(
            " rations {}/{} · bits {} ",
            sheet.rations_left,
            super::data::RATIONS_PER_DAY,
            sheet.bits
        ),
        None => String::new(),
    };
    frame.render_widget(Clear, rect);
    frame.render_widget(
        Paragraph::new(lines).style(text).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(glow(Neon::Cyan))
                .title(Span::styled(" the end of the row ", lit(Neon::Cyan)))
                .title_bottom(Span::styled(budget, dim_text)),
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
