//! Cue panel tests.
//!
//! The readouts are asserted exactly, because they are the words the player
//! reads and getting "draw" and "follow" the wrong way round is both easy and
//! invisible until someone plays a shot. The drawing is checked only where it
//! encodes a direction — the tip mark's vertical sense is the one place a sign
//! error would look plausible and play wrong.

use crate::app::games::pool_core::{
    canvas::Canvas,
    cue::{MAX_SPEED, MISCUE_LIMIT, NATURAL_ROLL_TIP, PowerBand, ShotMode},
    cue_ui::{self, CueView},
};

fn canvas() -> Canvas {
    Canvas::new(34, 18, [0, 0, 0])
}

#[test]
fn the_panel_fills_its_area() {
    let mut c = canvas();
    cue_ui::draw(&mut c, &CueView::default());
    let lines = c.to_lines();
    assert_eq!(lines.len(), 18);
    for line in &lines {
        let width: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
        assert_eq!(width, 34);
    }
}

#[test]
fn follow_marks_above_centre_and_draw_below() {
    // Screen rows grow downward while the tip offset grows upward, so this is
    // exactly where a sign error hides: the panel would show draw for follow
    // and the shot would still be legal.
    //
    // The face is drawn magnified, but only as far as `TIP_FACE` of the drawn
    // ball — so natural roll (0.4 of 0.5) sits four fifths of the way up the
    // *usable* face, well past the two fifths a true-scale face would give it
    // and comfortably short of the ball's own rim.
    let mut c = canvas();
    let panel = cue_ui::draw(
        &mut c,
        &CueView {
            tip: [0.0, NATURAL_ROLL_TIP],
            ..CueView::default()
        },
    );
    let (centre, radius) = (panel.cue, panel.cue_radius);
    assert!(radius > 0.0);

    let face = panel.tip_radius;
    assert!(
        face < radius,
        "the settable face is inside the ball, not the whole of it"
    );
    let scale = NATURAL_ROLL_TIP / MISCUE_LIMIT;
    let above = c.get(centre.0 as i32, (centre.1 - face * scale) as i32);
    let below = c.get(centre.0 as i32, (centre.1 + face * scale) as i32);
    let mark = [210, 60, 60];
    assert_eq!(above, mark, "follow should mark the top of the face");
    assert_ne!(below, mark, "and not the bottom");
    assert_ne!(
        c.get(centre.0 as i32, (centre.1 - radius * 0.4) as i32),
        mark,
        "and the magnification is real: the mark is past where a true-scale \
         face would have put it"
    );
}

#[test]
fn the_target_ball_grows_as_it_gets_closer() {
    let near = drawn_target_width(0.2);
    let far = drawn_target_width(2.0);
    assert!(
        near > far,
        "a close ball should be drawn bigger: {near} vs {far}"
    );
    assert!(far >= 2, "even a long shot leaves something to aim at");
}

/// Width in pixels of the coloured target disc on its centre row.
fn drawn_target_width(distance: f64) -> usize {
    let mut c = canvas();
    cue_ui::draw(
        &mut c,
        &CueView {
            target: Some(3),
            distance,
            ..CueView::default()
        },
    );
    let row = (c.height() as f64 * 0.18) as i32;
    let colour = crate::app::games::pool_core::table_ui::ball_colour(3);
    (0..c.cols() as i32)
        .filter(|x| c.get(*x, row) == colour)
        .count()
}

#[test]
fn a_cushion_target_still_draws_something() {
    let mut c = canvas();
    cue_ui::draw(
        &mut c,
        &CueView {
            target: None,
            ..CueView::default()
        },
    );
    let row = (c.height() as f64 * 0.18) as i32;
    let backdrop = [14, 20, 18];
    assert!(
        (0..c.cols() as i32).any(|x| c.get(x, row) != backdrop),
        "picking a cushion should draw a rail, not an empty panel"
    );
}

