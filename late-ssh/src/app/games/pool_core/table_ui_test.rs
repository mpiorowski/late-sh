//! Table renderer tests.
//!
//! These check the mapping and the invariants a renderer can actually get
//! wrong — geometry round trips, every cell accounted for, potted balls gone,
//! the minimum ball size honoured. What a table *looks* like is not something
//! an assertion can hold, so nothing here tries.

use ratatui::style::Color;

use crate::app::games::pool_core::{
    ball::CUE,
    canvas::{Canvas, rgb},
    rack,
    shot::BallFrame,
    table::{BAR_BOX_7FT, TableSpec},
    table_ui::{self, CLOTH, Overlay, View},
};

const SPEC: TableSpec = BAR_BOX_7FT;
/// The minimum the board screen demands; see the layout note in the plan.
const COLS: u16 = 76;
const ROWS: u16 = 19;

fn canvas() -> Canvas {
    Canvas::new(COLS, ROWS, CLOTH)
}

fn frames() -> Vec<BallFrame> {
    rack::build(&SPEC, rack::RackKind::EightBall, 1)
        .balls
        .iter()
        .map(|b| BallFrame {
            id: b.id,
            pos: b.pos,
            potted: b.potted.is_some(),
        })
        .collect()
}

#[test]
fn the_canvas_fills_every_cell() {
    let mut c = canvas();
    let view = View::fit(&SPEC, &c);
    table_ui::draw(
        &mut c,
        &SPEC,
        &SPEC.geometry(),
        &view,
        &frames(),
        &Overlay::default(),
    );
    let lines = c.to_lines();
    assert_eq!(lines.len(), ROWS as usize, "one line per terminal row");
    for line in &lines {
        let width: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
        assert_eq!(width, COLS as usize, "every line must fill the width");
    }
}

#[test]
fn what_the_table_does_not_fill_is_a_room_and_not_more_cloth() {
    // The table keeps its aspect ratio, so one dimension always has slack. Left
    // as cloth it read as a green table of the wrong shape — a player at a
    // wide terminal saw a rail floating in a green field.
    let mut c = Canvas::new(COLS * 2, ROWS, CLOTH);
    let view = View::fit(&SPEC, &c);
    table_ui::draw(
        &mut c,
        &SPEC,
        &SPEC.geometry(),
        &view,
        &frames(),
        &Overlay::default(),
    );
    for (x, y) in [(0, 0), (COLS as i32 * 2 - 1, 0), (0, ROWS as i32 * 2 - 1)] {
        assert_eq!(
            c.get(x, y),
            table_ui::SURROUND,
            "the corner at {x},{y} is outside the table"
        );
    }
    assert_eq!(
        c.get(COLS as i32, ROWS as i32),
        CLOTH,
        "and the middle is still the cloth"
    );
}

#[test]
fn a_balls_rim_is_the_edge_of_its_own_disc() {
    // The rim used to be a circle walked by angle and rounded to a pixel
    // *index*, half a pixel off the convention `disc` uses. The two then
    // disagreed about which pixels were the edge, and on a ball three pixels
    // across that showed as cloth-coloured dots inside the rim and ball colour
    // outside it. Both are impossible if the rim is the disc's outer shell.
    let mut c = Canvas::new(40, 40, CLOTH);
    let colour = [10, 200, 10];
    let rim = [200, 10, 10];
    for (cx, cy) in [(20.0, 20.0), (12.5, 9.5), (7.25, 30.75)] {
        for r in [2.0, 2.5, 3.0, 4.5, 7.0] {
            c.disc(cx, cy, r, colour);
            table_ui::ring(&mut c, cx, cy, r, rim);
            for y in 0..40 {
                for x in 0..40 {
                    let dx = x as f64 + 0.5 - cx;
                    let dy = y as f64 + 0.5 - cy;
                    let inside = dx * dx + dy * dy <= r * r;
                    let painted = c.get(x, y) == colour || c.get(x, y) == rim;
                    assert_eq!(
                        inside, painted,
                        "at {x},{y} for a ball of {r} at {cx},{cy}: the rim and \
                         the disc must cover exactly the same pixels"
                    );
                }
            }
            c.fill_rect(0, 0, 40, 40, CLOTH);
        }
    }
}

