//! The realm board's map view: the board's own viewport bookkeeping, and the
//! right info rail (territory card, action queue, players). The drawing
//! itself — half-blocks, mips, screen-space borders — is
//! `common::worldmap::view`, which realm shares with `/map`. Mouse
//! clicks select via the render-recorded `map_geometry`.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use uuid::Uuid;

use crate::app::common::theme;

use crate::app::common::worldmap::{
    self,
    view::{BORDER_MAX_SCALE, Viewport, level_for, sample},
};

use super::map::WorldMap;
use super::resolver::RealmPlayerStatus;
use super::state::{RealmBoardState, RealmState, ownership_colors, points_left};

pub const RAIL_WIDTH: u16 = 34;

/// Fit the viewport on first draw, honour a pending focus request, and clamp
/// every draw — writing the result back through the board's cells so input
/// math sees exactly what was drawn.
fn fit_viewport(board: &RealmBoardState, map: &WorldMap, px_w: i32, px_h: i32) -> Viewport {
    let mut view = Viewport {
        scale: board.scale.get(),
        vx: board.view_x.get(),
        vy: board.view_y.get(),
        px_w,
        px_h,
    };
    if board.needs_fit.get() {
        view = Viewport::fitted(map, px_w, px_h);
        board.needs_fit.set(false);
    }
    // "Show me this country": frame its bounding box with room around it.
    if let Some(target) = board.focus_request.take()
        && let Some(focused) = Viewport::focused(map, target, px_w, px_h)
    {
        view = focused;
    }
    let view = view.clamped(map);
    board.scale.set(view.scale);
    board.view_x.set(view.vx);
    board.view_y.set(view.vy);
    view
}

/// The territory under a position in the map area (column/row), taking the
/// top pixel of that row. Shared by the crosshair select and the mouse hit
/// test, and sampled from the same level the renderer drew.
pub fn cell_at(board: &RealmBoardState, map: &WorldMap, col: u16, row: u16) -> Option<u16> {
    let view = Viewport {
        scale: board.scale.get(),
        vx: board.view_x.get(),
        vy: board.view_y.get(),
        px_w: 0,
        px_h: 0,
    };
    let (level, level_scale) = level_for(map, view.scale);
    let (bx, by) = view.base_at(col as i32, row as i32 * 2);
    sample(level, level_scale, bx, by)
}

pub fn draw_map_view(frame: &mut Frame, area: Rect, realm: &RealmState) {
    let Some(board) = realm.board.as_ref() else {
        return;
    };
    let Some(detail) = board.detail.as_ref() else {
        return;
    };
    let Some(map) = detail.map() else {
        return;
    };

    let (map_area, rail_area) = if area.width > RAIL_WIDTH + 40 {
        (
            Rect::new(area.x, area.y, area.width - RAIL_WIDTH - 1, area.height),
            Some(Rect::new(
                area.x + area.width - RAIL_WIDTH,
                area.y,
                RAIL_WIDTH,
                area.height,
            )),
        )
    } else {
        (area, None)
    };

    let px_w = map_area.width as i32;
    let px_h = map_area.height as i32 * 2;
    let view = fit_viewport(board, &map, px_w, px_h);
    board.map_geometry.set(Some(map_area));

    worldmap::view::paint(
        frame,
        map_area,
        &map,
        &view,
        &worldmap::view::Paint {
            colors: &ownership_colors(&detail.state),
            selected: board.selected,
            crosshair: true,
        },
    );

    if let Some(rail) = rail_area {
        draw_rail(frame, rail, realm, board);
    }
}

