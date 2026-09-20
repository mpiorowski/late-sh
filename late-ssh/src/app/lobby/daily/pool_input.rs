//! Pool's input: the key map and the mouse, for all three cue games.
//!
//! One file per surface concern, mirroring `pool_ui.rs`: `board_input`
//! dispatches here with a single call per event kind and knows nothing else
//! about pool. The gate (`is_pool_board`) lives beside the handlers it gates,
//! asked of `DailyMatchDetail::pool()` — the one list of pool detail variants
//! — because a hand-copied variant list in front of an input layer is how
//! snooker shipped with a board where nothing responded.
//!
//! Everything here works through `App` and the pub surface of the daily
//! state. The two things pool input needs that are not pub — the service
//! handle for sending a shot, and the aim-share throttle — stay behind
//! `DailyState` methods (`pool_send`, `pool_publish_aim`).

use crate::app::games::pool_core::cue::{MISCUE_LIMIT, PowerBand, ShotMode};
use crate::app::games::pool_core::shot::Shot;
use crate::app::input::{MouseButton, MouseEvent, MouseEventKind};
use crate::app::lobby::daily::pool::DailyPoolState;
use crate::app::lobby::daily::pool_draft::{PointerOutcome, PoolCueHit, PoolDraft};
use crate::app::lobby::daily::state::DailyMatchDetail;
use crate::app::state::App;

use super::board_input::within;

/// The draft, but only while this player may actually change it: their board,
/// their turn, nothing rolling and nothing already on the wire. The one gate
/// every gesture goes through.
fn draft_mut(app: &mut App) -> Option<(&mut PoolDraft, &DailyPoolState)> {
    app.daily.pool_draft_mut()
}

/// Pool's key map.
///
/// There is no sequence to walk: every part of a shot is adjustable at any
/// moment and the player may strike whenever they like. A key only says what
/// the mouse is currently steering, because a terminal has no key-up event to
/// hang a real hold on (see `ShotMode`), so `a` arms aim rather than meaning
/// "while a is down".
///
/// ```text
/// [  ]  '     step through the legal targets; `'` jumps to the obvious one
/// a           aim mode: the pointer walks the aim across the ball
/// e           spin mode: the pointer walks the tip across the cue ball
/// m           ball in hand, when a foul has granted one
/// p           name a pocket, when the shot has to call one
/// c           put the shot back to square: centre ball, dead-on aim
/// v           swap the overview for the view down the shot
/// y           snooker: after a foul, make them play it again
/// x  s  w     arm the stroke light / normal / strong, then draw and push
/// h  l        walk the aim by hand; H/L a fifth as far
/// ```
///
/// This overlaps wasd, which is why it runs before the shared cursor keys —
/// but pool has no cell cursor for wasd to move, so nothing is lost.
pub(crate) fn pool_key(app: &mut App, byte: u8) -> bool {
    if !is_pool_board(app) {
        return false;
    }
    match byte {
        b'[' => pool_cycle_target(app, -1),
        b']' => pool_cycle_target(app, 1),
        // Next to the brackets on purpose: the same hand, the same job.
        b'\'' => pool_next_in_line(app),
        b'a' | b'A' => pool_toggle_mode(app, ShotMode::Aim),
        b'e' | b'E' => pool_toggle_mode(app, ShotMode::Spin),
        // Refused outright unless a foul actually granted ball in hand, so the
        // key cannot be used to shuffle the cue ball out of a bad leave.
        b'm' | b'M' => pool_take_ball_in_hand(app),
        // Naming a pocket, for the shot that has to. Refused when nothing is
        // being called for, so `p` keeps its board-wide meaning everywhere
        // else. Without this and the click that does the same, eight-ball
        // cannot be won at all: a pot of the eight into an uncalled pocket is
        // a loss by rule.
        b'p' | b'P' => pool_cycle_pocket(app, 1),
        // Put the shot back to square: centre ball, dead-on aim. The mouse
        // says the same thing with a right-click.
        b'c' | b'C' => pool_reset_mode(app),
        // The camera, not a control: allowed whoever is at the table, because
        // watching a shot play out from behind the cue ball is half of why it
        // is worth having.
        b'v' | b'V' => pool_toggle_eye(app),
        // Snooker only, and only after a foul: hand the shot straight back
        // rather than play from where they left you. Refused otherwise, so it
        // costs the other games nothing.
        b'y' | b'Y' => pool_play_again(app),
        b'h' => pool_aim(app, -1, false),
        b'l' => pool_aim(app, 1, false),
        b'H' => pool_aim(app, -1, true),
        b'L' => pool_aim(app, 1, true),
        _ => match PowerBand::from_key(byte.to_ascii_lowercase() as char) {
            Some(band) => pool_toggle_mode(app, ShotMode::Stroke(band)),
            None => false,
        },
    }
}

