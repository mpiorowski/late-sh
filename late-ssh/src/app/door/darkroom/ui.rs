//! Rendering for the Dark Room door: the live page and the Games-hub landing
//! card. Upstream is deliberately bare (a column of buttons, a column of
//! stores, and text fading in), and the terminal is if anything a better fit
//! for that than the browser was, so nothing here decorates.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::common::theme;
use crate::app::door::landing;

use super::data::{self, Building, Resource};
use super::model::{Game, View};
use super::pace;
use super::state::{Row, State};

/// The live game page.
pub fn draw_page(frame: &mut Frame, area: Rect, state: &State) {
    let Some(game) = state.game() else {
        let loading = Paragraph::new(Line::from(Span::styled(
            "the dark is quiet...",
            Style::default().fg(theme::TEXT_DIM()),
        )))
        .centered();
        frame.render_widget(loading, area);
        return;
    };

    let block = Block::default()
        .title(format!(" {} ", title_for(state, game)))
        .title_style(
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([
        Constraint::Length(1), // room / village status
        Constraint::Length(1), // gap
        Constraint::Fill(1),   // actions | stores
        Constraint::Length(1), // gap
        Constraint::Length(6), // notifications
        Constraint::Length(1), // footer
    ])
    .split(inner);

    frame.render_widget(Paragraph::new(status_line(state, game)), rows[0]);

    let columns = Layout::horizontal([Constraint::Fill(1), Constraint::Length(24)]).split(rows[2]);
    frame.render_widget(Paragraph::new(action_lines(state)), columns[0]);
    frame.render_widget(Paragraph::new(stores_lines(game)), columns[1]);

    frame.render_widget(
        Paragraph::new(log_lines(state, rows[4].height as usize)),
        rows[4],
    );
    frame.render_widget(Paragraph::new(footer(state, game)), rows[5]);
}

fn title_for(state: &State, game: &Game) -> String {
    match state.view {
        View::Room => game.room_title().to_string(),
        View::Outside => game.outside_title().to_string(),
    }
}

/// The one line that says what the world is doing right now.
fn status_line(state: &State, game: &Game) -> Line<'static> {
    let text = match state.view {
        View::Room => format!(
            "the fire is {}. the room is {}.",
            game.fire.text(),
            game.temperature.text()
        ),
        View::Outside => format!(
            "pop {}/{}, {} gathering",
            game.population,
            game.max_population(),
            game.gatherers()
        ),
    };
    Line::from(Span::styled(text, Style::default().fg(theme::TEXT())))
}

/// The action column: what the player can do, with the cursor and any
/// cooldown or cost.
fn action_lines(state: &State) -> Vec<Line<'static>> {
    let selected = state.selected();
    state
        .rows()
        .into_iter()
        .map(|row| {
            let is_selected = row == selected;
            let marker = if is_selected { "> " } else { "  " };
            let label_style = if is_selected {
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme::TEXT())
            };
            let mut spans = vec![
                Span::styled(marker, Style::default().fg(theme::AMBER())),
                Span::styled(state.row_label(row), label_style),
            ];
            let cooldown = state.row_cooldown(row);
            if cooldown > 0 {
                spans.push(Span::styled(
                    format!("  {cooldown}s"),
                    Style::default().fg(theme::TEXT_FAINT()),
                ));
            } else if let Some(cost) = cost_hint(state, row) {
                spans.push(Span::styled(
                    format!("  {cost}"),
                    Style::default().fg(theme::TEXT_DIM()),
                ));
            }
            Line::from(spans)
        })
        .collect()
}

fn cost_hint(state: &State, row: Row) -> Option<String> {
    let cost = state.row_cost(row);
    if cost.is_empty() {
        return None;
    }
    Some(
        cost.iter()
            .map(|(resource, amount)| format!("{amount} {}", resource.label()))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

/// The stores column. Only resources the player has actually seen appear,
/// which is how the game reveals itself. Each row carries its net income per
/// tick, the terminal stand-in for upstream's hover tooltip, so wood quietly
/// climbing (the builder, the gatherers) is visible and attributable.
fn stores_lines(game: &Game) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(Span::styled(
        "stores",
        Style::default()
            .fg(theme::AMBER())
            .add_modifier(Modifier::BOLD),
    ))];
    let income = game.income_per_tick();
    let tick = pace::slowed(data::INCOME_DELAY);
    for resource in Resource::ALL {
        if !game.has_seen(resource) {
            continue;
        }
        let value = match income.get(&resource) {
            Some(rate) => format!("{} {}/{}s", game.store(resource), fmt_income(*rate), tick),
            None => game.store(resource).to_string(),
        };
        lines.push(landing::stat(resource.label(), &value, 12));
    }
    let standing: Vec<&Building> = Building::ALL
        .iter()
        .filter(|building| game.building_count(**building) > 0)
        .collect();
    if !standing.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "buildings",
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )));
        for building in standing {
            // Traps list bare and baited separately, like upstream's village.
            if *building == Building::Trap {
                let (bare, baited) = game.trap_rows();
                if bare > 0 {
                    lines.push(landing::stat("trap", &bare.to_string(), 12));
                }
                if baited > 0 {
                    lines.push(landing::stat("baited trap", &baited.to_string(), 12));
                }
                continue;
            }
            lines.push(landing::stat(
                building.label(),
                &game.building_count(*building).to_string(),
                12,
            ));
        }
    }
    lines
}