#[test]
fn a_pocket_is_drawn_the_size_it_plays() {
    // Deriving the hole from the exaggerated ball meant retuning a mouth
    // changed how a pocket played and not how it looked — the worst possible
    // way for a player to find out.
    let wide = TableSpec {
        corner_mouth: SPEC.corner_mouth * 1.5,
        side_mouth: SPEC.side_mouth * 1.5,
        ..SPEC
    };
    // Big enough that the real mouth beats the "at least as wide as a drawn
    // ball" floor, which is what governs a cramped terminal.
    let count = |spec: &TableSpec| {
        let mut c = Canvas::new(220, 60, CLOTH);
        let view = View::fit(spec, &c);
        table_ui::draw(
            &mut c,
            spec,
            &spec.geometry(),
            &view,
            &[],
            &Overlay::default(),
        );
        (0..c.cols() as i32)
            .flat_map(|x| (0..c.height() as i32).map(move |y| (x, y)))
            .filter(|(x, y)| c.get(*x, *y) == [10, 10, 12])
            .count()
    };
    assert!(
        count(&wide) > count(&SPEC),
        "a wider mouth must draw a wider pocket"
    );
}

#[test]
fn a_pocket_never_spills_off_the_end_of_the_table() {
    // A corner pocket is drawn on the chord across the corner, so an unclipped
    // disc runs diagonally past the end of the table and the corners read as
    // bites out of the room rather than out of the cloth.
    let mut c = Canvas::new(220, 60, CLOTH);
    let view = View::fit(&SPEC, &c);
    table_ui::draw(
        &mut c,
        &SPEC,
        &SPEC.geometry(),
        &view,
        &frames(),
        &Overlay::default(),
    );
    let (x0, y0) = view.to_px([0.0, 0.0]);
    let (x1, y1) = view.to_px([SPEC.length, SPEC.width]);
    // The rails are the outermost thing the table draws; nothing of the table
    // may appear beyond them.
    // The rail is drawn in whole pixels, so allow the rounding it does.
    let slack = view.rail_px().ceil();
    let mut in_the_wood = 0;
    for y in 0..c.height() as i32 {
        for x in 0..c.cols() as i32 {
            if c.get(x, y) != [10, 10, 12] {
                continue;
            }
            assert!(
                (x as f64) >= x0 - slack
                    && (x as f64) <= x1 + slack
                    && (y as f64) >= y0 - slack
                    && (y as f64) <= y1 + slack,
                "a pocket pixel at {x},{y} is outside the table \
                 ({x0:.1},{y0:.1})-({x1:.1},{y1:.1})"
            );
            if (x as f64) < x0 || (x as f64) > x1 || (y as f64) < y0 || (y as f64) > y1 {
                in_the_wood += 1;
            }
        }
    }
    // And they sit *on* the edge rather than inside it. A pocket drawn wholly
    // on the cloth is a black blob a ball's width in from the corner, which is
    // what centring a corner on its mouth chord produced.
    assert!(
        in_the_wood > 0,
        "the pockets should straddle the rails, not float on the cloth"
    );
}

