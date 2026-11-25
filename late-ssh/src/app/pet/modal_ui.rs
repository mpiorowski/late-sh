//! The care box: a pen the pet walks in, with everything its owner can do
//! for it listed underneath. The pen is the tile's own box (`draw_pet_box`),
//! so a click on the pet, a ball on the floor and a trick all happen here
//! exactly as they do on Home.

use late_core::models::pet::{Form, PetSpecies, Stage, Trick};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::session::{PetState, visit_line};
use super::state::{TRICK_DAYS, treat_title};
use super::ui::{Neighbours, PetView, draw_pet_box, guest_label, pick_name, still_rows};
use crate::app::common::theme;

const MODAL_WIDTH: u16 = 72;
/// The border, six pen rows, a blank, the summary, a blank, up to ten
/// care rows (a pick takes three or four of them, guests one), and the key
/// row.
const MODAL_HEIGHT: u16 = 22;
const PEN_ROWS: u16 = 6;
/// The care rows' label column.
const LABEL_WIDTH: usize = 9;
/// The width each drawing takes in a row of picks.
const PICK_CELL: usize = 13;

pub(crate) fn draw(frame: &mut Frame, area: Rect, view: &PetView<'_>) {
    let state = view.state;
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);
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
    ])
    .split(inner);

    draw_pet_box(frame, layout[0], view, Neighbours::default());
    frame.render_widget(Paragraph::new(summary_line(state).centered()), layout[2]);
    frame.render_widget(Paragraph::new(care_lines(state)), layout[4]);
    frame.render_widget(Paragraph::new(footer_line().centered()), layout[5]);
}

/// What it is and how far along: `young cat · day 12 · bond 4 · idle`,
/// and where it is staying while it is out calling. An egg has no mood to
/// report and goes nowhere.
fn summary_line(state: &PetState) -> Line<'static> {
    let growth = state.growth();
    let days = text(&format!("day {}", growth.days_together));
    let bond = text(&format!("bond {}", growth.bond_days));
    let stage = growth.stage();
    match stage {
        Stage::Egg => Line::from(vec![text("an egg"), dot(), days, dot(), bond]),
        Stage::Baby | Stage::Young | Stage::Adult | Stage::Elder => {
            let mut spans = vec![
                text(&format!("{} {}", stage.as_str(), state.species.as_str())),
                dot(),
                days,
                dot(),
                bond,
                dot(),
                text(state.mood().as_str()),
            ];
            if let Some((host, left)) = state.calling() {
                spans.push(dot());
                spans.push(text(&format!("visiting {host}, {left} min left")));
            }
            Line::from(spans)
        }
    }
}