fn draw_rail(frame: &mut Frame, area: Rect, realm: &RealmState, board: &RealmBoardState) {
    let Some(detail) = board.detail.as_ref() else {
        return;
    };
    let role = realm.viewer_role();
    let Some(map) = detail.map() else {
        return;
    };
    let state = &detail.state;
    let mut lines: Vec<Line> = Vec::new();
    let dim = Style::default().fg(theme::TEXT_DIM());
    let text = Style::default().fg(theme::TEXT());
    let bright = Style::default().fg(theme::AMBER());

    // Selected territory card.
    match board.selected.and_then(|t| map.territory(t)) {
        Some(t) => {
            lines.push(Line::from(Span::styled(
                format!("{} ({})", t.name, t.iso),
                bright.add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                format!(
                    "pop {}  ·  {} km²",
                    compact(t.population),
                    compact(t.area_km2)
                ),
                dim,
            )));
            let owner_line = match state.ownership.get(&t.id) {
                Some(owner) => {
                    let name = state
                        .player(*owner)
                        .map(|p| p.username.clone())
                        .unwrap_or_else(|| "?".into());
                    let (r, g, b) = super::state::player_color(state, *owner);
                    Line::from(vec![
                        Span::styled("held by ", dim),
                        Span::styled(name, Style::default().fg(Color::Rgb(r, g, b))),
                    ])
                }
                None => Line::from(Span::styled("unclaimed", dim)),
            };
            lines.push(owner_line);
            // Walls, and what they are doing to whoever wants this place.
            let level = state.fort_level(t.id);
            if state.option(super::rulesets::OPTION_FORTIFY) {
                let bite = 1.0 - state.fortification(t.id, detail.today());
                // The same bars the target table draws, so a glance at
                // either reads the same way.
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{} ", fort_bar(level, state.ruleset.fort_max_level)),
                        Style::default().fg(if level > 1 {
                            theme::AMBER_DIM()
                        } else {
                            theme::TEXT_FAINT()
                        }),
                    ),
                    Span::styled(
                        if level > 1 {
                            format!("dug in ×{level} · costs an attacker {:.0}%", bite * 100.0)
                        } else {
                            "open ground".to_string()
                        },
                        dim,
                    ),
                ]));
            }
            let action = if !role.acts() {
                // Odds are a player's number: they are computed from land
                // the viewer does not have. Showing them to a visitor would
                // be inventing a war they are not in.
                Span::styled("", dim)
            } else if state.ownership.get(&t.id) == Some(&realm.user_id) {
                Span::styled("yours", dim)
            } else if let Some(p) =
                realm.projected_probability(&super::resolver::RealmAction::Claim { target: t.id })
            {
                let label = if state.ownership.contains_key(&t.id) {
                    "attack"
                } else {
                    "claim"
                };
                // How the route gets there is the whole story now, so the
                // card says it in words rather than a bare number.
                let note = realm
                    .selected_reach()
                    .map(|r| format!(" · {}", r.describe()))
                    .unwrap_or_default();
                // The opening truce outranks the odds: say so plainly.
                let locked = state.ownership.get(&t.id).and_then(|defender| {
                    realm.selected_reach().and_then(|r| {
                        state
                            .attack_allowed(realm.user_id, *defender, r, detail.today())
                            .err()
                    })
                });
                match locked {
                    Some(reason) => {
                        Span::styled(reason.message(), Style::default().fg(theme::AMBER_DIM()))
                    }
                    None => Span::styled(format!("{label} odds ~{:.0}%{note}", p * 100.0), text),
                }
            } else {
                Span::styled("", dim)
            };
            lines.push(Line::from(action));
        }
        None => {
            lines.push(Line::from(Span::styled("no territory selected", dim)));
            lines.push(Line::from(Span::styled(
                "arrows pan · drag to pan · enter select center",
                dim,
            )));
        }
    }
    lines.push(Line::from(""));

    // Points card: what you have left, and when it comes back. A visitor
    // has neither, so they get the state of the world instead.
    if !role.acts() {
        lines.push(Line::from(Span::styled(role.label(), bright)));
        lines.push(Line::from(Span::styled(
            format!(
                "{} of {} territories claimed",
                state.ownership.len(),
                map.territories.len()
            ),
            dim,
        )));
        let scale = board.scale.get();
        lines.push(Line::from(Span::styled(
            format!(
                "zoom ~{}km per pixel",
                (scale * 10.0).round().max(1.0) as i64
            ),
            dim,
        )));
        lines.push(Line::from(""));
    } else if let Some((left, cap)) = points_left(board, realm.user_id) {
        let flight = if board.act_in_flight {
            " · acting"
        } else {
            ""
        };
        lines.push(Line::from(Span::styled(
            format!("action points  {left}/{cap} left today{flight}"),
            bright,
        )));
        let reset = super::svc::next_reset(chrono::Utc::now(), detail.reset_hour_utc);
        let secs = (reset - chrono::Utc::now()).num_seconds().max(0);
        lines.push(Line::from(Span::styled(
            format!(
                "refills in {:02}:{:02}:{:02} · {}",
                secs / 3600,
                (secs % 3600) / 60,
                secs % 60,
                super::svc::reset_hour_label(detail.reset_hour_utc, realm.viewer_tz)
            ),
            dim,
        )));
        // Roughly how much ground a drawn pixel covers, so the zoom means
        // something (the base grid is ~10km per cell at the equator).
        let scale = board.scale.get();
        let km = (scale * 10.0).round().max(1.0) as i64;
        let borders = if scale <= BORDER_MAX_SCALE {
            ""
        } else {
            " · zoom in for borders"
        };
        lines.push(Line::from(Span::styled(
            format!("zoom ~{km}km per pixel{borders}"),
            dim,
        )));
        lines.push(Line::from(""));
    }

    // What just happened, newest first: with instant resolution the log is
    // the pulse of the game, so a few lines of it live on the map.
    if let Some(day) = state.last_day.as_ref()
        && !day.entries.is_empty()
    {
        lines.push(Line::from(Span::styled("latest", bright)));
        for entry in day.entries.iter().rev().take(4) {
            lines.push(super::log_ui::entry_line_public(entry, state, &map));
        }
        lines.push(Line::from(""));
    }

    // Players card.
    lines.push(Line::from(Span::styled("players", bright)));
    let mut players: Vec<_> = state.players.iter().collect();
    players.sort_by_key(|p| std::cmp::Reverse(state.territory_count(p.user_id)));
    for p in players.iter().take(area.height as usize) {
        lines.push(player_line(state, p.user_id, realm.user_id));
    }

    // The rail is 34 columns wide and its content is prose, not a table:
    // wrap it rather than losing the end of every sentence that runs long
    // (a phase explanation, a log line naming two players and a country).
    let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