#[test]
fn a_big_ball_wears_a_painted_number_inside_its_own_edge() {
    // A borrowed terminal glyph is drawn at the cell's size whatever the ball
    // is, so on a large table the number stops growing with the disc it is on
    // and turns into a speck. Painted digits scale with the ball — but they
    // have to stay *on* it: ink outside the disc reads as a mark on the cloth.
    let mut c = Canvas::new(400, 95, CLOTH);
    let view = View::fit(&SPEC, &c);
    assert!(
        view.ball_px() > 6.0,
        "this test needs a table big enough to paint on: {}",
        view.ball_px()
    );
    let balls = frames();
    table_ui::draw(
        &mut c,
        &SPEC,
        &SPEC.geometry(),
        &view,
        &balls,
        &Overlay::default(),
    );

    let ink = [16, 16, 18];
    let mut painted = 0;
    for y in 0..c.height() as i32 {
        for x in 0..c.cols() as i32 {
            if c.get(x, y) != ink {
                continue;
            }
            painted += 1;
            let on_a_ball = balls.iter().any(|ball| {
                let (bx, by) = view.to_px(ball.pos);
                (x as f64 + 0.5 - bx).hypot(y as f64 + 0.5 - by) <= view.ball_px()
            });
            assert!(on_a_ball, "ink at {x},{y} is not on any ball");
        }
    }
    // Fifteen numbered balls, each carrying at least a few pixels of digit.
    assert!(painted > 15 * 5, "hardly any number got painted: {painted}");

    // The font must not be mirrored. A `1` is the digit that catches it: its
    // flag hangs off the *left* of the stem, so the leftmost column of its ink
    // carries two rows and the rightmost only one. Read the glyph literals the
    // wrong way round and that swaps — which is exactly what happened when
    // they were written visually but indexed low-bit-first.
    let one = balls.iter().find(|b| b.id == 1).expect("the one ball");
    let (bx, by) = view.to_px(one.pos);
    let mut columns: Vec<(i32, i32)> = Vec::new();
    for y in 0..c.height() as i32 {
        for x in 0..c.cols() as i32 {
            if c.get(x, y) != ink {
                continue;
            }
            if (x as f64 + 0.5 - bx).hypot(y as f64 + 0.5 - by) > view.ball_px() {
                continue;
            }
            match columns.iter_mut().find(|(cx, _)| *cx == x) {
                Some((_, count)) => *count += 1,
                None => columns.push((x, 1)),
            }
        }
    }
    columns.sort_unstable();
    let (_, leftmost) = *columns.first().expect("the one ball wears a number");
    let (_, rightmost) = *columns.last().expect("the one ball wears a number");
    assert!(
        leftmost > rightmost,
        "a `1` is flagged on the left: {leftmost} pixels in its leftmost column \
         against {rightmost} in its rightmost"
    );
}

#[test]
fn a_solid_is_plain_and_a_stripe_wears_its_band() {
    // The rim used to be how a stripe was told from its solid, which put an
    // outline on all fifteen for the sake of seven. A stripe is now painted
    // the way a real one looks from above — colour across the middle, white at
    // both ends — and a solid is just a disc.
    let mut c = Canvas::new(220, 60, CLOTH);
    let view = View::fit(&SPEC, &c);
    // Two balls, far apart, so neither one's halo touches the other.
    let balls = vec![
        BallFrame {
            id: 3,
            pos: [SPEC.length * 0.25, SPEC.width * 0.5],
            potted: false,
        },
        BallFrame {
            id: 11,
            pos: [SPEC.length * 0.75, SPEC.width * 0.5],
            potted: false,
        },
    ];
    table_ui::draw(
        &mut c,
        &SPEC,
        &SPEC.geometry(),
        &view,
        &balls,
        &Overlay::default(),
    );

    let across = |ball: &BallFrame| {
        let (bx, by) = view.to_px(ball.pos);
        let row = by.floor() as i32;
        let reach = view.ball_px().floor() as i32;
        ((bx.floor() as i32 - reach)..=(bx.floor() as i32 + reach))
            .map(|x| c.get(x, row))
            .collect::<Vec<_>>()
    };

    let solid = across(&balls[0]);
    let white = [245, 243, 236];
    assert!(
        !solid.contains(&white),
        "a solid carries no white at all: {solid:?}"
    );
    assert!(
        solid.contains(&table_ui::ball_colour(3)),
        // The very centre is the painted number, so look across the whole row.
        "its face is its own colour: {solid:?}"
    );
    // Its edge is shaded rather than outlined: darker, but still its own hue,
    // and drawn *inside* the disc so it can never touch the neighbour it is
    // separating from. A ring of cloth outside the ball did the separating for
    // one round and carved bites out of every overlapping neighbour.
    let edge = *solid.first().expect("a ball wide enough to have an edge");
    assert_ne!(edge, CLOTH, "the edge belongs to the ball");
    let (hue, shade) = (table_ui::ball_colour(3), edge);
    assert!(
        shade != hue && shade[0] > shade[1] && shade[0] > shade[2],
        "a shaded red is still red: {shade:?}"
    );

    let stripe = across(&balls[1]);
    assert_eq!(stripe.first(), Some(&white), "white at the left end");
    assert_eq!(stripe.last(), Some(&white), "and at the right end");
    assert!(
        stripe.contains(&table_ui::ball_colour(11)),
        "with the hue running between them: {stripe:?}"
    );
}

