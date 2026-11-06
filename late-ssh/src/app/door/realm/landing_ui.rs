//! The Realm card on the Games hub: the standing world as this session sees
//! it (which season, on what world, how far along, whether the gates are
//! open, the muster clock while it runs), the last world's end while it is
//! still listed, what the pot pays, and the keys before and after Enter.
//! Drawn from the snapshot every session already holds, so a frame never
//! queries. Laid out like the other door landings (DCSS is the reference):
//! logo, pitch, a scrap of the game, then headed sections.

use chrono::Utc;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use crate::app::common::theme;
use crate::app::door::landing;

use super::resolver::{RealmNewsKind, RealmPlayerStatus, WinKind};
use super::state::{PLAYER_PALETTES, RealmState};
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
    landing::render_scrolled(
        frame,
        inner,
        Paragraph::new(lines).wrap(Wrap { trim: false }),
        scroll,
    )
}

/// The card's lines, top to bottom. Pure, so the card can be tested without
/// a frame.
pub(crate) fn landing_lines(realm: &RealmState, width: usize) -> Vec<Line<'static>> {
    // The standing world's rules, which say how it is played and won; the
    // house's own when nothing stands.
    let rules = realm
        .live()
        .and_then(|game| super::rulesets::ruleset_by_id(&game.ruleset_id))
        .unwrap_or(super::svc::house_ruleset());
    let mut lines = vec![Line::raw("")];
    lines.extend(logo());
    lines.extend([
        Line::raw(""),
        Line::from(vec![
            Span::styled(
                "The house's standing world war ",
                Style::default()
                    .fg(theme::TEXT_BRIGHT())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "one map, everyone on it",
                Style::default().fg(theme::AMBER_DIM()),
            ),
        ]),
        Line::from(Span::styled(
            format!(
                "Claim land, take it from your neighbours, dig in. {}",
                how_it_is_won(rules)
            ),
            Style::default().fg(theme::TEXT_DIM()),
        )),
        Line::from(Span::styled(
            format!(
                "{}-{} players \u{b7} {} \u{b7} a new world rises the day one falls",
                rules.min_players.max(2),
                rules.max_players,
                rules.tagline
            ),
            Style::default().fg(theme::AMBER_DIM()),
        )),
        Line::raw(""),
        land_strip(),
        land_legend(),
        Line::raw(""),
    ]);

    match realm.live() {
        Some(game) => lines.extend(world_lines(realm, game, width)),
        None => {
            lines.push(landing::heading("The world"));
            lines.push(landing::stat(
                "rising",
                "the next world is being raised, a minute at most",
                LABEL_PAD,
            ));
        }
    }
    if let Some(last) = realm.snapshot.last_finished() {
        let ended = match last
            .winner_user_id
            .and_then(|w| last.players.iter().find(|p| p.user_id == w))
        {
            Some(winner) => {
                // News written before there was more than one way to win
                // carries no kind, and the one way was the conquest.
                let kind = last
                    .news
                    .as_ref()
                    .and_then(|news| {
                        let RealmNewsKind::Ended { win_kind, .. } = news.kind else {
                            return None;
                        };
                        win_kind
                    })
                    .unwrap_or(WinKind::LastStanding);
                format!(
                    "season {} · {} {}",
                    last.season,
                    winner.username,
                    kind.lounge_phrase()
                )
            }
            None => format!("season {} · dissolved, nobody held it", last.season),
        };
        lines.push(landing::stat("last", &ended, LABEL_PAD));
    }
    lines.push(Line::raw(""));

    lines.extend([
        landing::heading("How it runs"),
        landing::stat(
            "days",
            "your points come back once a day; miss one and most of it banks",
            LABEL_PAD,
        ),
        landing::stat(
            "odds",
            "a border is a fair fight, a strike across the map is a long shot",
            LABEL_PAD,
        ),
        landing::stat(
            "new",
            "nobody can touch you on your first day, only neighbours for a while after",
            LABEL_PAD,
        ),
        landing::stat(
            "quiet",
            "go quiet and your empire crumbles; nobody is kicked, it just gets taken",
            LABEL_PAD,
        ),
        Line::raw(""),
        landing::heading("Rewards"),
        landing::stat(
            "pot",
            "chips for the conqueror, growing with every rival who plays the war out",
            LABEL_PAD,
        ),
        landing::stat(
            "split",
            "up to 3 players the winner takes all \u{b7} 4-6: 70/30 \u{b7} 7+: 60/25/15",
            LABEL_PAD,
        ),
        Line::from(Span::styled(
            "  A win takes a week of play against real rivals. Leavers and the unplayed are not paid.",
            Style::default().fg(theme::TEXT_FAINT()),
        )),
        Line::raw(""),
        landing::heading("Launch"),
        landing::action(">", "Enter", enter_label(realm), theme::SUCCESS()),
    ]);
    if realm.can_join() {
        lines.push(landing::hint(
            "j",
            "on the map, pick a colour and march in",
            LABEL_PAD,
        ));
    }
    lines.extend([
        Line::raw(""),
        landing::heading("Once Inside"),
        landing::hint("a", "take the selected land, or order the take", LABEL_PAD),
        landing::hint("f", "dig in: raise the walls on land you hold", LABEL_PAD),
        landing::hint(
            "o",
            "tonight's orders, and what the last night did",
            LABEL_PAD,
        ),
        landing::hint(
            "tab",
            "map \u{b7} targets \u{b7} orders \u{b7} overview \u{b7} log \u{b7} history",
            LABEL_PAD,
        ),
        landing::hint("F4", "the world's chat, beside the map", LABEL_PAD),
        landing::hint("`", "step out to chat; the board stays open", LABEL_PAD),
        landing::hint("q", "back to the hub; your land stays yours", LABEL_PAD),
    ]);
    lines
}

