use late_core::models::user::{AsciiPiece, TextPiece};

use super::{FRAME_MS, Picture, js_i32, js_round, picture, seconds};

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

#[test]
fn a_frame_edge_is_the_half_tier() {
    assert_eq!(FRAME_MS, 132);
    assert_eq!(seconds(0), 0.0);
    assert_eq!(seconds(1000), 132.0);
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