/// Whether the open board is one of the cue games.
///
/// Asked of the *detail* rather than of the roster enum, and that is the whole
/// point: this one gate turns off every pool key and every pool mouse handler
/// at once, so a roster variant left out of it is a board where nothing at all
/// responds. Snooker shipped that way for exactly one commit. `pool()` already
/// has to name every pool detail variant to exist, so there is now one list
/// instead of two that can disagree.
pub(crate) fn is_pool_board(app: &App) -> bool {
    app.daily
        .board
        .as_ref()
        .and_then(|board| board.detail.as_ref())
        .and_then(DailyMatchDetail::pool)
        .is_some()
}

/// Mouse on a pool board.
///
/// Bare motion is the main event, not clicks: `?1003h` any-event tracking is
/// already on, so the pointer steers whatever mode is armed without a button
/// being held. The buttons carry the two things a motion stream cannot say —
/// **left commits** (keep it, put the cue down) and **right cancels** (put it
/// back to what it was when the mode was armed).
///
/// Left acts on the **release**, and only when the press did not travel. A
/// press that moves is a *re-grip* — the mouse lifted off the pad — which is
/// the only way to keep turning an aim once the pointer has run out of screen.
/// That is also why the click cannot be taken on the press: until the button
/// comes up there is no telling the two apart.
pub(crate) fn handle_pool_mouse(app: &mut App, mouse: &MouseEvent) -> bool {
    // Mouse coordinates are 1-based; the frame buffer is 0-based.
    let x = mouse.x.saturating_sub(1);
    let y = mouse.y.saturating_sub(1);

    match mouse.kind {
        // Motion, with or without a button. Consumed only when it actually
        // moved something, so a pointer crossing an idle board is free.
        MouseEventKind::Moved | MouseEventKind::Drag => {
            // Ball in hand is the one mode the pointer steers through the
            // table rather than as a bare delta: the cue ball follows to
            // wherever it is pointing, so the spot is chosen by looking at it.
            if let Some(at) = table_point(app, x, y)
                && pool_hover_table(app, at)
            {
                return true;
            }
            pool_pointer_moved(app, x, y, mouse.kind == MouseEventKind::Drag)
        }
        // Right button: zero whatever is armed — centre the tip, straighten
        // the aim — and stay in it. Taken on the press rather than the release
        // so it feels immediate, and the matching release is swallowed below
        // so it cannot also read as the end of a stroke. Esc is the one that
        // restores and exits; this is the one players actually want mid-aim.
        MouseEventKind::Down if mouse.button == Some(MouseButton::Right) => {
            pool_reset_mode(app);
            true
        }
        MouseEventKind::Up if mouse.button == Some(MouseButton::Right) => true,
        MouseEventKind::Up => {
            if mouse.button != Some(MouseButton::Left) {
                return false;
            }
            if pool_pointer_released(app) {
                click_at(app, x, y);
            }
            true
        }
        MouseEventKind::Down => {
            if mouse.button != Some(MouseButton::Left) {
                return false;
            }
            pool_pointer_pressed(app, x, y);
            true
        }
        _ => false,
    }
}

