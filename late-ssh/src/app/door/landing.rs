//! Shared line builders for the door-game landing pages (Lateania, Rebels,
//! NetHack). Keeping the section/stat/action/hint styling in one place stops the
//! three landings from drifting apart, as they had. The rules these encode:
//! amber-bold is for headings only, hint keys are bright-bold, and the action
//! label is full-bright. Per-game flavor (logos, art, glyphs, quotes) stays in
//! each game's own render module.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::common::theme;

/// A landing section heading. Amber-bold is reserved for these.
pub fn heading(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))
}

/// A `label  value` stat row. `pad` is the label column width; each landing sizes
/// it to its own longest label so the value column lines up.
pub fn stat(label: &str, value: &str, pad: usize) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{label:<pad$}"),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.to_string(), Style::default().fg(theme::TEXT_DIM())),
    ])
}

/// A launch/action row: `marker key  label`, with the marker and key tinted by
/// `color` (e.g. green to go, red to destroy) and the label at full brightness.
pub fn action(marker: &str, key: &str, label: &str, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{marker} "), Style::default().fg(color)),
        Span::styled(
            format!("{key:<8}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(label.to_string(), Style::default().fg(theme::TEXT())),
    ])
}

/// A `key  label` hint row. `pad` sizes the key column to the landing's longest
/// key so the labels line up.
pub fn hint(key: &str, label: &str, pad: usize) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!("{key:<pad$}"),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(label.to_string(), Style::default().fg(theme::TEXT_DIM())),
    ])
}

/// The handle-aware Launch block for doors that key saves by the arcade handle
/// (DCSS, NetHack): the one-time claim prompt, the in-flight states, and the
/// ready-to-play action. Always exactly three lines, so the landing's chrome
/// never moves as lookups and claims resolve. `play_action` is the door's own
/// "Enter to play" line, shown once the handle is claimed.
pub fn handle_launch_block(
    status: crate::app::door::arcade::HandleStatus,
    entry: &str,
    play_action: Line<'static>,
) -> Vec<Line<'static>> {
    use crate::app::door::arcade::HandleStatus;

    let dim = |text: String| Line::from(Span::styled(text, Style::default().fg(theme::TEXT_DIM())));
    match status {
        HandleStatus::Loading => vec![
            dim("Checking your arcade name...".to_string()),
            Line::from(""),
            Line::from(""),
        ],
        HandleStatus::Missing { error } => {
            let notice = match error {
                Some(msg) => Line::from(Span::styled(msg, Style::default().fg(theme::ERROR()))),
                None => Line::from(Span::styled(
                    "Shown publicly with your games. Cannot be changed later.",
                    Style::default().fg(theme::TEXT_FAINT()),
                )),
            };
            vec![
                Line::from(vec![
                    Span::styled("> ", Style::default().fg(theme::SUCCESS())),
                    Span::styled("claim your arcade name: ", Style::default().fg(theme::TEXT())),
                    Span::styled(
                        entry.to_string(),
                        Style::default()
                            .fg(theme::TEXT_BRIGHT())
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("_", Style::default().fg(theme::AMBER())),
                ]),
                dim("3-20 characters: letters, digits, underscore. Enter claims and plays."
                    .to_string()),
                notice,
            ]
        }
        HandleStatus::Claiming => vec![
            dim(format!("Claiming {entry}...")),
            Line::from(""),
            Line::from(""),
        ],
        HandleStatus::Claimed(name) => vec![
            play_action,
            dim(format!("Playing as {name}.")),
            Line::from(""),
        ],
        HandleStatus::Failed => vec![
            Line::from(Span::styled(
                "Couldn't check your arcade name.",
                Style::default().fg(theme::ERROR()),
            )),
            action(">", "Enter", "retry", theme::SUCCESS()),
            Line::from(""),
        ],
    }
}