pub fn player_line(
    state: &super::resolver::RealmGameState,
    user_id: Uuid,
    viewer: Uuid,
) -> Line<'static> {
    let Some(p) = state.player(user_id) else {
        return Line::from("");
    };
    let (r, g, b) = super::state::player_color(state, user_id);
    let held = state.territory_count(user_id);
    let status = match p.status {
        RealmPlayerStatus::Alive => String::new(),
        RealmPlayerStatus::Eliminated => " · eliminated".into(),
        RealmPlayerStatus::Left => " · left".into(),
        RealmPlayerStatus::Kicked => " · kicked".into(),
    };
    let you = if user_id == viewer { " (you)" } else { "" };
    Line::from(vec![
        Span::styled("■ ", Style::default().fg(Color::Rgb(r, g, b))),
        Span::styled(
            format!("{}{you}", p.username),
            Style::default().fg(theme::TEXT()),
        ),
        Span::styled(
            format!("  {held}{status}"),
            Style::default().fg(theme::TEXT_DIM()),
        ),
    ])
}

/// Fortification as bars: at a glance you want "how solid", not arithmetic.
/// Shared by the rail and the target table so the two never drift apart.
pub fn fort_bar(level: u8, max: u8) -> String {
    let max = max.max(1);
    (1..=max)
        .map(|n| if n <= level { '▮' } else { '·' })
        .collect()
}

