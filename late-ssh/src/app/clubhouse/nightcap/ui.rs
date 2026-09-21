//! Nightcap's one screen: a title, the row of stools, the last few things
//! said, a footer, and the room's composer under it all while you hold a
//! stool. Deliberately no backdrop art, no camera, no floor plan to walk
//! around: see the module doc on `state.rs` for why this stays small.
//!
//! The chat here is the last handful of lines and nothing else. The room is
//! hidden from Home, so there is no scrollback anywhere: what is said at
//! this bar scrolls off it.

use std::collections::HashMap;
use std::time::Duration;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use uuid::Uuid;

use late_core::models::chat_message::ChatMessage;
use late_core::models::drinks::drunk_label_word;

use crate::app::common::primitives::thousands;
use crate::app::common::theme;
use crate::usernames::UsernameLookup;

use super::lobby::SEAT_COUNT;
use super::state::{Drink, State};

/// The most lines the bar keeps on the wall, whatever the height.
const MAX_LINES: usize = 8;

pub(crate) struct NightcapView<'a> {
    pub state: &'a State,
    /// The nightcap room's tail, newest first; empty off this screen.
    pub messages: &'a [ChatMessage],
    pub usernames: &'a UsernameLookup<'a>,
    /// The tavern's drunk map, so a stool shows the same `(word)` chat does.
    pub drunk_levels: &'a HashMap<Uuid, u8>,
    /// The shared composer block, pinned under the bar. `Some` only while
    /// this session holds a stool: the footer appearing is the "you may
    /// speak" signal.
    pub composer: Option<crate::app::chat::ui::ComposerBlockView<'a>>,
}

pub(crate) fn draw(frame: &mut Frame, area: Rect, view: NightcapView<'_>) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let Some(composer) = &view.composer else {
        draw_bar(frame, area, &view);
        return;
    };
    // The composer footer keeps the compact height the dashboard card uses:
    // one placeholder line while idle, growing with the draft while typing.
    let composer_text_width = area.width.saturating_sub(2).max(1) as usize;
    let composer_lines = crate::app::chat::ui::chat_composer_lines_for_height(
        composer.composer,
        composer_text_width,
    )
    .max(crate::app::chat::ui::composer_placeholder_lines(
        composer,
        composer_text_width,
    ));
    let composer_height = (composer_lines.min(4) as u16 + 2).min(area.height.saturating_sub(4));
    let layout =
        Layout::vertical([Constraint::Fill(1), Constraint::Length(composer_height)]).split(area);

    draw_bar(frame, layout[0], &view);
    crate::app::chat::ui::draw_composer_block(frame, layout[1], composer);
}

fn draw_bar(frame: &mut Frame, area: Rect, view: &NightcapView<'_>) {
    let [header, seats, _gap, lines, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(SEAT_COUNT as u16),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(area);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                "nightcap",
                Style::default()
                    .fg(theme::AMBER_GLOW())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "a quiet spot out back of the clubhouse. the seated may speak.",
                Style::default().fg(theme::TEXT_DIM()),
            )),
        ]),
        header,
    );

    draw_seats(frame, seats, view);
    draw_lines(frame, lines, view);
    draw_footer(frame, footer, view.state);
}

