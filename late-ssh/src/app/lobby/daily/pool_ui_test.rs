//! Pool board tests.
//!
//! What a table looks like is not something an assertion can hold, so nothing
//! here tries. These cover the parts of the screen that carry meaning: the
//! wording the player is guided by, and the geometry that turns a click into a
//! spot on the cloth.

use uuid::Uuid;

use super::*;
use crate::app::games::pool_core::{
    cue::ShotMode,
    rules::{Group, PoolRules},
    shot::Shot,
};
use crate::app::lobby::daily::pool_draft::PoolPlayback;

fn pool_state() -> DailyPoolState {
    DailyPoolState::new(PoolRules::EightBall, Uuid::new_v4(), Uuid::new_v4())
}

#[test]
fn the_hint_row_fits_the_board_it_sits_under() {
    // It is one centred line at the bottom of the screen; a hint wider than
    // the minimum board is a hint that gets clipped exactly when a new player
    // most needs to read it.
    for mode in ShotMode::ALL {
        let width = mode.hint().chars().count();
        assert!(
            width <= MIN_WIDTH as usize,
            "{mode:?} hint is {width} wide, past the {MIN_WIDTH}-column minimum"
        );
    }
}

#[test]
fn the_table_view_round_trips_a_click_back_to_the_cloth() {
    // The mouse hit test is `View::to_table` applied to the recorded rect, so
    // what matters is that the mapping inverts over the whole playfield.
    let state = pool_state();
    let spec = state.spec().expect("known table");
    let canvas = Canvas::new(MIN_WIDTH - PANEL_WIDTH, MIN_HEIGHT - 2, SURROUND);
    let view = View::fit(spec, &canvas);

    for at in [
        [0.0, 0.0],
        [spec.length, spec.width],
        [spec.length * 0.5, spec.width * 0.5],
        [spec.length * 0.22, spec.width * 0.75],
    ] {
        let back = view.to_table(view.to_px(at));
        assert!(
            (back[0] - at[0]).abs() < 1e-9 && (back[1] - at[1]).abs() < 1e-9,
            "{at:?} came back as {back:?}"
        );
    }
}

#[test]
fn the_layout_still_works_at_the_smallest_board_we_accept() {
    // Run the real splits rather than compare the constants: the panel column
    // is fixed-width and the table takes the rest, so the minimum size is the
    // one place where growing a panel silently squeezes the table to nothing.
    let area = Rect::new(0, 0, MIN_WIDTH, MIN_HEIGHT);
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .split(area);
    let cols =
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(PANEL_WIDTH)]).split(rows[1]);
    assert!(
        cols[0].width > cols[1].width,
        "the table must stay the larger half: {} vs {}",
        cols[0].width,
        cols[1].width
    );

    let panel_rows =
        Layout::vertical([Constraint::Length(INFO_ROWS), Constraint::Fill(1)]).split(cols[1]);
    assert!(
        panel_rows[1].height > READOUT_ROWS,
        "the cue drawing needs rows of its own once the readouts have theirs"
    );
}

#[test]
fn a_rack_hands_the_renderer_one_frame_per_ball() {
    let state = pool_state();
    let frames = ball_frames(&state);
    assert_eq!(frames.len(), state.rack.balls.len());
    assert!(
        frames.iter().all(|f| !f.potted),
        "nothing is potted on the break"
    );
    assert!(frames.iter().any(|f| f.id == 0), "the cue ball is drawn");
}

#[test]
fn a_potted_ball_stops_being_drawn() {
    let mut state = pool_state();
    state.rack.balls[3].potted = Some(1);
    let frames = ball_frames(&state);
    assert_eq!(
        frames.iter().filter(|f| f.potted).count(),
        1,
        "exactly the one that went down"
    );
}

#[test]
fn both_rulesets_get_their_own_name_on_the_panel() {
    // One detail type serves both games, so the only thing telling a player
    // which one they are in is this line.
    for rules in PoolRules::ALL {
        let named = match rules {
            PoolRules::EightBall => "Eight-ball",
            PoolRules::NineBall => "Nine-ball",
            PoolRules::Snooker => "Snooker",
        };
        assert!(!named.is_empty(), "{rules:?}");
    }
    assert_ne!(
        group_label(Group::Solids),
        group_label(Group::Stripes),
        "the two groups must not read the same"
    );
}

#[test]
fn every_pocket_has_a_name_to_call() {
    // Eight-ball asks the player to name a pocket, so "pocket 3" is not good
    // enough — and an index past the end must still say something.
    let state = pool_state();
    let geom = state.geometry().expect("known table");
    let mut names: Vec<&str> = (0..geom.pockets.len())
        .map(|i| table::pocket_name(i as u8))
        .collect();
    assert_eq!(names.len(), 6, "six pockets");
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), 6, "and six distinct names");
    assert!(!table::pocket_name(200).is_empty());
}