/// The pitch's last sentence: how this world is won, as its rules say.
fn how_it_is_won(rules: &super::rulesets::RealmRuleset) -> String {
    let dominate = rules.domination_share_tiers.last().map(|(_, share)| *share);
    match (rules.domination_nights, dominate, rules.season_days) {
        (nights @ 1.., Some(share), 1..) => format!(
            "Hold {:.0}% of it (more in a small realm) for {nights} nights running and the \
             world is yours, or have the most of it when the season runs out.",
            share * 100.0
        ),
        (nights @ 1.., Some(share), 0) => format!(
            "Hold {:.0}% of it (more in a small realm) for {nights} nights running and the \
             world is yours.",
            share * 100.0
        ),
        (_, _, 1..) => {
            "Have the most of it when the season runs out and the world is yours.".to_string()
        }
        (_, _, 0) => "Conquer every rival and the world is yours.".to_string(),
    }
}

/// What Enter does right now, in the card's own words.
fn enter_label(realm: &RealmState) -> &'static str {
    match realm.live() {
        None => match realm.snapshot.last_finished() {
            Some(_) => "see how the last world ended",
            None => "enter the realm",
        },
        Some(game) if game.is_member(realm.user_id) => "back to your war",
        Some(game) if game.joinable => "look over the world, then take your spot",
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
            RealmPlayerStatus::Alive => match me.territories {
                1 => "you hold 1 territory".to_string(),
                n => format!("you hold {n} territories"),
            },
            RealmPlayerStatus::Eliminated => "you fell; the world is yours to watch".to_string(),
            RealmPlayerStatus::Left => "you withdrew".to_string(),
            RealmPlayerStatus::Kicked => "the world went quiet around you".to_string(),
        };
        lines.push(landing::stat("you", &mine, LABEL_PAD));
    }
    lines
}

fn logo() -> Vec<Line<'static>> {
    [
        "██████╗ ███████╗ █████╗ ██╗     ███╗   ███╗",
        "██╔══██╗██╔════╝██╔══██╗██║     ████╗ ████║",
        "██████╔╝█████╗  ███████║██║     ██╔████╔██║",
        "██╔══██╗██╔══╝  ██╔══██║██║     ██║╚██╔╝██║",
        "██║  ██║███████╗██║  ██║███████╗██║ ╚═╝ ██║",
        "╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝╚══════╝╚═╝     ╚═╝",
    ]
    .into_iter()
    .map(|line| {
        Line::from(Span::styled(
            line,
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        ))
    })
    .collect()
}

/// A shade of a player colour, the way the map paints it: pale is open
/// ground, dark is walled up.
fn shade(palette: usize, level: usize) -> Color {
    let (r, g, b) = PLAYER_PALETTES[palette].shades[level];
    Color::Rgb(r, g, b)
}

/// The two empires the strip and its legend are painted in.
const YOU: usize = 7;
const RIVAL: usize = 0;

/// A scrap of frontier: two empires, their walls, and the free ground between
/// them, in the colours the board uses. Says "territory game" before a word
/// of the card is read.
fn land_strip() -> Line<'static> {
    let land = |cells: &'static str, color: Color| Span::styled(cells, Style::default().fg(color));
    let free = |cells: &'static str| Span::styled(cells, Style::default().fg(theme::TEXT_FAINT()));
    Line::from(vec![
        free("  ░░░"),
        land("█████", shade(YOU, 1)),
        land("██", shade(YOU, 2)),
        land("███", shade(YOU, 1)),
        free("░░░░"),
        land("████", shade(RIVAL, 1)),
        land("███", shade(RIVAL, 2)),
        land("██████", shade(RIVAL, 1)),
        free("░░"),
        land("██", shade(YOU, 1)),
        free("░░░"),
    ])
}

/// Decodes the strip above.
fn land_legend() -> Line<'static> {
    let word = |w: &'static str| Span::styled(w, Style::default().fg(theme::TEXT_DIM()));
    let swatch = |color: Color| Span::styled("█", Style::default().fg(color));
    Line::from(vec![
        word("  "),
        swatch(shade(YOU, 1)),
        word(" your land   "),
        swatch(shade(RIVAL, 1)),
        word(" a rival   "),
        swatch(shade(YOU, 2)),
        word(" walled up   "),
        Span::styled("░", Style::default().fg(theme::TEXT_FAINT())),
        word(" free ground"),
    ])
}
