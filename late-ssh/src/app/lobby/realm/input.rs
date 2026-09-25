//! Input for `Screen::Realm`. Map view: arrows or a left-button drag pan,
//! `+`/`-` zoom around the center, Enter/Space selects the territory under
//! the center crosshair, a click selects directly, `a` (or `f` on your own
//! land) acts on the selection
//! (resolved on the spot). Target view: `j`/`k` walk the table, Enter or `a`
//! acts on the highlighted row. Tab cycles map -> targets -> overview -> log.
//! `n`/`N` walks your own territories, framing each on the map, and `g` in
//! either list does the same for the highlighted row. `q`/Esc returns to the
//! lobby modal — there is no quitting a running realm.

use crate::app::common::primitives::Screen;
use crate::app::input::{MouseButton, MouseEvent, MouseEventKind, ParsedInput};
use crate::app::state::App;

use super::map_ui::cell_at;
use super::state::RealmView;

pub(crate) fn handle_event(app: &mut App, event: &ParsedInput) -> bool {
    match event {
        ParsedInput::Byte(byte) => handle_key(app, *byte),
        ParsedInput::Char(ch) if ch.is_ascii() => handle_key(app, *ch as u8),
        ParsedInput::Arrow(key) => {
            handle_arrow(app, *key);
            true
        }
        ParsedInput::Mouse(mouse) => handle_mouse(app, mouse),
        _ => false,
    }
}

pub(crate) fn handle_key(app: &mut App, byte: u8) -> bool {
    if byte == b'`' {
        return crate::app::workspace::cycle::cycle_game_workspace(app);
    }
    let Some(board) = app.realm.board.as_ref() else {
        return false;
    };
    let view = board.view;
    // The results card takes the next key, whatever it is: it is news, not a
    // menu, and it should never eat a second keystroke.
    if board.results_open {
        app.realm.close_results();
        // Esc and q close only the card; everything else falls through so a
        // key pressed at the wrong moment still does its job.
        if byte == 0x1B || byte == b'q' || byte == b'Q' {
            return true;
        }
    }
    match byte {
        b'\t' => {
            if let Some(b) = app.realm.board.as_mut() {
                b.view = b.view.next();
                // Both the log and the history read the archive, and neither
                // has anything to show until it is loaded.
                if matches!(b.view, RealmView::Log | RealmView::History) && b.logs.is_empty() {
                    app.realm.load_more_logs();
                }
            }
            true
        }
        b'q' | b'Q' | 0x1B => {
            close_board(app);
            true
        }
        // There is no abandoning a running realm: stopping is something you
        // do by not playing, and your land stays there to be taken.
        // Call the result back up.
        b'r' => {
            app.realm.show_results();
            true
        }
        b'x' | b'X' => {
            let playing = app
                .realm
                .board
                .as_ref()
                .and_then(|b| b.detail.as_ref())
                .and_then(|d| d.state.player(app.realm.user_id))
                .is_some_and(|p| p.status == super::resolver::RealmPlayerStatus::Alive);
            if playing {
                app.banner = Some(crate::app::common::primitives::Banner::info(
                    "Realm: you cannot leave a realm once it has started — \
                     stop playing and your land is there to be taken",
                ));
            }
            true
        }
        _ => match view {
            RealmView::History => {
                match byte {
                    // The whole view is one control: the slider. `[` and `]`
                    // walk it, h/l do the same for a hand already on them,
                    // and there is deliberately no pan or zoom here — the map
                    // is a thumbnail of a day, and a view with two things to
                    // drive is a view where neither is obvious.
                    b'[' | b'h' | b'H' => app.realm.history_step(-1),
                    b']' | b'l' | b'L' => app.realm.history_step(1),
                    // The ends of the run, which on a two-month game is a
                    // long way to walk one day at a time.
                    b'g' => app.realm.history_jump(false),
                    b'G' => app.realm.history_jump(true),
                    _ => return false,
                }
                true
            }
            RealmView::Map => handle_map_key(app, byte),
            RealmView::Targets => {
                match byte {
                    b'j' | b'J' => app.realm.move_targets_cursor(1),
                    b'k' | b'K' => app.realm.move_targets_cursor(-1),
                    // Show the highlighted row on the map, framed.
                    b'g' | b'G' => {
                        if let Some(row) = app.realm.selected_target() {
                            focus_territory(app, row.id);
                        }
                    }
                    b'\r' | b'\n' | b' ' | b'a' | b'A' => {
                        let row = app.realm.selected_target();
                        match row {
                            // Enter on your own land shows it on the map; it
                            // never spends a point.
                            Some(row) if row.mine => focus_territory(app, row.id),
                            Some(row) => {
                                if let Some(banner) = app.realm.act_on(row.id) {
                                    app.banner = Some(banner);
                                }
                            }
                            None => {}
                        }
                    }
                    // And `f` digs in on the highlighted row, wherever the
                    // cursor happens to be.
                    b'f' | b'F' => {
                        let row = app.realm.selected_target();
                        if let Some(row) = row
                            && let Some(banner) = app.realm.fortify_on(row.id)
                        {
                            app.banner = Some(banner);
                        }
                    }
                    _ => return false,
                }
                true
            }
            RealmView::Overview => {
                match byte {
                    // j/k walk your holdings; the view scrolls with them.
                    b'j' | b'J' => {
                        app.realm.move_holdings_cursor(1);
                        scroll_overview(app, 1);
                    }
                    b'k' | b'K' => {
                        app.realm.move_holdings_cursor(-1);
                        scroll_overview(app, -1);
                    }
                    b'g' | b'G' | b'\r' | b'\n' | b' ' => {
                        if let Some(target) = app.realm.selected_holding() {
                            focus_territory(app, target);
                        }
                    }
                    _ => return false,
                }
                true
            }
            RealmView::Log => {
                match byte {
                    b'j' | b'J' => scroll_log(app, 1),
                    b'k' | b'K' => scroll_log(app, -1),
                    _ => return false,
                }
                true
            }
        },
    }
}