#[test]
fn overlapping_balls_keep_their_own_shape() {
    // Balls are drawn larger than life on true positions, so neighbours in a
    // rack overlap. Whatever separates them has to live *inside* each ball:
    // the version that ringed them with cloth had every ball carve a bite out
    // of the one drawn before it, and a cluster came out as rectangles.
    let mut c = Canvas::new(220, 60, CLOTH);
    let view = View::fit(&SPEC, &c);
    let r = view.ball_px();
    // Two balls a hair over one true diameter apart: touching on the table,
    // overlapping once drawn.
    let left = [SPEC.length * 0.5, SPEC.width * 0.5];
    let right = [left[0] + SPEC.ball_radius * 2.0, left[1]];
    let balls = vec![
        BallFrame {
            id: 3,
            pos: left,
            potted: false,
        },
        BallFrame {
            id: 6,
            pos: right,
            potted: false,
        },
    ];
    table_ui::draw(
        &mut c,
        &SPEC,
        &SPEC.geometry(),
        &view,
        &balls,
        &Overlay::default(),
    );

    // The first ball drawn keeps every pixel it owns that the second does not.
    let (lx, ly) = view.to_px(left);
    let (rx, ry) = view.to_px(right);
    for dy in -(r as i32)..=(r as i32) {
        for dx in -(r as i32)..=(r as i32) {
            let (x, y) = (lx.floor() as i32 + dx, ly.floor() as i32 + dy);
            let mine = (x as f64 + 0.5 - lx).hypot(y as f64 + 0.5 - ly) <= r - 1.0;
            let theirs = (x as f64 + 0.5 - rx).hypot(y as f64 + 0.5 - ry) <= r;
            if !mine || theirs {
                continue;
            }
            assert_ne!(
                c.get(x, y),
                CLOTH,
                "the ball at {left:?} lost {x},{y} to its neighbour"
            );
        }
    }
}

#[test]
fn a_rack_never_mixes_painted_numbers_with_borrowed_ones() {
    // Every ball on a table is drawn the same size, so whether the rack paints
    // its numbers is one decision for the rack. It used to be taken per ball,
    // against whether that ball's digits fit inside its own disc — so the
    // single-digit balls came out painted and the two-digit ones came out in
    // the terminal's font: two alphabets on one rack. Two digits spilling a
    // little past the edge is the smaller ugliness by far.
    let mut c = Canvas::new(220, 60, CLOTH);
    let view = View::fit(&SPEC, &c);
    let balls = frames();
    assert!(
        (3.5..5.7).contains(&view.ball_px()),
        "this test wants a ball too small for two digits to fit inside: {}",
        view.ball_px()
    );
    table_ui::draw(
        &mut c,
        &SPEC,
        &SPEC.geometry(),
        &view,
        &balls,
        &Overlay::default(),
    );

    let ink = [16, 16, 18];
    let numbered = balls.iter().filter(|b| b.id != CUE);
    for ball in numbered {
        let (bx, by) = view.to_px(ball.pos);
        let reach = view.ball_px() * 2.0;
        let painted = (0..c.height() as i32).any(|y| {
            (0..c.cols() as i32).any(|x| {
                c.get(x, y) == ink && (x as f64 + 0.5 - bx).hypot(y as f64 + 0.5 - by) <= reach
            })
        });
        assert!(painted, "ball {} wears no painted number", ball.id);
    }
}

#[test]
fn the_eye_view_is_not_a_mirror_of_the_overview() {
    // A ball the overview draws *above* the centre line is one the eye view
    // must draw to the *left*, and the sign error that swaps them is invisible
    // in the code: `forward × up` is the honest right-hand answer, and it is
    // the wrong one here, because the overview draws +y downward the way
    // screen rows run. Two views that disagree about which side of the table a
    // ball is on are worse than one view.
    use crate::app::games::pool_core::table_3d::Eye;

    let spec = SPEC;
    let cue = [spec.length * 0.25, spec.width * 0.5];
    let near_side = [spec.length * 0.62, spec.width * 0.18];
    let far_side = [spec.length * 0.62, spec.width * 0.82];

    let canvas = Canvas::new(76, 26, CLOTH);
    let view = View::fit(&spec, &canvas);
    let (_, over_near) = view.to_px(near_side);
    let (_, over_far) = view.to_px(far_side);

    let eye = Eye::behind(cue, 0.0, &spec, &canvas);
    let (eye_near, _, _) = eye.to_screen(near_side, spec.ball_radius).expect("in view");
    let (eye_far, _, _) = eye.to_screen(far_side, spec.ball_radius).expect("in view");

    assert!(over_near < over_far, "the overview puts this one higher");
    assert!(
        eye_near < eye_far,
        "so the eye view has to put it further left: {eye_near} against {eye_far}"
    );
}