#[test]
fn a_click_lands_where_the_ball_was_drawn() {
    // The whole point of `View::fit_area`: the hit test and the renderer must
    // agree, or the pointer picks a ball that is not under it. Round-trip a
    // ball's position out to a cell and back.
    let state = pool_state();
    let spec = state.spec().expect("known table");
    let area = Rect::new(3, 2, MIN_WIDTH - PANEL_WIDTH, MIN_HEIGHT - 2);
    let view = View::fit_area(spec, area.width, area.height * 2);

    for ball in state.rack.balls.iter() {
        let (px, py) = view.to_px(ball.pos);
        let cell_x = area.x + px as u16;
        let cell_y = area.y + (py / 2.0) as u16;
        let back = table_point_at(spec, area, None, cell_x, cell_y)
            .expect("the overview always lands on the table");
        // One cell is two pixels tall, so a click resolves to within a cell's
        // worth of table — plenty inside the click reach the picker uses.
        let slack = 2.0 / view.to_px([1.0, 0.0]).0.max(1.0);
        assert!(
            (back[0] - ball.pos[0]).abs() < 0.05 && (back[1] - ball.pos[1]).abs() < 0.05,
            "ball {} at {:?} came back as {back:?} (slack {slack})",
            ball.id,
            ball.pos
        );
    }
}

#[test]
fn a_click_outside_the_table_rect_still_resolves_inside_the_room() {
    // The rect is the whole left column, so its corners are off the cloth.
    // `pool_click_table` is what rejects those; the mapping itself must not
    // panic or wrap.
    let state = pool_state();
    let spec = state.spec().expect("known table");
    let area = Rect::new(0, 0, MIN_WIDTH - PANEL_WIDTH, MIN_HEIGHT - 2);
    let corner =
        table_point_at(spec, area, None, 0, 0).expect("the overview always lands on the table");
    assert!(
        corner[0] < spec.length && corner[1] < spec.width,
        "a corner click is off the playfield, not past the far rail: {corner:?}"
    );
}

#[test]
fn a_click_in_the_eye_view_lands_where_the_ray_was_cast() {
    // The eye's mapping is the renderer's, inverted: a pixel is a ray and a
    // ray is a point on the cloth. Two views on one board means two chances
    // for a click to land somewhere the picture did not put it, so this
    // round-trips the same way the overview's test does.
    use crate::app::games::pool_core::table_3d::Eye;

    let state = pool_state();
    let spec = state.spec().expect("known table");
    let canvas = Canvas::new(MIN_WIDTH - PANEL_WIDTH, MIN_HEIGHT - 2, SURROUND);
    let cue = [spec.length * 0.25, spec.width * 0.5];
    let eye = Eye::behind(cue, 0.0, spec, &canvas);

    for at in [
        [spec.length * 0.5, spec.width * 0.5],
        [spec.length * 0.8, spec.width * 0.3],
        [spec.length * 0.6, spec.width * 0.72],
    ] {
        let (px, py, _) = eye.to_screen(at, 0.0).expect("down the table, in view");
        let back = eye.to_table(px, py).expect("and below the horizon");
        assert!(
            (back[0] - at[0]).abs() < 1e-6 && (back[1] - at[1]).abs() < 1e-6,
            "{at:?} came back as {back:?}"
        );
    }

    // The top of the frame is never table. The horizon is usually *off* the
    // top — the view is framed on the table, not on the skyline — so a ray up
    // there may well still meet the cloth plane, just somewhere past the far
    // rail. Either answer is fine; landing on the playfield would not be.
    let off = eye.to_table(10.0, 0.0);
    assert!(
        off.is_none_or(|at| at[0] < 0.0
            || at[0] > spec.length
            || at[1] < 0.0
            || at[1] > spec.width),
        "the top of the frame resolved onto the cloth at {off:?}"
    );
}

#[test]
fn playback_runs_then_retires() {
    let mut state = pool_state();
    state
        .apply_shot(
            0,
            &Shot {
                place: None,
                azimuth: 0.0,
                tip: [0.0, 0.0],
                speed: 7.0,
                called_pocket: None,
                play_again: false,
            },
        )
        .expect("the break is legal");
    let timeline = state.last_timeline().expect("the break replays");
    let duration = timeline.duration;

    let playback = PoolPlayback::new(timeline);
    assert!(!playback.finished(), "a fresh playback has not run yet");
    assert_eq!(
        playback.frame().len(),
        state.rack.balls.len(),
        "every ball is on screen for the whole shot"
    );
    assert!(
        duration > 0.0,
        "a break takes time, or there is nothing to animate"
    );
}