#[test]
fn power_pulls_the_cue_back() {
    let tip_row = |power: f64| {
        let mut c = canvas();
        let panel = cue_ui::draw(
            &mut c,
            &CueView {
                power,
                mode: ShotMode::Stroke(PowerBand::Normal),
                ..CueView::default()
            },
        );
        let x = panel.cue.0 as i32;
        let start = (panel.cue.1 + panel.cue_radius) as i32;
        (start..c.height() as i32).find(|y| c.get(x, *y) == [80, 120, 170])
    };
    let resting = tip_row(0.0).expect("the cue is drawn at rest");
    let pulled = tip_row(1.0).expect("and at full power");
    assert!(
        pulled > resting,
        "full power should sit the tip further back: {resting} then {pulled}"
    );
}

#[test]
fn the_cue_ball_is_shaded_all_over_with_no_pixels_left_behind() {
    // The shading sweep used a different pixel-centre convention from the disc
    // it was shading, so pixels the sweep missed stayed at full white — more
    // of them the bigger the ball, which a fixed-width panel had been hiding.
    let mut c = Canvas::new(60, 40, [0, 0, 0]);
    let panel = cue_ui::draw(
        &mut c,
        &CueView {
            tip: [0.0, 0.0],
            ..CueView::default()
        },
    );
    let (cx, cy) = panel.cue;
    let r = panel.cue_radius;
    assert!(r > 8.0, "this test wants a big panel: {r}");

    // Every pixel of the ball's lower half is shaded away from pure white; the
    // upper-left crescent is the highlight and is meant to stay bright.
    for y in 0..c.height() as i32 {
        for x in 0..c.cols() as i32 {
            let (ddx, ddy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
            if ddx * ddx + ddy * ddy > r * r || ddx + ddy < r * 0.2 {
                continue;
            }
            assert_ne!(
                c.get(x, y),
                [242, 240, 232],
                "{x},{y} is on the shaded side and was left at full white"
            );
        }
    }
}

#[test]
fn a_struck_cue_stays_thrown_through_the_ball() {
    // The one piece of feedback that the stroke registered. Without it the cue
    // snapped back to rest the instant the gesture completed, which looks
    // exactly like a stroke that never took.
    let tip_row = |follow_through: bool| {
        let mut c = canvas();
        let panel = cue_ui::draw(
            &mut c,
            &CueView {
                power: 0.5,
                mode: ShotMode::Stroke(PowerBand::Normal),
                follow_through,
                ..CueView::default()
            },
        );
        let x = panel.cue.0 as i32;
        (0..c.height() as i32)
            .find(|y| c.get(x, *y) == [80, 120, 170])
            .expect("the cue is drawn")
    };
    assert!(
        tip_row(true) < tip_row(false),
        "a struck cue sits forward of a drawn-back one"
    );
}

// ── Readouts ──────────────────────────────────────────────────────────

#[test]
fn spin_reads_in_the_players_language() {
    assert_eq!(cue_ui::spin_label([0.0, 0.0]), "spin centre · centre");
    assert_eq!(cue_ui::spin_label([0.3, 0.0]), "spin 0.30 left · centre");
    assert_eq!(cue_ui::spin_label([-0.3, 0.0]), "spin 0.30 right · centre");
    assert_eq!(cue_ui::spin_label([0.0, 0.4]), "spin centre · 0.40 follow");
    assert_eq!(cue_ui::spin_label([0.0, -0.4]), "spin centre · 0.40 draw");
}

#[test]
fn aim_reads_as_a_bearing() {
    assert_eq!(cue_ui::aim_label(0.0), "aim   0.0°");
    assert_eq!(cue_ui::aim_label(std::f64::consts::PI), "aim 180.0°");
    assert_eq!(
        cue_ui::aim_label(-std::f64::consts::FRAC_PI_2),
        "aim 270.0°",
        "a negative bearing should wrap, not print a minus sign"
    );
}

#[test]
fn the_stroke_reads_as_a_band_a_bar_and_a_speed() {
    // The bar is drawn against the armed *band*, not the whole speed range, so
    // a full pull looks the same in every band and reads differently. That is
    // the point of having bands: the pointer's whole travel is spent inside
    // the range the player asked for.
    for band in PowerBand::ALL {
        let full = band.ceiling();
        let label = cue_ui::power_label(full, Some(band), MAX_SPEED);
        assert!(
            label.starts_with(&format!("{} ▓▓▓▓▓▓▓▓▓▓", band.label())),
            "a full pull in {band:?} should fill the bar: {label}"
        );
        assert!(
            label.ends_with(&format!("{:.1} m/s", full * MAX_SPEED)),
            "and report its own top speed: {label}"
        );
    }

    let light = cue_ui::power_label(
        PowerBand::Light.ceiling(),
        Some(PowerBand::Light),
        MAX_SPEED,
    );
    let strong = cue_ui::power_label(
        PowerBand::Strong.ceiling(),
        Some(PowerBand::Strong),
        MAX_SPEED,
    );
    assert_ne!(light, strong, "the same bar must not read the same speed");

    let half = cue_ui::power_label(
        PowerBand::Normal.ceiling() / 2.0,
        Some(PowerBand::Normal),
        MAX_SPEED,
    );
    assert!(half.starts_with("normal ▓▓▓▓▓░░░░░"), "got {half}");
    assert!(
        cue_ui::power_label(0.0, None, MAX_SPEED).starts_with("stroke ░░░░░░░░░░"),
        "an unarmed cue still has something to say"
    );
}

#[test]
fn the_bands_climb_and_reach_the_top() {
    let ceilings: Vec<f64> = PowerBand::ALL.iter().map(|b| b.ceiling()).collect();
    assert!(
        ceilings.windows(2).all(|w| w[0] < w[1]),
        "light < normal < strong: {ceilings:?}"
    );
    assert_eq!(
        PowerBand::Strong.ceiling(),
        1.0,
        "the top band must reach a real break"
    );
    // Overlapping ranges: every speed under the cap is reachable from more
    // than one band, so there is no gap to fall into between them.
    assert!(PowerBand::Light.ceiling() < PowerBand::Normal.ceiling());
}

#[test]
fn a_band_is_pinned_to_how_hard_people_actually_hit() {
    // The first cut spread the bands evenly over the range, which put a
    // *normal* full pull at two thirds of a break and made every shot a slam.
    // These are the speeds the names promise, in m/s.
    let top = |band: PowerBand| band.ceiling() * MAX_SPEED;
    assert!(
        top(PowerBand::Light) < 2.5,
        "a light stroke is a roll, not a shot: {}",
        top(PowerBand::Light)
    );
    assert!(
        (3.0..5.5).contains(&top(PowerBand::Normal)),
        "a normal full pull is a firm pot down the table: {}",
        top(PowerBand::Normal)
    );
    assert!(
        top(PowerBand::Strong) > 8.0,
        "and only the top band breaks a rack: {}",
        top(PowerBand::Strong)
    );
}

#[test]
fn each_band_has_its_own_key_and_answers_to_it() {
    let mut keys: Vec<char> = PowerBand::ALL.iter().map(|b| b.key()).collect();
    keys.sort_unstable();
    keys.dedup();
    assert_eq!(keys.len(), 3, "three bands, three distinct keys");
    for band in PowerBand::ALL {
        assert_eq!(PowerBand::from_key(band.key()), Some(band));
    }
    assert_eq!(PowerBand::from_key('z'), None);
}

// ── The stage machine ─────────────────────────────────────────────────

#[test]
fn only_a_stroke_mode_carries_a_band() {
    assert_eq!(ShotMode::Idle.band(), None);
    assert_eq!(ShotMode::Aim.band(), None);
    assert_eq!(ShotMode::Spin.band(), None);
    for band in PowerBand::ALL {
        assert_eq!(ShotMode::Stroke(band).band(), Some(band));
    }
}

#[test]
fn every_mode_tells_the_player_what_to_do() {
    // The hint beside the mode's name is the only guidance about the mouse
    // there is. A blank one leaves a player looking at a table with no idea
    // what a click does.
    for mode in ShotMode::ALL {
        let hint = mode.hint();
        assert!(!hint.is_empty(), "{mode:?} needs a hint");
        assert!(!mode.label().is_empty(), "{mode:?} needs a label");
        assert!(
            ["click", "move", "pull", "pointer"]
                .iter()
                .any(|word| hint.contains(word)),
            "{mode:?} hint should say what the mouse does: {hint}"
        );
    }
}

#[test]
fn a_tall_panel_spends_its_rows_on_the_cue_and_not_on_the_balls() {
    // The complaint: plenty of empty panel under a cue that barely moved. The
    // stroke is read off how far the cue travels, so every row past what the
    // balls and the sighting line need belongs below the cue ball — and the
    // balls must not shrink to pay for it.
    let panel_of = |rows: u16| {
        let mut c = Canvas::new(40, rows, [0, 0, 0]);
        let panel = cue_ui::draw(&mut c, &CueView::default());
        (panel, c.height() as f64)
    };
    let (short, short_h) = panel_of(18);
    let (tall, tall_h) = panel_of(44);

    assert_eq!(
        short.cue_radius, tall.cue_radius,
        "the balls are the panel's width, not its height"
    );
    let room = |panel: cue_ui::PanelHit, h: f64| h - (panel.cue.1 + panel.cue_radius);
    assert!(
        room(tall, tall_h) > room(short, short_h) * 2.5,
        "doubling the panel's height should more than double the cue's room: \
         {} then {}",
        room(short, short_h),
        room(tall, tall_h)
    );
}

#[test]
fn a_full_pull_keeps_the_butt_of_the_cue_on_screen() {
    // The draw-back is scaled to the room under the ball; take all of it and
    // the cue slides off the bottom, which is no cue at exactly the moment it
    // is being aimed.
    let mut c = Canvas::new(40, 30, [0, 0, 0]);
    cue_ui::draw(
        &mut c,
        &CueView {
            power: 1.0,
            mode: ShotMode::Stroke(PowerBand::Strong),
            ..CueView::default()
        },
    );
    let wood = [186, 146, 92];
    let bottom = c.height() as i32 - 1;
    assert!(
        (0..c.cols() as i32).any(|x| c.get(x, bottom) == wood),
        "the cue still reaches the bottom of the panel at a full pull"
    );
}

#[test]
fn the_panel_says_invalid_target_and_only_that() {
    // The panel does not ring its ball. It is drawn alone and enormous, with
    // nothing beside it for a coloured edge to be read against — which is the
    // whole of what makes the same mark work on the table — so it says the one
    // thing worth saying, in words, and only when the answer is no.
    let warned = |view: CueView| {
        let mut c = Canvas::new(40, 30, [0, 0, 0]);
        cue_ui::draw(&mut c, &view);
        // Spans are per cell, so the row has to be reassembled before it can
        // be read as a sentence.
        c.to_lines().iter().any(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
                .contains("Invalid Target")
        })
    };
    assert!(
        warned(CueView {
            target: Some(3),
            target_fault: true,
            ..CueView::default()
        }),
        "a ball the striker may not hit says so"
    );
    assert!(
        !warned(CueView {
            target: Some(3),
            target_fault: false,
            ..CueView::default()
        }),
        "a legal target is not worth a word"
    );
    assert!(
        !warned(CueView {
            target: None,
            ..CueView::default()
        }),
        "and neither is a cushion"
    );
}