/// One row for each thing its owner can do, the key beside it. A pet with
/// its days shows what it can become (the three animals an egg hatches
/// into, the two drawings a hatched pet grows into) in place of the row
/// that says how far along it is.
fn care_lines(state: &PetState) -> Vec<Line<'static>> {
    let growth = state.growth();
    let stage = growth.stage();
    let hatched = match stage {
        Stage::Egg => false,
        Stage::Baby | Stage::Young | Stage::Adult | Stage::Elder => true,
    };

    let mut treats = vec![label("treat")];
    match (state.waiting(), state.fed_today()) {
        (Some(kind), _) => {
            treats.push(glow(&format!("[{}]", kind.as_str())));
            treats.push(gap());
            treats.push(gap());
            treats.push(key("f"));
            treats.push(text(match hatched {
                true => " give it: it goes by its title for the day",
                false => " once it hatches",
            }));
        }
        (None, Some(kind)) => treats.push(text(&format!(
            "it ate a {} treat: {} until the day ends",
            kind.as_str(),
            treat_title(kind)
        ))),
        (None, None) => treats.push(text(
            "none waiting: one a day, from a win in any game or ten minutes in a door",
        )),
    }

    let grow = match (state.ready_to_hatch(), state.offers()) {
        (true, _) => hatch_lines(),
        (false, Some(offers)) => grow_lines(state.species, offers),
        (false, None) => vec![vec![
            label("grow"),
            text(&match (stage, stage.next(), growth.days_to_next()) {
                (Stage::Egg, _, Some(days)) => format!("it hatches in {}", days_text(days)),
                (
                    Stage::Baby | Stage::Young | Stage::Adult | Stage::Elder,
                    Some(next),
                    Some(days),
                ) => {
                    format!("{} in {}", next.as_str(), days_text(days))
                }
                (_, Some(_), None) | (_, None, _) => "fully grown".to_string(),
            }),
        ]],
    };

    let play = vec![
        label("play"),
        key("b"),
        text(" ball"),
        gap(),
        gap(),
        key("m"),
        text(" toy mouse"),
        dot(),
        text("or click the floor"),
    ];

    let mut tricks = vec![label("tricks")];
    let teaching = state.teaching();
    for trick in Trick::ALL {
        let known = growth.tricks.contains(&trick);
        let progress = match (known, growth.learning == Some(trick)) {
            (true, _) => " ok".to_string(),
            (false, true) => format!(" {}/{TRICK_DAYS}", growth.learning_days),
            (false, false) => String::new(),
        };
        let word = format!("{}{progress}", trick.as_str());
        tricks.push(match (teaching == Some(trick), known) {
            (true, _) => glow(&format!(">{word}")),
            (false, true) => text(&word),
            (false, false) => faint(&word),
        });
        tricks.push(gap());
        tricks.push(gap());
    }
    tricks.push(key("t"));
    tricks.push(text(" next "));
    tricks.push(key("p"));
    tricks.push(text(" practise "));
    tricks.push(key("r"));
    tricks.push(text(" perform"));

    let came_by = vec![
        label("came by"),
        text(&match state.care.came_by.as_slice() {
            [] => "nobody since you last looked".to_string(),
            visits => {
                let name = state.display_name();
                let mut said: Vec<String> = visits
                    .iter()
                    .take(2)
                    .map(|visit| visit_line(visit, &name))
                    .collect();
                if visits.len() > 2 {
                    said.push(format!("and {} more", visits.len() - 2));
                }
                said.join(", ")
            }
        }),
    ];

    // Who is staying, all of them: the pen draws the newest two.
    let here_now = match state.guests() {
        [] => None,
        guests => {
            let mut named: Vec<String> = guests.iter().take(3).map(guest_label).collect();
            if guests.len() > 3 {
                named.push(format!("and {} more", guests.len() - 3));
            }
            Some(vec![
                label("here now"),
                glow(&named.join(", ")),
                dot(),
                text("each stays an hour"),
            ])
        }
    };

    // The commands that change what it is, for a pet that is something.
    let change = vec![
        label("change"),
        faint("/petname <name>"),
        dot(),
        faint("/pet cat, dog or bird"),
        dot(),
        faint("/pet away"),
        dot(),
        faint("/pet reset"),
    ];

    let mut rows = vec![treats];
    rows.extend(grow);
    rows.extend([play, tricks, came_by]);
    rows.extend(here_now);
    if hatched {
        rows.push(change);
    }
    rows.into_iter().map(Line::from).collect()
}

/// The hatch: the three babies side by side, the key that picks each
/// under it.
fn hatch_lines() -> Vec<Vec<Span<'static>>> {
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
    vec![ears, faces, keys]
}

/// Growing up: the pet as each of the two drawings would leave it, side by
/// side, the key that picks each under it. The part picked is the one the
/// stage adds: the top at young, the feet at adult, the face at elder.
fn grow_lines(species: PetSpecies, offers: [Form; 2]) -> Vec<Vec<Span<'static>>> {
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
    vec![crowns, faces, floors, keys]
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

fn days_text(days: i32) -> String {
    match days {
        1 => "1 day".to_string(),
        days => format!("{days} days"),
    }
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

fn text(label: &str) -> Span<'static> {
    Span::styled(label.to_string(), Style::default().fg(theme::TEXT_DIM()))
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
    Span::raw(" ")
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