#[test]
fn the_cue_ball_has_table_behind_it_in_the_eye_view() {
    // A stroke needs somewhere to come from. Tied to the canvas width alone
    // the focal length grew without bound on a wide short terminal, and the
    // near foreground — the cue ball with it — fell off the bottom of the
    // frame entirely.
    use crate::app::games::pool_core::table_3d::Eye;

    let spec = SPEC;
    let cue = [spec.length * 0.3, spec.width * 0.5];
    for (cols, rows) in [(76u16, 26u16), (200, 30), (60, 40), (300, 28)] {
        let canvas = Canvas::new(cols, rows, CLOTH);
        let eye = Eye::behind(cue, 0.0, &spec, &canvas);
        let (_, y, _) = eye.to_screen(cue, spec.ball_radius).expect("just ahead");
        let height = canvas.height() as f64;
        assert!(
            y < height * 0.85,
            "at {cols}x{rows} the cue ball sits at {y} of {height}, with no room behind it"
        );
        assert!(
            y > height * 0.4,
            "and at {cols}x{rows} it floats at {y} of {height}, halfway up the screen"
        );
    }
}

#[test]
fn a_ball_against_the_far_cushion_does_not_float_into_the_room() {
    // The vertical is scaled harder than the horizontal, so a ball's own
    // height off the cloth — under three centimetres — came out as several
    // pixels of lift, and balls near the far rail rose clean over it and hung
    // in the room. Drawing each ball as a sprite standing on the spot it
    // touches makes that impossible; this is the test that says so.
    use crate::app::games::pool_core::table_3d::{self, Eye, Sight};

    let spec = SPEC;
    let cue = [spec.length * 0.2, spec.width * 0.5];
    let against_the_rail = [spec.length - spec.ball_radius, spec.width * 0.5];
    let balls = vec![
        BallFrame {
            id: CUE,
            pos: cue,
            potted: false,
        },
        BallFrame {
            id: 9,
            pos: against_the_rail,
            potted: false,
        },
    ];

    let mut c = Canvas::new(90, 30, CLOTH);
    let eye = Eye::behind(cue, 0.0, &spec, &c);
    table_3d::draw(
        &mut c,
        &spec,
        &spec.geometry(),
        &eye,
        &balls,
        &Sight::default(),
    );

    // The ball stands *on* the spot it touches: the bottom of its disc is the
    // projection of its contact point. Project the centre instead and the
    // whole disc lifts by the ball's own height — which the vertical scale
    // magnifies into several pixels, and which is how balls ended up hanging
    // in the room above the far rail.
    //
    // Overlapping the rail is not the bug and is not tested for: from this low
    // a ball frozen on the far cushion covers it, the way it does in a photo.
    let colour = table_ui::ball_colour(9);
    let foot = eye
        .to_screen(against_the_rail, 0.0)
        .expect("down the table, in view")
        .1;
    let painted: Vec<i32> = (0..c.height() as i32)
        .filter(|y| (0..c.cols() as i32).any(|x| c.get(x, *y) == colour))
        .collect();
    let bottom = *painted.last().expect("the nine was drawn at all") as f64;
    assert!(
        (bottom - foot).abs() <= 1.5,
        "the nine's disc ends at {bottom} but it touches the cloth at {foot}"
    );
}