#[test]
fn a_stripe_in_the_panel_stays_round() {
    // Its white caps used to be rectangles with the overshoot erased after,
    // and the erase tested one pixel and wrote to another whenever the ball's
    // centre was not on a whole one — so a column went missing down one side
    // and the squared corners stayed on the other. Nothing may be painted
    // outside the disc, whatever the centre lands on.
    use crate::app::games::pool_core::table_ui::{WHITE, ball_colour};

    let bg = [0, 0, 0];
    for cols in [33u16, 34, 40, 59] {
        let mut c = Canvas::new(cols, 30, bg);
        let panel = cue_ui::draw(
            &mut c,
            &CueView {
                target: Some(15),
                distance: 0.2,
                ..CueView::default()
            },
        );
        let (x, y) = panel.target;
        let r = panel.target_radius;
        for py in 0..c.height() as i32 {
            for px in 0..c.cols() as i32 {
                let d = (px as f64 + 0.5 - x).hypot(py as f64 + 0.5 - y);
                if d <= r {
                    continue;
                }
                let seen = c.get(px, py);
                assert_ne!(
                    seen, WHITE,
                    "{cols} cols: white outside the ball at ({px}, {py}), {d:.2} out"
                );
                assert_ne!(
                    seen,
                    ball_colour(15),
                    "{cols} cols: ball colour outside the ball at ({px}, {py}), {d:.2} out"
                );
            }
        }
    }
}

