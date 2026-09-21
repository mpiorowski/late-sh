//! The impulse model, asked the questions a player would: which way does a
//! cut throw the object ball, and what does english do to it.

use std::f64::consts::PI;

use crate::app::games::pool_core::{
    collide,
    table::{BAR_BOX_7FT, TableSpec},
};

const SPEC: TableSpec = BAR_BOX_7FT;

fn rotate(v: [f64; 2], angle: f64) -> [f64; 2] {
    let (sin, cos) = angle.sin_cos();
    [v[0] * cos - v[1] * sin, v[0] * sin + v[1] * cos]
}

#[test]
fn a_cut_throws_the_object_ball_along_the_cue_balls_travel() {
    // Line of centres along +x; the cue ball arrives thirty degrees off it,
    // travelling toward -y as well as +x. Friction drags the object ball
    // the same way: a few degrees toward -y, which is the negative side of
    // `ẑ × n = +y`. Real tables throw two to five degrees on such a cut.
    let n = [1.0, 0.0];
    let dir = rotate(n, -PI / 6.0);
    let throw = collide::throw(&SPEC, dir, n, 0.0);
    assert!(throw < 0.0, "thrown toward the cue ball's side: {throw}");
    let degrees = throw.abs().to_degrees();
    assert!(
        (2.0..=5.0).contains(&degrees),
        "a plausible few degrees, not a nudge or a swerve: {degrees}°"
    );

    // The other way round, the other way.
    let mirrored = collide::throw(&SPEC, rotate(n, PI / 6.0), n, 0.0);
    assert!(
        (mirrored + throw).abs() < 1e-12,
        "symmetric: {mirrored} vs {throw}"
    );
}

#[test]
fn a_full_ball_throws_nothing_and_gearing_english_cancels_a_cut() {
    let n = [1.0, 0.0];
    assert_eq!(
        collide::throw(&SPEC, n, n, 0.0),
        0.0,
        "straight on, no slip"
    );

    // On the cut above the surfaces slide at `dir · t = -1/2`; english that
    // spins the contact the opposite way, `R·ωz / V = +1/2`, leaves nothing
    // to drag and the ball leaves on the line of centres.
    let dir = rotate(n, -PI / 6.0);
    let geared = collide::throw(&SPEC, dir, n, 0.5);
    assert!(
        geared.abs() < 1e-12,
        "gearing english kills the throw: {geared}"
    );
    // And the opposite english throws it further than a plain cut, up to the
    // friction cap.
    let plain = collide::throw(&SPEC, dir, n, 0.0).abs();
    let against = collide::throw(&SPEC, dir, n, -0.5).abs();
    assert!(
        against >= plain,
        "outside english adds throw: {against} vs {plain}"
    );
}
