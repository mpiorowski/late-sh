//! How a realm looks inside the Lobby modal: its row in the list, the
//! create overlay, and the colour picker a joiner gets.
//!
//! This lived in `lobby/modal_ui.rs` until it was two fifths of that file.
//! None of it is *about* the Lobby modal — it is about realms, and it moves
//! with the rest of them; what the modal keeps is the list it goes in. The
//! widgets both files draw with are `lobby/modal_widgets.rs`.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::common::theme;
use crate::app::lobby::modal_ui::{DETAIL_COL, GAME_COL, NAME_COL};
use crate::app::lobby::modal_widgets::{
    centered_rect, col, gap, key, marker_span, text, truncate_text, wrap_blurb,
};

use super::map::MAPS;
use super::rulesets::{OPTIONS, PACES, RULESETS};
use super::state::{CreateStep, PLAYER_PALETTES, RealmCreateDraft, RealmJoinDraft, RealmState};
use super::svc::{
    REALM_MAX_ACTIVE_GAMES, REALM_NAME_MAX, REALM_POT_PER_PLAYER, RealmGameItem, reset_hour_label,
};

/// One realm game row: who runs it, ruleset/map, roster fill, and what Enter
/// does from the viewer's seat.
pub(in crate::app::lobby) fn realm_line(
    realm: &RealmState,
    game: &RealmGameItem,
    selected: bool,
) -> Line<'static> {
    let member = game.is_member(realm.user_id);
    let creator = game
        .players
        .iter()
        .find(|p| p.user_id == game.creator_id)
        .map(|p| p.username.clone())
        .unwrap_or_else(|| "player".to_string());
    // The name leads: several running games all read "standard · earth"
    // otherwise. Who runs it moves into the detail column.
    let owner = game.name.clone();
    let by = if game.creator_id == realm.user_id {
        "yours".to_string()
    } else {
        format!("by {creator}")
    };
    let detail = format!(
        "{by} · {} · {}/{} players",
        game.pace_name.to_lowercase(),
        game.players.len(),
        game.max_players
    );
    let finished = game.winner_user_id.is_some()
        || realm
            .snapshot
            .finished_games
            .iter()
            .any(|g| g.id == game.id);
    let mustering = crate::app::lobby::realm::svc::muster_left(game.created, chrono::Utc::now());
    let (status, status_style) = if let Some(left) = mustering.filter(|_| !finished) {
        // The one row in the list with a deadline on it: whatever else is
        // true of the realm, what matters is that it has not started yet.
        (
            format!(
                "opens in {} · {}",
                crate::app::lobby::realm::svc::countdown_label(left),
                if member { "you are in" } else { "enter join" }
            ),
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
    } else if finished {
        let winner = game
            .winner_user_id
            .and_then(|w| game.players.iter().find(|p| p.user_id == w))
            .map(|p| p.username.clone());
        (
            match winner {
                Some(name) => format!("{name} won"),
                None => "dissolved".to_string(),
            },
            Style::default().fg(theme::AMBER()),
        )
    } else if member {
        (
            "at war · enter play".to_string(),
            Style::default()
                .fg(theme::AMBER())
                .add_modifier(Modifier::BOLD),
        )
    } else if game.players.len() >= game.max_players as usize {
        ("full".to_string(), Style::default().fg(theme::TEXT_FAINT()))
    } else if game.joinable {
        // A realm is live from the moment it is made, so joining is always
        // joining a war in progress — the row says how far along it is.
        (
            format!("enter join · w watch · {:.0}% taken", game.claimed * 100.0),
            Style::default().fg(theme::AMBER_DIM()),
        )
    } else {
        (
            "closed · enter watch".to_string(),
            Style::default().fg(theme::TEXT_FAINT()),
        )
    };

    let mut spans = vec![marker_span(selected)];
    spans.push(Span::styled(
        col(&owner, NAME_COL),
        Style::default().fg(if member {
            theme::TEXT_BRIGHT()
        } else {
            theme::TEXT()
        }),
    ));
    spans.push(Span::styled(
        col(&detail, GAME_COL + DETAIL_COL),
        Style::default().fg(theme::TEXT_DIM()),
    ));
    spans.push(Span::styled(status, status_style));
    Line::from(spans)
}

