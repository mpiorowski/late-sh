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

use late_core::api_types::NowPlaying;
use late_core::models::chat_message::ChatMessage;
use late_core::models::drinks::drunk_label_word;

use crate::app::common::composer::build_composer_rows;
use crate::app::common::primitives::thousands;
use crate::app::common::theme;
use crate::usernames::UsernameLookup;

use super::lobby::SEAT_COUNT;
use super::state::{Drink, State};

/// The most lines the bar keeps on the wall, whatever the height.
const MAX_LINES: usize = 10;
/// The tab board: a title row plus `TAB_BOARD_SIZE` lines.
const TAB_BOARD_ROWS: u16 = 1 + super::wall::TAB_BOARD_SIZE as u16;

pub(crate) struct NightcapView<'a> {
    pub state: &'a State,
    /// The nightcap room's tail, newest first; empty off this screen.
    pub messages: &'a [ChatMessage],
    pub usernames: &'a UsernameLookup<'a>,
    /// The tavern's drunk map, so a stool shows the same `(word)` chat does.
    pub drunk_levels: &'a HashMap<Uuid, u8>,
    /// What the jukebox is playing, one of the TV's captions.
    pub now_playing: Option<&'a NowPlaying>,
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
    let [
        header,
        tv,
        _gap_a,
        seats,
        _gap_b,
        tab,
        _gap_c,
        lines,
        notice,
        footer,
    ] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(SEAT_COUNT as u16),
        Constraint::Length(1),
        Constraint::Length(TAB_BOARD_ROWS),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
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
                "a quiet spot out back of the clubhouse. the seated may speak. no ai allowed.",
                Style::default().fg(theme::TEXT_DIM()),
            )),
        ]),
        header,
    );

    draw_tv(frame, tv, view);
    draw_seats(frame, seats, view);
    draw_tab_board(frame, tab, view);
    draw_lines(frame, lines, view);
    draw_notice(frame, notice, view.state);
    draw_footer(frame, footer, view.state);
}

/// The muted TV in the corner: one caption at a time, held for a while,
/// from whatever the house knows tonight. Nothing here is posted or said;
/// it is something to look at, and maybe something to talk about.
fn draw_tv(frame: &mut Frame, area: Rect, view: &NightcapView<'_>) {
    let mut captions: Vec<String> = Vec::new();
    if let Some(now) = view.now_playing {
        captions.push(format!("♪ {}", now.track));
    }
    if let Some(activity) = view.state.last_activity() {
        captions.push(activity.to_string());
    }
    let wall = view.state.wall();
    if let Some(headline) = &wall.headline {
        captions.push(format!("tonight's paper: {headline}"));
    }
    if let Some(piece) = &wall.newest_piece {
        captions.push(format!(
            "new on the artboard: \"{}\" by {}",
            piece.title, piece.artist
        ));
    }
    let caption = match captions.get(view.state.tv_pick(captions.len())) {
        Some(caption) => caption.clone(),
        None => "static".to_string(),
    };
    let width = area.width.saturating_sub(6) as usize;
    let caption: String = caption.chars().take(width).collect();
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("▢ tv  ", Style::default().fg(theme::TEXT_FAINT())),
            Span::styled(caption, Style::default().fg(theme::TEXT_DIM())),
        ])),
        area,
    );
}

/// The tab board: who has bought the house the most rounds, all time.
fn draw_tab_board(frame: &mut Frame, area: Rect, view: &NightcapView<'_>) {
    let dim = Style::default().fg(theme::TEXT_DIM());
    let faint = Style::default().fg(theme::TEXT_FAINT());
    let mut lines = vec![Line::from(Span::styled("the tab · rounds bought", faint))];
    let tab = &view.state.wall().tab;
    if tab.is_empty() {
        lines.push(Line::from(Span::styled(
            "nobody has bought the stools a round yet.",
            Style::default()
                .fg(theme::TEXT_FAINT())
                .add_modifier(Modifier::ITALIC),
        )));
    }
    for (idx, buyer) in tab.iter().enumerate() {
        let rounds = if buyer.rounds == 1 { "round" } else { "rounds" };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", idx + 1), faint),
            Span::styled(
                buyer.username.clone(),
                Style::default().fg(theme::TEXT_BRIGHT()),
            ),
            Span::styled(
                format!(
                    "  {} {rounds}  {} chips",
                    buyer.rounds,
                    thousands(buyer.chips)
                ),
                dim,
            ),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), area);
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
                push_carving(&mut spans, view, idx);
                Line::from(spans)
            }
            None => {
                let mut spans = vec![
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
                ];
                push_carving(&mut spans, view, idx);
                Line::from(spans)
            }
        };
        frame.render_widget(Paragraph::new(line), layout[idx]);
    }
}

