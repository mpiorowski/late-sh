//! The box: a title, the pitch, a breaker carrying its two keys, centered
//! over whatever page the login landed on, in the tour's voice and frame.

use late_core::radio::RadioStation;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::app::common::theme;
use crate::app::door::hub::state::{HubGame, HubGroup};
use crate::app::help_modal::data::SHELL_INSTALL_COMMAND;

use super::state::Moment;

/// Blank columns between the frame and the text, as in the tour's boxes.
const SIDE_PADDING: u16 = 2;
/// Columns a roster line may run to before the next station starts a new
/// one, so the box stays readable on an 80-column page.
const ROSTER_WIDTH: usize = 68;

/// What the box says for this session: `hears` is whether a client is
/// paired on its token (the `late` CLI), `listen_url` the public listen
/// page.
pub(crate) struct View<'a> {
    pub moment: Moment,
    pub hears: bool,
    pub listen_url: &'a str,
}

struct Box_ {
    title: &'static str,
    lines: Vec<Line<'static>>,
    keys: Vec<Span<'static>>,
}

fn compose(view: &View<'_>) -> Box_ {
    let key = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(theme::TEXT());
    let name = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(theme::TEXT_DIM());
    let stations = RadioStation::enabled().count();
    let roster = roster_lines(dim);
    let listen = view
        .listen_url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .to_string();
    let web = listen.trim_end_matches("/listen").to_string();
    match (view.moment, view.hears) {
        (Moment::HearIt, false) => Box_ {
            title: " ☾ the house has a radio ☽ ",
            lines: vec![Line::from(vec![
                Span::styled(format!("{stations} radio stations"), name),
                Span::styled(" and a ", text),
                Span::styled("YouTube booth", name),
                Span::styled(", all night, the same for everyone.", text),
            ])]
            .into_iter()
            .chain(roster.clone())
            .chain(vec![
                Line::from(Span::styled("over plain ssh you hear none of it.", text)),
                Line::default(),
                Line::from(vec![
                    Span::styled("the ", text),
                    Span::styled("late", name),
                    Span::styled(
                        " CLI plays it in your terminal, the chat and the boards beside it:",
                        text,
                    ),
                ]),
                Line::from(Span::styled(format!("    {SHELL_INSTALL_COMMAND}"), name)),
                Line::from(vec![
                    Span::styled("then run ", text),
                    Span::styled("late", name),
                    Span::styled(" instead of ", text),
                    Span::styled("ssh late.sh", name),
                    Span::styled(".", text),
                ]),
                Line::default(),
                Line::from(vec![
                    Span::styled("no install? ", text),
                    Span::styled(listen, name),
                    Span::styled(" plays it in any browser, on a phone too.", text),
                ]),
            ])
            .collect(),
            keys: vec![
                Span::styled("[Enter] ", key),
                Span::styled("the install guide, with a QR for your phone · ", text),
                Span::styled("[Esc] ", key),
                Span::styled("later", text),
            ],
        },
        (Moment::HearIt, true) => Box_ {
            title: " ☾ the house has a radio ☽ ",
            lines: vec![Line::from(vec![
                Span::styled(format!("{stations} radio stations"), name),
                Span::styled(" and a ", text),
                Span::styled("YouTube booth", name),
                Span::styled(", all night, the same for everyone.", text),
            ])]
            .into_iter()
            .chain(roster)
            .chain(vec![
                Line::from(vec![
                    Span::styled("you are on the ", text),
                    Span::styled("late", name),
                    Span::styled(" CLI, so you can hear it.", text),
                ]),
                Line::default(),
                Line::from(vec![
                    Span::styled("[m] ", key),
                    Span::styled("mute and unmute · ", text),
                    Span::styled("[v] ", key),
                    Span::styled("then ", text),
                    Span::styled("[r] ", key),
                    Span::styled("picks a station", text),
                ]),
                Line::from(vec![
                    Span::styled("[v] ", key),
                    Span::styled("then ", text),
                    Span::styled("[v] ", key),
                    Span::styled("opens the ", text),
                    Span::styled("Music Booth", name),
                    Span::styled(": queue a track for the house, for chips.", text),
                ]),
            ])
            .collect(),
            keys: vec![
                Span::styled("[Enter] ", key),
                Span::styled("pick a station · ", text),
                Span::styled("[Esc] ", key),
                Span::styled("later", text),
            ],
        },
        (Moment::BigGames, false | true) => Box_ {
            title: " ☾ the big games ☽ ",
            lines: vec![Line::from(vec![
                Span::styled("page ", text),
                Span::styled("[3]", key),
                Span::styled(
                    " is the Games hub: whole games, your save kept between nights.",
                    text,
                ),
            ])]
            .into_iter()
            .chain(hub_lines(dim, name))
            .chain(vec![
                Line::default(),
                Line::from(vec![
                    Span::styled("the ", text),
                    Span::styled("Lobby", name),
                    Span::styled(", ", text),
                    Span::styled("[Ctrl+G]", key),
                    Span::styled(" from anywhere: chess, poker, pool, snooker,", text),
                ]),
                Line::from(Span::styled("blackjack, with whoever is up.", text)),
                Line::from(vec![
                    Span::styled("a run in progress shows on the hub's live rail: ", text),
                    Span::styled("[Enter]", key),
                    Span::styled(" on it watches.", text),
                ]),
                Line::default(),
                Line::from(Span::styled(
                    "and one the hub does not list. something is moving in the static,",
                    dim,
                )),
                Line::from(Span::styled("for those who stick around.", dim)),
            ])
            .collect(),
            keys: vec![
                Span::styled("[Enter] ", key),
                Span::styled("open the Games hub · ", text),
                Span::styled("[Esc] ", key),
                Span::styled("later", text),
            ],
        },
        (Moment::People, false | true) => Box_ {
            title: " ☾ the people ☽ ",
            lines: vec![
                Line::from(vec![
                    Span::styled("page ", text),
                    Span::styled("[5]", key),
                    Span::styled(" is People, three shelves: ", text),
                    Span::styled("[", key),
                    Span::styled(" and ", text),
                    Span::styled("]", key),
                    Span::styled(" walk them.", text),
                ]),
                Line::default(),
                Line::from(vec![
                    Span::styled(format!("  {:<11}", "Map"), dim),
                    Span::styled(
                        "who is here, on the Earth: online, this month, ever (",
                        text,
                    ),
                    Span::styled("[m]", key),
                    Span::styled(").", text),
                ]),
                Line::from(vec![
                    Span::styled(format!("  {:<11}", "Profiles"), dim),
                    Span::styled(
                        "work cards: what people do and know, and their projects (",
                        text,
                    ),
                    Span::styled("[w]", key),
                    Span::styled(").", text),
                ]),
                Line::from(vec![
                    Span::styled(format!("  {:<11}", "Jobs"), dim),
                    Span::styled(
                        "remote postings, new every night, matched to your card (",
                        text,
                    ),
                    Span::styled("[/]", key),
                    Span::styled(").", text),
                ]),
                Line::default(),
                Line::from(vec![
                    Span::styled("your card is a page on the web too: ", text),
                    Span::styled(format!("{web}/profiles"), name),
                    Span::styled(".", text),
                ]),
                Line::from(vec![
                    Span::styled("[p]", key),
                    Span::styled(" on any name in chat opens theirs.", text),
                ]),
            ],
            keys: vec![
                Span::styled("[Enter] ", key),
                Span::styled("open People · ", text),
                Span::styled("[Esc] ", key),
                Span::styled("later", text),
            ],
        },
    }
}

