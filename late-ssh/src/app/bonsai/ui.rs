use std::{collections::BTreeSet, time::SystemTime};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::{
    care::BranchTarget,
    state::{BonsaiState, Stage},
};
use crate::app::common::theme;

const HIGH_STAGE_FORM_VARIANTS: usize = 3;

pub(crate) struct TreeOverlay<'a> {
    pub targets: &'a [BranchTarget],
    pub cut_branch_ids: &'a BTreeSet<i32>,
    pub cursor_x: usize,
    pub cursor_y: usize,
    pub show_selection: bool,
}

/// Render the bonsai widget for the sidebar. Takes a fixed area.
pub fn draw_bonsai(frame: &mut Frame, area: Rect, state: &BonsaiState, beat: f32) {
    let title = if state.is_alive {
        format!(" Bonsai ({}d) ", state.age_days)
    } else {
        " Bonsai [RIP] ".to_string()
    };

    let mut block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if state.is_alive {
            theme::BORDER()
        } else {
            theme::TEXT_FAINT()
        }));
    if let Some(hint) = water_hint_title(state.can_water(), area.width) {
        block = block.title_top(hint);
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 2 || inner.width < 10 {
        return;
    }

    let stage = state.stage();
    let wilting = state.is_wilting();
    let tree_art = tree_ascii(stage, state.seed, wilting);
    let status_lines = status_lines(state);

    // Layout: tree art on top, status at bottom
    let tree_height = tree_art.len();
    let status_height = status_lines.len();
    let available = inner.height as usize;

    let mut lines: Vec<Line<'_>> = Vec::new();

    // Anchor tree to the bottom — pot sits right above the status rows,
    // empty sky fills above.
    let tree_space = available.saturating_sub(status_height);
    let padding_top = tree_space.saturating_sub(tree_height);
    for _ in 0..padding_top {
        lines.push(Line::from(""));
    }

    lines.extend(render_tree_art_lines(
        stage,
        state.seed,
        wilting,
        inner.width as usize,
        beat,
        None,
    ));

    // Pad to push status to bottom
    while lines.len() < available.saturating_sub(status_height) {
        lines.push(Line::from(""));
    }

    lines.extend(status_lines);

    frame.render_widget(Paragraph::new(lines), inner);
}

pub(crate) fn render_tree_art_lines(
    stage: Stage,
    seed: i64,
    wilting: bool,
    width: usize,
    beat: f32,
    overlay: Option<TreeOverlay<'_>>,
) -> Vec<Line<'static>> {
    let tree_art = tree_ascii(stage, seed, wilting);
    let leaf_color = if wilting {
        theme::AMBER_DIM()
    } else {
        leaf_color_for_stage(stage)
    };
    let trunk_color = if wilting {
        theme::TEXT_FAINT()
    } else {
        theme::AMBER()
    };

    // Sway: slow sine oscillation kicked by detected beats, canopy lines only
    let has_canopy = matches!(
        stage,
        Stage::Young | Stage::Mature | Stage::Ancient | Stage::Blossom
    );
    let sway_time = SystemTime::UNIX_EPOCH
        .elapsed()
        .unwrap_or_default()
        .as_secs_f64();
    let sway_base = (sway_time * 2.0).sin(); // ~3s period
    let sway_amplitude = beat.clamp(0.0, 1.0) as f64 * 1.5;
    let w = width;

    // Count canopy lines (contain @, #, or *) for per-line falloff
    let canopy_count = if has_canopy {
        tree_art
            .iter()
            .filter(|l| l.chars().any(|c| matches!(c, '@' | '#' | '*')))
            .count()
    } else {
        0
    };

    let mut lines = Vec::new();
    for (_i, art_line) in tree_art.iter().enumerate() {
        // Only canopy lines sway; top of canopy sways most
        let is_canopy = has_canopy && art_line.chars().any(|c| matches!(c, '@' | '#' | '*'));
        let offset = if is_canopy && canopy_count > 0 {
            // Find this line's position within canopy lines (0 = topmost)
            let canopy_idx = tree_art[.._i]
                .iter()
                .filter(|l| l.chars().any(|c| matches!(c, '@' | '#' | '*')))
                .count();
            let line_factor = if canopy_count <= 1 {
                1.0
            } else {
                1.0 - (canopy_idx as f64 / (canopy_count - 1) as f64)
            };
            (sway_base * sway_amplitude * line_factor).round() as i32
        } else {
            0
        };

        let mut spans = Vec::new();
        let chars: Vec<char> = art_line.chars().collect();
        for (x, ch) in chars.iter().copied().enumerate() {
            let cursor_here = overlay.as_ref().is_some_and(|overlay| {
                overlay.show_selection && overlay.cursor_x == x && overlay.cursor_y == _i
            });

            if let Some(target) = overlay.as_ref().and_then(|overlay| {
                overlay
                    .targets
                    .iter()
                    .find(|target| target.x == x && target.y == _i)
            }) {
                let cut = overlay
                    .as_ref()
                    .is_some_and(|overlay| overlay.cut_branch_ids.contains(&target.id));
                let display = if cut { ch } else { target.glyph };
                let mut style = Style::default().fg(if cut {
                    theme::TEXT_FAINT()
                } else {
                    target_color(target.id)
                });
                if cursor_here {
                    style = style
                        .fg(theme::AMBER_GLOW())
                        .bg(theme::BG_SELECTION())
                        .add_modifier(Modifier::BOLD);
                }
                spans.push(Span::styled(display.to_string(), style));
                continue;
            }

            let color = match ch {
                '|' | '/' | '\\' | '_' | '~' => trunk_color,
                '.' | '\'' | ',' | '*' | '@' | '#' | 'o' | 'O' => leaf_color,
                '[' | ']' | '=' => theme::TEXT_DIM(), // pot
                _ => theme::TEXT_FAINT(),
            };
            let mut style = Style::default().fg(color);
            if cursor_here {
                style = style
                    .fg(theme::AMBER_GLOW())
                    .bg(theme::BG_SELECTION())
                    .add_modifier(Modifier::BOLD);
            }
            spans.push(Span::styled(cursor_display(ch, cursor_here), style));
        }

        // Manual centering with sway offset
        let art_width = chars.len();
        let base_pad = w.saturating_sub(art_width) / 2;
        let pad = (base_pad as i32 + offset).max(0) as usize;
        let pad = pad.min(w.saturating_sub(art_width));
        spans.insert(0, Span::raw(" ".repeat(pad)));
        lines.push(Line::from(spans));
    }
    lines
}

