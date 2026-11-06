use late_core::models::leaderboard::ArcadeGame;

use super::arcade_art::art;

const ART_ROWS: usize = 5;
const ART_COLUMNS: usize = 16;

/// Every arcade game has a picture that fits the card's corner: plain
/// ASCII, at most `ART_ROWS` by `ART_COLUMNS`, no blank or trailing
/// space to misalign it.
#[test]
fn every_arcade_game_has_a_picture_that_fits() {
    for game in ArcadeGame::all() {
        let rows = art(game);
        assert!(!rows.is_empty(), "{game:?} has no picture");
        assert!(rows.len() <= ART_ROWS, "{game:?} is {} rows", rows.len());
        for row in rows {
            assert!(row.is_ascii(), "{game:?}: {row:?}");
            assert!(!row.trim().is_empty(), "{game:?}: a blank row");
            assert_eq!(row.trim_end(), *row, "{game:?}: trailing space");
            assert!(row.len() <= ART_COLUMNS, "{game:?}: {row:?} is too wide");
        }
    }
}