#[test]
fn the_cue_does_not_begin_where_the_ball_ends() {
    // Two zones that used to touch: the bottom of the cue ball's face and the
    // top of the cue. A click aimed at maximum screw and landing a pixel low
    // armed the stroke instead of placing the tip, which on a panel drawn in
    // half blocks is a whole terminal row of slop. There is now clear air
    // between them, and it means "done" like any other dead part of the panel.
    for rows in [14u16, 22, 36] {
        let mut c = Canvas::new(40, rows, [0, 0, 0]);
        let panel = cue_ui::draw(&mut c, &CueView::default());
        assert!(
            panel.cue_top > panel.cue.1 + panel.cue_radius,
            "{rows} rows: the stroke zone starts below the ball, not at it \
             ({} vs {})",
            panel.cue_top,
            panel.cue.1 + panel.cue_radius
        );
        assert!(
            panel.tip_radius < panel.cue_radius,
            "{rows} rows: the settable face is smaller than the drawn ball"
        );
        // And the cue itself is drawn no higher than the zone that arms it,
        // or the picture and the pointer disagree about what is cue.
        let wood = [186, 146, 92];
        let tip = [80, 120, 170];
        let highest = (0..c.height() as i32)
            .find(|y| {
                (0..c.cols() as i32).any(|x| {
                    let px = c.get(x, *y);
                    px == wood || px == tip
                })
            })
            .expect("the cue is drawn");
        assert!(
            highest as f64 >= panel.cue.1 + panel.cue_radius,
            "{rows} rows: the cue is drawn into the ball ({highest})"
        );
    }
}

#[test]
fn the_cue_ball_wears_no_ring_until_the_tip_is_being_placed() {
    // A grey circle inside a white ball reads as part of the ball rather than
    // as a boundary, and it was the busiest thing on a panel whose job is to
    // show one ball clearly. The limit still holds whether or not it is drawn;
    // the ring is a cue for the moment the tip is actually being moved.
    use crate::app::games::pool_core::table_ui::GUIDE;

    let ringed = |mode: ShotMode| {
        let mut c = Canvas::new(40, 30, [0, 0, 0]);
        let panel = cue_ui::draw(
            &mut c,
            &CueView {
                mode,
                ..CueView::default()
            },
        );
        let (x, y) = panel.cue;
        let r = panel.tip_radius;
        ((y - r) as i32..=(y - r + 2.0) as i32)
            .any(|py| ((x - 1.0) as i32..=(x + 1.0) as i32).any(|px| c.get(px, py) == GUIDE))
    };
    assert!(ringed(ShotMode::Spin), "armed, the limit is drawn");
    assert!(!ringed(ShotMode::Idle), "idle, the ball is just a ball");
    assert!(!ringed(ShotMode::Aim));
}