/// `12.3M` style figures for the info rail.
pub fn compact(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1}k", n as f64 / 1_000.0),
        1_000_000..=999_999_999 => format!("{:.1}M", n as f64 / 1_000_000.0),
        _ => format!("{:.1}B", n as f64 / 1_000_000_000.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::common::worldmap::view::{MIN_SCALE, ZOOM_STEP, max_scale};
    use crate::app::lobby::realm::map::map_by_id;

    /// Perceptual distance, so "these two look alike" is a number rather
    /// than an opinion. CIELAB ΔE76: under about 10 two fills are easy to
    /// mistake on a glance across a map.
    fn delta_e(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
        fn lab(c: (u8, u8, u8)) -> (f64, f64, f64) {
            fn lin(u: u8) -> f64 {
                let u = f64::from(u) / 255.0;
                if u <= 0.04045 {
                    u / 12.92
                } else {
                    ((u + 0.055) / 1.055).powf(2.4)
                }
            }
            let (r, g, b) = (lin(c.0), lin(c.1), lin(c.2));
            let x = (r * 0.4124 + g * 0.3576 + b * 0.1805) / 0.95047;
            let y = r * 0.2126 + g * 0.7152 + b * 0.0722;
            let z = (r * 0.0193 + g * 0.1192 + b * 0.9505) / 1.08883;
            fn f(t: f64) -> f64 {
                if t > 0.008856 {
                    t.cbrt()
                } else {
                    7.787 * t + 16.0 / 116.0
                }
            }
            let (fx, fy, fz) = (f(x), f(y), f(z));
            (116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz))
        }
        let (a, b) = (lab(a), lab(b));
        ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2) + (a.2 - b.2).powi(2)).sqrt()
    }

    /// Fifty fills share the map — ten players by five levels of digging in
    /// — plus water, open ground and the selection ring. Two of them looking
    /// alike is a player misreading who owns what, which is the one thing
    /// the map exists to say. Guarded by measurement, because the last
    /// hand-picked table had a silver and a brown that were all but the same
    /// colour and nobody noticed until someone played them together.
    #[test]
    fn no_two_map_fills_look_alike() {
        use crate::app::lobby::realm::resolver::PALETTE_SLOTS;
        use crate::app::lobby::realm::state::PLAYER_PALETTES;
        // The service hands out colours by index against this count, so a
        // palette shorter than it would offer a colour that does not exist.
        assert_eq!(PLAYER_PALETTES.len(), PALETTE_SLOTS as usize);
        for (i, a) in PLAYER_PALETTES.iter().map(|p| p.shades).enumerate() {
            for (j, b) in PLAYER_PALETTES
                .iter()
                .map(|p| p.shades)
                .enumerate()
                .skip(i + 1)
            {
                for (li, ca) in a.iter().enumerate() {
                    for (lj, cb) in b.iter().enumerate() {
                        let d = delta_e(*ca, *cb);
                        assert!(
                            d >= 15.0,
                            "player {i} level {} and player {j} level {} are ΔE {d:.1} apart",
                            li + 1,
                            lj + 1
                        );
                    }
                }
            }
        }
        // And each player's own ladder has to be read as steps, or the
        // fortification shading says nothing.
        for (i, ladder) in PLAYER_PALETTES.iter().map(|p| p.shades).enumerate() {
            for step in ladder.windows(2) {
                let d = delta_e(step[0], step[1]);
                assert!(d >= 8.0, "player {i} has a ΔE {d:.1} step in its ladder");
            }
        }
        // Nothing may be mistaken for the sea, for unclaimed ground, or for
        // the selection ring drawn over it.
        for (i, ladder) in PLAYER_PALETTES.iter().map(|p| p.shades).enumerate() {
            for (level, c) in ladder.iter().enumerate() {
                for (what, other) in [
                    ("water", worldmap::view::WATER_COLOR),
                    ("free land", worldmap::view::BLANK_LAND_COLOR),
                    ("the selection ring", worldmap::view::SELECTION_COLOR),
                ] {
                    let d = delta_e(*c, other);
                    assert!(
                        d >= 20.0,
                        "player {i} level {} is ΔE {d:.1} from {what}",
                        level + 1
                    );
                }
            }
        }
    }

    #[test]
    fn compact_figures() {
        assert_eq!(compact(950), "950");
        assert_eq!(compact(14_500), "14.5k");
        assert_eq!(compact(67_059_887), "67.1M");
        assert_eq!(compact(1_400_000_000), "1.4B");
    }

    #[test]
    fn the_refill_countdown_tracks_the_games_own_hour() {
        use chrono::{TimeZone, Utc};
        // A game refilling at 18:00 UTC, looked at at 17:00 UTC.
        let now = Utc.timestamp_opt(86_400 * 100 + 17 * 3_600, 0).unwrap();
        let reset = super::super::svc::next_reset(now, 18);
        assert_eq!((reset - now).num_seconds(), 3_600);
        // An hour later the next one is a whole day out.
        let now = Utc.timestamp_opt(86_400 * 100 + 18 * 3_600, 0).unwrap();
        let reset = super::super::svc::next_reset(now, 18);
        assert_eq!((reset - now).num_seconds(), 86_400);
    }

    #[test]
    fn zoom_steps_are_small_and_bounded() {
        let map = map_by_id("earth").unwrap();
        let (px_w, px_h) = (200, 100);
        let ceiling = max_scale(&map, px_w, px_h);
        // The whole world fits at the ceiling, and the ceiling is a fit, not
        // an arbitrary number.
        assert!((map.grid.width as f32 / ceiling) <= px_w as f32 + 1.0);
        // A step is a fifth-ish, not a doubling: zooming reads as smooth,
        // and below 1.0 the base grid is magnified — finer than the old
        // power-of-two levels could go.
        const _: () = assert!(ZOOM_STEP > 1.1 && ZOOM_STEP < 1.5);
        const _: () = assert!(MIN_SCALE < 1.0);
        // Ten steps out from the floor still fits inside the ceiling on a
        // normal terminal, so the zoom has real range.
        let steps = (ceiling / MIN_SCALE).log(ZOOM_STEP);
        assert!(steps > 10.0, "only {steps} zoom steps");
    }

    #[test]
    fn the_sampled_level_is_never_coarser_than_the_zoom() {
        let map = map_by_id("earth").unwrap();
        for scale in [0.25f32, 0.5, 1.0, 1.7, 2.0, 3.9, 8.0, 40.0] {
            let (level, level_scale) = level_for(&map, scale);
            assert!(
                level_scale <= scale.max(1.0),
                "level {level_scale} too coarse for scale {scale}"
            );
            // And the level really is the one that claims that footprint.
            assert_eq!(
                map.grid.width as f32 / level.width as f32,
                level_scale,
                "level width disagrees with its scale"
            );
        }
        // Native scale samples the full-detail grid.
        let (level, level_scale) = level_for(&map, 1.0);
        assert_eq!(level.width, map.grid.width);
        assert_eq!(level_scale, 1.0);
    }

    #[test]
    fn outlines_only_appear_once_you_are_looking_at_a_region() {
        let map = map_by_id("earth").unwrap();
        // A whole-world view on a normal terminal is well past the cutoff:
        // outlining there is noise, not information.
        assert!(max_scale(&map, 200, 100) > BORDER_MAX_SCALE);
        // Zoomed into a country or a continent, outlines are on.
        const _: () = assert!(1.0 <= BORDER_MAX_SCALE && 4.0 <= BORDER_MAX_SCALE);
        // Framing an ordinary country lands well inside the outlined range,
        // so the borders are there when you are actually looking at them.
        // (The very widest boxes belong to countries that straddle the
        // antimeridian — framing one of those IS a whole-world view.)
        let (px_w, px_h) = (200, 100);
        let ceiling = max_scale(&map, px_w, px_h);
        let ordinary = map
            .territories
            .iter()
            .find(|t| {
                let (x0, y0, x1, y1) = t.bbox;
                (x1 - x0) > 20 && (x1 - x0) < map.grid.width / 16 && (y1 - y0) > 10
            })
            .expect("earth has ordinary-sized countries");
        let (x0, y0, x1, y1) = ordinary.bbox;
        let framed = (((x1 - x0 + 1) as f32 * 3.0) / px_w as f32)
            .max(((y1 - y0 + 1) as f32 * 3.0) / px_h as f32)
            .min(ceiling);
        assert!(
            framed <= BORDER_MAX_SCALE,
            "{} framed at {framed}, past the outline cutoff",
            ordinary.name
        );
    }

    #[test]
    fn territory_boxes_frame_their_own_land() {
        let map = map_by_id("earth").unwrap();
        for t in map.territories.iter().take(40) {
            let (x0, y0, x1, y1) = t.bbox;
            assert!(x0 <= x1 && y0 <= y1, "{} has an inverted box", t.name);
            // The recorded center sits inside the box it came from.
            assert!(
                (x0..=x1).contains(&t.center.0) && (y0..=y1).contains(&t.center.1),
                "{} center outside its box",
                t.name
            );
        }
        // Russia-sized countries get big boxes, microstates small ones.
        let widest = map
            .territories
            .iter()
            .max_by_key(|t| t.bbox.2 - t.bbox.0)
            .unwrap();
        assert!(widest.bbox.2 - widest.bbox.0 > map.grid.width / 8);
    }
}
