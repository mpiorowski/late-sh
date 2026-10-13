use late_core::models::user::{AsciiPiece, AsciiSelection, Scene, SceneStyle};

use super::PiecePickerState;

#[test]
fn opens_on_the_piece_playing_and_walks_the_list_without_wrapping() {
    let mut picker = PiecePickerState::default();
    assert!(!picker.is_open());
    picker.open(AsciiSelection::Piece(AsciiPiece {
        scene: Scene::AlpineDawn,
        style: SceneStyle::Pixels,
    }));
    assert!(picker.is_open());
    assert_eq!(
        picker.selected(),
        AsciiSelection::Piece(AsciiPiece {
            scene: Scene::AlpineDawn,
            style: SceneStyle::Pixels
        })
    );
    picker.move_cursor(-10);
    assert_eq!(
        picker.selected(),
        AsciiSelection::Piece(AsciiPiece {
            scene: Scene::Earthrise,
            style: SceneStyle::Dots
        })
    );
    picker.move_cursor(10);
    assert_eq!(picker.selected(), AsciiSelection::Randomer);
    picker.set_cursor(3);
    assert_eq!(
        picker.selected(),
        AsciiSelection::Piece(AsciiPiece {
            scene: Scene::MistyForest,
            style: SceneStyle::Pixels
        })
    );
    picker.close();
    assert!(!picker.is_open());
}

#[test]
fn opens_on_the_configured_random_mode() {
    let mut picker = PiecePickerState::default();
    picker.open(AsciiSelection::Random);
    assert_eq!(picker.selected(), AsciiSelection::Random);
    picker.move_cursor(1);
    assert_eq!(picker.selected(), AsciiSelection::Randomer);
    picker.close();
    picker.open(AsciiSelection::Randomer);
    assert_eq!(picker.selected(), AsciiSelection::Randomer);
}