fn target_color(id: i32) -> Color {
    match id.rem_euclid(4) {
        0 => theme::ERROR(),
        1 => theme::AMBER_GLOW(),
        2 => theme::BONSAI_BLOOM(),
        _ => theme::SUCCESS(),
    }
}

fn cursor_display(ch: char, cursor_here: bool) -> String {
    if cursor_here && ch == ' ' {
        "+".to_string()
    } else {
        ch.to_string()
    }
}

fn status_lines(state: &BonsaiState) -> Vec<Line<'static>> {
    status_line_specs(state.is_alive, state.stage(), state.can_water())
        .into_iter()
        .map(|spec| match spec {
            StatusLineSpec::DeadHint => Line::from(Span::styled(
                "Press w to plant anew",
                Style::default().fg(theme::TEXT_FAINT()),
            ))
            .centered(),
            StatusLineSpec::WateredToday => Line::from(Span::styled(
                "Watered today",
                Style::default().fg(theme::SUCCESS()),
            ))
            .centered(),
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
enum StatusLineSpec {
    DeadHint,
    WateredToday,
}

fn status_line_specs(is_alive: bool, _stage: Stage, can_water: bool) -> Vec<StatusLineSpec> {
    if !is_alive {
        return vec![StatusLineSpec::DeadHint];
    }

    let mut lines = Vec::new();
    if !can_water {
        lines.push(StatusLineSpec::WateredToday);
    }
    lines
}

fn water_hint_title(can_water: bool, width: u16) -> Option<Line<'static>> {
    if !can_water || width < 12 {
        return None;
    }
    Some(
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "w",
                Style::default()
                    .fg(theme::AMBER())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" care ", Style::default().fg(theme::TEXT_DIM())),
        ])
        .right_aligned(),
    )
}

fn leaf_color_for_stage(stage: Stage) -> ratatui::style::Color {
    match stage {
        Stage::Dead => theme::TEXT_FAINT(),
        Stage::Seed => theme::TEXT_DIM(),
        Stage::Sprout => theme::BONSAI_SPROUT(),
        Stage::Sapling => theme::BONSAI_LEAF(),
        Stage::Young => theme::BONSAI_CANOPY(),
        Stage::Mature => theme::BONSAI_CANOPY(),
        Stage::Ancient => theme::BONSAI_BLOOM(),
        Stage::Blossom => theme::BONSAI_BLOOM(),
    }
}

// ── ASCII Art per stage ──────────────────────────────────────

/// Japanese bonsai style for the variant picked by this seed, when applicable.
/// Returned as `(short_name, english_gloss)`. Seed/Sprout/Dead return None —
/// they are too young (or too gone) to carry a formal style.
pub fn tree_variant_name(stage: Stage, seed: i64) -> Option<(&'static str, &'static str)> {
    let v = seed.unsigned_abs() as usize;
    let style = match stage {
        Stage::Dead | Stage::Seed | Stage::Sprout => return None,
        Stage::Sapling => match v % 6 {
            0 => ("Chokkan", "formal upright"),
            1 => ("Shakan", "slanting"),
            2 => ("Hokidachi", "broom"),
            3 => ("Sideshoot", "lateral bud"),
            4 => ("Han-kengai", "semi-cascade"),
            _ => ("Futago", "twin shoot"),
        },
        Stage::Young => match style_variant(seed, 7) {
            0 => ("Chokkan", "formal upright"),
            1 => ("Moyogi", "informal upright"),
            2 => ("Shakan", "slanting"),
            3 => ("Fukinagashi", "windswept"),
            4 => ("Hokidachi", "broom"),
            5 => ("Sokan", "twin trunk"),
            _ => ("Bunjingi", "literati"),
        },
        Stage::Mature | Stage::Ancient => match style_variant(seed, 8) {
            0 => ("Chokkan", "formal upright"),
            1 => ("Moyogi", "informal upright"),
            2 => ("Shakan", "slanting"),
            3 => ("Fukinagashi", "windswept"),
            4 => ("Sokan", "twin trunk"),
            5 => ("Hokidachi", "broom"),
            6 => ("Bunjingi", "literati"),
            _ => ("Neagari", "exposed root"),
        },
        Stage::Blossom => match style_variant(seed, 8) {
            0 => ("Chokkan", "flowering upright"),
            1 => ("Moyogi", "flowering curve"),
            2 => ("Shakan", "flowering slant"),
            3 => ("Fukinagashi", "flowering windswept"),
            4 => ("Sokan", "flowering twin"),
            5 => ("Hokidachi", "flowering broom"),
            6 => ("Bunjingi", "flowering literati"),
            _ => ("Neagari", "flowering exposed root"),
        },
    };
    Some(style)
}