#[test]
fn the_baulk_line_is_drawn_where_the_rule_puts_it() {
    // The chalk is cosmetic except for this one line: it is the kitchen
    // boundary, the same number `rules::placement_ok` enforces, so a player
    // put in hand behind it can see where "behind it" ends. A decorative line
    // in the wrong place would be worse than no line at all.
    use crate::app::games::pool_core::rules;

    let spec = SPEC;
    let head = rules::head_string(&spec);
    let mid = spec.width / 2.0;
    let tol = 0.004;

    assert!(table_ui::marking_at(&spec, [head, spec.width * 0.2], tol));
    assert!(!table_ui::marking_at(
        &spec,
        [head + spec.ball_radius * 4.0, spec.width * 0.2],
        tol
    ));

    // The D bulges back into the kitchen, not out over the playing area.
    let arc = spec.width * 0.3;
    assert!(table_ui::marking_at(&spec, [head - arc, mid], tol));
    assert!(!table_ui::marking_at(&spec, [head + arc, mid], tol));

    // And the spots are on the table's own reference points.
    assert!(table_ui::marking_at(&spec, rack::foot_spot(&spec), tol));
}

#[test]
fn both_views_paint_the_same_chalk() {
    // One predicate, sampled by both renderers, so the overview and the eye
    // cannot end up disagreeing about where the baulk line is — which would be
    // a strange thing for a player to have to notice.
    use crate::app::games::pool_core::table_3d::{self, Eye, Sight};

    let spec = SPEC;
    let mut over = Canvas::new(90, 14, CLOTH);
    let view = View::fit(&spec, &over);
    table_ui::draw(
        &mut over,
        &spec,
        &spec.geometry(),
        &view,
        &[],
        &Overlay::default(),
    );

    let mut eye_c = Canvas::new(90, 20, CLOTH);
    let eye = Eye::behind([spec.length * 0.5, spec.width * 0.5], 0.0, &spec, &eye_c);
    table_3d::draw(
        &mut eye_c,
        &spec,
        &spec.geometry(),
        &eye,
        &[],
        &Sight::default(),
    );

    for (name, c) in [("overview", &over), ("eye", &eye_c)] {
        let marks = (0..c.cols() as i32)
            .flat_map(|x| (0..c.height() as i32).map(move |y| (x, y)))
            .filter(|(x, y)| c.get(*x, *y) == table_ui::MARKING)
            .count();
        assert!(marks > 8, "the {name} drew no chalk on the cloth");
    }
}

#[test]
fn a_snooker_rack_draws_in_both_views() {
    // Twenty-two balls on a table nearly twice as long, in an id range the
    // pool renderer had never seen. Nothing here asserts beauty — it asserts
    // that every ball is on the cloth, drawn, and told apart from its
    // neighbours, which is what a rack of specks would fail.
    use crate::app::games::pool_core::table::SNOOKER_12FT;
    use crate::app::games::pool_core::table_3d::{self, Eye, Sight};

    let spec = SNOOKER_12FT;
    let balls: Vec<BallFrame> = rack::build(&spec, rack::RackKind::Snooker, 3)
        .balls
        .iter()
        .map(|b| BallFrame {
            id: b.id,
            pos: b.pos,
            potted: b.potted.is_some(),
        })
        .collect();
    assert_eq!(balls.len(), 22);

    let mut over = Canvas::new(110, 30, CLOTH);
    let view = View::fit(&spec, &over);
    assert!(
        view.ball_px() >= 3.0,
        "a snooker ball has to stay visible on a table this big: {}",
        view.ball_px()
    );
    table_ui::draw(
        &mut over,
        &spec,
        &spec.geometry(),
        &view,
        &balls,
        &Overlay::default(),
    );

    // Every ball's own colour appears somewhere on the overview: a rack where
    // the reds swallow each other is a rack you cannot read.
    for id in [16u8, 31, 32, 33, 34, 35, 36, CUE] {
        let colour = table_ui::ball_colour(id);
        let found = (0..over.cols() as i32)
            .any(|x| (0..over.height() as i32).any(|y| over.get(x, y) == colour));
        assert!(found, "ball {id} was drawn nowhere");
    }

    let mut eye_c = Canvas::new(110, 30, CLOTH);
    let cue = balls
        .iter()
        .find(|b| b.id == CUE)
        .expect("the cue ball")
        .pos;
    let eye = Eye::behind(cue, 0.0, &spec, &eye_c);
    table_3d::draw(
        &mut eye_c,
        &spec,
        &spec.geometry(),
        &eye,
        &balls,
        &Sight::default(),
    );
    let cloth = (0..eye_c.cols() as i32)
        .flat_map(|x| (0..eye_c.height() as i32).map(move |y| (x, y)))
        .filter(|(x, y)| eye_c.get(*x, *y) == CLOTH)
        .count();
    assert!(cloth > 200, "the eye view drew a table, not a void");
}

