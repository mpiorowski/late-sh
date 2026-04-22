use std::time::SystemTime;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use super::state::{BonsaiState, Stage};
use crate::app::common::theme;

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

    // Render tree lines
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
    let w = inner.width as usize;

    // Count canopy lines (contain @, #, or *) for per-line falloff
    let canopy_count = if has_canopy {
        tree_art
            .iter()
            .filter(|l| l.chars().any(|c| matches!(c, '@' | '#' | '*')))
            .count()
    } else {
        0
    };

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
        for ch in art_line.chars() {
            let color = match ch {
                '|' | '/' | '\\' | '_' | '~' => trunk_color,
                '.' | '\'' | ',' | '*' | '@' | '#' | 'o' | 'O' => leaf_color,
                '[' | ']' | '=' => theme::TEXT_DIM(), // pot
                _ => theme::TEXT_FAINT(),
            };
            spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
        }

        // Manual centering with sway offset
        let art_width = art_line.chars().count();
        let base_pad = w.saturating_sub(art_width) / 2;
        let pad = (base_pad as i32 + offset).max(0) as usize;
        let pad = pad.min(w.saturating_sub(art_width));
        spans.insert(0, Span::raw(" ".repeat(pad)));
        lines.push(Line::from(spans));
    }

    // Pad to push status to bottom
    while lines.len() < available.saturating_sub(status_height) {
        lines.push(Line::from(""));
    }

    lines.extend(status_lines);

    frame.render_widget(Paragraph::new(lines), inner);
}

