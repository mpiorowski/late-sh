use chrono::NaiveDate;

use crate::app::arcade::share::{Row, ShareCard};

use super::card;

#[test]
fn card_is_the_picture_in_half_blocks_under_a_sized_header() {
    let picture = vec![
        vec![false, true, true, false],
        vec![true, false, false, true],
        vec![true, true, true, true],
        vec![false, true, true, false],
    ];
    let card = card(
        NaiveDate::from_ymd_opt(2026, 4, 12).unwrap(),
        "easy",
        &picture,
    );
    assert_eq!(
        card,
        ShareCard {
            title: "late.sh Nonograms #2 · easy 4×4".to_string(),
            rows: vec![Row::Text("▄▀▀▄".to_string()), Row::Text("▀██▀".to_string()),],
        }
    );
}
