//! The shot line is the straight-line story a player reads off the table
//! before a shot, so the tests here are about what a player would see: where
//! the line ends, which ball it is on, which way the object ball goes.

use std::f64::consts::PI;

use crate::app::games::pool_core::{
    aim::{self, Hit, LegKind, MAX_POT_CUT, SIGHT_REACH},
    ball::{Ball, CUE},
    cue::Strike,
    shot::{BallFrame, RackState},
    sim,
    table::{BAR_BOX_7FT, TableSpec},
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
    let object = line.object.expect("the object ball has a leg");
    assert!(object.cut.abs() < 1e-9, "a full ball is no cut");
    assert!(
        matches!(object.hit, Hit::Ball { id: 2, .. }),
        "and it runs into the two: {:?}",
        object.hit
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
fn pot_lines_are_the_clear_pockets_easiest_first() {
    let geom = SPEC.geometry();
    let r = SPEC.ball_radius;
    // The object ball a hand's width off the top-right corner, on its
    // diagonal; the cue ball back down the table on the same diagonal, so
    // that pocket is a full ball. The other pockets are behind or across.
    let corner = geom.pockets[2].center;
    let object = [corner[0] - 0.25, corner[1] - 0.25];
    let cue = [corner[0] - 0.9, corner[1] - 0.9];
    let balls = vec![ball(CUE, cue), ball(9, object)];

    let lines = aim::pot_lines(&SPEC, &geom, &balls, cue, 9, 0.0);
    assert!(!lines.is_empty(), "the corner is on");
    assert_eq!(lines[0].pocket, 2, "the full-ball pocket comes first");
    assert!(
        lines[0].cut < 1e-6,
        "and it is a full ball: {}",
        lines[0].cut
    );
    assert!(
        lines.windows(2).all(|pair| pair[0].cut <= pair[1].cut),
        "easiest first"
    );
    assert!(
        lines.iter().all(|line| line.cut <= MAX_POT_CUT),
        "nothing steeper than a player would try"
    );
    // The bearing it hands back really is the pot: walk it and the object
    // ball's own leg ends in that pocket.
    let line = aim::shot_line(&SPEC, &geom, &balls, cue, lines[0].azimuth, 0.0);
    let leg = line.object.expect("contact");
    assert!(
        matches!(leg.hit, Hit::Pocket { index: 2, .. }),
        "the nine drops: {:?}",
        leg.hit
    );

    // A ball parked on the line to the pocket takes that pot off the list.
    let blocker = [object[0] + 0.12, object[1] + 0.12];
    let mut blocked = balls.clone();
    blocked.push(ball(4, blocker));
    let lines = aim::pot_lines(&SPEC, &geom, &blocked, cue, 9, 0.0);
    assert!(
        lines.iter().all(|line| line.pocket != 2),
        "the four is in the way: {lines:?}"
    );

    // And a ball between the cue ball and the ghost takes every pot off.
    let in_front = [cue[0] + 2.0 * r + 0.05, cue[1] + 2.0 * r + 0.05];
    let mut walled = balls.clone();
    walled.push(ball(5, in_front));
    let lines = aim::pot_lines(&SPEC, &geom, &walled, cue, 9, 0.0);
    assert!(
        lines.iter().all(|line| line.pocket != 2),
        "the five is in the way of the cue ball: {lines:?}"
    );
}

#[test]
fn a_pot_line_on_a_cut_really_pots_under_the_physics() {
    // The aid and the referee have to agree. A cut shot throws the object
    // ball off the line of centres (ball-on-ball friction drags it along
    // with the cue ball's sideways travel), and over a table's length that
    // is more than a pocket forgives. So the bearing `pot_lines` hands back
    // is checked against the simulator, not against its own geometry: strike
    // along it and the ball must drop in the pocket it named.
    let geom = SPEC.geometry();
    let r = SPEC.ball_radius;
    let pocket = geom.pockets[0].center;
    let object = [1.0, 0.6];
    let to_pocket = unit([pocket[0] - object[0], pocket[1] - object[1]]);
    // A thirty degree cut from half a metre back, with over a metre of
    // object-ball travel to the pocket to let the throw show.
    let ghost = [
        object[0] - 2.0 * r * to_pocket[0],
        object[1] - 2.0 * r * to_pocket[1],
    ];
    let dir = rotate(to_pocket, -PI / 6.0);
    let cue = [ghost[0] - 0.5 * dir[0], ghost[1] - 0.5 * dir[1]];
    let balls = vec![ball(CUE, cue), ball(1, object)];

    let lines = aim::pot_lines(&SPEC, &geom, &balls, cue, 1, 0.0);
    let line = lines
        .iter()
        .find(|line| line.pocket == 0)
        .unwrap_or_else(|| panic!("the corner is on: {lines:?}"));
    assert!(
        (line.cut - PI / 6.0).abs() < 5.0f64.to_radians(),
        "a thirty degree cut, give or take the throw: {}°",
        line.cut.to_degrees()
    );

    let rack = RackState {
        balls: vec![Ball::resting(CUE, cue), Ball::resting(1, object)],
    };
    let strike = Strike::new(line.azimuth, 0.0, 0.0, 2.5).expect("a playable stroke");
    let result = sim::simulate(&SPEC, &geom, &rack, &strike);
    assert_eq!(
        result.outcome.pocket_of(1),
        Some(0),
        "struck along the pot line the one drops in the corner; it ended at {:?}",
        result.rack.get(1).map(|b| b.pos)
    );

    // And the drawn object leg tells the same story as the bearing.
    let drawn = aim::shot_line(&SPEC, &geom, &balls, cue, line.azimuth, 0.0);
    let leg = drawn.object.expect("contact");
    assert!(
        matches!(leg.hit, Hit::Pocket { index: 0, .. }),
        "the object leg ends in the same pocket: {:?}",
        leg.hit
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