/// A left click that stayed put, resolved against what the last render drew.
fn click_at(app: &mut App, x: u16, y: u16) {
    let Some(board) = &app.daily.board else {
        return;
    };
    let table = board.target_geometry.get();
    let cue = board.cue_geometry.get();

    // The panel is checked first: it is the smaller, more specific half of the
    // screen, and it is laid out as the shot itself, so where the click lands
    // says which part of the shot it is about.
    if let Some(hit) = cue
        && within(hit.area, x, y)
        && hit.panel.cue_radius > 0.0
    {
        click_cue_panel(app, hit, x, y);
        return;
    }
    if let Some(area) = table
        && within(area, x, y)
    {
        // Idle: the click picks a target. Armed: it commits, and the click is
        // deliberately not also a re-target — that would throw away the
        // adjustment it is there to keep. A click above the eye view's horizon
        // is the room: it commits, like anywhere else off the cloth.
        match table_point(app, x, y) {
            Some(at) if pool_click_table(app, at) => {}
            _ => {
                pool_commit_mode(app);
            }
        }
        return;
    }
    // Anywhere else on the board still commits, so there is always somewhere
    // harmless to click to mean "done".
    pool_commit_mode(app);
}

/// A click in the cue panel.
///
/// The panel draws the shot from the shooter's eye — target ball at the top,
/// cue ball in front of you, cue below it — so its zones *are* the parts of
/// the shot, and a click can arm the one it lands on without the player ever
/// learning a key. Top half: the aim. The ball's face: the tip, placed and
/// committed in the one gesture, because clicking a spot on the face is
/// exactly saying "there, done". Below the ball: the cue, so the stroke.
fn click_cue_panel(app: &mut App, hit: PoolCueHit, x: u16, y: u16) {
    // Mid-stroke a press is the start of the pull, never a mode change.
    if pool_mode(app).is_some_and(|mode| mode.band().is_some()) {
        return;
    }
    // Two pixel rows to the terminal row, matching the half-block canvas the
    // panel is drawn into.
    let px = (x - hit.area.x) as f64 + 0.5;
    let py = (y - hit.area.y) as f64 * 2.0 + 0.5;
    let across = (px - hit.panel.cue.0) / hit.panel.cue_radius;
    // Screen rows grow downward, the tip offset grows up the face: get this
    // backwards and follow becomes draw.
    let up = (hit.panel.cue.1 - py) / hit.panel.cue_radius;
    if across.hypot(up) <= 1.0 {
        pool_click_cue(app, across, up);
        pool_commit_mode(app);
        return;
    }
    if py < hit.panel.cue.1 - hit.panel.cue_radius {
        pool_toggle_mode(app, ShotMode::Aim);
        return;
    }
    if py > hit.panel.cue.1 + hit.panel.cue_radius {
        // The middle band, which is what an unqualified "shoot" means and
        // what `s` arms. The other two stay a keypress away.
        pool_toggle_mode(app, ShotMode::Stroke(PowerBand::Normal));
        return;
    }
    // Beside the ball: no part of the shot lives there, so it means "done".
    pool_commit_mode(app);
}

/// The spot on the cloth under the pointer, when it is over the table at all.
fn table_point(app: &App, x: u16, y: u16) -> Option<[f64; 2]> {
    let board = app.daily.board.as_ref()?;
    let area = board.target_geometry.get()?;
    if !within(area, x, y) {
        return None;
    }
    let eye = board.pool_eye_geometry.get();
    let spec = pool_spec(app)?;
    super::pool_ui::table_point_at(spec, area, eye, x, y)
}

fn pool_spec(app: &App) -> Option<&'static crate::app::games::pool_core::table::TableSpec> {
    app.daily
        .board
        .as_ref()?
        .detail
        .as_ref()?
        .pool()?
        .state
        .spec()
        .ok()
}

/// `[` / `]`: step through the balls this player may legally hit first.
pub(crate) fn pool_cycle_target(app: &mut App, delta: isize) -> bool {
    let Some((draft, state)) = draft_mut(app) else {
        return false;
    };
    draft.cycle_target(state, delta);
    true
}

/// `a` / `e` / `x` / `s` / `w`: arm a mode, or drop it if it is already
/// running. Returns whether the key was for this board.
pub(crate) fn pool_toggle_mode(app: &mut App, mode: ShotMode) -> bool {
    let Some((draft, _)) = draft_mut(app) else {
        return false;
    };
    draft.toggle_mode(mode);
    true
}

/// Left click, or the mode's own key: keep the adjustment and put the cue
/// down. Not for stroke mode, where a press is the start of the pull.
pub(crate) fn pool_commit_mode(app: &mut App) -> bool {
    match draft_mut(app) {
        Some((draft, _)) if draft.mode.band().is_none() => draft.commit(),
        _ => false,
    }
}

