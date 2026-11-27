//! The care box: a pen the pet walks in, with everything its owner can do
//! for it listed underneath. The pen is the tile's own box (`draw_pet_box`),
//! so a click on the pet, a ball on the floor and a trick all happen here
//! exactly as they do on Home.
//!
//! Each care row is a label and pieces that flow under it: a row too long
//! for the box goes on under its own value column, never past the border,
//! and the box is as tall as its rows.

use late_core::models::pet::{Form, PetSpecies, Stage, Trick, VisitKind};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::session::PetState;
use super::state::{Need, TRICK_DAYS, hatch_wait, treat_title, treat_wait};
use super::ui::{Neighbours, PetView, draw_pet_box, guest_label, pick_name, still_rows};
use crate::app::common::{primitives::truncate_cells, theme};

const MODAL_WIDTH: u16 = 76;
const PEN_ROWS: u16 = 6;
/// The border, the pen, a blank, the summary, a blank, then the care rows,
/// a blank and the key row.
const CHROME_ROWS: u16 = 2 + PEN_ROWS + 3 + 2;
/// The care rows' label column.
const LABEL_WIDTH: usize = 9;
/// The label and the column of air before it.
const LABEL_CELLS: usize = LABEL_WIDTH + 1;
/// The width each drawing takes in a row of picks.
const PICK_CELL: usize = 13;
/// How many visitors and guests a row names before it counts the rest.
const NAMED: usize = 3;

pub(crate) fn draw(frame: &mut Frame, area: Rect, view: &PetView<'_>) {
    let state = view.state;
    let width = MODAL_WIDTH.min(area.width);
    let rows = care_lines(state, usize::from(width.saturating_sub(2)));
    let height = CHROME_ROWS + rows.len() as u16;
    let popup = centered_rect(width, height, area);
    frame.render_widget(Clear, popup);

    let title = match state.title() {
        Some(title) => format!(" {} {title} ", state.display_name()),
        None => format!(" {} ", state.display_name()),
    };
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let layout = Layout::vertical([
        Constraint::Length(PEN_ROWS),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(inner);

    draw_pet_box(frame, layout[0], view, Neighbours::default());
    frame.render_widget(Paragraph::new(summary_line(state).centered()), layout[2]);
    frame.render_widget(Paragraph::new(rows), layout[4]);
    frame.render_widget(Paragraph::new(footer_line().centered()), layout[6]);
}

/// What it is and how far along: `young cat · day 12 · bond 4 · idle`. An
/// egg has no mood to report.
fn summary_line(state: &PetState) -> Line<'static> {
    let growth = state.growth();
    let days = text(&format!("day {}", growth.days_together));
    let bond = text(&format!("bond {}", growth.bond_days));
    let stage = growth.stage();
    match stage {
        Stage::Egg => Line::from(vec![text("an egg"), dot(), days, dot(), bond]),
        Stage::Baby | Stage::Young | Stage::Adult | Stage::Elder => Line::from(vec![
            text(&format!("{} {}", stage.as_str(), state.species.as_str())),
            dot(),
            days,
            dot(),
            bond,
            dot(),
            text(state.mood().as_str()),
        ]),
    }
}

/// One unbreakable run of a care row, and what stands before it when it
/// does not start a line: a `·` between things, a space between words.
struct Piece {
    lead: Span<'static>,
    spans: Vec<Span<'static>>,
}

/// A thing of its own on the row, set off from the last by a `·`.
fn part(spans: Vec<Span<'static>>) -> Piece {
    Piece { lead: dot(), spans }
}

/// A run of dim words that may break between any two, set off from what
/// came before by `lead`.
fn words(lead: Span<'static>, said: &str) -> Vec<Piece> {
    said.split_whitespace()
        .enumerate()
        .map(|(at, word)| Piece {
            lead: match at {
                0 => lead.clone(),
                _ => gap(),
            },
            spans: vec![text(word)],
        })
        .collect()
}

/// A row: `label` on its first line, the pieces flowed into the `width`
/// after the label column, each line after the first under the value
/// column. A piece wider than that column on its own is cut with an
/// ellipsis; nothing reaches the border.
fn flow(label_text: &str, pieces: Vec<Piece>, width: usize) -> Vec<Line<'static>> {
    let room = width.saturating_sub(LABEL_CELLS + 1).max(1);
    let mut lines: Vec<Vec<Span<'static>>> = Vec::new();
    let mut line: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    for piece in pieces {
        let spans = clip(piece.spans, room);
        let cells: usize = spans.iter().map(Span::width).sum();
        match (line.is_empty(), used + piece.lead.width() + cells <= room) {
            (true, _) => {
                line = spans;
                used = cells;
            }
            (false, true) => {
                used += piece.lead.width() + cells;
                line.push(piece.lead);
                line.extend(spans);
            }
            (false, false) => {
                lines.push(std::mem::replace(&mut line, spans));
                used = cells;
            }
        }
    }
    lines.push(line);
    lines
        .into_iter()
        .enumerate()
        .map(|(at, spans)| {
            let mut row = vec![label(match at {
                0 => label_text,
                _ => "",
            })];
            row.extend(spans);
            Line::from(row)
        })
        .collect()
}

