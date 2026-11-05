use late_core::models::user::{AsciiPiece, Scene, SceneStyle};

use super::{
    AURORA_RATE, AURORA_SWEEP_FRAMES, AURORA_SWEEP_RATE, Cadence, EARTH_FRAME_MS, EARTH_RATE,
    FRAME_MS, QUARTER_FRAME_MS, SLOW_FRAME_MS, SLOW_RATE, SWEEP_STEP, cadence, frame_index, js_i32,
    js_round, picture, seconds,
};
use crate::app::ascii::aurora_fjord;

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

/// Alpine dawn plays a frame on every half-tier edge in dots and every
/// quarter-tier edge in pixels, at the wall clock's pace; a 1Hz piece
/// plays one every so many 1Hz edges at a fraction of it: a hundred
/// seconds of wall time are one second of the forest's fog, the Earth
/// takes a notch every fourth second, the aurora in pixels drifts a fifth
/// of a second every second, and in dots its front takes a hundred seconds
/// to carry it one `SWEEP_STEP`.
#[test]
fn a_frame_edge_is_the_tier_its_bytes_earn() {
    let alpine = AsciiPiece {
        scene: Scene::AlpineDawn,
        style: SceneStyle::Dots,
    };
    let alpine_pixels = AsciiPiece {
        scene: Scene::AlpineDawn,
        style: SceneStyle::Pixels,
    };
    let aurora = AsciiPiece {
        scene: Scene::AuroraFjord,
        style: SceneStyle::Dots,
    };
    let aurora_pixels = AsciiPiece {
        scene: Scene::AuroraFjord,
        style: SceneStyle::Pixels,
    };
    let forest = AsciiPiece {
        scene: Scene::MistyForest,
        style: SceneStyle::Dots,
    };
    let earth = AsciiPiece {
        scene: Scene::Earthrise,
        style: SceneStyle::Dots,
    };
    assert_eq!(
        (FRAME_MS, QUARTER_FRAME_MS, SLOW_FRAME_MS, EARTH_FRAME_MS),
        (132, 264, 1000, 4000)
    );
    assert_eq!(cadence(alpine), Cadence::Half);
    assert_eq!(cadence(alpine_pixels), Cadence::Quarter);
    assert_eq!(
        cadence(aurora),
        Cadence::Slow {
            frame_ms: SLOW_FRAME_MS,
            rate: AURORA_SWEEP_RATE
        }
    );
    assert_eq!(
        cadence(aurora_pixels),
        Cadence::Slow {
            frame_ms: SLOW_FRAME_MS,
            rate: AURORA_RATE
        }
    );
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
    for scene in [Scene::Earthrise, Scene::MistyForest] {
        assert_eq!(
            cadence(AsciiPiece {
                scene,
                style: SceneStyle::Pixels
            }),
            cadence(AsciiPiece {
                scene,
                style: SceneStyle::Dots
            }),
            "a crawling scene's cadence is the scene's in either style"
        );
    }
    assert_eq!(frame_index(alpine, 131), 0);
    assert_eq!(frame_index(alpine, 132), 1);
    assert_eq!(frame_index(alpine_pixels, 263), 0);
    assert_eq!(frame_index(alpine_pixels, 264), 1);
    assert_eq!(frame_index(forest, 999), 0);
    assert_eq!(frame_index(forest, 1000), 1);
    assert_eq!(frame_index(earth, 3999), 0);
    assert_eq!(frame_index(earth, 4000), 1);
    assert_eq!(seconds(alpine, 0), 0.0);
    assert_eq!(seconds(alpine, 1000), 132.0);
    assert_eq!(seconds(alpine_pixels, 500), 132.0);
    assert_eq!(seconds(aurora, AURORA_SWEEP_FRAMES), SWEEP_STEP);
    assert_eq!(seconds(aurora_pixels, 5), 1.0);
    assert_eq!(seconds(forest, 100), 1.0);
    assert_eq!(seconds(earth, 1), 4.0 * EARTH_RATE);
}

#[test]
fn sessions_asking_for_one_frame_share_it_in_either_style() {
    let dots = AsciiPiece {
        scene: Scene::MistyForest,
        style: SceneStyle::Dots,
    };
    let pixels = AsciiPiece {
        scene: Scene::MistyForest,
        style: SceneStyle::Pixels,
    };
    let first = picture(dots, 7);
    assert!(std::sync::Arc::ptr_eq(&first, &picture(dots, 7)));
    assert!(std::sync::Arc::ptr_eq(&first, &picture(pixels, 7)));
}

/// The aurora's styles play different pictures, so at one play time the
/// cache hands each its own, and neither pushes the other out.
#[test]
fn the_aurora_keeps_its_own_picture_in_each_style_at_one_play_time() {
    let dots = AsciiPiece {
        scene: Scene::AuroraFjord,
        style: SceneStyle::Dots,
    };
    let pixels = AsciiPiece {
        scene: Scene::AuroraFjord,
        style: SceneStyle::Pixels,
    };
    // Frame 50 in dots and frame 5 in pixels are both one second of play.
    assert_eq!(seconds(dots, 50), seconds(pixels, 5));
    let swept = picture(dots, 50);
    let drifted = picture(pixels, 5);
    assert_eq!(swept.cells, aurora_fjord::sweep(1.0).cells);
    assert_eq!(drifted.cells, aurora_fjord::frame(1.0).cells);
    assert!(std::sync::Arc::ptr_eq(&swept, &picture(dots, 50)));
    assert!(std::sync::Arc::ptr_eq(&drifted, &picture(pixels, 5)));
}

#[test]
fn every_piece_is_a_scene_on_the_originals_grid() {
    for piece in AsciiPiece::ALL {
        let scene = picture(piece, 3);
        assert_eq!((scene.cols, scene.rows), (200, 100), "{}", piece.label());
        assert_eq!(scene.cells.len(), 200 * 100, "{}", piece.label());
    }
}

/// Alpine dawn's two styles run different cadences, so one frame number is
/// a different play time in each: the cache hands each style its own play
/// time's picture, and the same play time in both styles is one picture.
#[test]
fn a_frame_number_is_its_own_styles_play_time_in_the_cache() {
    let dots = AsciiPiece {
        scene: Scene::AlpineDawn,
        style: SceneStyle::Dots,
    };
    let pixels = AsciiPiece {
        scene: Scene::AlpineDawn,
        style: SceneStyle::Pixels,
    };
    let dots_frame = picture(dots, 1001);
    let pixels_frame = picture(pixels, 1001);
    assert_ne!(
        dots_frame.cells, pixels_frame.cells,
        "frame 1001 is 132s of play in dots and 264s in pixels"
    );
    assert_eq!(
        picture(dots, 2002).cells,
        pixels_frame.cells,
        "the dots' frame 2002 is the pixels' frame 1001"
    );
}