/// `v`: swap between the overview and the shooter's eye.
///
/// Only on a pool board, and allowed while watching or waiting as well as
/// while shooting — it is a camera, not a control, and the shot playing
/// out from behind the cue ball is the best reason to have it.
pub(crate) fn pool_toggle_eye(app: &mut App) -> bool {
    if !is_pool_board(app) {
        return false;
    }
    if let Some(board) = &mut app.daily.board {
        board.pool_eye = !board.pool_eye;
    }
    true
}

/// `y`: hand the shot straight back after a foul, which is snooker's
/// alone. Refused unless the last shot actually fouled.
pub(crate) fn pool_play_again(app: &mut App) -> bool {
    let can = draft_mut(app).is_some_and(|(_, state)| state.may_return);
    if can {
        app.daily.pool_send(Shot {
            play_again: true,
            ..Shot::default()
        });
    }
    can
}

/// `p`: step through the pockets, for the shot that has to name one.
///
/// The mouse names a pocket by pointing at it, which is the obvious way;
/// this is the keyboard's, and it is not optional politeness — without a
/// way to call, eight-ball cannot be won at all.
pub(crate) fn pool_cycle_pocket(app: &mut App, delta: isize) -> bool {
    let Some((draft, state)) = draft_mut(app) else {
        return false;
    };
    if !state.requires_call() {
        return false;
    }
    let Ok(spec) = state.spec() else {
        return false;
    };
    let count = spec.geometry().pockets.len() as isize;
    if count == 0 {
        return false;
    }
    let next = match draft.called_pocket {
        Some(index) => (index as isize + delta).rem_euclid(count),
        None if delta < 0 => count - 1,
        None => 0,
    };
    draft.called_pocket = Some(next as u8);
    true
}

/// Right click: zero whatever is armed, and stay in it.
///
/// "Put the spin back to centre" is what a player reaches for far more
/// often than "undo the last few pixels of it", and having to drop the
/// mode and re-arm it to get there made centring the tip the most awkward
/// thing on the board. Esc still restores-and-exits, so nothing is lost.
pub(crate) fn pool_reset_mode(app: &mut App) -> bool {
    match draft_mut(app) {
        Some((draft, state)) => draft.reset(state),
        None => false,
    }
}

/// `h`/`l` walk the aim, `H`/`L` walk it a fifth as far for the last
/// fraction of a degree. These work whether or not aim mode is armed —
/// they are unambiguous on their own, so there is nothing to gate them on.
pub(crate) fn pool_aim(app: &mut App, delta: isize, fine: bool) -> bool {
    let Some((draft, _)) = draft_mut(app) else {
        return false;
    };
    draft.key_aim(delta, fine);
    true
}

/// A click on the table view. `at` is already in table coordinates.
///
/// Picks the ball under the click when there is one and it is a legal
/// target, and otherwise takes the bare point — which is how a cushion
/// gets picked, and the one thing the keyboard cannot do.
pub(crate) fn pool_click_table(app: &mut App, at: [f64; 2]) -> bool {
    let Some((draft, state)) = draft_mut(app) else {
        return false;
    };
    // Holding the cue ball, the click sets it down instead of picking a
    // target — that is the whole of the mode, so it also ends it. Left
    // click is "there, done" everywhere else on this board and placement
    // should not be the one thing that needs a key to finish.
    if draft.mode == ShotMode::Place {
        return draft.place_and_commit(state, at);
    }
    // Otherwise only while nothing is armed. With a mode running the click
    // is that mode's commit, and re-targeting mid-aim would throw away the
    // very adjustment the click is there to keep.
    if draft.mode != ShotMode::Idle {
        return false;
    }
    let Ok(spec) = state.spec() else {
        return false;
    };
    // Down to the eight: a click near a pocket names it. This outranks
    // picking a target because there is only one legal target left — the
    // eight — so the pockets are the only thing on the cloth left worth
    // pointing at, and naming one is the whole of what the shot needs.
    if draft.call_pocket_at(state, at) {
        return true;
    }
    // Generous: the drawn ball is bigger than life, so the click target
    // should be too, or the picture and the pointer disagree.
    let reach = spec.ball_radius * 3.0;
    let hit = state
        .legal_targets()
        .into_iter()
        .filter_map(|id| state.rack.get(id).map(|ball| (id, ball.pos)))
        .map(|(id, pos)| (id, (pos[0] - at[0]).hypot(pos[1] - at[1])))
        .filter(|(_, d)| *d <= reach)
        .min_by(|a, b| a.1.total_cmp(&b.1));
    match hit {
        Some((id, _)) => draft.aim_at_ball(state, id),
        None => draft.aim_at_point(at),
    }
    true
}

