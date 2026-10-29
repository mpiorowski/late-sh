//! The orders view: tonight's orders, each priced as the board now stands,
//! what is left to order, and beneath them the dispatch, what the last night
//! did fight by fight with the odds the board showed beside the odds rolled.
//! Also the order prompt, drawn where the map's rail stands over whatever
//! view `a`/`f` opened it from.
//!
//! Orders are secret until the night, and this is where that is kept: a
//! viewer only ever sees their own. A visitor gets the last night whole.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use uuid::Uuid;

use crate::app::common::theme;

use super::history_ui::day_label;
use super::log_ui::{entry_lines, fought_line};
use super::map::WorldMap;
use super::map_ui::RAIL_WIDTH;
use super::night::{self, Projection};
use super::resolver::{DayResult, LogEntry, Order, RealmGameState, TakerRoll};
use super::state::{
    RealmState, ViewerRole, dispatch_line, player_color, points_label, points_left,
};
use super::targets_ui::pad_to;
use super::ui::truncate;

pub fn draw_orders_view(frame: &mut Frame, area: Rect, realm: &RealmState) {
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
    let role = realm.viewer_role();
    let bright = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(theme::TEXT_DIM());
    let mut lines: Vec<Line<'static>> = Vec::new();

    // The orders take at most the top half of the view, the list scrolling
    // under its heading to keep the cursor row on screen, so a day's worth
    // of one-point orders never pushes the dispatch off the bottom. The
    // dispatch gets the rest and clips there on its own.
    let height = usize::from(area.height);
    let room = (height / 2).max(4).saturating_sub(2).max(1);
    match role {
        ViewerRole::Playing => {
            let points = points_left(board, realm.user_id)
                .map(|(left, cap)| format!(" · {left}/{cap} pts left to order"))
                .unwrap_or_default();
            let flight = if board.act_in_flight {
                " · sending"
            } else {
                ""
            };
            lines.push(Line::from(Span::styled(
                format!("tonight's orders{points}{flight}"),
                bright,
            )));
            let orders = realm.my_orders();
            if orders.is_empty() {
                lines.push(Line::from(Span::styled(
                    "  none yet · a on a territory orders a take, f on yours raises its walls",
                    dim,
                )));
            }
            let cursor = board.orders_cursor.min(orders.len().saturating_sub(1));
            let start = cursor
                .saturating_sub(room / 2)
                .min(orders.len().saturating_sub(room));
            for (idx, order) in orders.iter().enumerate().skip(start).take(room) {
                lines.push(order_line(
                    *order,
                    realm.order_projection(*order),
                    state,
                    &map,
                    idx == cursor,
                    area.width as usize,
                ));
            }
            lines.push(Line::from(Span::styled(
                "  nobody else can see these until the night",
                Style::default().fg(theme::TEXT_FAINT()),
            )));
        }
        ViewerRole::Fallen | ViewerRole::Watching => {
            lines.push(Line::from(Span::styled(
                format!("{} · orders are secret until the night", role.label()),
                bright,
            )));
        }
    }
    let top = Rect {
        height: lines.len().min(height) as u16,
        ..area
    };
    let rest = Rect {
        y: area.y + top.height,
        height: area.height - top.height,
        ..area
    };
    frame.render_widget(Paragraph::new(lines), top);

    let mut dispatch: Vec<Line<'static>> = vec![Line::raw("")];
    match state.last_night.as_ref() {
        None => dispatch.push(Line::from(Span::styled(
            "no night has passed yet · the first resolves when the day turns",
            dim,
        ))),
        Some(night) => dispatch.extend(dispatch_lines(night, state, &map, realm.user_id, role)),
    }
    frame.render_widget(Paragraph::new(dispatch).wrap(Wrap { trim: false }), rest);
}

/// One order in the list: what it is, what it costs, and its odds now.
fn order_line(
    order: Order,
    projection: Option<Projection>,
    state: &RealmGameState,
    map: &WorldMap,
    selected: bool,
    width: usize,
) -> Line<'static> {
    let name = map
        .territory(order.target())
        .map(|t| t.name.clone())
        .unwrap_or_else(|| format!("#{}", order.target()));
    let name_style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT())
    };
    let dim = Style::default().fg(theme::TEXT_DIM());
    let (glyph, size, odds, note) = match (order, projection) {
        (Order::Take { points, .. }, Some(p)) => (
            match state.ownership.contains_key(&order.target()) {
                true => "⚔",
                false => "·",
            },
            points_label(points.get()),
            format!("{:.0}%", p.probability * 100.0),
            format!(
                "from {}{}",
                map.territory(p.from)
                    .map(|t| t.name.clone())
                    .unwrap_or_default(),
                if p.landing { " by sea" } else { "" }
            ),
        ),
        // The front it was ordered across is gone (a holding of yours left
        // the board); the night will fizzle it unless that changes.
        (Order::Take { points, .. }, None) => (
            "⚔",
            points_label(points.get()),
            "—".to_string(),
            "no front now".to_string(),
        ),
        (Order::Fortify { levels, target }, _) => (
            "⛨",
            format!("+{} walls", levels.get()),
            String::new(),
            format!(
                "to {} of {}",
                state.fort_level(target) + levels.get(),
                state.ruleset.fort_max_level
            ),
        ),
    };
    let mut spans = vec![
        Span::styled(
            if selected { "▸ " } else { "  " },
            Style::default().fg(theme::AMBER()),
        ),
        Span::styled(format!("{glyph} "), dim),
        Span::styled(format!("{:<24}", truncate(&name, 24)), name_style),
        Span::styled(format!("{size:>10}"), dim),
        Span::styled(format!("{odds:>6}"), odds_style(projection)),
        Span::styled(
            format!("  {note}"),
            Style::default().fg(theme::TEXT_FAINT()),
        ),
    ];
    pad_to(&mut spans, width);
    Line::from(spans).style(theme::row_style(selected))
}