/// The create overlay's contents, built to a known text width so a long
/// tagline or a wordy note cannot run past the border. Split out from the
/// draw so the widths can be checked without a terminal.
fn realm_draft_lines(
    draft: &RealmCreateDraft,
    username: &str,
    viewer_tz: Option<chrono_tz::Tz>,
    text_width: usize,
) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = vec![Line::raw("")];
    match draft.step {
        CreateStep::Ruleset => {
            for (idx, ruleset) in RULESETS.iter().enumerate() {
                let is_selected = idx == draft.ruleset;
                lines.push(Line::from(vec![
                    Span::raw(" "),
                    marker_span(is_selected),
                    Span::styled(
                        format!("{:<12}", ruleset.display_name),
                        Style::default().fg(if is_selected {
                            theme::TEXT_BRIGHT()
                        } else {
                            theme::TEXT()
                        }),
                    ),
                    Span::styled(
                        truncate_text(ruleset.tagline, text_width.saturating_sub(15)),
                        Style::default().fg(theme::TEXT_FAINT()),
                    ),
                ]));
            }
            lines.push(Line::raw(""));
            for note in [
                format!(
                    "opens after a {} muster · others join as they arrive · max {REALM_MAX_ACTIVE_GAMES} games",
                    crate::app::lobby::realm::svc::muster_label()
                ),
                format!(
                    "pot grows with the table: {REALM_POT_PER_PLAYER} chips a player, split by place"
                ),
            ] {
                for chunk in wrap_blurb(&note, text_width.saturating_sub(3)) {
                    lines.push(Line::from(Span::styled(
                        format!("   {chunk}"),
                        Style::default().fg(theme::TEXT_FAINT()),
                    )));
                }
            }
        }
        CreateStep::Map => {
            for (idx, map) in MAPS.iter().enumerate() {
                let is_selected = idx == draft.map;
                lines.push(Line::from(vec![
                    Span::raw(" "),
                    marker_span(is_selected),
                    Span::styled(
                        format!("{:<12}", map.display_name),
                        Style::default().fg(if is_selected {
                            theme::TEXT_BRIGHT()
                        } else {
                            theme::TEXT()
                        }),
                    ),
                    Span::styled(
                        truncate_text(map.blurb, text_width.saturating_sub(15)),
                        Style::default().fg(theme::TEXT_FAINT()),
                    ),
                ]));
            }
            lines.push(Line::raw(""));
            for chunk in wrap_blurb(
                "the world this realm is fought over — the rules stay the same on any of them",
                text_width.saturating_sub(3),
            ) {
                lines.push(Line::from(Span::styled(
                    format!("   {chunk}"),
                    Style::default().fg(theme::TEXT_FAINT()),
                )));
            }
        }
        CreateStep::Color => {
            lines.extend(color_lines(draft.color, &[], text_width));
        }
        CreateStep::MapShape => {
            use crate::app::lobby::realm::mapgen::{
                GEN_MAX_CONTINENTS, GEN_MAX_ISLANDS, GEN_MAX_TERRITORIES, GEN_MIN_TERRITORIES,
            };
            let rows: [(&str, u32, u32, u32); 3] = [
                (
                    "continents",
                    draft.shape.continents as u32,
                    0,
                    GEN_MAX_CONTINENTS as u32,
                ),
                (
                    "islands",
                    draft.shape.islands as u32,
                    0,
                    GEN_MAX_ISLANDS as u32,
                ),
                (
                    "countries",
                    draft.shape.territories as u32,
                    GEN_MIN_TERRITORIES as u32,
                    GEN_MAX_TERRITORIES as u32,
                ),
            ];
            for (idx, (label, value, min, max)) in rows.iter().enumerate() {
                let is_selected = idx == draft.shape_field;
                // A bar rather than a number alone: where a value sits in its
                // own range is the thing being chosen, and 6 of 10 continents
                // reads at a glance in a way "6" does not.
                const BAR: u32 = 16;
                let filled = if max > min {
                    ((value - min) * BAR).div_ceil(max - min)
                } else {
                    0
                };
                lines.push(Line::from(vec![
                    Span::raw(" "),
                    marker_span(is_selected),
                    Span::styled(
                        format!("{label:<11}"),
                        Style::default().fg(if is_selected {
                            theme::TEXT_BRIGHT()
                        } else {
                            theme::TEXT()
                        }),
                    ),
                    Span::styled(
                        "█".repeat(filled as usize),
                        Style::default().fg(if is_selected {
                            theme::AMBER_GLOW()
                        } else {
                            theme::TEXT_DIM()
                        }),
                    ),
                    Span::styled(
                        "·".repeat((BAR - filled) as usize),
                        Style::default().fg(theme::TEXT_FAINT()),
                    ),
                    Span::styled(
                        format!(" {value:>3}"),
                        Style::default().fg(if is_selected {
                            theme::TEXT_BRIGHT()
                        } else {
                            theme::TEXT_DIM()
                        }),
                    ),
                ]));
            }
            lines.push(Line::raw(""));
            let note = if draft.shape_is_buildable() {
                "a world drawn for this game alone, named as it is made · every world is \
                 Earth-sized, so the same rules mean the same thing on it"
            } else {
                "a world needs at least one continent or one island — nothing can be \
                 fought over on an empty sea"
            };
            for chunk in wrap_blurb(note, text_width.saturating_sub(3)) {
                lines.push(Line::from(Span::styled(
                    format!("   {chunk}"),
                    Style::default().fg(if draft.shape_is_buildable() {
                        theme::TEXT_FAINT()
                    } else {
                        theme::ERROR()
                    }),
                )));
            }
        }
        CreateStep::Pace => {
            for (idx, pace) in PACES.iter().enumerate() {
                let is_selected = idx == draft.pace;
                lines.push(Line::from(vec![
                    Span::raw(" "),
                    marker_span(is_selected),
                    Span::styled(
                        format!("{:<8}", pace.display_name),
                        Style::default().fg(if is_selected {
                            theme::TEXT_BRIGHT()
                        } else {
                            theme::TEXT()
                        }),
                    ),
                    Span::styled(
                        // The overlay is 58 columns; marker, name and the
                        // pot multiplier claim the rest.
                        {
                            let room = text_width.saturating_sub(21);
                            format!("{:<room$}", truncate_text(pace.tagline, room))
                        },
                        Style::default().fg(theme::TEXT_FAINT()),
                    ),
                    // The prize rides with the tempo, so it belongs on the
                    // same line as the choice.
                    Span::styled(
                        format!("{:>5.2}× pot", pace.pot_scale),
                        Style::default().fg(theme::AMBER_DIM()),
                    ),
                ]));
            }
            lines.push(Line::raw(""));
            for chunk in wrap_blurb(
                "a faster realm hands out more points a day and shortens every window with them",
                text_width.saturating_sub(3),
            ) {
                lines.push(Line::from(Span::styled(
                    format!("   {chunk}"),
                    Style::default().fg(theme::TEXT_FAINT()),
                )));
            }
        }
        CreateStep::Options => {
            for (idx, option) in OPTIONS.iter().enumerate() {
                let is_selected = idx == draft.option;
                let on = draft
                    .options
                    .get(option.id)
                    .copied()
                    .unwrap_or(option.default_on);
                lines.push(Line::from(vec![
                    Span::raw(" "),
                    marker_span(is_selected),
                    Span::styled(
                        if on { "[on ] " } else { "[off] " },
                        Style::default().fg(if on {
                            theme::SUCCESS()
                        } else {
                            theme::TEXT_FAINT()
                        }),
                    ),
                    Span::styled(
                        option.display_name.to_string(),
                        Style::default().fg(if is_selected {
                            theme::TEXT_BRIGHT()
                        } else {
                            theme::TEXT()
                        }),
                    ),
                ]));
            }
            lines.push(Line::raw(""));
            // What the highlighted rule actually does, in the creator's
            // words rather than the ruleset's.
            if let Some(option) = OPTIONS.get(draft.option) {
                for chunk in wrap_blurb(option.blurb, text_width.saturating_sub(3)) {
                    lines.push(Line::from(Span::styled(
                        format!("   {chunk}"),
                        Style::default().fg(theme::TEXT_FAINT()),
                    )));
                }
            }
        }
        CreateStep::Name => {
            // Typed, with the blank-name default shown as the placeholder so
            // nobody has to wonder what an empty box will produce.
            let typed = !draft.name.is_empty();
            let shown = if typed {
                draft.name.clone()
            } else {
                format!("{}'s realm", username)
            };
            lines.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    shown,
                    Style::default().fg(if typed {
                        theme::TEXT_BRIGHT()
                    } else {
                        theme::TEXT_FAINT()
                    }),
                ),
                Span::styled("▏", Style::default().fg(theme::AMBER())),
            ]));
            lines.push(Line::raw(""));
            lines.push(Line::from(Span::styled(
                format!(
                    "   {} of {REALM_NAME_MAX} characters · blank takes your own name",
                    draft.name.chars().count()
                ),
                Style::default().fg(theme::TEXT_FAINT()),
            )));
        }
        CreateStep::ResetHour => {
            // A short window around the pick: the whole 24 would not fit, and
            // the neighbours are what a creator is actually choosing between.
            for offset in -3i16..=3 {
                let hour = (draft.hour + offset).rem_euclid(24);
                let is_selected = offset == 0;
                lines.push(Line::from(vec![
                    Span::raw(" "),
                    marker_span(is_selected),
                    Span::styled(
                        reset_hour_label(hour, viewer_tz),
                        Style::default().fg(if is_selected {
                            theme::TEXT_BRIGHT()
                        } else {
                            theme::TEXT_DIM()
                        }),
                    ),
                ]));
            }
            lines.push(Line::raw(""));
            // Without a zone on the account the rows can only speak UTC, and
            // a creator picking an hour in a clock they do not think in is
            // how a realm ends up refilling at four in the morning. Say where
            // the missing half comes from rather than showing one clock and
            // letting them guess it is theirs.
            let note = if viewer_tz.is_some() {
                "everyone's action points come back at this hour, every day"
            } else {
                "everyone's action points come back at this hour, every day · set a timezone in settings to read these in your own clock too"
            };
            for chunk in wrap_blurb(note, text_width.saturating_sub(3)) {
                lines.push(Line::from(Span::styled(
                    format!("   {chunk}"),
                    Style::default().fg(theme::TEXT_FAINT()),
                )));
            }
        }
    }
    lines.push(Line::raw(""));
    let footer = match draft.step {
        CreateStep::Pace => vec![
            Span::raw(" "),
            key("j/k"),
            text(" tempo"),
            gap(),
            key("enter"),
            text(" next"),
            gap(),
            key("esc"),
            text(" back"),
        ],
        CreateStep::Ruleset => vec![
            Span::raw(" "),
            key("j/k"),
            text(" rules"),
            gap(),
            key("enter"),
            text(" next"),
            gap(),
            key("esc"),
            text(" back"),
        ],
        CreateStep::MapShape => vec![
            Span::raw(" "),
            key("j/k"),
            text(" pick"),
            gap(),
            key("h/l"),
            text(" change"),
            gap(),
            key("enter"),
            text(" next"),
            gap(),
            key("esc"),
            text(" back"),
        ],
        CreateStep::ResetHour => vec![
            Span::raw(" "),
            key("j/k"),
            text(" hour"),
            gap(),
            key("enter"),
            text(" next"),
            gap(),
            key("esc"),
            text(" back"),
        ],
        CreateStep::Name => vec![
            Span::raw(" "),
            key("type"),
            text(" a name"),
            gap(),
            key("enter"),
            text(" next"),
            gap(),
            key("esc"),
            text(" back"),
        ],
        CreateStep::Map => vec![
            Span::raw(" "),
            key("j/k"),
            text(" world"),
            gap(),
            key("enter"),
            text(" next"),
            gap(),
            key("esc"),
            text(" back"),
        ],
        CreateStep::Color => vec![
            Span::raw(" "),
            key("j/k"),
            text(" colour"),
            gap(),
            key("enter"),
            text(" next"),
            gap(),
            key("esc"),
            text(" back"),
        ],
        CreateStep::Options => vec![
            Span::raw(" "),
            key("j/k"),
            text(" rules"),
            gap(),
            key("space"),
            text(" toggle"),
            gap(),
            key("enter"),
            text(" create"),
            gap(),
            key("esc"),
            text(" back"),
        ],
    };
    lines.push(Line::from(footer));

    lines
}

