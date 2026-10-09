use late_core::models::user::{AsciiPiece, Scene, SceneStyle};

use super::PiecePickerState;

#[test]
fn opens_on_the_piece_playing_and_walks_the_list_without_wrapping() {
    let mut picker = PiecePickerState::default();
    assert!(!picker.is_open());
    picker.open(AsciiPiece {
        scene: Scene::AlpineDawn,
        style: SceneStyle::Pixels,
    });
    assert!(picker.is_open());
    assert_eq!(
        picker.selected(),
        AsciiPiece {
            scene: Scene::AlpineDawn,
            style: SceneStyle::Pixels
        }
    );
    picker.move_cursor(-10);
    assert_eq!(
        picker.selected(),
        AsciiPiece {
            scene: Scene::Earthrise,
            style: SceneStyle::Dots
        }
    );
    picker.move_cursor(10);
    assert_eq!(
        picker.selected(),
        AsciiPiece {
            scene: Scene::AlpineDawn,
            style: SceneStyle::Pixels
        }
    );
    picker.set_cursor(3);
    assert_eq!(
        picker.selected(),
        AsciiPiece {
            scene: Scene::MistyForest,
            style: SceneStyle::Pixels
        }
    );
    picker.close();
    assert!(!picker.is_open());
}
