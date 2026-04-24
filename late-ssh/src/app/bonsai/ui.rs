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

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if state.is_alive {
            theme::BORDER()
        } else {
            theme::TEXT_FAINT()
        }));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    draw_water_hint(frame, area, state.can_water());

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

fn draw_water_hint(frame: &mut Frame, area: Rect, can_water: bool) {
    if !can_water || area.width < 12 {
        return;
    }
    let width = 9;
    let hint_area = Rect {
        x: area.x + area.width.saturating_sub(width + 2),
        y: area.y,
        width,
        height: 1,
    };
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            "w",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" care ", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(line), hint_area);
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
        Stage::Young => match v % 7 {
            0 => ("Chokkan", "formal upright"),
            1 => ("Moyogi", "informal upright"),
            2 => ("Shakan", "slanting"),
            3 => ("Fukinagashi", "windswept"),
            4 => ("Hokidachi", "broom"),
            5 => ("Sokan", "twin trunk"),
            _ => ("Bunjingi", "literati"),
        },
        Stage::Mature | Stage::Ancient => match v % 8 {
            0 => ("Chokkan", "formal upright"),
            1 => ("Moyogi", "informal upright"),
            2 => ("Shakan", "slanting"),
            3 => ("Fukinagashi", "windswept"),
            4 => ("Sokan", "twin trunk"),
            5 => ("Hokidachi", "broom"),
            6 => ("Bunjingi", "literati"),
            _ => ("Neagari", "exposed root"),
        },
        Stage::Blossom => match v % 8 {
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

pub(crate) fn tree_ascii(stage: Stage, seed: i64, _wilting: bool) -> Vec<&'static str> {
    // Seed → per-stage variant picker. Each stage applies its own modulo so we
    // can add variants stage-by-stage without shifting the others around.
    // Design language for mature stages: discrete "foliage pads" with visible
    // trunk between them, and lateral branches carrying side pads — a real
    // bonsai silhouette, not a blob on a stick.
    let v = seed.unsigned_abs() as usize;

    match stage {
        Stage::Dead => match v % 4 {
            // bare stick
            0 => vec!["   .   ", "  /|   ", " / |   ", "   |`  ", "  .|.  ", " [===] "],
            // withered stump
            1 => vec!["       ", "   ,.  ", "    \\  ", "   .|  ", "  .|.  ", " [===] "],
            // snapped twig
            2 => vec!["  .    ", "   `   ", "   |   ", "  .|   ", "  .|.  ", " [===] "],
            // leafless claw
            _ => vec![" .   . ", "  \\ /  ", "   V   ", "   |   ", "  .|.  ", " [===] "],
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
            0 => vec!["       ", "   ,   ", "  /|\\  ", "   |   ", "  .|.  ", " [===] "],
            // paired leaves
            1 => vec!["       ", "   .   ", "  '|,  ", "   |   ", "  .|.  ", " [===] "],
            // upward shoots
            2 => vec!["  ..   ", "   |   ", "   |,  ", "   |   ", "  .|.  ", " [===] "],
            // hooked shoot
            3 => vec!["   .   ", "   ,   ", "   |/  ", "   |   ", "  .|.  ", " [===] "],
            // twin shoots
            _ => vec!["  , ,  ", "  |,|  ", "  \\|/  ", "   |   ", "  .|.  ", " [===] "],
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
                "   .,   ",
                "   ,.,  ",
                "    |/  ",
                "    /   ",
                "   .|.  ",
                "  [===] ",
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
                "   ,.,  ",
                "  .'.,  ",
                " ~.|    ",
                "    |~. ",
                "   .|.  ",
                "  [===] ",
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
        Stage::Young => match v % 7 {
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
        Stage::Mature => match v % 8 {
            // Chokkan — three layered tiers, perfect symmetry
            0 => vec![
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
            // Moyogi — S-curve trunk, offset pads
            1 => vec![
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
            // Shakan — leaning left, compensating right pad
            2 => vec![
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
            // Fukinagashi — windswept right, trailing branches
            3 => vec![
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
            // Sokan — twin trunk, shared pot
            4 => vec![
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
            // Hokidachi — broom, fan crown
            5 => vec![
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
            // Bunjingi — literati, tall bare trunk, small crown
            6 => vec![
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
            // Neagari — exposed roots, layered crown
            _ => vec![
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
        },
        Stage::Ancient => match v % 8 {
            // Chokkan — five-tier classical layered
            0 => vec![
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
            // Moyogi — S-curve with three pads
            1 => vec![
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
            // Shakan — dramatic slant, three balancing pads
            2 => vec![
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
            // Fukinagashi — windswept right, long trailing foliage
            3 => vec![
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
            // Sokan — twin trunk, overlapping canopies
            4 => vec![
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
            // Hokidachi — broom, wide fan crown
            5 => vec![
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
            // Bunjingi — literati, tall stark trunk, tiny crown
            6 => vec![
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
            // Neagari — exposed root, layered crown
            _ => vec![
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
        },
        Stage::Blossom => match v % 8 {
            // Chokkan — layered with petals woven through
            0 => vec![
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
            // Moyogi — flowering S-curve, three pads
            1 => vec![
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
            // Shakan — slanting bloom, three cascading pads
            2 => vec![
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
            // Fukinagashi — wind-swept blossom
            3 => vec![
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
            // Sokan — twin flowering trunk
            4 => vec![
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
            // Hokidachi — flowering broom
            5 => vec![
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
            // Bunjingi — flowering literati
            6 => vec![
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
            // Neagari — blooming exposed root
            _ => vec![
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
        },
    }
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