fn draw_seats(frame: &mut Frame, area: Rect, view: &NightcapView<'_>) {
    let state = view.state;
    let snapshot = state.snapshot();
    let my_seat = state.my_seat();
    let rows: Vec<Constraint> = std::iter::repeat_n(Constraint::Length(1), SEAT_COUNT).collect();
    let layout = Layout::vertical(rows).split(area);

    for (idx, slot) in snapshot.iter().enumerate() {
        let seat_num = idx + 1;
        let mine = my_seat == Some(idx);
        let line = match slot {
            Some(occupant) => {
                let style = if mine {
                    Style::default()
                        .fg(theme::AMBER_GLOW())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme::TEXT_BRIGHT())
                };
                let name = if mine {
                    format!("{} (you)", occupant.username)
                } else {
                    occupant.username.clone()
                };
                let mut spans = vec![
                    Span::styled(
                        format!("{seat_num} "),
                        Style::default().fg(theme::TEXT_DIM()),
                    ),
                    Span::styled("●", style),
                    Span::raw(" "),
                    Span::styled(name, style),
                ];
                let level = view
                    .drunk_levels
                    .get(&occupant.user_id)
                    .copied()
                    .unwrap_or(0);
                if let Some(word) = drunk_label_word(level) {
                    spans.push(Span::styled(
                        format!(" ({word})"),
                        Style::default()
                            .fg(theme::AMBER_DIM())
                            .add_modifier(Modifier::ITALIC),
                    ));
                }
                if occupant.drinks > 0 {
                    spans.push(Span::raw("  "));
                    spans.push(Span::styled(
                        "●".repeat(occupant.drinks.min(5) as usize),
                        Style::default().fg(theme::AMBER_DIM()),
                    ));
                }
                spans.push(Span::styled(
                    format!("  {}", seated_label(occupant.seated_for)),
                    Style::default().fg(theme::TEXT_FAINT()),
                ));
                Line::from(spans)
            }
            None => Line::from(vec![
                Span::styled(
                    format!("{seat_num} "),
                    Style::default().fg(theme::TEXT_DIM()),
                ),
                Span::styled("○", Style::default().fg(theme::BORDER_DIM())),
                Span::styled(
                    " empty stool",
                    Style::default()
                        .fg(theme::TEXT_FAINT())
                        .add_modifier(Modifier::ITALIC),
                ),
            ]),
        };
        frame.render_widget(Paragraph::new(line), layout[idx]);
    }
}

/// How long a stool has been held, coarse on purpose: nobody at a quiet bar
/// is counting seconds.
pub(crate) fn seated_label(seated_for: Duration) -> String {
    let minutes = seated_for.as_secs() / 60;
    match minutes {
        0 => "just sat down".to_string(),
        1..=59 => format!("{minutes}m"),
        _ => format!("{}h {:02}m", minutes / 60, minutes % 60),
    }
}

/// The last few lines said at the bar, oldest at the top so the newest sits
/// nearest the composer. One line per message, cut to the width; a bar wall
/// has no room for paragraphs.
fn draw_lines(frame: &mut Frame, area: Rect, view: &NightcapView<'_>) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let keep = (area.height as usize).min(MAX_LINES);
    let width = area.width as usize;
    let name_style = Style::default().fg(theme::TEXT_DIM());
    let body_style = Style::default().fg(theme::TEXT_BRIGHT());
    let lines: Vec<Line> = view
        .messages
        .iter()
        .take(keep)
        .rev()
        .map(|message| {
            let name = view
                .usernames
                .get(&message.user_id)
                .map(String::as_str)
                .unwrap_or("someone");
            let body = message.body.lines().next().unwrap_or("");
            let prefix = format!("{name}  ");
            let room = width.saturating_sub(prefix.chars().count());
            let body: String = body.chars().take(room).collect();
            Line::from(vec![
                Span::styled(prefix, name_style),
                Span::styled(body, body_style),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &State) {
    let key = Style::default().fg(theme::AMBER_DIM());
    let hint = Style::default().fg(theme::TEXT_DIM());
    let spans = if state.menu_open() {
        let mut spans = Vec::new();
        for (idx, drink) in Drink::MENU.iter().enumerate() {
            spans.push(Span::styled(format!("{} ", idx + 1), key));
            spans.push(Span::styled(
                format!("{} {}  ", drink.name(), thousands(drink.price())),
                hint,
            ));
        }
        spans.push(Span::styled("r", key));
        spans.push(Span::styled(" a round for the stools  ", hint));
        spans.push(Span::styled("d", key));
        spans.push(Span::styled(" close", hint));
        spans
    } else if let Some(message) = &state.last_message {
        vec![Span::styled(
            message.clone(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::ITALIC),
        )]
    } else {
        vec![
            Span::styled("1-6", key),
            Span::styled(" sit/stand  ", hint),
            Span::styled("i", key),
            Span::styled(" say  ", hint),
            Span::styled("d", key),
            Span::styled(" drinks  ", hint),
            Span::styled("Esc", key),
            Span::styled(" back to the clubhouse", hint),
        ]
    };
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
