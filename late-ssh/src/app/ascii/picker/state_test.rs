use late_core::models::user::{AsciiPiece, Scene, SceneStyle, TextPiece};

use super::PiecePickerState;

#[test]
fn opens_on_the_piece_playing_and_walks_the_list_without_wrapping() {
    let mut picker = PiecePickerState::default();
    assert!(!picker.is_open());
    picker.open(AsciiPiece::Scene(Scene::AlpineDawn, SceneStyle::Pixels));
    assert!(picker.is_open());
    assert_eq!(
        picker.selected(),
        AsciiPiece::Scene(Scene::AlpineDawn, SceneStyle::Pixels)
    );
    picker.move_cursor(-10);
    assert_eq!(
        picker.selected(),
        AsciiPiece::Scene(Scene::MistyForest, SceneStyle::Dots)
    );
    picker.move_cursor(10);
    assert_eq!(picker.selected(), AsciiPiece::Text(TextPiece::Donut));
    picker.set_cursor(6);
    assert_eq!(picker.selected(), AsciiPiece::Text(TextPiece::Plasma));
    picker.close();
    assert!(!picker.is_open());
}