#[test]
fn a_corner_pocket_is_drawn_on_the_corner() {
    // The mouth chord of a corner runs diagonally across it, so its midpoint
    // is well inside the cloth; a disc centred there never touches the corner.
    // Drawn on the corner point instead, the four corners and the two sides
    // all line up on the same edge, the way a real table looks.
    let mut c = Canvas::new(220, 60, CLOTH);
    let view = View::fit(&SPEC, &c);
    table_ui::draw(
        &mut c,
        &SPEC,
        &SPEC.geometry(),
        &view,
        &frames(),
        &Overlay::default(),
    );
    for corner in [
        [0.0, 0.0],
        [0.0, SPEC.width],
        [SPEC.length, 0.0],
        [SPEC.length, SPEC.width],
    ] {
        let (px, py) = view.to_px(corner);
        assert_eq!(
            c.get(px.floor() as i32, py.floor() as i32),
            [10, 10, 12],
            "the cloth's corner at {corner:?} should be inside the pocket"
        );
    }
}

#[test]
fn table_coordinates_round_trip_through_the_view() {
    let c = canvas();
    let view = View::fit(&SPEC, &c);
    for at in [
        [0.0, 0.0],
        [SPEC.length, SPEC.width],
        [SPEC.length / 2.0, SPEC.width / 2.0],
        [0.31, 0.72],
    ] {
        let back = view.to_table(view.to_px(at));
        assert!(
            (back[0] - at[0]).abs() < 1e-9 && (back[1] - at[1]).abs() < 1e-9,
            "{at:?} round-tripped to {back:?}"
        );
    }
}

#[test]
fn the_table_is_drawn_the_right_way_round() {
    // Longer than it is wide, and centred. If the fit ever picked the wrong
    // axis the table would be drawn on its side and nothing else would notice.
    let c = canvas();
    let view = View::fit(&SPEC, &c);
    let (x0, y0) = view.to_px([0.0, 0.0]);
    let (x1, y1) = view.to_px([SPEC.length, SPEC.width]);
    assert!(x1 - x0 > y1 - y0, "the long axis should be horizontal");
    assert!(x0 >= 0.0 && y0 >= 0.0, "the table starts inside the canvas");
    assert!(
        x1 <= COLS as f64 && y1 <= (ROWS * 2) as f64,
        "and ends inside it"
    );
    assert!(
        ((x0 + x1) / 2.0 - COLS as f64 / 2.0).abs() < 1.5,
        "and is centred"
    );
}

#[test]
fn balls_are_drawn_large_enough_to_see() {
    // The whole reason the table view exaggerates: at this width a true-scale
    // ball would be under two pixels across.
    let c = canvas();
    let view = View::fit(&SPEC, &c);
    let true_px = SPEC.ball_radius * (view.to_px([1.0, 0.0]).0 - view.to_px([0.0, 0.0]).0);
    assert!(
        true_px < 2.0,
        "if a true-scale ball got this big the exaggeration could go"
    );
    assert!(view.ball_px() >= 2.5, "drawn radius is floored");
}

#[test]
fn the_exaggeration_fades_once_there_is_room() {
    // Balls are drawn bigger than life because a true-scale one is a speck.
    // That stops being true the moment the terminal is big enough, and a
    // *factor* keeps inflating forever — a wall-sized terminal ended up with
    // balls half again as large as the table said they were.
    use crate::app::games::pool_core::table::SNOOKER_12FT;

    for spec in [SPEC, SNOOKER_12FT] {
        let true_px = |cols: u16, rows: u16| {
            let c = Canvas::new(cols, rows, CLOTH);
            let view = View::fit(&spec, &c);
            let unit = view.to_px([1.0, 0.0]).0 - view.to_px([0.0, 0.0]).0;
            (spec.ball_radius * unit, view.ball_px())
        };

        // The exaggeration is a fixed number of pixels, so how much it
        // *matters* falls away as the true size grows. Assert that decay
        // rather than an absolute ratio: a twelve-foot table has a small ball
        // at any terminal size anyone actually has, and the property worth
        // having is that it stops growing, not that it hits a number.
        let inflation = |cols: u16, rows: u16| {
            let (t, drawn) = true_px(cols, rows);
            drawn / t
        };
        let (cramped, roomy, huge) = (inflation(80, 24), inflation(200, 60), inflation(600, 150));
        assert!(
            cramped > roomy && roomy > huge,
            "{spec:?} should inflate less the more room it has: \
             {cramped:.2} then {roomy:.2} then {huge:.2}"
        );
        assert!(huge < 1.4, "{spec:?} is still inflating hard: {huge:.2}");

        // But never smaller than life, and never below the floor that keeps a
        // cramped board's rack from being a row of dots.
        let (small_true, small_drawn) = true_px(80, 24);
        assert!(small_drawn >= 3.0, "{spec:?} floors at three pixels");
        assert!(small_drawn > small_true);
    }
}