/// The Games hub's roster by group, as the hub shows it to everyone (the
/// runners' own card left off), from the hub's own roster so a door added
/// or dropped shows up here without a copy.
fn hub_lines(dim: Style, name: Style) -> Vec<Line<'static>> {
    let groups = [
        HubGroup::House,
        HubGroup::Roguelikes,
        HubGroup::Remakes,
        HubGroup::Servers,
        HubGroup::Doors,
    ];
    groups
        .into_iter()
        .map(|group| {
            let names: Vec<&'static str> = HubGame::roster(false)
                .iter()
                .filter(|game| game.group() == group)
                .map(|game| game.label())
                .collect();
            Line::from(vec![
                Span::styled(format!("  {:<11}", group.label()), dim),
                Span::styled(names.join(" · "), name),
            ])
        })
        .collect()
}

/// Every enabled station by name, in catalogue order (grouped by section),
/// indented under the count and wrapped at [`ROSTER_WIDTH`].
fn roster_lines(style: Style) -> Vec<Line<'static>> {
    let mut lines: Vec<String> = Vec::new();
    for label in RadioStation::enabled().map(RadioStation::label) {
        match lines.last_mut() {
            Some(line) if line.width() + 3 + label.width() <= ROSTER_WIDTH => {
                line.push_str(" · ");
                line.push_str(label);
            }
            Some(_) | None => lines.push(format!("  {label}")),
        }
    }
    lines
        .into_iter()
        .map(|line| Line::from(Span::styled(line, style)))
        .collect()
}

/// The box, centered over the page a third of the way down like the
/// tour's, as wide as its longest line and never wider than the page.
pub(crate) fn draw(frame: &mut Frame, area: Rect, view: &View<'_>) {
    let composed = compose(view);
    let amber = Style::default().fg(theme::AMBER());
    let keys_width: usize = composed.keys.iter().map(Span::width).sum();
    let content = composed
        .lines
        .iter()
        .map(Line::width)
        .max()
        .unwrap_or(0)
        .max(composed.title.width())
        // A breaker needs some rule left of its keys to read as one.
        .max(keys_width + 12);
    let width = (content + 2 + 2 * usize::from(SIDE_PADDING))
        .min(usize::from(area.width).saturating_sub(2)) as u16;
    // The title, the pitch, a breathing row, the breaker, a breathing row
    // each side, the frame.
    let height = (composed.lines.len() as u16 + 7).min(area.height.saturating_sub(1));
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 3,
        width,
        height,
    };
    let block = Block::default().borders(Borders::ALL).border_style(amber);
    let inner = block.inner(rect).inner(Margin::new(SIDE_PADDING, 1));
    frame.render_widget(Clear, rect);
    frame.render_widget(block, rect);

    let mut lines = vec![Line::from(Span::styled(
        composed.title,
        amber.add_modifier(Modifier::BOLD),
    ))];
    lines.extend(composed.lines);
    // The breaker is the last row the box owns, so on a short page the
    // pitch is what gets cut, never the keys.
    let rows = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);
    frame.render_widget(Paragraph::new(lines), rows[0]);
    let lead = usize::from(inner.width).saturating_sub(keys_width + 4);
    let mut breaker = vec![Span::styled(format!("{} ", "─".repeat(lead)), amber)];
    breaker.extend(composed.keys);
    breaker.push(Span::styled(" ──", amber));
    frame.render_widget(Paragraph::new(Line::from(breaker)), rows[2]);
}