/// `spans` cut to `room` cells, the cut span ending in an ellipsis.
fn clip(spans: Vec<Span<'static>>, room: usize) -> Vec<Span<'static>> {
    let mut left = room;
    let mut kept = Vec::new();
    for span in spans {
        let cells = span.width();
        match (cells <= left, left) {
            (true, _) => {
                left -= cells;
                kept.push(span);
            }
            (false, 0) => break,
            (false, _) => {
                kept.push(Span::styled(
                    truncate_cells(&span.content, left),
                    span.style,
                ));
                break;
            }
        }
    }
    kept
}

/// The rows under the pen, flowed into `width`. A pet with what its next
/// stage asks for (days for an egg, treats after) shows
/// what it can become (the three animals an egg hatches into, the two
/// drawings a hatched pet grows into) in place of the row that says how
/// far along it is.
fn care_lines(state: &PetState, width: usize) -> Vec<Line<'static>> {
    let growth = state.growth();
    let stage = growth.stage();
    let hatched = match stage {
        Stage::Egg => false,
        Stage::Baby | Stage::Young | Stage::Adult | Stage::Elder => true,
    };
    let name = state.display_name();

    let mut rows = Vec::new();

    let treat = match (state.waiting(), state.fed_today(), hatched) {
        (Some(kind), _, true) => {
            let mut pieces = vec![
                part(vec![
                    glow(&format!("{} treat", kind.as_str())),
                    text(" waiting"),
                ]),
                part(vec![key("f"), text(" give it:")]),
            ];
            pieces.extend(words(
                gap(),
                &format!("{name} {} for the day", treat_title(kind)),
            ));
            pieces
        }
        (Some(kind), _, false) => vec![
            part(vec![
                glow(&format!("{} treat", kind.as_str())),
                text(" waiting"),
            ]),
            part(vec![text("it eats once it hatches")]),
        ],
        (None, Some(kind), _) => {
            let mut pieces = words(dot(), &format!("ate a {} treat", kind.as_str()));
            pieces.extend(words(
                dot(),
                &format!("{name} {} until the day ends", treat_title(kind)),
            ));
            pieces
        }
        (None, None, _) => {
            let mut pieces = words(dot(), "none waiting");
            pieces.extend(words(dot(), "win any game or spend ten minutes in a door"));
            pieces
        }
    };
    rows.extend(flow("treat", treat, width));

    match (state.ready_to_hatch(), state.offers()) {
        (true, _) => rows.extend(hatch_lines()),
        (false, Some(offers)) => rows.extend(grow_lines(state.species, offers)),
        (false, None) => {
            let said = match growth.still_needs() {
                Some((_, Need::Days(days))) => format!("it hatches {}", hatch_wait(days)),
                Some((next, Need::Treats(treats))) => treat_wait(next, treats),
                None => "fully grown".to_string(),
            };
            rows.extend(flow("grow", words(dot(), &said), width));
        }
    }

    if let Some((host, left)) = state.calling() {
        rows.extend(flow(
            "visiting",
            words(dot(), &format!("{host}, {left} min left")),
            width,
        ));
    }

    let mut play = vec![
        part(vec![key("b"), text(" ball")]),
        part(vec![key("m"), text(" toy mouse")]),
    ];
    play.extend(words(dot(), "or click the floor"));
    rows.extend(flow("play", play, width));

    let teaching = state.teaching();
    let tricks = Trick::ALL
        .into_iter()
        .map(|trick| {
            let known = growth.tricks.contains(&trick);
            let progress = match (known, growth.learning == Some(trick)) {
                (true, _) => " ✓".to_string(),
                (false, true) => format!(" {}/{TRICK_DAYS}", growth.learning_days),
                (false, false) => String::new(),
            };
            let word = format!("{}{progress}", trick.as_str());
            Piece {
                lead: Span::styled("  ", dim()),
                spans: vec![match (teaching == Some(trick), known) {
                    (true, _) => Span::styled(
                        format!("›{word}"),
                        Style::default()
                            .fg(theme::AMBER_GLOW())
                            .add_modifier(Modifier::BOLD),
                    ),
                    (false, true) => text(&word),
                    (false, false) => faint(&word),
                }],
            }
        })
        .collect();
    rows.extend(flow("tricks", tricks, width));
    rows.extend(flow(
        "",
        vec![
            part(vec![key("t"), text(" next")]),
            part(vec![key("p"), text(" practise")]),
            part(vec![key("r"), text(" perform")]),
        ],
        width,
    ));

    let came_by = match state.care.came_by.as_slice() {
        [] => words(dot(), "nobody since you last looked"),
        visits => {
            let mut pieces: Vec<Piece> = visits
                .iter()
                .take(NAMED)
                .map(|visit| Piece {
                    lead: Span::styled(", ", dim()),
                    spans: vec![text(&match visit.kind {
                        VisitKind::Pat => format!("{} petted it", visit.username),
                        VisitKind::Call => format!("{}'s pet called", visit.username),
                    })],
                })
                .collect();
            if visits.len() > NAMED {
                pieces.extend(words(
                    Span::styled(", ", dim()),
                    &format!("and {} more", visits.len() - NAMED),
                ));
            }
            pieces
        }
    };
    rows.extend(flow("came by", came_by, width));

    // Who is staying, all of them: the pen draws the newest two.
    match state.guests() {
        [] => {}
        guests => {
            let mut pieces: Vec<Piece> = guests
                .iter()
                .take(NAMED)
                .map(|guest| Piece {
                    lead: Span::styled(", ", dim()),
                    spans: vec![glow(&guest_label(guest))],
                })
                .collect();
            if guests.len() > NAMED {
                pieces.extend(words(
                    Span::styled(", ", dim()),
                    &format!("and {} more", guests.len() - NAMED),
                ));
            }
            pieces.extend(words(dot(), "each stays an hour"));
            rows.extend(flow("here now", pieces, width));
        }
    }

    // The commands that change what it is, for a pet that is something.
    if hatched {
        rows.extend(flow(
            "change",
            [
                "/petname <name>",
                "/pet cat|dog|bird",
                "/pet away",
                "/pet reset",
            ]
            .into_iter()
            .map(|said| part(vec![faint(said)]))
            .collect(),
            width,
        ));
    }

    rows
}