/// A click on the cue ball's face, as a fraction of its radius from the
/// centre. `up` is already flipped into table sense, so positive is follow.
///
/// Unlike the table click this is absolute: the tip goes where the pointer
/// is, because the face is drawn large enough to aim at directly.
pub(crate) fn pool_click_cue(app: &mut App, across: f64, up: f64) -> bool {
    let Some((draft, _)) = draft_mut(app) else {
        return false;
    };
    // Idle or spin only: in a stroke the press is the start of the pull,
    // and while aiming the face is not what the click is about.
    if !matches!(draft.mode, ShotMode::Idle | ShotMode::Spin) {
        return false;
    }
    draft.tip = [0.0, 0.0];
    // The drawn face is magnified: its whole radius is the half-radius the
    // tip may use, so a click at the edge is the miscue limit and not a
    // shot the server would refuse.
    draft.nudge_tip(across * MISCUE_LIMIT, up * MISCUE_LIMIT);
    true
}

/// Pointer motion. Returns whether it moved anything, so a mouse crossing
/// an idle board does not repaint on every reported pixel — and fires the
/// shot when the cue is pushed forward through the ball.
pub(crate) fn pool_pointer_moved(app: &mut App, x: u16, y: u16, button_down: bool) -> bool {
    let outcome = match draft_mut(app) {
        Some((draft, _)) => draft.pointer_moved(x, y, button_down),
        None => PointerOutcome::Ignored,
    };
    match outcome {
        PointerOutcome::Ignored => false,
        PointerOutcome::Changed => true,
        PointerOutcome::Strike => {
            app.daily.pool_fire();
            true
        }
    }
}

/// Pointer motion over the cloth while the cue ball is in hand: carry it
/// there and now, so the player is choosing a spot by looking at it rather
/// than clicking blind and finding out where it went.
///
/// Reports whether the ball actually moved — `free_spot` snaps, so a
/// pointer wandering inside one snap radius resolves to the same spot and
/// there is nothing to repaint.
pub(crate) fn pool_hover_table(app: &mut App, at: [f64; 2]) -> bool {
    let Some((draft, state)) = draft_mut(app) else {
        return false;
    };
    if draft.mode != ShotMode::Place {
        return false;
    }
    let before = draft.place;
    draft.put_down(state, at);
    draft.place != before
}

/// What the pointer is currently wired to, for input paths that must treat
/// a stroke differently from everything else.
pub(crate) fn pool_mode(app: &App) -> Option<ShotMode> {
    app.daily
        .board
        .as_ref()?
        .detail
        .as_ref()
        .and_then(DailyMatchDetail::pool)
        .map(|pool| pool.draft.mode)
}

/// `'`: jump to the ball most obviously on.
pub(crate) fn pool_next_in_line(app: &mut App) -> bool {
    let Some((draft, state)) = draft_mut(app) else {
        return false;
    };
    draft.next_in_line(state);
    true
}

/// `m`: take the cue ball in hand, when a foul has granted one.
pub(crate) fn pool_take_ball_in_hand(app: &mut App) -> bool {
    let Some((draft, state)) = draft_mut(app) else {
        return false;
    };
    if state.ball_in_hand.is_none() {
        return false;
    }
    draft.toggle_mode(ShotMode::Place);
    true
}

pub(crate) fn pool_pointer_pressed(app: &mut App, x: u16, y: u16) {
    if let Some((draft, _)) = draft_mut(app) {
        draft.pointer_pressed(x, y);
    }
}

/// Pointer released. Reports whether it was a click rather than the end of
/// a re-grip, which is what decides if the button meant anything.
pub(crate) fn pool_pointer_released(app: &mut App) -> bool {
    match draft_mut(app) {
        Some((draft, _)) => draft.pointer_released(),
        None => false,
    }
}