fn handle_map_key(app: &mut App, byte: u8) -> bool {
    match byte {
        b'+' | b'=' => zoom(app, -1),
        b'-' | b'_' => zoom(app, 1),
        b' ' | b'\r' | b'\n' => select_center(app),
        // Walk your own land, framing each holding.
        b'n' => cycle_own_territory(app, 1),
        b'N' => cycle_own_territory(app, -1),
        // Re-frame whatever is selected (after a jump from a list).
        b'z' | b'Z' => {
            let selected = app.realm.board.as_ref().and_then(|b| b.selected);
            if let Some(target) = selected {
                focus_territory(app, target);
            }
        }
        b'a' | b'A' => {
            if let Some(banner) = app.realm.act_selected() {
                app.banner = Some(banner);
            }
        }
        // The spade has its own key on purpose: a point spent on walls is a
        // point not spent taking ground, and nobody should spend one by
        // reaching for the attack key.
        b'f' | b'F' => {
            if let Some(banner) = app.realm.fortify_selected() {
                app.banner = Some(banner);
            }
        }
        _ => return false,
    }
    true
}

pub(crate) fn handle_arrow(app: &mut App, key: u8) {
    let Some(board) = app.realm.board.as_ref() else {
        return;
    };
    match board.view {
        RealmView::Map => {
            // Pan an eighth of the drawn area per press, in base-grid cells
            // so the step feels the same at every zoom.
            let scale = board.scale.get();
            let (step_x, step_y) = board
                .map_geometry
                .get()
                .map(|r| {
                    (
                        (r.width as f32 / 8.0).max(2.0) * scale,
                        (r.height as f32 * 2.0 / 8.0).max(2.0) * scale,
                    )
                })
                .unwrap_or((4.0 * scale, 4.0 * scale));
            match key {
                b'A' => board.view_y.set(board.view_y.get() - step_y),
                b'B' => board.view_y.set(board.view_y.get() + step_y),
                b'C' => board.view_x.set(board.view_x.get() + step_x),
                b'D' => board.view_x.set(board.view_x.get() - step_x),
                _ => {}
            }
        }
        RealmView::Targets => match key {
            b'A' => app.realm.move_targets_cursor(-1),
            b'B' => app.realm.move_targets_cursor(1),
            _ => {}
        },
        RealmView::Overview => match key {
            b'A' => {
                app.realm.move_holdings_cursor(-1);
                scroll_overview(app, -1);
            }
            b'B' => {
                app.realm.move_holdings_cursor(1);
                scroll_overview(app, 1);
            }
            _ => {}
        },
        RealmView::Log => match key {
            b'A' => scroll_log(app, -1),
            b'B' => scroll_log(app, 1),
            _ => {}
        },
        // Left and right run along the strip, which is the only thing this
        // view has to drive.
        RealmView::History => match key {
            b'C' => app.realm.history_step(1),
            b'D' => app.realm.history_step(-1),
            _ => {}
        },
    }
}