pub(crate) fn tree_ascii(stage: Stage, seed: i64, _wilting: bool) -> Vec<String> {
    // Seed → per-stage variant picker. Each stage applies its own modulo so we
    // can add variants stage-by-stage without shifting the others around.
    // Design language for mature stages: discrete "foliage pads" with visible
    // trunk between them, and lateral branches carrying side pads — a real
    // bonsai silhouette, not a blob on a stick.
    let v = seed.unsigned_abs() as usize;

    let style_count = high_stage_style_count(stage);
    let shape_count = high_stage_shape_count(stage);
    let shape = style_count.map_or(0, |sc| shape_variant(seed, sc, shape_count));
    let form = style_count.map_or(0, |sc| form_variant(seed, sc, shape_count));

    let lines = match stage {
        Stage::Dead => match v % 4 {
            // bare stick
            0 => vec![
                "   .   ", "  /|   ", " / |   ", "   |`  ", "  .|.  ", " [===] ",
            ],
            // withered stump
            1 => vec![
                "       ", "   ,.  ", "    \\  ", "   .|  ", "  .|.  ", " [===] ",
            ],
            // snapped twig
            2 => vec![
                "  .    ", "   `   ", "   |   ", "  .|   ", "  .|.  ", " [===] ",
            ],
            // leafless claw
            _ => vec![
                " .   . ", "  \\ /  ", "   V   ", "   |   ", "  .|.  ", " [===] ",
            ],
        },
        Stage::Seed => match v % 3 {
            // buried seed
            0 => vec!["       ", "       ", "   .   ", "  .|.  ", " [===] "],
            // tiny peek
            1 => vec!["       ", "   .   ", "   ,   ", "  .|.  ", " [===] "],
            // split shell
            _ => vec!["       ", "  . .  ", "   ,   ", "  .|.  ", " [===] "],
        },
        Stage::Sprout => match v % 5 {
            // three-leaf crown
            0 => vec![
                "       ", "   ,   ", "  /|\\  ", "   |   ", "  .|.  ", " [===] ",
            ],
            // paired leaves
            1 => vec![
                "       ", "   .   ", "  '|,  ", "   |   ", "  .|.  ", " [===] ",
            ],
            // upward shoots
            2 => vec![
                "  ..   ", "   |   ", "   |,  ", "   |   ", "  .|.  ", " [===] ",
            ],
            // hooked shoot
            3 => vec![
                "   .   ", "   ,   ", "   |/  ", "   |   ", "  .|.  ", " [===] ",
            ],
            // twin shoots
            _ => vec![
                "  , ,  ", "  |,|  ", "  \\|/  ", "   |   ", "  .|.  ", " [===] ",
            ],
        },
        Stage::Sapling => match v % 6 {
            // formal upright
            0 => vec![
                "   ,.,  ",
                "  '.'.  ",
                "   /|\\  ",
                "    |   ",
                "   .|.  ",
                "  [===] ",
            ],
            // slanting (Shakan)
            1 => vec![
                "   .,   ", "   ,.,  ", "    |/  ", "    /   ", "   .|.  ", "  [===] ",
            ],
            // broom start (Hokidachi)
            2 => vec![
                "  ,.,., ",
                "   \\|/  ",
                "    |   ",
                "    |   ",
                "   .|.  ",
                "  [===] ",
            ],
            // lateral bud — tiny side pad
            3 => vec![
                "   ,.,  ", "  .'.,  ", " ~.|    ", "    |~. ", "   .|.  ", "  [===] ",
            ],
            // semi-cascade (Han-kengai)
            4 => vec![
                "    .,  ",
                "    .', ",
                "    '\\  ",
                "    |   ",
                "   .|.  ",
                "  [===] ",
            ],
            // twin-shoot sapling
            _ => vec![
                "   , ,  ",
                "   ,.,  ",
                "   \\|/  ",
                "    |   ",
                "   .|.  ",
                "  [===] ",
            ],
        },
        Stage::Young => match style_variant(seed, 7) {
            // Chokkan — formal upright, top + lateral pads
            0 => vec![
                "     .###.      ",
                "    .#####.     ",
                "     '###'      ",
                "       |        ",
                "  .##. | .##.   ",
                " .####.|.####.  ",
                "  '##' | '##'   ",
                "       |        ",
                "      .|.       ",
                "     [===]      ",
            ],
            // Moyogi — informal upright, S-curve trunk
            1 => vec![
                "     .###.      ",
                "    .#####.     ",
                "     '###'      ",
                "      /         ",
                "    .#/         ",
                "   .###.        ",
                "    '#'\\        ",
                "        \\_      ",
                "         |      ",
                "        .|.     ",
                "       [===]    ",
            ],
            // Shakan — slanting with balancing pad
            2 => vec![
                "       .###.    ",
                "      .#####.   ",
                "       '###'    ",
                "       /        ",
                "      /         ",
                " .##_/          ",
                " ####           ",
                " '##'           ",
                "     \\          ",
                "     .|.        ",
                "    [===]       ",
            ],
            // Fukinagashi — windswept (right)
            3 => vec![
                "      .#####.   ",
                "     .#######.  ",
                "      '#####'   ",
                "     /          ",
                "    /           ",
                "   /            ",
                "  /             ",
                " /              ",
                "/               ",
                "|               ",
                ".|.             ",
                "[===]           ",
            ],
            // Hokidachi — broom, fan branches
            4 => vec![
                "    .######.    ",
                "   .########.   ",
                "    '######'    ",
                "     \\\\|//      ",
                "      \\|/       ",
                "       |        ",
                "       |        ",
                "      .|.       ",
                "     [===]      ",
            ],
            // Sokan — twin trunk
            5 => vec![
                "    .#.   .#.   ",
                "   .###. .###.  ",
                "    '#'   '#'   ",
                "     |     |    ",
                "     |     |    ",
                "      \\   /     ",
                "       \\ /      ",
                "        |       ",
                "       .|.      ",
                "      [===]     ",
            ],
            // Bunjingi — literati, tiny crown on tall trunk
            _ => vec![
                "      .#.       ",
                "     .###.      ",
                "      '#'       ",
                "       |        ",
                "       |        ",
                "       |        ",
                "      \\|        ",
                "       |        ",
                "       |        ",
                "      .|.       ",
                "     [===]      ",
            ],
        },
        Stage::Mature => match (style_variant(seed, 8), shape) {
            // Chokkan A — three layered tiers, perfect symmetry with lateral pads
            (0, 0) => vec![
                "         .@@@.        ",
                "        .@@@@@.       ",
                "         '@@@'        ",
                "           |          ",
                "    .@@@.  |  .@@@.   ",
                "   .@@@@@. | .@@@@@.  ",
                "    '@@@'  |  '@@@'   ",
                "           |          ",
                "        .@@@@@.       ",
                "       .@@@@@@@.      ",
                "        '@@@@@'       ",
                "           |          ",
                "          .|.         ",
                "         [===]        ",
            ],
            // Chokkan B — pure vertical cone, four tight tiers, no lateral spread
            (0, _) => vec![
                "          .@.         ",
                "          '@'         ",
                "           |          ",
                "         .@@@.        ",
                "         '@@@'        ",
                "           |          ",
                "        .@@@@@.       ",
                "         '@@@'        ",
                "           |          ",
                "      .@@@@@@@@@.     ",
                "       '@@@@@@@'      ",
                "           |          ",
                "          .|.         ",
                "         [===]        ",
            ],
            // Moyogi A — gentle S-curve trunk, offset pads
            (1, 0) => vec![
                "          .@@@.       ",
                "         @@@@@@@      ",
                "          '@@@'       ",
                "          /           ",
                "         /            ",
                "      .@/             ",
                "     @@@@             ",
                "      '@\\             ",
                "         \\_           ",
                "           \\_ .@@.    ",
                "             @@@@@    ",
                "              '@'     ",
                "              |       ",
                "             .|.      ",
                "            [===]     ",
            ],
            // Moyogi B — deeper zigzag, three offset pads
            (1, _) => vec![
                "            .@@.      ",
                "           @@@@@      ",
                "            '@'       ",
                "            /         ",
                "          .@/         ",
                "         @@@@         ",
                "          '@\\         ",
                "             \\        ",
                "              \\_.@@.  ",
                "                @@@@  ",
                "                 '@'  ",
                "                 |    ",
                "                .|.   ",
                "               [===]  ",
            ],
            // Shakan A — moderate lean LEFT, compensating right pad
            (2, 0) => vec![
                "           .@@@@.     ",
                "          .@@@@@@.    ",
                "           '@@@@'     ",
                "          /           ",
                "         /            ",
                "   .@@@./             ",
                "  .@@@@@@.            ",
                "   '@@@'              ",
                "        \\             ",
                "         \\            ",
                "          \\           ",
                "          .|.         ",
                "         [===]        ",
            ],
            // Shakan B — steep lean RIGHT, balancing left pad
            (2, _) => vec![
                "   .@@@.              ",
                "  @@@@@@@             ",
                "   '@@@'              ",
                "       \\              ",
                "        \\             ",
                "   .@@. \\             ",
                "  @@@@@@ \\            ",
                "   '@@'   \\           ",
                "           \\          ",
                "            \\         ",
                "             \\        ",
                "             .|.      ",
                "            [===]     ",
            ],
            // Fukinagashi A — windswept, canopy leaning right, branches trailing back
            (3, 0) => vec![
                "         ,@@@@@@.     ",
                "       .@@@@@@@@@@.   ",
                "         '@@@@@@'     ",
                "        /   /  /      ",
                "       /   /  /       ",
                "      /   /  /        ",
                "     /   /  /         ",
                "    /   /  /          ",
                "   /   /  /           ",
                "  /   /  /            ",
                " /    | /             ",
                "      |               ",
                "     .|.              ",
                "    [===]             ",
            ],
            // Fukinagashi B — mirrored, canopy leaning left, branches trailing right
            (3, _) => vec![
                "     .@@@@@@,         ",
                "   .@@@@@@@@@@.       ",
                "     '@@@@@@'         ",
                "      \\  \\   \\        ",
                "       \\  \\   \\       ",
                "        \\  \\   \\      ",
                "         \\  \\   \\     ",
                "          \\  \\   \\    ",
                "           \\  \\   \\   ",
                "            \\  \\   \\  ",
                "             \\ |    \\ ",
                "               |      ",
                "               .|.    ",
                "              [===]   ",
            ],
            // Sokan A — twin trunks rising from shared base, separate canopies
            (4, 0) => vec![
                "    .@@@.    .@@@.    ",
                "   .@@@@@.  .@@@@@.   ",
                "    '@@@'    '@@@'    ",
                "      |        |      ",
                "      |        |      ",
                "      |        |      ",
                "       \\      /       ",
                "        \\    /        ",
                "         \\  /         ",
                "          \\/          ",
                "          ||          ",
                "         .|.          ",
                "        [===]         ",
            ],
            // Sokan B — twin trunks with merged canopy, diverging below
            (4, _) => vec![
                "        .@@@@@.       ",
                "       .@@@@@@@.      ",
                "      .@@@@@@@@@.     ",
                "       '@@|@|@@'      ",
                "          | |         ",
                "         /   \\        ",
                "         |   |        ",
                "         |   |        ",
                "          \\ /         ",
                "           V          ",
                "           |          ",
                "          .|.         ",
                "         [===]        ",
            ],
            // Hokidachi A — classic broom, symmetric fan
            (5, 0) => vec![
                "        .@@@@@.       ",
                "      .@@@@@@@@@.     ",
                "     .@@@@@@@@@@@.    ",
                "      '@@@@@@@@@'     ",
                "       \\\\\\|///        ",
                "        \\\\|//         ",
                "         \\|/          ",
                "          |           ",
                "          |           ",
                "          |           ",
                "         .|.          ",
                "        [===]         ",
            ],
            // Hokidachi B — tall conical broom, stacked canopy
            (5, _) => vec![
                "           .@.        ",
                "          @@@@@       ",
                "         @@@@@@@      ",
                "        @@@@@@@@@     ",
                "       @@@@@@@@@@@    ",
                "        '@@@@@@@'     ",
                "         \\\\|||//      ",
                "          \\|||/       ",
                "           \\|/        ",
                "            |         ",
                "            |         ",
                "           .|.        ",
                "          [===]       ",
            ],
            // Bunjingi A — literati, tall bare trunk with a single small crown
            (6, 0) => vec![
                "          .@@.        ",
                "         .@@@@.       ",
                "          '@@'        ",
                "           |          ",
                "           |          ",
                "           |          ",
                "         .@|          ",
                "         @@|          ",
                "          '|          ",
                "           |          ",
                "           |          ",
                "           |          ",
                "          .|.         ",
                "         [===]        ",
            ],
            // Bunjingi B — literati with multiple micro pads along the trunk
            (6, _) => vec![
                "           .@.        ",
                "           @@@        ",
                "           '@'        ",
                "            |         ",
                "            |         ",
                "          .@|         ",
                "          @@|         ",
                "           '|         ",
                "            |@.       ",
                "            |@@       ",
                "            |'        ",
                "            |         ",
                "           .|.        ",
                "          [===]       ",
            ],
            // Neagari A — exposed roots, layered crown with lateral pads
            (_, 0) => vec![
                "         .@@@.        ",
                "        .@@@@@.       ",
                "         '@@@'        ",
                "           |          ",
                "    .@@.   |   .@@.   ",
                "   .@@@@._ | _.@@@@.  ",
                "    '@@' \\ | / '@@'   ",
                "          \\|/         ",
                "           |          ",
                "          .|.         ",
                "        _/ | \\_       ",
                "       /   |   \\      ",
                "        [=====]       ",
            ],
            // Neagari B — dramatic splayed roots under an elevated pot
            (_, _) => vec![
                "          .@@@.       ",
                "         @@@@@@@      ",
                "          '@@@'       ",
                "           |          ",
                "       .@@.|.@@.      ",
                "      @@@@@|@@@@@     ",
                "       '@@'|'@@'      ",
                "           |          ",
                "          .|.         ",
                "       __/ | \\__      ",
                "      /    |    \\     ",
                "        [=====]       ",
            ],
        },
        Stage::Ancient => match (style_variant(seed, 8), shape) {
            // Chokkan A — five-tier classical layered with lateral pads
            (0, 0) => vec![
                "          .@@@.       ",
                "         .@@@@@.      ",
                "          '@@@'       ",
                "            |         ",
                "   .@@@.    |    .@@@.",
                "  .@@@@@._  |  _.@@@@.",
                "   '@@@'  \\ | /  '@@@'",
                "           \\|/        ",
                "            |         ",
                "   .@@@@@.  |  .@@@@@.",
                " .@@@@@@@@. | .@@@@@@.",
                "   '@@@@@'  |  '@@@@@'",
                "            |         ",
                "           .|.        ",
                "          [===]       ",
            ],
            // Chokkan B — pure vertical cone, five stacked tiers
            (0, _) => vec![
                "           .@.        ",
                "           @@@        ",
                "           '@'        ",
                "            |         ",
                "         .@@@@@.      ",
                "         '@@@@@'      ",
                "            |         ",
                "        .@@@@@@@.     ",
                "         '@@@@@'      ",
                "            |         ",
                "      .@@@@@@@@@@@.   ",
                "       '@@@@@@@@@'    ",
                "            |         ",
                "           .|.        ",
                "          [===]       ",
            ],
            // Moyogi A — gentle S-curve with three pads
            (1, 0) => vec![
                "           .@@@@.     ",
                "          .@@@@@@.    ",
                "           '@@@@'     ",
                "           /          ",
                "          /           ",
                "       .@/            ",
                "      @@@@            ",
                "       '@\\            ",
                "          \\           ",
                "           \\_.@@@.    ",
                "             @@@@@    ",
                "              '@'\\    ",
                "                 \\    ",
                "                .|.   ",
                "               [===]  ",
            ],
            // Moyogi B — deeper zigzag, wider reach, three offset pads
            (1, _) => vec![
                "            .@@@.     ",
                "           @@@@@@@    ",
                "            '@@@'     ",
                "            /         ",
                "           /          ",
                "         .@/          ",
                "        @@@@@         ",
                "         '@\\          ",
                "            \\         ",
                "             \\_.@@@.  ",
                "               @@@@@  ",
                "                '@'\\  ",
                "                   \\  ",
                "                  .|. ",
                "                 [===]",
            ],
            // Shakan A — dramatic slant left with three balancing pads
            (2, 0) => vec![
                "             .@@@.    ",
                "            .@@@@@.   ",
                "             '@@@'    ",
                "            /         ",
                "           /          ",
                "     .@@./            ",
                "    @@@@@@            ",
                "     '@@\\             ",
                "         \\            ",
                "   .@@.   \\           ",
                "  @@@@@@   \\          ",
                "   '@@'     \\         ",
                "             .|.      ",
                "            [===]     ",
            ],
            // Shakan B — steep slant right with balancing left pad
            (2, _) => vec![
                "  .@@@@.              ",
                " @@@@@@@@             ",
                "  '@@@@'              ",
                "        \\             ",
                "         \\            ",
                "   .@@.   \\           ",
                "  @@@@@@   \\          ",
                "   '@@'     \\         ",
                "             \\        ",
                "              \\       ",
                "               \\      ",
                "                \\     ",
                "                 .|.  ",
                "                [===] ",
            ],
            // Fukinagashi A — windswept right, long trailing foliage
            (3, 0) => vec![
                "         ,@@@@@@@@.   ",
                "       .@@@@@@@@@@@@. ",
                "         '@@@@@@@@'   ",
                "        /   /  /      ",
                "       /   /  /       ",
                "      /   /  /        ",
                "     /   /  /         ",
                "    /   /  /          ",
                "   /   /  /           ",
                "  /   /  /            ",
                " /   /  /             ",
                "/   /  /              ",
                "/      |              ",
                "      .|.             ",
                "     [===]            ",
            ],
            // Fukinagashi B — mirrored, canopy left, branches trailing right
            (3, _) => vec![
                "    .@@@@@@@@@.       ",
                "  .@@@@@@@@@@@@@@.    ",
                "    '@@@@@@@@'        ",
                "      \\  \\  \\         ",
                "       \\  \\  \\        ",
                "        \\  \\  \\       ",
                "         \\  \\  \\      ",
                "          \\  \\  \\     ",
                "           \\  \\  \\    ",
                "            \\  \\  \\   ",
                "             \\  \\  \\  ",
                "              \\  |    ",
                "                 |    ",
                "                .|.   ",
                "               [===]  ",
            ],
            // Sokan A — twin trunks, overlapping separate canopies
            (4, 0) => vec![
                "     .@@@.   .@@@.    ",
                "    @@@@@@@ @@@@@@@   ",
                "     '@@@'   '@@@'    ",
                "       |       |      ",
                "    .@@|@@.  .@@|@@.  ",
                "    @@@|@@@  @@@|@@@  ",
                "     '@|@'    '@|@'   ",
                "       |       |      ",
                "       |       |      ",
                "        \\     /       ",
                "         \\   /        ",
                "          \\ /         ",
                "          .|.         ",
                "         [===]        ",
            ],
            // Sokan B — twin trunks with unified merged canopy above
            (4, _) => vec![
                "       .@@@@@@@.      ",
                "      .@@@@@@@@@.     ",
                "     .@@@@@@@@@@@.    ",
                "      '@@|@|@@@'      ",
                "         | |          ",
                "         | |          ",
                "        /   \\         ",
                "        |   |         ",
                "        |   |         ",
                "        |   |         ",
                "         \\ /          ",
                "          V           ",
                "          |           ",
                "         .|.          ",
                "        [===]         ",
            ],
            // Hokidachi A — classic broom, wide symmetric fan crown
            (5, 0) => vec![
                "      .@@@@@@@@@.     ",
                "    .@@@@@@@@@@@@@.   ",
                "   .@@@@@@@@@@@@@@@.  ",
                "    '@@@@@@@@@@@@@'   ",
                "     \\\\\\\\|||////      ",
                "      \\\\\\|||///       ",
                "       \\\\|||//        ",
                "        \\|||/         ",
                "         \\|/          ",
                "          |           ",
                "          |           ",
                "          |           ",
                "         .|.          ",
                "        [===]         ",
            ],
            // Hokidachi B — tall conical broom, stacked triangular canopy
            (5, _) => vec![
                "            .@.       ",
                "           @@@@@      ",
                "          @@@@@@@     ",
                "         @@@@@@@@@    ",
                "        @@@@@@@@@@@   ",
                "       @@@@@@@@@@@@@  ",
                "        '@@@@@@@@@'   ",
                "         \\\\\\|||///    ",
                "          \\\\|||//     ",
                "           \\|||/      ",
                "            \\|/       ",
                "             |        ",
                "             |        ",
                "            .|.       ",
                "           [===]      ",
            ],
            // Bunjingi A — literati, tall stark trunk with single small crown
            (6, 0) => vec![
                "         .@@@.        ",
                "        .@@@@@.       ",
                "         '@@@'        ",
                "          |           ",
                "          |           ",
                "          |           ",
                "          |           ",
                "        .@|           ",
                "        @@|           ",
                "         '|           ",
                "          |           ",
                "          |           ",
                "          |           ",
                "         .|.          ",
                "        [===]         ",
            ],
            // Bunjingi B — literati with multiple micro pads along the trunk
            (6, _) => vec![
                "           .@.        ",
                "           @@@        ",
                "           '@'        ",
                "            |         ",
                "            |         ",
                "          .@|         ",
                "          @@|         ",
                "           '|         ",
                "            |@.       ",
                "            |@@       ",
                "            |'        ",
                "          .@|         ",
                "          @@|         ",
                "           .|.        ",
                "          [===]       ",
            ],
            // Neagari A — layered crown, exposed roots at the pot
            (_, 0) => vec![
                "          .@@@.       ",
                "         .@@@@@.      ",
                "          '@@@'       ",
                "            |         ",
                "   .@@@.    |    .@@@.",
                "  @@@@@@@_  |  _@@@@@@",
                "   '@@@' \\  |  / '@@@'",
                "          \\ | /       ",
                "           \\|/        ",
                "            |         ",
                "         .@@|@@.      ",
                "         '@@|@@'      ",
                "            |         ",
                "        _/ .|. \\_     ",
                "       [=======]      ",
            ],
            // Neagari B — dramatic splayed roots under an elevated pot
            (_, _) => vec![
                "          .@@@.       ",
                "         @@@@@@@      ",
                "          '@@@'       ",
                "           |          ",
                "       .@@.|.@@.      ",
                "      @@@@@|@@@@@     ",
                "       '@@'|'@@'      ",
                "           |          ",
                "          .|.         ",
                "          _|_         ",
                "       __/ | \\__      ",
                "      /    |    \\     ",
                "     /     |     \\    ",
                "        [=======]     ",
            ],
        },
        Stage::Blossom => match (style_variant(seed, 8), shape) {
            // Chokkan A — layered flowering with petals woven through
            (0, 0) => vec![
                "         .*@@*.       ",
                "        *@@@@@*.      ",
                "         '@*@'        ",
                "            |         ",
                "   .@*@.    |    .@*@.",
                "  *@@@@*_   |   _*@@@*",
                "   '@*@'  \\ | /  '@*@'",
                "           \\|/        ",
                "            |         ",
                "   .*@@@*.  |  .*@@@*.",
                " *@@*@@@*.  |  .*@@@*@",
                "   '*@@*'   |   '*@@*'",
                "            |         ",
                "           .|.        ",
                "          [===]       ",
            ],
            // Chokkan B — pure vertical cone of flowering tiers
            (0, _) => vec![
                "           .*.        ",
                "           *@*        ",
                "           '*'        ",
                "            |         ",
                "         .*@*@.       ",
                "         '*@*'        ",
                "            |         ",
                "        .*@*@*.       ",
                "         '*@*'        ",
                "            |         ",
                "      .*@*@*@*@*.     ",
                "       '*@*@*@*'      ",
                "            |         ",
                "           .|.        ",
                "          [===]       ",
            ],
            // Moyogi A — flowering S-curve, three pads
            (1, 0) => vec![
                "           .*@@*.     ",
                "          *@@@*@@*    ",
                "           '*@@'      ",
                "           /          ",
                "          /           ",
                "       .*/            ",
                "      *@@*            ",
                "       '@\\            ",
                "          \\           ",
                "           \\_.*@*.    ",
                "             *@@@*    ",
                "              '*'\\    ",
                "                 \\    ",
                "                .|.   ",
                "               [===]  ",
            ],
            // Moyogi B — deeper flowering zigzag with three offset pads
            (1, _) => vec![
                "            .*@*.     ",
                "           *@@*@@*    ",
                "            '*@'      ",
                "            /         ",
                "          .*/         ",
                "         *@@*         ",
                "          '*\\         ",
                "             \\        ",
                "              \\_.*@*. ",
                "                *@@*  ",
                "                 '*'  ",
                "                 |    ",
                "                .|.   ",
                "               [===]  ",
            ],
            // Shakan A — slanting flowering, three cascading pads
            (2, 0) => vec![
                "             .*@*.    ",
                "            *@@@@@*   ",
                "             '*@'     ",
                "            /         ",
                "           /          ",
                "     .@*./            ",
                "    @@*@@@            ",
                "     '@*\\             ",
                "         \\            ",
                "   .*@.   \\           ",
                "  *@@*@*   \\          ",
                "   '@*'     \\         ",
                "             .|.      ",
                "            [===]     ",
            ],
            // Shakan B — steep flowering slant the other way
            (2, _) => vec![
                "  .*@*.               ",
                " *@@*@@*              ",
                "  '*@'                ",
                "      \\               ",
                "       \\              ",
                "   .*@. \\             ",
                "  *@*@*  \\            ",
                "   '*'    \\           ",
                "           \\          ",
                "            \\         ",
                "             \\        ",
                "              \\       ",
                "               .|.    ",
                "              [===]   ",
            ],
            // Fukinagashi A — flowering windswept, canopy right
            (3, 0) => vec![
                "         ,*@*@*@*.    ",
                "       .*@@*@@*@@@*.  ",
                "         '*@*@*@*'    ",
                "        /   /  /      ",
                "       /   /  /       ",
                "      /   /  /        ",
                "     /   /  /         ",
                "    /   /  /          ",
                "   /   /  /           ",
                "  /   /  /            ",
                " /   /  /             ",
                "/   /  /              ",
                "/      |              ",
                "      .|.             ",
                "     [===]            ",
            ],
            // Fukinagashi B — mirrored flowering windswept, canopy left
            (3, _) => vec![
                "    .*@*@*@*.         ",
                "  .*@@*@@*@@*.        ",
                "    '*@*@*@*'         ",
                "      \\  \\  \\         ",
                "       \\  \\  \\        ",
                "        \\  \\  \\       ",
                "         \\  \\  \\      ",
                "          \\  \\  \\     ",
                "           \\  \\  \\    ",
                "            \\  \\  \\   ",
                "             \\  \\  \\  ",
                "              \\ |     ",
                "                |     ",
                "               .|.    ",
                "              [===]   ",
            ],
            // Sokan A — twin flowering trunks, separate canopies
            (4, 0) => vec![
                "    .*@*.   .*@*.     ",
                "   *@@*@@@. *@@*@@@.  ",
                "    '*@*'    '*@*'    ",
                "       |       |      ",
                "    .@*|*@.  .@*|*@.  ",
                "   *@@*|*@*  *@@*|*@* ",
                "    '*@|*'    '*@|*'  ",
                "       |       |      ",
                "        \\     /       ",
                "         \\   /        ",
                "          \\ /         ",
                "          .|.         ",
                "         [===]        ",
            ],
            // Sokan B — twin trunks with unified flowering canopy
            (4, _) => vec![
                "       .*@*@*@*.      ",
                "      *@@*@@*@@*.     ",
                "     .*@*@*@*@*@*.    ",
                "      '*@|@|@*@*'     ",
                "         | |          ",
                "         | |          ",
                "        /   \\         ",
                "        |   |         ",
                "        |   |         ",
                "        |   |         ",
                "         \\ /          ",
                "          V           ",
                "          |           ",
                "         .|.          ",
                "        [===]         ",
            ],
            // Hokidachi A — classic flowering broom
            (5, 0) => vec![
                "    .*@*@*@*@*@*.     ",
                "  .*@@@*@@@*@@@*@@*.  ",
                "   *@*@*@*@*@*@*@*@   ",
                "  .*@@@*@@@*@@@*@@*.  ",
                "    '*@*@*@*@*@*'     ",
                "     \\\\\\\\|||////      ",
                "      \\\\\\|||///       ",
                "       \\\\|||//        ",
                "        \\|||/         ",
                "         \\|/          ",
                "          |           ",
                "          |           ",
                "         .|.          ",
                "        [===]         ",
            ],
            // Hokidachi B — tall flowering cone broom
            (5, _) => vec![
                "            .*.       ",
                "           *@*@*      ",
                "          *@*@*@*     ",
                "         *@*@*@*@*    ",
                "        *@*@*@*@*@*   ",
                "       *@*@*@*@*@*@*  ",
                "        '*@*@*@*@*'   ",
                "         \\\\\\|||///    ",
                "          \\\\|||//     ",
                "           \\|||/      ",
                "            \\|/       ",
                "             |        ",
                "             |        ",
                "            .|.       ",
                "           [===]      ",
            ],
            // Bunjingi A — flowering literati, small single crown
            (6, 0) => vec![
                "         .*@*.        ",
                "        *@@*@@*       ",
                "         '*@'         ",
                "          |           ",
                "          |           ",
                "          |           ",
                "          |           ",
                "        .*|           ",
                "        *@|           ",
                "         '|           ",
                "          |           ",
                "          |           ",
                "          |           ",
                "         .|.          ",
                "        [===]         ",
            ],
            // Bunjingi B — flowering literati with multiple micro pads
            (6, _) => vec![
                "           .*.        ",
                "           *@*        ",
                "           '*'        ",
                "            |         ",
                "            |         ",
                "          .*|         ",
                "          *@|         ",
                "           '|         ",
                "            |*.       ",
                "            |*@       ",
                "            |'        ",
                "          .*|         ",
                "          *@|         ",
                "           .|.        ",
                "          [===]       ",
            ],
            // Neagari A — blooming exposed root, layered crown
            (_, 0) => vec![
                "         .*@*.        ",
                "        *@@*@@*       ",
                "         '*@'         ",
                "           |          ",
                "   .*@*.   |   .*@*.  ",
                "  *@@*@@*_ | _*@@*@@* ",
                "   '@*'  \\ | /  '@*'  ",
                "          \\|/         ",
                "           |          ",
                "        .*@|@*.       ",
                "        *@*|*@*       ",
                "         '*|*'        ",
                "           |          ",
                "       _/ .|. \\_      ",
                "      [=======]       ",
            ],
            // Neagari B — flowering with dramatic splayed roots
            (_, _) => vec![
                "          .*@*.       ",
                "         *@@*@@*      ",
                "          '*@'        ",
                "           |          ",
                "       .*@.|.@*.      ",
                "      *@*@*|*@*@*     ",
                "       '*@'|'*@'      ",
                "           |          ",
                "          .|.         ",
                "          _|_         ",
                "       __/ | \\__      ",
                "      /    |    \\     ",
                "     /     |     \\    ",
                "        [=======]     ",
            ],
        },
    };

    form_tree_art(lines, form)
}