fn odds_style(projection: Option<Projection>) -> Style {
    match projection.map(|p| (p.probability * 100.0).round() as i64) {
        Some(odds) if odds >= 50 => Style::default().fg(theme::SUCCESS()),
        Some(odds) if odds >= 15 => Style::default().fg(theme::AMBER()),
        Some(_) | None => Style::default().fg(theme::TEXT_DIM()),
    }
}

/// What the last night did. A player gets their count, then their own
/// fights (as a taker or as the holder), each roll with the odds their board
/// showed when it was ordered and why the two differ; then the night's
/// walls, fizzles and falls that were theirs, and every win or fall. A
/// visitor gets every line of it.
fn dispatch_lines(
    night: &DayResult,
    state: &RealmGameState,
    map: &WorldMap,
    viewer: Uuid,
    role: ViewerRole,
) -> Vec<Line<'static>> {
    let bright = Style::default()
        .fg(theme::AMBER())
        .add_modifier(Modifier::BOLD);
    let mut lines = vec![Line::from(Span::styled(
        format!("the last night · {}", day_label(night.day)),
        bright,
    ))];
    let everything = role == ViewerRole::Watching;
    if !everything {
        lines.push(Line::from(Span::styled(
            format!("  {}", dispatch_line(night, viewer)),
            Style::default().fg(theme::TEXT()),
        )));
    }
    let mut shown_any = false;
    for entry in &night.entries {
        match entry {
            LogEntry::Fought {
                target,
                holder,
                taken_by,
                takers,
                bounced,
                ..
            } => {
                let involved = everything
                    || *holder == Some(viewer)
                    || takers.iter().any(|roll| roll.user_id == viewer);
                if !involved {
                    continue;
                }
                shown_any = true;
                for roll in takers {
                    lines.push(fought_line(
                        state, map, *target, *holder, *taken_by, *bounced, roll,
                    ));
                    if roll.user_id == viewer
                        && let Some(why) = why_the_odds_moved(night, map, *target, *holder, roll)
                    {
                        lines.push(Line::from(Span::styled(
                            format!("      your board showed {:.0}%: {why}", roll.shown * 100.0),
                            Style::default().fg(theme::TEXT_FAINT()),
                        )));
                    }
                }
            }
            LogEntry::Fortified { user_id, .. } | LogEntry::Fizzled { user_id, .. } => {
                if everything || *user_id == viewer {
                    shown_any = true;
                    lines.extend(entry_lines(entry, state, map));
                }
            }
            // Falls and the end are the night's news for everyone.
            LogEntry::Eliminated { .. } | LogEntry::Won { .. } => {
                shown_any = true;
                lines.extend(entry_lines(entry, state, map));
            }
            LogEntry::Claimed { .. }
            | LogEntry::Attacked { .. }
            | LogEntry::Spawned { .. }
            | LogEntry::Left { .. }
            | LogEntry::Kicked { .. } => {
                if everything {
                    shown_any = true;
                    lines.extend(entry_lines(entry, state, map));
                }
            }
        }
    }
    if !shown_any {
        lines.push(Line::from(Span::styled(
            "  a quiet night: nobody fought",
            Style::default().fg(theme::TEXT_DIM()),
        )));
    }
    lines
}

