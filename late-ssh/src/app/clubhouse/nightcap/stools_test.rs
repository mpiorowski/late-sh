use std::time::Duration;

use late_core::models::presence::{ClubhouseStand, NightcapStand, PresenceRecord, Spot};
use uuid::Uuid;

use super::{OwnStool, SeatView, Stools, stools};

fn record(n: u128, stool: Option<(u8, i64)>) -> PresenceRecord {
    PresenceRecord {
        session_id: Uuid::from_u128(1_000 + n),
        user_id: Uuid::from_u128(n),
        username: format!("user{n:03}"),
        clubhouse: ClubhouseStand {
            spot: Spot::Door,
            since_ms: 0,
            emote: None,
            petted_dog_at_ms: None,
        },
        nightcap: stool.map(|(stool, sat_at_ms)| NightcapStand {
            stool,
            sat_at_ms,
            drinks: 1,
        }),
        street: None,
    }
}

fn standing_watcher() -> OwnStool<'static> {
    OwnStool {
        session_id: Uuid::from_u128(u128::MAX),
        user_id: Uuid::from_u128(u128::MAX),
        username: "watcher",
        stand: None,
    }
}

fn seat(n: u128, sat_at_ms: i64, now_ms: i64) -> Option<SeatView> {
    Some(SeatView {
        user_id: Uuid::from_u128(n),
        username: format!("user{n:03}"),
        seated_for: Duration::from_millis((now_ms - sat_at_ms) as u64),
        drinks: 1,
    })
}

#[test]
fn the_row_shows_who_sits_where_and_since_when() {
    let records = vec![
        record(1, Some((0, 1_000))),
        record(2, None),
        record(3, Some((4, 2_000))),
    ];

    let row = stools(&records, &standing_watcher(), 61_000);

    let expected: Stools = [
        seat(1, 1_000, 61_000),
        None,
        None,
        None,
        seat(3, 2_000, 61_000),
        None,
    ];
    assert_eq!(row, expected);
}

#[test]
fn a_stool_taken_twice_at_once_goes_to_the_earlier_sitter() {
    let records = vec![record(1, Some((2, 1_500))), record(2, Some((2, 1_000)))];

    let row = stools(&records, &standing_watcher(), 3_000);

    assert_eq!(row[2], seat(2, 1_000, 3_000));
    assert!(
        row.iter()
            .flatten()
            .all(|s| s.user_id != Uuid::from_u128(1)),
        "the later sitter is on no stool"
    );
}

#[test]
fn your_own_stool_shows_before_it_comes_back_and_one_user_holds_one_stool() {
    let mut phone = record(1, Some((5, 900)));
    phone.session_id = Uuid::from_u128(77);
    let records = vec![record(1, Some((1, 1_000))), phone];
    let own = OwnStool {
        session_id: Uuid::from_u128(9),
        user_id: Uuid::from_u128(9),
        username: "me",
        stand: Some(NightcapStand {
            stool: 3,
            sat_at_ms: 2_000,
            drinks: 0,
        }),
    };

    let row = stools(&records, &own, 2_000);

    assert_eq!(row[5], seat(1, 900, 2_000), "the earlier device's stool");
    assert_eq!(row[1], None, "the same user never holds two");
    assert_eq!(
        row[3],
        Some(SeatView {
            user_id: Uuid::from_u128(9),
            username: "me".to_string(),
            seated_for: Duration::ZERO,
            drinks: 0,
        })
    );
}