fn handle_mouse(app: &mut App, mouse: &MouseEvent) -> bool {
    let Some(board) = app.realm.board.as_ref() else {
        return false;
    };
    if board.view != RealmView::Map {
        return false;
    }
    let Some(rect) = board.map_geometry.get() else {
        return false;
    };
    // Terminal mouse coords are 1-based.
    let x = mouse.x.saturating_sub(1);
    let y = mouse.y.saturating_sub(1);
    let inside = x >= rect.x && y >= rect.y && x < rect.x + rect.width && y < rect.y + rect.height;

    match mouse.kind {
        // Wheel zooms around the center, the way every map does.
        MouseEventKind::ScrollUp if inside => {
            zoom(app, -1);
            true
        }
        MouseEventKind::ScrollDown if inside => {
            zoom(app, 1);
            true
        }
        MouseEventKind::Down if mouse.button == Some(MouseButton::Left) && inside => {
            // Hold the anchor; whether this turns out to be a click or a drag
            // is decided by whether the pointer moves before release.
            board.drag_anchor.set(Some((x, y)));
            board.dragged.set(false);
            true
        }
        MouseEventKind::Drag if mouse.button == Some(MouseButton::Left) => {
            let Some((from_x, from_y)) = board.drag_anchor.get() else {
                return false;
            };
            // Drag the world under the pointer: the map follows the hand, so
            // the delta is subtracted. Pixel rows are two per terminal row.
            let scale = board.scale.get();
            let dx = (f32::from(from_x) - f32::from(x)) * scale;
            let dy = (f32::from(from_y) - f32::from(y)) * 2.0 * scale;
            if dx != 0.0 || dy != 0.0 {
                board.dragged.set(true);
                board.view_x.set(board.view_x.get() + dx);
                board.view_y.set(board.view_y.get() + dy);
                board.drag_anchor.set(Some((x, y)));
            }
            true
        }
        MouseEventKind::Up if mouse.button == Some(MouseButton::Left) => {
            let was_drag = board.dragged.get();
            board.drag_anchor.set(None);
            board.dragged.set(false);
            // A press that never moved is a selection.
            if !was_drag && inside {
                select_cell(app, x - rect.x, y - rect.y);
            }
            true
        }
        _ => false,
    }
}

/// Select whatever territory sits under the center crosshair.
fn select_center(app: &mut App) {
    let Some(board) = app.realm.board.as_ref() else {
        return;
    };
    let Some(rect) = board.map_geometry.get() else {
        return;
    };
    select_cell(app, rect.width / 2, rect.height / 2);
}

fn select_cell(app: &mut App, col: u16, row: u16) {
    let Some(board) = app.realm.board.as_ref() else {
        return;
    };
    let Some(map) = board.detail.as_ref().and_then(|d| d.map()) else {
        return;
    };
    let hit = cell_at(board, &map, col, row).filter(|c| *c != super::map::WATER);
    if let Some(b) = app.realm.board.as_mut() {
        b.selected = hit;
    }
}