/// Why a roll's odds are not the odds its board showed, in the words of the
/// night's own log: the holder's walls went up tonight, a neighbour of
/// theirs was fought over (so it lent the target no support), or it came
/// off the sea. `None` when the two agree.
fn why_the_odds_moved(
    night: &DayResult,
    map: &WorldMap,
    target: u16,
    holder: Option<Uuid>,
    roll: &TakerRoll,
) -> Option<String> {
    if (roll.shown - roll.probability).abs() < 0.005 {
        return None;
    }
    let neighbours: Vec<u16> = map
        .territory(target)
        .map(|t| t.land_links.iter().map(|(n, _)| *n).collect())
        .unwrap_or_default();
    let walls_went_up = night.entries.iter().any(|entry| {
        matches!(
            entry,
            LogEntry::Fortified { user_id, target: walled, .. }
                if Some(*user_id) == holder && (*walled == target || neighbours.contains(walled))
        )
    });
    let neighbour_engaged = holder.is_some()
        && night.entries.iter().any(|entry| {
            matches!(
                entry,
                LogEntry::Fought { target: fought, holder: h, .. }
                    if *h == holder && *fought != target && neighbours.contains(fought)
            )
        });
    let landing = !neighbours.contains(&roll.from);
    let mut why = Vec::new();
    if walls_went_up {
        why.push("walls went up");
    }
    if neighbour_engaged {
        why.push("a neighbour of theirs was engaged");
    }
    if landing {
        why.push("a landing");
    }
    Some(match why.is_empty() {
        true => "the board moved after you ordered".to_string(),
        false => why.join(", "),
    })
}

/// The prompt, over the right of the body: what the order is, its size, and
/// what it would fetch if the night fell now.
pub fn draw_prompt(frame: &mut Frame, body: Rect, realm: &RealmState) {
    let Some(board) = realm.board.as_ref() else {
        return;
    };
    let Some(order) = board.prompt else {
        return;
    };
    let Some(detail) = board.detail.as_ref() else {
        return;
    };
    let Some(map) = detail.map() else {
        return;
    };
    let state = &detail.state;
    let width = RAIL_WIDTH.min(body.width);
    let rect = Rect::new(
        body.x + body.width - width,
        body.y,
        width,
        9.min(body.height),
    );
    let text = Style::default().fg(theme::TEXT());
    let dim = Style::default().fg(theme::TEXT_DIM());
    let bright = Style::default()
        .fg(theme::AMBER_GLOW())
        .add_modifier(Modifier::BOLD);
    let name = map
        .territory(order.target())
        .map(|t| t.name.clone())
        .unwrap_or_default();
    let after = night::order_quote(state, realm.user_id, order, detail.today()).left_after;

    let mut lines: Vec<Line<'static>> = Vec::new();
    match order {
        Order::Take { points, target } => {
            let holder = state.ownership.get(&target).copied();
            let verb = match holder {
                Some(_) => "attack",
                None => "claim",
            };
            lines.push(Line::from(Span::styled(
                truncate(
                    &format!("{verb} {name}"),
                    usize::from(width).saturating_sub(2),
                ),
                bright,
            )));
            if let Some(holder) = holder {
                let (r, g, b) = player_color(state, holder);
                lines.push(Line::from(vec![
                    Span::styled("held by ", dim),
                    Span::styled(
                        state
                            .player(holder)
                            .map(|p| p.username.clone())
                            .unwrap_or_default(),
                        Style::default().fg(Color::Rgb(r, g, b)),
                    ),
                ]));
            }
            lines.push(Line::from(vec![
                Span::styled("with ", dim),
                Span::styled(format!("◂ {} ▸", points_label(points.get())), bright),
            ]));
            match realm.prompt_projection() {
                Some(p) => {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{:.0}%", p.probability * 100.0),
                            odds_style(Some(p)),
                        ),
                        Span::styled(
                            format!(
                                " if the night fell now · front {:.1}{}{}",
                                p.front,
                                if p.landing { " · landing" } else { "" },
                                if p.encircled { " · encircled" } else { "" }
                            ),
                            dim,
                        ),
                    ]));
                    lines.push(Line::from(Span::styled(
                        format!(
                            "from {}",
                            map.territory(p.from)
                                .map(|t| t.name.clone())
                                .unwrap_or_default()
                        ),
                        dim,
                    )));
                }
                None => lines.push(Line::from(Span::styled("no front behind it", dim))),
            }
        }
        Order::Fortify { levels, target } => {
            lines.push(Line::from(Span::styled(
                truncate(
                    &format!("walls on {name}"),
                    usize::from(width).saturating_sub(2),
                ),
                bright,
            )));
            lines.push(Line::from(vec![
                Span::styled("raise ", dim),
                Span::styled(
                    format!(
                        "◂ {} ▸",
                        match levels.get() {
                            1 => "1 level".to_string(),
                            n => format!("{n} levels"),
                        }
                    ),
                    bright,
                ),
            ]));
            lines.push(Line::from(Span::styled(
                format!(
                    "to {} of {} · up before tonight's fights",
                    state.fort_level(target) + levels.get(),
                    state.ruleset.fort_max_level
                ),
                text,
            )));
        }
    }
    lines.push(Line::from(Span::styled(
        format!("{after} pts left after this"),
        dim,
    )));
    lines.push(Line::from(Span::styled(
        "enter place · esc cancel",
        Style::default().fg(theme::TEXT_FAINT()),
    )));

    frame.render_widget(Clear, rect);
    let block = Block::default()
        .title(" order for tonight ")
        .title_style(bright)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