fn high_stage_style_count(stage: Stage) -> Option<usize> {
    match stage {
        Stage::Young => Some(7),
        Stage::Mature | Stage::Ancient | Stage::Blossom => Some(8),
        _ => None,
    }
}

/// Per-style hand-tuned silhouette count. Stages not listed here default to 1
/// — they fall back to the single canonical silhouette per style.
fn high_stage_shape_count(stage: Stage) -> usize {
    match stage {
        Stage::Mature | Stage::Ancient | Stage::Blossom => 2,
        _ => 1,
    }
}

fn style_variant(seed: i64, style_count: usize) -> usize {
    seed.unsigned_abs() as usize % style_count
}

fn shape_variant(seed: i64, style_count: usize, shape_count: usize) -> usize {
    if shape_count <= 1 {
        return 0;
    }
    (seed.unsigned_abs() as usize / style_count) % shape_count
}

fn form_variant(seed: i64, style_count: usize, shape_count: usize) -> usize {
    let divisor = style_count * shape_count.max(1);
    (seed.unsigned_abs() as usize / divisor) % HIGH_STAGE_FORM_VARIANTS
}

fn form_tree_art(lines: Vec<&'static str>, form: usize) -> Vec<String> {
    lines
        .into_iter()
        .map(|line| retexture_line(line, form))
        .collect()
}