#[test]
fn a_potted_ball_is_not_drawn() {
    let mut with_ball = canvas();
    let view = View::fit(&SPEC, &with_ball);
    let geom = SPEC.geometry();

    let mut frames = frames();
    table_ui::draw(
        &mut with_ball,
        &SPEC,
        &geom,
        &view,
        &frames,
        &Overlay::default(),
    );

    // Pot every object ball and redraw; the two must differ.
    for frame in frames.iter_mut() {
        if frame.id != CUE {
            frame.potted = true;
        }
    }
    let mut empty = canvas();
    table_ui::draw(
        &mut empty,
        &SPEC,
        &geom,
        &view,
        &frames,
        &Overlay::default(),
    );

    assert_ne!(
        render(&with_ball),
        render(&empty),
        "a rack and an empty table should not look the same"
    );
}

#[test]
fn stripes_and_solids_use_the_same_hue() {
    // A real set is like this, and it is what lets colour carry the identity
    // at three pixels: the white band is the only difference.
    for solid in 1..=7u8 {
        assert_eq!(
            table_ui::ball_colour(solid),
            table_ui::ball_colour(solid + 8),
            "the {} and the {} share a hue",
            solid,
            solid + 8
        );
    }
    assert!(table_ui::is_stripe(9) && !table_ui::is_stripe(7));
}

#[test]
fn the_cue_ball_is_the_brightest_thing_on_the_cloth() {
    let cue = table_ui::ball_colour(CUE);
    for id in 1..=15u8 {
        let other = table_ui::ball_colour(id);
        assert!(
            cue.iter().map(|c| *c as u32).sum::<u32>() > other.iter().map(|c| *c as u32).sum(),
            "the cue ball should outshine ball {id}"
        );
    }
}

#[test]
fn the_aim_overlay_changes_the_picture() {
    let geom = SPEC.geometry();
    let mut plain = canvas();
    let view = View::fit(&SPEC, &plain);
    table_ui::draw(
        &mut plain,
        &SPEC,
        &geom,
        &view,
        &frames(),
        &Overlay::default(),
    );

    let mut aimed = canvas();
    table_ui::draw(
        &mut aimed,
        &SPEC,
        &geom,
        &view,
        &frames(),
        &Overlay {
            aim_to: Some([SPEC.length * 0.7, SPEC.width * 0.5]),
            highlight: Some(1),
            ..Overlay::default()
        },
    );
    assert_ne!(render(&plain), render(&aimed), "the aim line should show");
}

#[test]
fn cloth_is_the_background_when_nothing_is_drawn() {
    let c = Canvas::new(4, 2, CLOTH);
    let lines = c.to_lines();
    let span = &lines[0].spans[0];
    assert_eq!(span.content.as_ref(), "▀");
    assert_eq!(span.style.fg, Some(rgb(CLOTH)));
    assert_eq!(span.style.bg, Some(rgb(CLOTH)));
}

/// A comparable rendering: the glyph plus both colours of every cell.
fn render(canvas: &Canvas) -> Vec<(String, Option<Color>, Option<Color>)> {
    canvas
        .to_lines()
        .into_iter()
        .flat_map(|line| {
            line.spans
                .into_iter()
                .map(|s| (s.content.to_string(), s.style.fg, s.style.bg))
                .collect::<Vec<_>>()
        })
        .collect()
}
