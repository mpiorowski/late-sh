//! The shot line is the straight-line story a player reads off the table
//! before a shot, so the tests here are about what a player would see: where
//! the line ends, which ball it is on, which way the object ball goes.

use std::f64::consts::PI;

use crate::app::games::pool_core::{
    aim::{self, Hit, LegKind, OBJECT_GUIDE_REACH, SIGHT_REACH},
    ball::{Ball, CUE},
    cue::Strike,
    shot::{BallFrame, RackState},
    sim,
    table::{BAR_BOX_7FT, SNOOKER_12FT, TableSpec},
};

const SPEC: TableSpec = BAR_BOX_7FT;

fn ball(id: u8, pos: [f64; 2]) -> BallFrame {
    BallFrame {
        id,
        pos,
        potted: false,
    }
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn unit(v: [f64; 2]) -> [f64; 2] {
    let len = v[0].hypot(v[1]);
    [v[0] / len, v[1] / len]
}

fn rotate(v: [f64; 2], angle: f64) -> [f64; 2] {
    let (sin, cos) = angle.sin_cos();
    [v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos]
}

#[test]
fn the_line_stops_at_the_first_ball_and_the_ghost_touches_it() {
    let cue = [0.3, SPEC.width / 2.0];
    let near = [0.9, SPEC.width / 2.0];
    let far = [1.5, SPEC.width / 2.0];
    let balls = [ball(CUE, cue), ball(1, near), ball(2, far)];
    let line = aim::shot_line(&SPEC, &SPEC.geometry(), &balls, cue, 0.0, 0.0);

    let Hit::Ball { id, ghost } = line.hit else {
        panic!("dead ahead is a ball, got {:?}", line.hit);
    };
    assert_eq!(id, 1, "the nearer ball, not the one behind it");
    assert!(
        (distance(ghost, near) - 2.0 * SPEC.ball_radius).abs() < 1e-9,
        "the ghost is where the cue ball touches"
    );
    assert_eq!(line.target(), Some(1));
    let (_, offset) = line.sighted.expect("sighted on the one");
    assert!(offset.abs() < 1e-9, "dead centre is offset zero");

    // A full ball sends the object ball straight on and stops the cue ball.
    // Its guide is short: it shows the way the one leaves, and stops well
    // before the two it will run into, so the line does not play the shot.
    let object = line.object.expect("the object ball has a leg");
    assert!(object.cut.abs() < 1e-9, "a full ball is no cut");
    let reach = OBJECT_GUIDE_REACH * SPEC.length;
    assert!(
        distance(object.to, [near[0] + reach, near[1]]) < 1e-9,
        "the guide runs its fixed reach straight on: {:?}",
        object.to
    );
    assert!(
        distance(object.to, far) > 2.0 * SPEC.ball_radius,
        "and stops short of the two"
    );

    // Something inside the reach stops the guide where the balls touch.
    let close = [near[0] + 3.0 * SPEC.ball_radius, near[1]];
    let crowded = [ball(CUE, cue), ball(1, near), ball(2, close)];
    let line = aim::shot_line(&SPEC, &SPEC.geometry(), &crowded, cue, 0.0, 0.0);
    let object = line.object.expect("the object ball has a leg");
    assert!(
        distance(object.to, [close[0] - 2.0 * SPEC.ball_radius, close[1]]) < 1e-9,
        "the guide ends on contact with the two: {:?}",
        object.to
    );
    assert_eq!(line.tangent, None, "a stun full ball leaves no stun line");
    assert_eq!(line.rebound, None);
    assert_eq!(
        line.legs().iter().map(|leg| leg.kind).collect::<Vec<_>>(),
        vec![LegKind::Cue, LegKind::Object(1)]
    );
}

#[test]
fn a_miss_ends_on_the_rail_and_comes_off_it_at_the_same_angle() {
    let cue = [0.3, 0.5];
    let balls = [ball(CUE, cue)];
    // Down and to the right at 45 degrees: the bottom rail (y = width) is
    // nearer than the end rail, so that is the one it meets, short of the
    // side pocket.
    let line = aim::shot_line(&SPEC, &SPEC.geometry(), &balls, cue, PI / 4.0, 0.0);

    let Hit::Cushion { at, normal } = line.hit else {
        panic!("nothing in the way but the rail, got {:?}", line.hit);
    };
    assert!(
        (at[1] - (SPEC.width - SPEC.ball_radius)).abs() < 1e-9,
        "the centre stops a radius short of the rail: {at:?}"
    );
    assert_eq!(normal, [0.0, -1.0]);
    let rebound = line.rebound.expect("a rail sends it back");
    assert!(rebound[1] < at[1], "back up the table");
    // Angle in equals angle out: the rebound climbs as fast as the approach fell.
    let fell = at[1] - cue[1];
    let ran = at[0] - cue[0];
    let climbed = at[1] - rebound[1];
    let ran_on = rebound[0] - at[0];
    assert!(
        (climbed / ran_on - fell / ran).abs() < 1e-9,
        "in {fell}/{ran}, out {climbed}/{ran_on}"
    );
    assert_eq!(line.sighted, None, "no ball anywhere near the line");
    assert_eq!(line.object, None);
}

#[test]
fn straight_into_a_pocket_mouth_says_so() {
    let geom = SPEC.geometry();
    // The top-right corner pocket, from a spot on its diagonal.
    let corner = &geom.pockets[2];
    let cue = [corner.center[0] - 0.4, corner.center[1] - 0.4];
    let balls = [ball(CUE, cue)];
    let line = aim::shot_line(&SPEC, &geom, &balls, cue, PI / 4.0, 0.0);
    assert!(
        matches!(line.hit, Hit::Pocket { index: 2, .. }),
        "the line runs into the pocket: {:?}",
        line.hit
    );
    assert_eq!(line.rebound, None, "a pocket sends nothing back");
}

#[test]
fn the_sighted_offset_and_the_cut_are_signed_to_the_screen() {
    // Table +y is screen down. Looking along +x from behind the cue ball,
    // screen right is +y. A ball sitting *above* the line (smaller y) has the
    // line passing to its right, and gets cut to the left.
    let cue = [0.3, 0.5];
    let above = [0.9, 0.5 - SPEC.ball_radius];
    let balls = [ball(CUE, cue), ball(3, above)];
    let line = aim::shot_line(&SPEC, &SPEC.geometry(), &balls, cue, 0.0, 0.0);

    let (id, offset) = line.sighted.expect("sighted");
    assert_eq!(id, 3);
    assert!(
        (offset - 1.0).abs() < 1e-9,
        "one radius to the right of centre: {offset}"
    );
    let object = line.object.expect("contact");
    assert!(object.cut < 0.0, "cut to the left: {}", object.cut);
    assert!(
        (object.cut.abs() - PI / 6.0).abs() < 1e-9,
        "a half-ball hit is a thirty degree cut: {}",
        object.cut
    );
    let tangent = line.tangent.expect("a cut leaves a stun line");
    assert!(
        tangent[1] > line.hit.at()[1],
        "and the cue ball goes the other way, to the right: {tangent:?}"
    );

    // Walk the line off the ball: the sight stays on it past the miss, so the
    // panel does not snap to the rail while the edge is being felt for.
    // The line already passes a radius to the right of the ball, so turning
    // right walks it further off.
    let turn = |radii: f64| ((radii - 1.0) * SPEC.ball_radius / (above[0] - cue[0])).atan();
    let line = aim::shot_line(
        &SPEC,
        &SPEC.geometry(),
        &balls,
        cue,
        turn(SIGHT_REACH - 0.2),
        0.0,
    );
    assert!(
        matches!(line.hit, Hit::Cushion { .. }),
        "the cue ball misses: {:?}",
        line.hit
    );
    assert_eq!(line.target(), Some(3), "but the three is still sighted");
    let line = aim::shot_line(
        &SPEC,
        &SPEC.geometry(),
        &balls,
        cue,
        turn(SIGHT_REACH + 0.2),
        0.0,
    );
    assert_eq!(line.target(), None, "further and it is off the ball");
}

#[test]
fn the_object_guide_points_where_the_physics_sends_the_ball() {
    // The short guide has to point the right way. A cut throws the object
    // ball off the line of centres (ball-on-ball friction drags it along
    // with the cue ball's sideways travel), so the guide is checked against
    // the simulator, not against its own geometry: strike a thirty degree
    // cut and the one leaves along the drawn leg.
    let geom = SPEC.geometry();
    let r = SPEC.ball_radius;
    let object = [SPEC.length / 2.0, SPEC.width / 2.0];
    let centres = [1.0, 0.0];
    let ghost = [object[0] - 2.0 * r, object[1]];
    let dir = rotate(centres, -PI / 6.0);
    let cue = [ghost[0] - 0.5 * dir[0], ghost[1] - 0.5 * dir[1]];
    let azimuth = dir[1].atan2(dir[0]);
    let balls = vec![ball(CUE, cue), ball(1, object)];

    let drawn = aim::shot_line(&SPEC, &geom, &balls, cue, azimuth, 0.0);
    let leg = drawn.object.expect("contact");
    assert!(
        (leg.cut.abs() - PI / 6.0).abs() < 1e-6,
        "a thirty degree cut: {}°",
        leg.cut.to_degrees()
    );
    let guide = unit([leg.to[0] - leg.from[0], leg.to[1] - leg.from[1]]);

    let rack = RackState {
        balls: vec![Ball::resting(CUE, cue), Ball::resting(1, object)],
    };
    let strike = Strike::new(azimuth, 0.0, 0.0, 2.5).expect("a playable stroke");
    let result = sim::simulate(&SPEC, &geom, &rack, &strike);
    let reach = OBJECT_GUIDE_REACH * SPEC.length;
    let travelled = (0..result.timeline.frame_count())
        .map(|frame| result.timeline.sample(frame as f64 / result.timeline.hz))
        .filter_map(|frames| frames.into_iter().find(|b| b.id == 1).map(|b| b.pos))
        .find(|pos| distance(*pos, object) >= reach)
        .expect("the one travels past the guide's reach");
    let actual = unit([travelled[0] - object[0], travelled[1] - object[1]]);
    let apart = (guide[0] * actual[1] - guide[1] * actual[0])
        .atan2(guide[0] * actual[0] + guide[1] * actual[1])
        .to_degrees();
    assert!(
        apart.abs() < 0.5,
        "the one leaves along the guide, off by {apart}°"
    );
}

#[test]
fn the_object_guide_is_the_same_share_of_every_table() {
    // The table is scaled to fit the same panel whatever the game, so a guide
    // measured in ball radii came out half as long on the snooker table as on
    // the bar box. Measured against the table it reads the same everywhere.
    let share = |spec: &TableSpec| {
        let cue = [0.3, spec.width / 2.0];
        let object = [0.9, spec.width / 2.0];
        let balls = [ball(CUE, cue), ball(1, object)];
        let line = aim::shot_line(spec, &spec.geometry(), &balls, cue, 0.0, 0.0);
        let leg = line.object.expect("the object ball has a leg");
        distance(leg.from, leg.to) / spec.length
    };
    let bar_box = share(&BAR_BOX_7FT);
    let snooker = share(&SNOOKER_12FT);
    assert!(
        (bar_box - snooker).abs() < 1e-9,
        "bar box {bar_box} against snooker {snooker}"
    );
}

#[test]
fn the_line_stops_on_a_jaw_tip_like_the_physics_does() {
    // A rail ends at its jaw tip and the physics treats the tip as a point
    // bumper: a ball whose centre passes within a radius of it is clipped. A
    // line that only tests the rail's segment reports a clean drop where
    // the table would rattle the ball, so `cast` has to see the tip too.
    let geom = SPEC.geometry();
    let r = SPEC.ball_radius;
    let tip = geom.cushions[0].a;
    // Down and left at 45 degrees, passing the tip on the pocket side at
    // nine tenths of a radius: past the end of the rail, inside the bumper.
    let dir = unit([-1.0, -1.0]);
    let perp = [dir[1], -dir[0]];
    let graze = [tip[0] + perp[0] * 0.9 * r, tip[1] + perp[1] * 0.9 * r];
    let from = [graze[0] - 0.5 * dir[0], graze[1] - 0.5 * dir[1]];

    let hit = aim::cast(&SPEC, &geom, &[ball(CUE, from)], from, dir, &[CUE]);
    let Hit::Cushion { at, normal } = hit else {
        panic!("the jaw tip is in the way, got {hit:?}");
    };
    assert!(
        (distance(at, tip) - r).abs() < 1e-9,
        "the centre stops a radius off the tip: {at:?} vs {tip:?}"
    );
    let radial = unit([at[0] - tip[0], at[1] - tip[1]]);
    assert!(
        distance(normal, radial) < 1e-9,
        "and comes off it radially, the way a point bumper sends it: {normal:?}"
    );
}