/// Swap heavy foliage chars for a distinct texture per form. Edge markers
/// (`.`, `'`, `,`) and `*` flowers pass through untouched so pad silhouettes
/// and blossoms stay intact.
fn retexture_line(line: &str, form: usize) -> String {
    if form == 0 {
        return line.to_string();
    }
    line.chars()
        .map(|ch| match (form, ch) {
            // Airy: heavy foliage reads as open dots.
            (1, '@') | (1, '#') => 'o',
            // Dense: swap the main foliage char to the other heavy glyph.
            (2, '@') => '#',
            (2, '#') => '@',
            _ => ch,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_ascii_returns_lines_for_all_stages() {
        let stages = [
            Stage::Dead,
            Stage::Seed,
            Stage::Sprout,
            Stage::Sapling,
            Stage::Young,
            Stage::Mature,
            Stage::Ancient,
            Stage::Blossom,
        ];

        for stage in stages {
            for seed in 0..3 {
                let lines = tree_ascii(stage, seed, false);
                assert!(
                    !lines.is_empty(),
                    "stage {:?} seed {seed} has no art",
                    stage
                );
            }
        }
    }

    #[test]
    fn different_seeds_can_produce_different_variants() {
        let a = tree_ascii(Stage::Young, 0, false);
        let b = tree_ascii(Stage::Young, 1, false);
        let c = tree_ascii(Stage::Young, 2, false);

        assert!(a != b || b != c || a != c);
    }

    #[test]
    fn high_stage_seeds_can_keep_style_with_different_forms() {
        let style = tree_variant_name(Stage::Mature, 0);
        assert_eq!(style, tree_variant_name(Stage::Mature, 8));
        assert_eq!(style, tree_variant_name(Stage::Mature, 16));

        let upright = tree_ascii(Stage::Mature, 0, false);
        let slim = tree_ascii(Stage::Mature, 8, false);
        let full = tree_ascii(Stage::Mature, 16, false);

        assert_ne!(upright, slim);
        assert_ne!(upright, full);
        assert_ne!(slim, full);
    }

    #[test]
    fn status_specs_for_dead_tree_show_respawn_hint() {
        assert_eq!(
            status_line_specs(false, Stage::Dead, false),
            vec![StatusLineSpec::DeadHint]
        );
    }

    #[test]
    fn status_specs_show_stage_and_watering_status() {
        assert_eq!(status_line_specs(true, Stage::Young, true), vec![]);
        assert_eq!(
            status_line_specs(true, Stage::Young, false),
            vec![StatusLineSpec::WateredToday]
        );
    }
}