/// The colour list, shared by the creator's step and the join overlay.
///
/// Each row wears its own ladder — the five shades that colour will paint on
/// the map, pale to dug-in — because "green" is a word and the map is not.
/// Taken colours stay on the list, greyed and named: knowing who is already
/// red is part of choosing.
fn color_lines(cursor: usize, taken: &[(usize, String)], text_width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (idx, palette) in PLAYER_PALETTES.iter().enumerate() {
        let held = taken.iter().find(|(slot, _)| *slot == idx);
        let is_selected = idx == cursor && held.is_none();
        let mut spans = vec![Span::raw(" "), marker_span(is_selected)];
        for (r, g, b) in palette.shades {
            spans.push(Span::styled(
                "▮",
                Style::default().fg(if held.is_some() {
                    theme::TEXT_FAINT()
                } else {
                    Color::Rgb(r, g, b)
                }),
            ));
        }
        spans.push(Span::styled(
            format!(" {:<9}", palette.name),
            Style::default().fg(match (held.is_some(), is_selected) {
                (true, _) => theme::TEXT_FAINT(),
                (false, true) => theme::TEXT_BRIGHT(),
                (false, false) => theme::TEXT(),
            }),
        ));
        if let Some((_, who)) = held {
            let room = text_width.saturating_sub(21);
            spans.push(Span::styled(
                truncate_text(&format!("taken by {who}"), room),
                Style::default().fg(theme::TEXT_FAINT()),
            ));
        }
        lines.push(Line::from(spans));
    }
    lines
}

