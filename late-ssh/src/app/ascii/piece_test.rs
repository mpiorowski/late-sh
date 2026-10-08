use late_core::models::user::{AsciiPiece, Scene, SceneStyle, TextPiece};

use super::{
    Cadence, FRAME_MS, Picture, SLOW_FRAME_MS, cadence, frame_index, js_i32, js_round, picture,
    seconds,
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
/// clock's pace; the slow piece plays one a second at a crawl, so two
/// hundred seconds of wall time are one second of its fog.
#[test]
fn a_frame_edge_is_the_half_tier_or_the_slow_second() {
    let donut = AsciiPiece::Text(TextPiece::Donut);
    let forest = AsciiPiece::Scene(Scene::MistyForest, SceneStyle::Dots);
    assert_eq!((FRAME_MS, SLOW_FRAME_MS), (132, 1000));
    assert_eq!(cadence(donut), Cadence::Half);
    assert_eq!(cadence(forest), Cadence::Slow);
    assert_eq!(
        cadence(AsciiPiece::Scene(Scene::MistyForest, SceneStyle::Pixels)),
        Cadence::Slow,
        "a scene's cadence is the scene's in either style"
    );
    assert_eq!(frame_index(donut, 131), 0);
    assert_eq!(frame_index(donut, 132), 1);
    assert_eq!(frame_index(forest, 999), 0);
    assert_eq!(frame_index(forest, 1000), 1);
    assert_eq!(seconds(donut, 0), 0.0);
    assert_eq!(seconds(donut, 1000), 132.0);
    assert_eq!(seconds(forest, 100), 1.0);
}

#[test]
fn sessions_asking_for_one_frame_share_it() {
    let Picture::Text(first) = picture(AsciiPiece::Text(TextPiece::Donut), 7, 80, 24) else {
        panic!("the donut is fixed-size text art");
    };
    let Picture::Text(again) = picture(AsciiPiece::Text(TextPiece::Donut), 7, 120, 40) else {
        panic!("the donut is fixed-size text art");
    };
    assert!(std::sync::Arc::ptr_eq(&first, &again));
}

#[test]
fn every_piece_draws_a_picture_of_its_kind() {
    for piece in AsciiPiece::ALL {
        match (piece, picture(piece, 3, 30, 10)) {
            (AsciiPiece::Scene(..), Picture::Shaded(scene)) => {
                assert_eq!((scene.cols, scene.rows), (200, 100));
            }
            (AsciiPiece::Text(TextPiece::LavaLamp), Picture::Text(art)) => {
                assert_eq!((art.cols, art.rows), (30, 27));
            }
            (AsciiPiece::Text(TextPiece::Donut), Picture::Text(art)) => {
                assert_eq!((art.cols, art.rows), (40, 22));
            }
            (AsciiPiece::Text(TextPiece::Plasma), Picture::Field(art)) => {
                assert_eq!((art.cols, art.rows), (30, 10));
            }
            (piece, _) => panic!("{} drew the wrong kind of picture", piece.label()),
        }
    }
}