/// The hatch: the three babies side by side, the key that picks each
/// under it.
fn hatch_lines() -> Vec<Line<'static>> {
    let mut ears = vec![label("hatch")];
    let mut faces = vec![label("")];
    let mut keys = vec![label("")];
    for (at, species) in PetSpecies::ALL.into_iter().enumerate() {
        let [_, ear_row, face_row] = still_rows(species, Form::Baby);
        ears.push(glow(&format!("{ear_row:<PICK_CELL$}")));
        faces.push(glow(&format!("{face_row:<PICK_CELL$}")));
        let name = species.as_str();
        keys.push(gap());
        keys.push(key(&(at + 1).to_string()));
        keys.push(text(&format!(" {name:<width$}", width = PICK_CELL - 3)));
    }
    [ears, faces, keys].into_iter().map(Line::from).collect()
}

/// Growing up: the pet as each of the two drawings would leave it, side by
/// side, the key that picks each under it. The part picked is the one the
/// stage adds: the top at young, the feet at adult, the face at elder.
fn grow_lines(species: PetSpecies, offers: [Form; 2]) -> Vec<Line<'static>> {
    let mut rows = [vec![label("grow")], vec![label("")], vec![label("")]];
    let mut keys = vec![label("")];
    for (at, form) in offers.into_iter().enumerate() {
        for (row, drawn) in rows.iter_mut().zip(still_rows(species, form)) {
            row.push(glow(&format!("{drawn:<PICK_CELL$}")));
        }
        keys.push(gap());
        keys.push(key(&(at + 1).to_string()));
        let name = pick_name(species, form);
        keys.push(text(&format!(" {name:<width$}", width = PICK_CELL - 3)));
    }
    let [crowns, faces, floors] = rows;
    [crowns, faces, floors, keys]
        .into_iter()
        .map(Line::from)
        .collect()
}

fn footer_line() -> Line<'static> {
    Line::from(vec![
        text("click it to pet it"),
        dot(),
        key("?"),
        text(" guide"),
        dot(),
        key("q"),
        text(" close"),
    ])
}

fn label(name: &str) -> Span<'static> {
    Span::styled(
        format!(" {name:<LABEL_WIDTH$}"),
        Style::default().fg(theme::TEXT_FAINT()),
    )
}

fn key(label: &str) -> Span<'static> {
    Span::styled(
        label.to_string(),
        Style::default()
            .fg(theme::AMBER_DIM())
            .add_modifier(Modifier::BOLD),
    )
}

fn dim() -> Style {
    Style::default().fg(theme::TEXT_DIM())
}

fn text(label: &str) -> Span<'static> {
    Span::styled(label.to_string(), dim())
}

fn faint(label: &str) -> Span<'static> {
    Span::styled(label.to_string(), Style::default().fg(theme::TEXT_FAINT()))
}

fn glow(label: &str) -> Span<'static> {
    Span::styled(label.to_string(), Style::default().fg(theme::AMBER_GLOW()))
}

fn dot() -> Span<'static> {
    Span::styled(" · ", Style::default().fg(theme::BORDER_DIM()))
}

fn gap() -> Span<'static> {
    Span::styled(" ", dim())
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

#[cfg(test)]
#[path = "modal_ui_test.rs"]
mod modal_ui_test;