/// Joining a realm already under way: which colour arrives. Nobody picks a
/// colour somebody on that map is already wearing, which is the whole reason
/// this is a dialog rather than a confirm.
pub(in crate::app::lobby) fn draw_realm_join_overlay(
    frame: &mut Frame,
    popup: Rect,
    draft: &RealmJoinDraft,
) {
    let width = 58u16.min(popup.width);
    let text_width = width.saturating_sub(4) as usize;
    let mut lines: Vec<Line<'static>> = vec![Line::raw("")];
    lines.push(Line::from(Span::styled(
        format!(
            "   {}",
            truncate_text(&draft.game_name, text_width.saturating_sub(3))
        ),
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::raw(""));
    lines.extend(color_lines(draft.color, &draft.taken, text_width));
    lines.push(Line::raw(""));
    for chunk in wrap_blurb(
        "you spawn where there is room and your first days are protected — nobody can touch you while you find your feet",
        text_width.saturating_sub(3),
    ) {
        lines.push(Line::from(Span::styled(
            format!("   {chunk}"),
            Style::default().fg(theme::TEXT_FAINT()),
        )));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(vec![
        Span::raw(" "),
        key("j/k"),
        text(" colour"),
        gap(),
        key("enter"),
        text(" join"),
        gap(),
        key("esc"),
        text(" cancel"),
    ]));

    let height = (lines.len() as u16 + 2).min(popup.height);
    let rect = centered_rect(width, height, popup);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .title(" join realm — your colour ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(Paragraph::new(lines), inner);
}

/// The realm create-game overlay: the rules, the world, the tempo, the hour
/// this game's points come back each day (shown in the creator's own clock as
/// well as UTC — a game meant for one side of the planet is exactly what that
/// step is for), a name, a colour, and the rules to bend.
pub(in crate::app::lobby) fn draw_realm_draft_overlay(
    frame: &mut Frame,
    popup: Rect,
    draft: &RealmCreateDraft,
    realm: &RealmState,
) {
    let width = 58u16.min(popup.width);
    // Everything below is written to fit this, and the box is then sized to
    // what was written — doing it the other way round is how lines end up
    // clipped against a height guessed before the content existed.
    let text_width = width.saturating_sub(4) as usize;
    let title = match draft.step {
        CreateStep::Ruleset => " new realm game — rules ",
        CreateStep::Map => " new realm game — world ",
        CreateStep::Color => " new realm game — your colour ",
        CreateStep::Pace => " new realm game — tempo ",
        CreateStep::Options => " new realm game — rules to bend ",
        CreateStep::ResetHour => " new realm game — daily reset ",
        CreateStep::Name => " new realm game — name ",
        CreateStep::MapShape => " new realm game — shape of the world ",
    };

    let lines = realm_draft_lines(draft, &realm.username, realm.viewer_tz, text_width);
    let height = (lines.len() as u16 + 2).min(popup.height);
    let rect = centered_rect(width, height, popup);
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
#[path = "modal_ui_test.rs"]
mod modal_ui_test;