/// What is carved into a stool, trailing its row. The wood belongs to the
/// stool, not the sitter: it shows whether or not anyone is on it.
fn push_carving(spans: &mut Vec<Span<'static>>, view: &NightcapView<'_>, stool: usize) {
    let Some(Some(carving)) = view.state.wall().carvings.get(stool) else {
        return;
    };
    spans.push(Span::styled(
        format!("  ✎ {}", carving.body),
        Style::default()
            .fg(theme::TEXT_FAINT())
            .add_modifier(Modifier::ITALIC),
    ));
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

/// The last few things said at the bar, oldest at the top so the newest
/// sits nearest the composer. Each message wraps the way the composer
/// wraps it (word breaks, newlines kept), continuation rows indented under
/// the name; the newest messages take the rows there are, up to
/// `MAX_LINES` messages.
fn draw_lines(frame: &mut Frame, area: Rect, view: &NightcapView<'_>) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let now = chrono::Utc::now();
    let budget = area.height as usize;
    // Newest first, each message's rows as a block; stop once the next
    // block would not fit whole. Blocks are reversed at the end so the
    // oldest shown sits at the top.
    let mut blocks: Vec<Vec<Line>> = Vec::new();
    let mut used = 0;
    for message in view.messages.iter().take(MAX_LINES) {
        let (name_style, body_style) = age_styles(now - message.created);
        let name = view
            .usernames
            .get(&message.user_id)
            .map(String::as_str)
            .unwrap_or("someone");
        let prefix = format!("{name}  ");
        let indent = " ".repeat(prefix.chars().count());
        let body_width = (area.width as usize)
            .saturating_sub(prefix.chars().count())
            .max(1);
        let rows = build_composer_rows(message.body.trim_end(), body_width);
        if used + rows.len() > budget {
            break;
        }
        used += rows.len();
        let block: Vec<Line> = rows
            .into_iter()
            .enumerate()
            .map(|(idx, row)| {
                let lead = if idx == 0 {
                    Span::styled(prefix.clone(), name_style)
                } else {
                    Span::raw(indent.clone())
                };
                Line::from(vec![lead, Span::styled(row.text, body_style)])
            })
            .collect();
        blocks.push(block);
    }
    let lines: Vec<Line> = blocks.into_iter().rev().flatten().collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// How a line on the wall is lit by its age. No clock anywhere: what was
/// said in the last hour is bright, older than that dims, older than a day
/// fades to the same faint as the carvings, so an old conversation reads
/// as old instead of pretending to be live.
fn age_styles(age: chrono::Duration) -> (Style, Style) {
    let dim = Style::default().fg(theme::TEXT_DIM());
    if age < chrono::Duration::hours(1) {
        (dim, Style::default().fg(theme::TEXT_BRIGHT()))
    } else if age < chrono::Duration::days(1) {
        (Style::default().fg(theme::TEXT_FAINT()), dim)
    } else {
        let faint = Style::default().fg(theme::TEXT_FAINT());
        (faint, faint.add_modifier(Modifier::ITALIC))
    }
}

/// What the house last said to this patron, on its own row above the keys.
/// It sits over the hints rather than in place of them: the line after a
/// round is the one a patron reads while looking for the key that carves a
/// stool, and taking the keys away to say it leaves them stuck.
fn draw_notice(frame: &mut Frame, area: Rect, state: &State) {
    let Some(message) = &state.last_message else {
        return;
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            message.clone(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::ITALIC),
        ))),
        area,
    );
}

fn draw_footer(frame: &mut Frame, area: Rect, state: &State) {
    let key = Style::default().fg(theme::AMBER_DIM());
    let hint = Style::default().fg(theme::TEXT_DIM());
    let spans = if let Some(draft) = state.carving_text() {
        vec![
            Span::styled("✎ ", key),
            Span::styled(draft, Style::default().fg(theme::TEXT_BRIGHT())),
            Span::styled("▏", Style::default().fg(theme::AMBER())),
            Span::styled("  Enter", key),
            Span::styled(" carve  ", hint),
            Span::styled("Esc", key),
            Span::styled(" drop the knife", hint),
        ]
    } else if state.menu_open() {
        let mut spans = Vec::new();
        for (idx, drink) in Drink::MENU.iter().enumerate() {
            spans.push(Span::styled(format!("{} ", idx + 1), key));
            spans.push(Span::styled(
                format!("{} {}", drink.name(), thousands(drink.price())),
                hint,
            ));
            // Only the house measure comes off a round, so the drinks a
            // patron is holding are counted against the one pour they pay
            // for (`Drink::on_the_round`).
            match (drink.on_the_round(), state.free_drinks()) {
                (true, waiting) if waiting > 0 => spans.push(Span::styled(
                    format!(" (free x{waiting})"),
                    Style::default().fg(theme::AMBER()),
                )),
                _ => {}
            }
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled("r", key));
        spans.push(Span::styled(" a round for the stools  ", hint));
        spans.push(Span::styled("d", key));
        spans.push(Span::styled(" close", hint));
        spans
    } else {
        vec![
            Span::styled("1-6", key),
            Span::styled(" sit/stand  ", hint),
            Span::styled("i", key),
            Span::styled(" say  ", hint),
            Span::styled("d", key),
            Span::styled(" drinks  ", hint),
            Span::styled("c", key),
            Span::styled(" carve  ", hint),
            Span::styled("Esc", key),
            Span::styled(" back to the clubhouse", hint),
        ]
    };
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
