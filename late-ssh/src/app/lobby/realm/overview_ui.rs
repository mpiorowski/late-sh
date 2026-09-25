//! The realm overview view: standings table (every player, color swatch,
//! territories, strength, status) plus your own holdings list.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};

use unicode_width::UnicodeWidthStr;

use crate::app::common::theme;

use super::map_ui::compact;
use super::resolver::{PlayerPhase, RealmPlayerStatus};
use super::state::{RealmState, player_color};

pub fn draw_overview_view(frame: &mut Frame, area: Rect, realm: &RealmState) {
    let Some(board) = realm.board.as_ref() else {
        return;
    };
    let Some(detail) = board.detail.as_ref() else {
        return;
    };
    let Some(map) = detail.map() else {
        return;
    };
    let state = &detail.state;
    let dim = Style::default().fg(theme::TEXT_DIM());
    let text = Style::default().fg(theme::TEXT());
    let bright = Style::default().fg(theme::AMBER());

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        format!(
            "standings — {} of {} territories claimed",
            state.ownership.len(),
            map.territories.len()
        ),
        bright.add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    let today = detail.today();
    // World totals, so a player's share means something.
    let world_pop: u64 = map.territories.iter().map(|t| t.population).sum();
    let world_area: u64 = map.territories.iter().map(|t| t.area_km2).sum();

    let mut players: Vec<_> = state.players.iter().collect();
    players.sort_by(|a, b| {
        let ta = state.territory_count(a.user_id);
        let tb = state.territory_count(b.user_id);
        tb.cmp(&ta).then_with(|| a.username.cmp(&b.username))
    });
    lines.push(Line::from(Span::styled(
        format!(
            "  {:<18}{:>5} {:>9} {:>11} {:>7} {:>6} {:>6} {:>6}  {}",
            "player", "land", "people", "area km²", "share", "str", "days", "walls", "status"
        ),
        Style::default().fg(theme::TEXT_FAINT()),
    )));
    for p in &players {
        let (r, g, b) = player_color(state, p.user_id);
        let held = state.territory_count(p.user_id);
        let standing = state.standing(p.user_id, today);
        // Strength as it actually counts: an empire left unruled defends
        // with less than its size suggests.
        let strength = state.effective_strength(p.user_id, &map, today);
        // Population and land are the currency of this game — the odds are
        // built from them — so the standings say them out loud.
        let (pop, area) = state
            .ownership
            .iter()
            .filter(|(_, owner)| **owner == p.user_id)
            .filter_map(|(t, _)| map.territory(*t))
            .fold((0u64, 0u64), |(pop, area), t| {
                (pop + t.population, area + t.area_km2)
            });
        // Walls raised, so a table can see who is digging rather than taking.
        let walls: u32 = state
            .ownership
            .iter()
            .filter(|(_, owner)| **owner == p.user_id)
            .map(|(t, _)| u32::from(state.fort_level(*t).saturating_sub(1)))
            .sum();
        let share = if world_pop > 0 {
            pop as f64 / world_pop as f64 * 100.0
        } else {
            0.0
        };
        let status = match p.status {
            RealmPlayerStatus::Alive => {
                let phase = state.phase(p.user_id, today);
                if phase != PlayerPhase::Involved {
                    // Where someone stands decides who may come for them, so
                    // it outranks their points in the status column.
                    format!("{} · {}", phase.label(), phase.blurb())
                } else if standing.quiet_days > 0 {
                    // Quiet days are the clock that decides both what they
                    // will bank and how fast their grip slips.
                    let mut note = format!("quiet {}d", standing.quiet_days);
                    if standing.decaying() {
                        note.push_str(&format!(" · crumbling ×{:.2}", standing.decay));
                    }
                    note
                } else if standing.banked > 0 {
                    format!("{} left (+{} banked)", standing.left(), standing.banked)
                } else {
                    format!("{} pts left", standing.left())
                }
            }
            RealmPlayerStatus::Eliminated => "conquered".to_string(),
            RealmPlayerStatus::Left => "left".to_string(),
            RealmPlayerStatus::Kicked => "gone".to_string(),
        };
        let you = if p.user_id == realm.user_id {
            " (you)"
        } else {
            ""
        };
        lines.push(Line::from(vec![
            Span::styled("■ ", Style::default().fg(Color::Rgb(r, g, b))),
            Span::styled(format!("{:<18}", format!("{}{you}", p.username)), text),
            Span::styled(
                format!(
                    "{held:>5} {:>9} {:>11} {share:>6.1}% {strength:>6.1} {:>6} {walls:>6}  {status}",
                    compact(pop),
                    compact(area),
                    p.active_days
                ),
                dim,
            ),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!(
            "world: {} people · {} km² · {} of {} territories taken",
            compact(world_pop),
            compact(world_area),
            state.ownership.len(),
            map.territories.len()
        ),
        dim,
    )));
    // Your own clock, spelled out: nobody should have to infer the rules
    // that decide their week from a number going down.
    let me = state.standing(realm.user_id, today);
    if state.player(realm.user_id).is_some() {
        let to_win = state.days_to_win(realm.user_id);
        let played = state
            .player(realm.user_id)
            .map(|p| p.active_days)
            .unwrap_or(0);
        let phase = state.phase(realm.user_id, today);
        if phase != PlayerPhase::Involved {
            let next = if phase == PlayerPhase::New {
                PlayerPhase::Rampup
            } else {
                PlayerPhase::Involved
            };
            lines.push(Line::from(Span::styled(
                format!(
                    "you are {} for {}d — {}",
                    phase.label(),
                    state.days_until(realm.user_id, today, next),
                    phase.blurb()
                ),
                Style::default().fg(theme::SUCCESS()),
            )));
        }
        lines.push(Line::from(Span::styled(
            format!(
                "you: {}/{} days played{}",
                played,
                state.ruleset.min_active_days_to_win,
                if to_win > 0 {
                    format!(" · {to_win} more before you can win")
                } else {
                    " · you have played enough to win".to_string()
                }
            ),
            Style::default().fg(theme::AMBER()),
        )));
        let banked = if me.banked > 0 {
            format!(" (+{} banked from days away)", me.banked)
        } else {
            String::new()
        };
        lines.push(Line::from(Span::styled(
            format!("today: {} of {} points{banked}", me.left(), me.allowance),
            dim,
        )));
        // The ladder, stated as what happens next rather than as a table.
        let next = match me.bank_rate {
            r if r >= 0.999 => "a missed day banks in full".to_string(),
            r if r > 0.0 => format!("another missed day banks {:.0}%", r * 100.0),
            _ => "further missed days bank nothing".to_string(),
        };
        let crumble = if me.decaying() {
            format!(" · your grip has slipped to ×{:.2}", me.decay)
        } else if me.quiet_days > 0 {
            format!(
                " · {} quiet days before your empire starts to crumble",
                i32::from(state.ruleset.dormancy_grace_days) - me.quiet_days + 1
            )
        } else {
            String::new()
        };
        lines.push(Line::from(Span::styled(
            format!("{next}{crumble}"),
            Style::default().fg(theme::TEXT_FAINT()),
        )));
        lines.push(Line::from(""));
    }

    // What the game is playing for. The pot follows the players who have
    // actually played, so it grows as a realm fills with people who stay.
    let plan = super::svc::payout_plan(super::svc::paying_players(state), state.ruleset.pot_scale);
    let pot: i64 = plan.iter().map(|(_, chips)| chips).sum();
    let places = match plan.len() {
        // One paid place: saying the same figure twice helps nobody.
        0 | 1 => "winner takes all".to_string(),
        _ => plan
            .iter()
            .enumerate()
            .map(|(rank, (_, chips))| {
                let place = match rank {
                    0 => "1st",
                    1 => "2nd",
                    _ => "3rd",
                };
                format!("{place} {}", compact(*chips as u64))
            })
            .collect::<Vec<_>>()
            .join(" · "),
    };
    lines.push(Line::from(Span::styled(
        format!(
            "pot: {} chips — {places} · {} tempo",
            compact(pot as u64),
            state.ruleset.pace_name.to_lowercase()
        ),
        Style::default().fg(theme::AMBER_DIM()),
    )));

    // Whether anyone can still arrive, which is a fact about the map now
    // rather than about a lobby that opened and closed.
    let claimed = state.claimed_fraction(&map);
    lines.push(Line::from(Span::styled(
        if state.joinable(&map) {
            format!(
                "open to newcomers — {:.0}% of the map claimed, doors close at {:.0}%",
                claimed * 100.0,
                state.ruleset.join_max_claimed * 100.0
            )
        } else {
            format!(
                "closed to newcomers — {:.0}% of the map is claimed",
                claimed * 100.0
            )
        },
        Style::default().fg(theme::TEXT_FAINT()),
    )));
    lines.push(Line::from(""));

    // Your holdings: a cursor walks them, and `g` shows the highlighted one
    // on the map (framed at a zoom that actually fits it).
    let role = realm.viewer_role();
    let holdings = state.holdings(realm.user_id);
    let cursor = board.holdings_cursor.min(holdings.len().saturating_sub(1));
    if role == super::state::ViewerRole::Watching {
        // Nothing below is theirs; saying "you hold nothing" would imply
        // they are in a war they never joined.
        lines.push(Line::from(Span::styled(
            "you are watching this realm — press q to leave it to them",
            dim,
        )));
    } else {
        lines.push(Line::from(Span::styled(
            format!("your territories ({})", holdings.len()),
            bright,
        )));
        if holdings.is_empty() {
            lines.push(Line::from(Span::styled("  none — you hold nothing", dim)));
        }
    }
    for (idx, id) in holdings.iter().enumerate() {
        let Some(t) = map.territory(*id) else {
            continue;
        };
        let selected = idx == cursor;
        let mut spans = vec![
            Span::styled(
                if selected { "▸ " } else { "  " },
                Style::default().fg(theme::AMBER()),
            ),
            Span::styled(
                format!("{:<28}", t.name),
                if selected {
                    Style::default()
                        .fg(theme::TEXT_BRIGHT())
                        .add_modifier(Modifier::BOLD)
                } else {
                    text
                },
            ),
            Span::styled(
                format!(
                    "pop {:>7}  {:>9} km²",
                    compact(t.population),
                    compact(t.area_km2)
                ),
                dim,
            ),
        ];
        // The fill runs to the edge of the panel: a highlight that stops at
        // the last column reads as "these characters" rather than "this row".
        let used: usize = spans
            .iter()
            .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
            .sum();
        if used < area.width as usize {
            spans.push(Span::raw(" ".repeat(area.width as usize - used)));
        }
        lines.push(Line::from(spans).style(theme::row_style(selected)));
    }

    let scroll = board.overview_scroll.min(lines.len().saturating_sub(1));
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0));
    frame.render_widget(paragraph, area);
}
