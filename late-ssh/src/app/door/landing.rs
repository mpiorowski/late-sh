//! Shared line builders for the door-game landing pages (Lateania, Rebels,
//! NetHack). Keeping the section/stat/action/hint styling in one place stops the
//! three landings from drifting apart, as they had. The rules these encode:
//! amber-bold is for headings only, hint keys are bright-bold, and the action
//! label is full-bright. Per-game flavor (logos, art, glyphs, quotes) stays in
//! each game's own render module.

use ratatui::Frame;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

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
        // The claim itself happens in the dedicated modal (`draw_name_modal`);
        // this is what shows behind it, and after Esc closes it.
        HandleStatus::Missing { .. } => vec![
            action(">", "Enter", "claim your arcade name", theme::SUCCESS()),
            dim("One name for every arcade game; you need it before playing.".to_string()),
            Line::from(""),
        ],
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

const NAME_MODAL_WIDTH: u16 = 62;
const NAME_MODAL_HEIGHT: u16 = 13;

/// The one-time arcade-name claim modal, shared by every door that keys saves
/// by the handle (DCSS, NetHack). Pops centered over the door landing the
/// first time an account without a handle tries to play; disappears for good
/// once a name is claimed. Every state keeps the same fixed layout so nothing
/// jumps while a claim resolves.
pub fn draw_name_modal(
    frame: &mut Frame,
    area: Rect,
    status: crate::app::door::arcade::HandleStatus,
    entry: &str,
) {
    use crate::app::door::arcade::HandleStatus;

    let popup = centered_rect(NAME_MODAL_WIDTH, NAME_MODAL_HEIGHT, area);
    frame.render_widget(Clear, popup);

    let block = Block::default()
        .title(" Your arcade name ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let rows = Layout::vertical([
        Constraint::Length(1), // breathing room
        Constraint::Length(3), // what this is
        Constraint::Length(1), // gap
        Constraint::Length(1), // name input
        Constraint::Length(1), // gap
        Constraint::Length(1), // rules / error / progress
        Constraint::Min(0),    // spacer
        Constraint::Length(1), // footer
    ])
    .split(inner);

    let intro = vec![
        Line::from(Span::styled(
            "One name for every arcade game. It labels your saves",
            Style::default().fg(theme::TEXT()),
        )),
        Line::from(Span::styled(
            "and public scores, and cannot be changed later.",
            Style::default().fg(theme::TEXT()),
        )),
        // Sequell and dcss-stats merge players by bare name across servers,
        // so a common name inherits someone else's score history.
        Line::from(Span::styled(
            "Roguelike stat sites merge by name; pick one that's yours.",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
    ];
    frame.render_widget(Paragraph::new(intro).centered(), rows[1]);

    let input = Line::from(vec![
        Span::styled("> ", Style::default().fg(theme::SUCCESS())),
        Span::styled("name: ", Style::default().fg(theme::TEXT())),
        Span::styled(
            entry.to_string(),
            Style::default()
                .fg(theme::TEXT_BRIGHT())
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("_", Style::default().fg(theme::AMBER())),
    ]);
    frame.render_widget(Paragraph::new(input).centered(), rows[3]);

    let status_line = match &status {
        HandleStatus::Missing { error: Some(msg) } => Line::from(Span::styled(
            msg.clone(),
            Style::default().fg(theme::ERROR()),
        )),
        HandleStatus::Missing { error: None } => Line::from(Span::styled(
            "3-20 characters: letters, digits, underscore.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        HandleStatus::Claiming => Line::from(Span::styled(
            "Claiming the name...",
            Style::default().fg(theme::AMBER()),
        )),
        HandleStatus::Failed => Line::from(Span::styled(
            "Couldn't reach the name service.",
            Style::default().fg(theme::ERROR()),
        )),
        // Loading and Claimed never show the modal.
        HandleStatus::Loading | HandleStatus::Claimed(_) => Line::from(""),
    };
    frame.render_widget(Paragraph::new(status_line).centered(), rows[5]);

    let footer_cols = Layout::horizontal([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Fill(1),
        Constraint::Length(2),
    ])
    .split(rows[7]);
    let enter_label = if matches!(status, HandleStatus::Failed) {
        " retry"
    } else {
        " claim and play"
    };
    let left = Line::from(vec![
        Span::styled("Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(enter_label, Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(left), footer_cols[1]);
    let right = Line::from(vec![
        Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" not now", Style::default().fg(theme::TEXT_DIM())),
    ]);
    frame.render_widget(Paragraph::new(right).right_aligned(), footer_cols[2]);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height.min(area.height))])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width.min(area.width))])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
