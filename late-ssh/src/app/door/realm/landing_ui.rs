//! The Realm card on the Games hub: the standing world as this session sees
//! it (which season, on what world, how far along, whether the gates are
//! open, the muster clock while it runs), the last world's end while it is
//! still listed, and what Enter does. Drawn from the snapshot every session
//! already holds, so a frame never queries.

use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::common::theme;
use crate::app::door::landing;

use super::resolver::RealmPlayerStatus;
use super::state::RealmState;
use super::svc::{RealmGameItem, countdown_label, muster_left, reset_hour_label};
use super::ui::truncate;

const LABEL_PAD: usize = 8;

/// Draw the card into `area`, scrolled `scroll` rows; returns how far it
/// could scroll, which the hub clamps input to.
pub fn draw_landing(frame: &mut Frame, area: Rect, realm: &RealmState, scroll: u16) -> u16 {
    let inner = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area)[1];
    let lines = landing_lines(realm, usize::from(inner.width));
    let max_scroll = (lines.len() as u16).saturating_sub(inner.height);
    let scroll = scroll.min(max_scroll);
    frame.render_widget(Paragraph::new(lines).scroll((scroll, 0)), inner);
    max_scroll
}

/// The card's lines, top to bottom. Pure, so the card can be tested without
/// a frame.
pub(crate) fn landing_lines(realm: &RealmState, width: usize) -> Vec<Line<'static>> {
    let mut lines = vec![Line::raw("")];
    lines.extend(title_art());
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "one world, and everyone in the house is on it.",
        Style::default().fg(theme::TEXT()),
    )));
    lines.push(Line::raw(""));
    for text in [
        "Claim land, take it from your neighbours, dig in. A few points a",
        "day, each one resolved the moment you spend it. The world is won",
        "when one player has conquered every rival, and the next one rises",
        "the day it falls.",
    ] {
        lines.push(Line::from(Span::styled(
            text,
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    lines.push(Line::raw(""));

    match realm.live() {
        Some(game) => lines.extend(world_lines(realm, game, width)),
        None => {
            lines.push(landing::heading("The world"));
            lines.push(landing::hint(
                "rising",
                "the next world is being raised, a minute at most",
                LABEL_PAD,
            ));
        }
    }
    lines.push(Line::raw(""));

    if let Some(last) = realm.snapshot.last_finished() {
        lines.push(landing::heading("The last world"));
        let ended = match last
            .winner_user_id
            .and_then(|w| last.players.iter().find(|p| p.user_id == w))
        {
            Some(winner) => format!("season {} · conquered by {}", last.season, winner.username),
            None => format!("season {} · dissolved, nobody held it", last.season),
        };
        lines.push(landing::hint("ended", &ended, LABEL_PAD));
        lines.push(Line::raw(""));
    }

    lines.push(landing::heading("How it runs here"));
    lines.push(landing::hint(
        "days",
        "your points come back once a day; miss one and most of it banks",
        LABEL_PAD,
    ));
    lines.push(landing::hint(
        "odds",
        "a border is a fair fight, a strike across the map is a long shot",
        LABEL_PAD,
    ));
    lines.push(landing::hint(
        "new",
        "nobody can touch you on your first day, only neighbours for a while after",
        LABEL_PAD,
    ));
    lines.push(landing::hint(
        "quiet",
        "go quiet and your empire crumbles; nobody is kicked, it just gets taken",
        LABEL_PAD,
    ));
    lines.push(landing::hint(
        "pot",
        "the conqueror takes a chip pot, shared with second and third in big wars",
        LABEL_PAD,
    ));
    lines.push(Line::raw(""));

    lines.push(Line::from(Span::styled(
        "  ` hops in and out with the board kept open",
        Style::default().fg(theme::TEXT_FAINT()),
    )));
    lines.push(Line::raw(""));
    lines.push(landing::action(
        "\u{25b6}",
        "Enter",
        enter_label(realm),
        theme::SUCCESS(),
    ));
    lines
}

/// What Enter does right now, in the card's own words.
fn enter_label(realm: &RealmState) -> &'static str {
    match realm.live() {
        None => match realm.snapshot.last_finished() {
            Some(_) => "see how the last world ended",
            None => "enter the realm",
        },
        Some(game) if game.is_member(realm.user_id) => "enter the realm",
        Some(game) if game.joinable => "enter the realm, then j to take your spot",
        Some(_) => "watch the realm",
    }
}

/// The standing world, as a block of stats.
fn world_lines(realm: &RealmState, game: &RealmGameItem, width: usize) -> Vec<Line<'static>> {
    let now = Utc::now();
    let mut lines = vec![landing::heading("The world")];
    let world = match game.map_id.as_str() {
        super::map::GENERATED_MAP_ID => "a drawn world".to_string(),
        _ => "Earth".to_string(),
    };
    lines.push(landing::stat(
        "season",
        &format!(
            "{} · {world} · {} pace",
            game.name,
            game.pace_name.to_lowercase()
        ),
        LABEL_PAD,
    ));
    let gates = match (muster_left(game.created, now), game.joinable) {
        (Some(left), _) => format!(
            "open · mustering, the first point is spent in {}",
            countdown_label(left)
        ),
        (None, true) => format!(
            "open · {:.0}% of the world is claimed",
            game.claimed * 100.0
        ),
        (None, false) => format!(
            "closed · {:.0}% of the world is claimed, watch this one",
            game.claimed * 100.0
        ),
    };
    lines.push(landing::stat("gates", &gates, LABEL_PAD));
    lines.push(landing::stat(
        "refill",
        &reset_hour_label(game.reset_hour_utc, realm.viewer_tz),
        LABEL_PAD,
    ));
    let alive: Vec<&str> = game
        .players
        .iter()
        .filter(|p| p.status == RealmPlayerStatus::Alive)
        .map(|p| p.username.as_str())
        .collect();
    let roster = match alive.len() {
        0 => "nobody has marched in yet".to_string(),
        n => {
            let names = alive.join(", ");
            let room = width.saturating_sub(LABEL_PAD + 2 + 12);
            format!("{n} alive · {}", truncate(&names, room))
        }
    };
    lines.push(landing::stat("roster", &roster, LABEL_PAD));
    if let Some(me) = game.players.iter().find(|p| p.user_id == realm.user_id) {
        let mine = match me.status {
            RealmPlayerStatus::Alive => format!("you hold {} territories", me.territories),
            RealmPlayerStatus::Eliminated => "you fell; the world is yours to watch".to_string(),
            RealmPlayerStatus::Left => "you withdrew".to_string(),
            RealmPlayerStatus::Kicked => "the world went quiet around you".to_string(),
        };
        lines.push(landing::stat("you", &mine, LABEL_PAD));
    }
    lines
}

fn title_art() -> Vec<Line<'static>> {
    let style = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    [
        "██▀███  ▓█████ ▄▄▄       ██▓     ███▄ ▄███▓",
        "▓██ ▒ ██▒▓█   ▀▒████▄    ▓██▒    ▓██▒▀█▀ ██▒",
        "▓██ ░▄█ ▒▒███  ▒██  ▀█▄  ▒██░    ▓██    ▓██░",
        "▒██▀▀█▄  ▒▓█  ▄░██▄▄▄▄██ ▒██░    ▒██    ▒██ ",
        "░██▓ ▒██▒░▒████▒▓█   ▓██▒░██████▒▒██▒   ░██▒",
        "░ ▒▓ ░▒▓░░░ ▒░ ░▒▒   ▓▒█░░ ▒░▓  ░░ ▒░   ░  ░",
    ]
    .into_iter()
    .map(|row| Line::from(Span::styled(row, style)))
    .collect()
}