fn status_lines(state: &BonsaiState) -> Vec<Line<'static>> {
    status_line_specs(state.is_alive, state.stage(), state.can_water())
        .into_iter()
        .map(|spec| match spec {
            StatusLineSpec::DeadHint => Line::from(Span::styled(
                "Press r to plant anew",
                Style::default().fg(theme::TEXT_FAINT()),
            ))
            .centered(),
            StatusLineSpec::StageLabel(label) => Line::from(Span::styled(
                label,
                Style::default()
                    .fg(theme::TEXT_MUTED())
                    .add_modifier(Modifier::BOLD),
            ))
            .centered(),
            StatusLineSpec::CanWater => Line::from(vec![
                Span::styled("w", Style::default().fg(theme::AMBER())),
                Span::styled(" water", Style::default().fg(theme::TEXT_DIM())),
            ])
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
    StageLabel(String),
    CanWater,
    WateredToday,
}

fn status_line_specs(is_alive: bool, stage: Stage, can_water: bool) -> Vec<StatusLineSpec> {
    if !is_alive {
        return vec![StatusLineSpec::DeadHint];
    }

    let mut lines = vec![StatusLineSpec::StageLabel(stage.label().to_string())];
    if can_water {
        lines.push(StatusLineSpec::CanWater);
    } else {
        lines.push(StatusLineSpec::WateredToday);
    }
    lines
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

pub(super) fn tree_ascii(stage: Stage, seed: i64, _wilting: bool) -> Vec<&'static str> {
    // Seed → per-stage variant picker. Each stage applies its own modulo so we
    // can add variants stage-by-stage without shifting the others around.
    let v = seed.unsigned_abs() as usize;

    match stage {
        Stage::Dead => match v % 3 {
            // bare stick
            0 => vec!["   .  ", "  /|  ", " / |  ", "  .|. ", " [===]"],
            // withered stump
            1 => vec!["      ", "  ,.  ", "   \\  ", "  .|. ", " [===]"],
            // snapped twig
            _ => vec!["  .   ", "   `  ", "  .|  ", "  .|. ", " [===]"],
        },
        Stage::Seed => match v % 2 {
            // buried seed
            0 => vec!["      ", "      ", "   .  ", "  .|. ", " [===]"],
            // tiny peek
            _ => vec!["      ", "   .  ", "   ,  ", "  .|. ", " [===]"],
        },
        Stage::Sprout => match v % 4 {
            // three-leaf crown
            0 => vec!["      ", "   ,  ", "  /|\\ ", "  .|. ", " [===]"],
            // paired leaves
            1 => vec!["      ", "   .  ", "  '|, ", "  .|. ", " [===]"],
            // upward shoots
            2 => vec!["  ..  ", "   |  ", "   |, ", "  .|. ", " [===]"],
            // hooked shoot
            _ => vec!["   .  ", "   ,  ", "   |/ ", "  .|. ", " [===]"],
        },
        Stage::Sapling => match v % 4 {
            // formal upright
            0 => vec!["  ..  ", "  .'' ", "  /|\\ ", "   |  ", "  .|. ", " [===]"],
            // slanting
            1 => vec!["   ., ", "  '., ", "   |/ ", "   |  ", "  .|. ", " [===]"],
            // broom start
            2 => vec![" .,., ", "  \\|/ ", "   |  ", "   |  ", "  .|. ", " [===]"],
            // semi-cascade start
            _ => vec!["      ", "  ., ,", "  '\\| ", "   |  ", "  .|. ", " [===]"],
        },
        Stage::Young => match v % 5 {
            // formal upright
            0 => vec![
                "  .##.  ",
                " .####. ",
                " ##/\\## ",
                "  /  \\  ",
                "  |  |  ",
                "  .|.   ",
                " [===]  ",
            ],
            // informal upright
            1 => vec![
                "   .#.  ",
                " .####. ",
                " ##||## ",
                "   /\\   ",
                "   ||   ",
                "  .|.   ",
                " [===]  ",
            ],
            // twin-canopy
            2 => vec![
                "  ,##,  ",
                " .####. ",
                "  #/\\#  ",
                "  /  \\  ",
                "  |__|  ",
                "  .|.   ",
                " [===]  ",
            ],
            // slanting (Shakan)
            3 => vec![
                "   .##. ",
                "  .####.",
                "  ##/## ",
                "    /\\  ",
                "   / |  ",
                "  .|.   ",
                " [===]  ",
            ],
            // broom (Hokidachi)
            _ => vec![
                " .####. ",
                " ###### ",
                " ##/\\## ",
                "   ||   ",
                "   ||   ",
                "  .|.   ",
                " [===]  ",
            ],
        },
        Stage::Mature => match v % 5 {
            // formal upright
            0 => vec![
                "   .@@@.   ",
                " .@@@@@@@. ",
                " @@@/~\\@@@ ",
                "  @@| |@@  ",
                "    / \\    ",
                "   /   \\   ",
                "   |   |   ",
                "    .|.    ",
                "   [===]   ",
            ],
            // informal upright
            1 => vec![
                "  .,@@@,.  ",
                " .@@@@@@@. ",
                " @@/   \\@@ ",
                "  @|   |@  ",
                "   \\  /    ",
                "    ||     ",
                "    ||     ",
                "    .|.    ",
                "   [===]   ",
            ],
            // split trunk
            2 => vec![
                "   .@@@.   ",
                " .@@@ @@@. ",
                " @@@| |@@@ ",
                "  @@\\ /@@  ",
                "    | |    ",
                "   /   \\   ",
                "   |   |   ",
                "    .|.    ",
                "   [===]   ",
            ],
            // twin-trunk (Sokan)
            3 => vec![
                "  .@@.@@.  ",
                " @@@@.@@@@ ",
                " @@@|.|@@@ ",
                " @@||.||@@ ",
                "   \\|.|/   ",
                "    \\ /    ",
                "     |     ",
                "    .|.    ",
                "   [===]   ",
            ],
            // windswept (Fukinagashi)
            _ => vec![
                "    .@@@@@.",
                "  .@@@@@@@ ",
                "  @@@@@@@  ",
                "  @@@@@/   ",
                "    \\|/    ",
                "     |     ",
                "     |     ",
                "    .|.    ",
                "   [===]   ",
            ],
        },
        Stage::Ancient => match v % 5 {
            // formal upright
            0 => vec![
                "     .@@@.     ",
                "   .@@@@@@@.   ",
                "  .@@@@@@@@@.  ",
                " .@@@@@@@@@@@. ",
                " @@@@/~~~\\@@@@ ",
                "  @@@|   |@@@  ",
                "    /     \\    ",
                "   /  / \\  \\   ",
                "   | /   \\ |   ",
                "   |/     \\|   ",
                "    |     |    ",
                "     \\   /     ",
                "      .|.      ",
                "     [===]     ",
            ],
            // tilted crown
            1 => vec![
                "   .@@@@@@.    ",
                " .@@@@@@@@@@.  ",
                " @@@@@@@@@@@@. ",
                " @@@@@/\\@@@@@@.",
                "  @@@@|  |@@@@.",
                "    @@/  \\@@   ",
                "     /    \\    ",
                "    / \\  / \\   ",
                "   |   \\/   |  ",
                "   |   ||   |  ",
                "    \\  ||  /   ",
                "     \\ || /    ",
                "      .|.      ",
                "     [===]     ",
            ],
            // broad canopy
            2 => vec![
                "     .@@@@.    ",
                "   .@@@@@@@@.  ",
                "  .@@@@@@@@@@. ",
                " .@@@@@@@@@@@@.",
                " @@@@/~~\\@@@@@ ",
                "  @@@|  |@@@@  ",
                "    /    \\     ",
                "   / /\\   \\    ",
                "  | |  |   |   ",
                "  |  \\/    |   ",
                "   \\  ||  /    ",
                "    \\ || /     ",
                "      .|.      ",
                "     [===]     ",
            ],
            // literati (Bunjingi) — tall thin trunk
            3 => vec![
                "     .@@@.     ",
                "   .@@@@@@@.   ",
                "   @@/~~~\\@@   ",
                "    @|   |@    ",
                "     |   |     ",
                "     |   |     ",
                "     |   |     ",
                "     |   |     ",
                "     |   |     ",
                "      \\ /      ",
                "       |       ",
                "       |       ",
                "      .|.      ",
                "     [===]     ",
            ],
            // broom (Hokidachi) — fan canopy
            _ => vec![
                "  @@@@@@@@@@@  ",
                " @@@@@@@@@@@@@ ",
                " @@@@@@@@@@@@@ ",
                " @@@@@@@@@@@@@ ",
                " @@@/|||||\\@@@ ",
                "  @@|     |@@  ",
                "    |     |    ",
                "    |     |    ",
                "    |     |    ",
                "    \\     /    ",
                "     \\   /     ",
                "      \\ /      ",
                "      .|.      ",
                "     [===]     ",
            ],
        },
        Stage::Blossom => match v % 5 {
            // formal flowering
            0 => vec![
                "    .*@@@@@*.    ",
                "  .*@@@*@*@@@@.  ",
                " .*@@@@@@@@@@@*. ",
                " .*@@@@*@@@@@@*. ",
                " *@@@@/~~~\\@@@@* ",
                "  *@@@|   |@@@*  ",
                "     /     \\     ",
                "    / */ \\* \\    ",
                "    | /   \\ |    ",
                "    |/     \\|    ",
                "     |     |     ",
                "      \\   /      ",
                "       .|.       ",
                "      [===]      ",
            ],
            // tilted bloom
            1 => vec![
                "   .*@@@@@@*.    ",
                " .*@@@@*@@@@@@.  ",
                " .*@@@*@@*@@@@*. ",
                " *@@@@@/\\*@@@@@*.",
                "  *@@@@|  |@@@@*.",
                "     @@/  \\@@    ",
                "      /    \\     ",
                "     / *\\*/ \\    ",
                "    |   \\/   |   ",
                "    |   ||   |   ",
                "     \\  ||  /    ",
                "      \\ || /     ",
                "       .|.       ",
                "      [===]      ",
            ],
            // broad bloom
            2 => vec![
                "     .*@@@@*.    ",
                "  .*@@*@@@*@@*.  ",
                "  .*@@@@*@@@@@*. ",
                " .*@@@@@@@@@@@@*.",
                " *@@@@/~~\\@@@@@* ",
                "  *@@@|  |@@@@*  ",
                "     /    \\      ",
                "    / */\\*  \\    ",
                "   |  |  |   |   ",
                "   |   \\/    |   ",
                "    \\  ||   /    ",
                "     \\ ||  /     ",
                "       .|.       ",
                "      [===]      ",
            ],
            // flowering literati
            3 => vec![
                "      .@*@.      ",
                "    .*@@@@@*.    ",
                "   .*@@@*@@@*.   ",
                "    *@@/~\\@@*    ",
                "      @| |@      ",
                "       | |       ",
                "       | |       ",
                "       | |       ",
                "       | |       ",
                "       | |       ",
                "       \\ /       ",
                "        |        ",
                "       .|.       ",
                "      [===]      ",
            ],
            // flowering broom
            _ => vec![
                "  *@*@*@*@*@*@*  ",
                " @@@@@@@@@@@@@@@ ",
                " @*@*@*@*@*@*@*@ ",
                " @@@@@@@@@@@@@@@ ",
                " @@@/|||||||\\@@@ ",
                "  @@|       |@@  ",
                "    |       |    ",
                "    |       |    ",
                "    |       |    ",
                "    \\       /    ",
                "     \\     /     ",
                "      \\   /      ",
                "       .|.       ",
                "      [===]      ",
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
        assert_eq!(
            status_line_specs(true, Stage::Young, true),
            vec![
                StatusLineSpec::StageLabel("Young Tree".to_string()),
                StatusLineSpec::CanWater,
            ]
        );
        assert_eq!(
            status_line_specs(true, Stage::Young, false),
            vec![
                StatusLineSpec::StageLabel("Young Tree".to_string()),
                StatusLineSpec::WateredToday,
            ]
        );
    }
}
