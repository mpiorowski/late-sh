use late_core::models::user::{AsciiPiece, Scene, SceneStyle};

use super::{
    Cadence, EARTH_FRAME_MS, EARTH_RATE, FRAME_MS, SLOW_FRAME_MS, SLOW_RATE, cadence, frame_index,
    js_i32, js_round, picture, seconds,
};

#[test]
fn js_round_rounds_halves_up_like_javascript() {
    assert_eq!(js_round(0.5), 1.0);
    assert_eq!(js_round(-0.5), 0.0);
    assert_eq!(js_round(-1.5), -1.0);
    assert_eq!(js_round(2.4), 2.0);
}

#[test]
fn js_i32_truncates_toward_zero_and_wraps() {
    assert_eq!(js_i32(-1.7), -1);
    assert_eq!(js_i32(3.9), 3);
    assert_eq!(js_i32(4_294_967_297.0), 1);
}

/// A lively piece plays a frame on every half-tier edge at the wall
/// clock's pace; a slow piece plays one every so many 1Hz edges at a
/// crawl: a hundred seconds of wall time are one second of the forest's
/// fog, and the Earth takes a notch every fourth second.
#[test]
fn a_frame_edge_is_the_half_tier_or_the_slow_second() {
    let aurora = AsciiPiece {
        scene: Scene::AuroraFjord,
        style: SceneStyle::Dots,
    };
    let forest = AsciiPiece {
        scene: Scene::MistyForest,
        style: SceneStyle::Dots,
    };
    let earth = AsciiPiece {
        scene: Scene::Earthrise,
        style: SceneStyle::Dots,
    };
    assert_eq!((FRAME_MS, SLOW_FRAME_MS, EARTH_FRAME_MS), (132, 1000, 4000));
    assert_eq!(cadence(aurora), Cadence::Half);
    assert_eq!(
        cadence(forest),
        Cadence::Slow {
            frame_ms: SLOW_FRAME_MS,
            rate: SLOW_RATE
        }
    );
    assert_eq!(
        cadence(earth),
        Cadence::Slow {
            frame_ms: EARTH_FRAME_MS,
            rate: EARTH_RATE
        }
    );
    assert_eq!(
        cadence(AsciiPiece {
            scene: Scene::MistyForest,
            style: SceneStyle::Pixels
        }),
        cadence(forest),
        "a scene's cadence is the scene's in either style"
    );
    assert_eq!(frame_index(aurora, 131), 0);
    assert_eq!(frame_index(aurora, 132), 1);
    assert_eq!(frame_index(forest, 999), 0);
    assert_eq!(frame_index(forest, 1000), 1);
    assert_eq!(frame_index(earth, 3999), 0);
    assert_eq!(frame_index(earth, 4000), 1);
    assert_eq!(seconds(aurora, 0), 0.0);
    assert_eq!(seconds(aurora, 1000), 132.0);
    assert_eq!(seconds(forest, 100), 1.0);
    assert_eq!(seconds(earth, 1), 4.0 * EARTH_RATE);
}

#[test]
fn sessions_asking_for_one_frame_share_it_in_either_style() {
    let dots = AsciiPiece {
        scene: Scene::AuroraFjord,
        style: SceneStyle::Dots,
    };
    let pixels = AsciiPiece {
        scene: Scene::AuroraFjord,
        style: SceneStyle::Pixels,
    };
    let first = picture(dots, 7);
    assert!(std::sync::Arc::ptr_eq(&first, &picture(dots, 7)));
    assert!(std::sync::Arc::ptr_eq(&first, &picture(pixels, 7)));
}

#[test]
fn every_piece_is_a_scene_on_the_originals_grid() {
    for piece in AsciiPiece::ALL {
        let scene = picture(piece, 3);
        assert_eq!((scene.cols, scene.rows), (200, 100), "{}", piece.label());
        assert_eq!(scene.cells.len(), 200 * 100, "{}", piece.label());
    }
}