/// Zoom in (`delta = -1`) or out (`+1`), keeping the viewport center fixed.
/// The scale is continuous, so one press is a small step rather than a jump
/// to the next power of two.
fn zoom(app: &mut App, delta: i8) {
    let Some(board) = app.realm.board.as_ref() else {
        return;
    };
    let Some(map) = board.detail.as_ref().and_then(|d| d.map()) else {
        return;
    };
    let Some(rect) = board.map_geometry.get() else {
        return;
    };
    let px_w = rect.width as i32;
    let px_h = rect.height as i32 * 2;
    let old_scale = board.scale.get();
    let ceiling = crate::app::common::worldmap::view::max_scale(&map, px_w, px_h);
    let factor = if delta < 0 {
        1.0 / crate::app::common::worldmap::view::ZOOM_STEP
    } else {
        crate::app::common::worldmap::view::ZOOM_STEP
    };
    let new_scale =
        (old_scale * factor).clamp(crate::app::common::worldmap::view::MIN_SCALE, ceiling);
    if (new_scale - old_scale).abs() < f32::EPSILON {
        return;
    }
    // Hold the center: the point under the crosshair stays under it.
    let center_x = board.view_x.get() + px_w as f32 * old_scale / 2.0;
    let center_y = board.view_y.get() + px_h as f32 * old_scale / 2.0;
    board.scale.set(new_scale);
    board.view_x.set(center_x - px_w as f32 * new_scale / 2.0);
    board.view_y.set(center_y - px_h as f32 * new_scale / 2.0);
}

/// Frame a territory on the map: switch to the map view and ask the renderer
/// to center and zoom on it (only the renderer knows the drawn area).
pub(crate) fn focus_territory(app: &mut App, target: u16) {
    if let Some(board) = app.realm.board.as_mut() {
        board.view = RealmView::Map;
        board.selected = Some(target);
        board.focus_request.set(Some(target));
    }
}

/// Walk your own territories, framing each in turn — the way to find your
/// far-flung holdings without hunting the map for them.
fn cycle_own_territory(app: &mut App, delta: isize) {
    let user_id = app.realm.user_id;
    let Some(board) = app.realm.board.as_ref() else {
        return;
    };
    let Some(detail) = board.detail.as_ref() else {
        return;
    };
    let holdings = detail.state.holdings(user_id);
    if holdings.is_empty() {
        app.banner = Some(crate::app::common::primitives::Banner::info(
            "Realm: you hold no territory",
        ));
        return;
    }
    let current = board
        .selected
        .and_then(|sel| holdings.iter().position(|t| *t == sel));
    let next = match current {
        Some(idx) => (idx as isize + delta).rem_euclid(holdings.len() as isize) as usize,
        // Not on one of yours: start at the first (or last, going back).
        None if delta < 0 => holdings.len() - 1,
        None => 0,
    };
    focus_territory(app, holdings[next]);
}

fn scroll_overview(app: &mut App, delta: i32) {
    if let Some(b) = app.realm.board.as_mut() {
        b.overview_scroll = (b.overview_scroll as i32 + delta).max(0) as usize;
    }
}

fn scroll_log(app: &mut App, delta: i32) {
    let mut near_end = false;
    if let Some(b) = app.realm.board.as_mut() {
        b.log_scroll = (b.log_scroll as i32 + delta).max(0) as usize;
        // Heuristic page trigger: every day renders at least two lines.
        let rendered = b
            .logs
            .iter()
            .map(|d| d.result.entries.len() + 2)
            .sum::<usize>();
        near_end = delta > 0 && b.log_scroll + 10 >= rendered;
    }
    if near_end {
        app.realm.load_more_logs();
    }
}

pub(crate) fn close_board(app: &mut App) {
    let target = app.realm.close_board().unwrap_or(Screen::Dashboard);
    app.set_screen(target);
    app.show_lobby_modal = true;
}