/// A signed income rate, whole when it is whole ("+2", "-3"), one decimal when
/// it is not ("+0.5").
fn fmt_income(rate: f64) -> String {
    if rate.fract() == 0.0 {
        format!("{:+}", rate as i64)
    } else {
        format!("{rate:+.1}")
    }
}

/// The notification log, newest last, filling the space it has.
fn log_lines(state: &State, height: usize) -> Vec<Line<'static>> {
    let all: Vec<&str> = state.log().collect();
    let start = all.len().saturating_sub(height);
    all[start..]
        .iter()
        .map(|message| {
            Line::from(Span::styled(
                (*message).to_string(),
                Style::default().fg(theme::TEXT_DIM()),
            ))
        })
        .collect()
}

/// The key hints, plus the honest word on today's allowance: a village that
/// has stopped growing must never look like a bug.
fn footer(state: &State, game: &Game) -> Line<'static> {
    let mut spans = vec![
        Span::styled("Enter", Style::default().fg(theme::AMBER_DIM())),
        Span::styled(" do   ", Style::default().fg(theme::TEXT_DIM())),
    ];
    if matches!(state.view, View::Outside) {
        spans.push(Span::styled("+/-", Style::default().fg(theme::AMBER_DIM())));
        spans.push(Span::styled(
            " worker   ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
        spans.push(Span::styled("</>", Style::default().fg(theme::AMBER_DIM())));
        spans.push(Span::styled(
            " x10   ",
            Style::default().fg(theme::TEXT_DIM()),
        ));
    }
    if game.forest_unlocked {
        spans.push(Span::styled("Tab", Style::default().fg(theme::AMBER_DIM())));
        let other = match state.view {
            View::Room => " outside   ",
            View::Outside => " the room   ",
        };
        spans.push(Span::styled(other, Style::default().fg(theme::TEXT_DIM())));
    }
    spans.push(Span::styled("Esc", Style::default().fg(theme::AMBER_DIM())));
    spans.push(Span::styled(
        " leave",
        Style::default().fg(theme::TEXT_DIM()),
    ));

    let remaining = state.credit_remaining();
    let (text, color) = if state.credit_exhausted() {
        (
            "   the village rests until tomorrow".to_string(),
            theme::AMBER_DIM(),
        )
    } else {
        (
            format!(
                "   {}h{:02}m of village time left today",
                remaining / 3600,
                (remaining % 3600) / 60
            ),
            theme::TEXT_FAINT(),
        )
    };
    spans.push(Span::styled(text, Style::default().fg(color)));
    Line::from(spans)
}

/// The two-column landing card for the Games hub.
pub fn draw_landing(frame: &mut Frame, area: Rect, delete_confirm: bool) {
    let mut lines = vec![
        landing::heading(data::TITLE),
        Line::from(""),
        Line::from(Span::styled(
            "the fire is dead. the room is freezing.",
            Style::default().fg(theme::TEXT()),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Light it, and see what the light brings in. Then build a",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(Span::styled(
            "village around it, one hut at a time.",
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(""),
        landing::heading("How it runs here"),
        landing::hint("pace", "the village runs slower than the original", 10),
        landing::hint(
            "time",
            "it grows while you are connected, anywhere on late.sh",
            10,
        ),
        landing::hint(
            "daily",
            &format!(
                "{}h of village time a day, so it lasts weeks",
                pace::DAILY_CREDIT_SECS / 3600
            ),
            10,
        ),
        landing::hint(
            "floor",
            &format!(
                "even a short visit banks {}m once the village stands",
                pace::DAILY_CREDIT_FLOOR_SECS / 60
            ),
            10,
        ),
        Line::from(""),
    ];

    if delete_confirm {
        lines.push(landing::action(
            "!",
            "d",
            "press again to burn it all down and start over",
            theme::ERROR(),
        ));
    } else {
        lines.push(landing::action(
            ">",
            "Enter",
            "light the fire",
            theme::SUCCESS(),
        ));
        lines.push(landing::action("x", "d", "start over", theme::ERROR()));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "A port of A Dark Room by Michael Townsend / Doublespeak Games,",
        Style::default().fg(theme::TEXT_FAINT()),
    )));
    lines.push(Line::from(Span::styled(
        "open sourced under the MPL. The original, and the paid mobile",
        Style::default().fg(theme::TEXT_FAINT()),
    )));
    lines.push(Line::from(Span::styled(
        "and Steam versions, are at doublespeakgames.com.",
        Style::default().fg(theme::TEXT_FAINT()),
    )));

    frame.render_widget(Paragraph::new(lines), area);
}
