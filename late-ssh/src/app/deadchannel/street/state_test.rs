use late_core::models::presence::{ClubhouseStand, PresenceRecord, Spot, StreetStand};
use uuid::Uuid;

use super::{StreetPresence, StreetRunner, StreetView, street_view};

fn record(user_id: Uuid, street: Option<StreetStand>) -> PresenceRecord {
    PresenceRecord {
        session_id: Uuid::now_v7(),
        user_id,
        username: "someone".to_string(),
        clubhouse: ClubhouseStand {
            spot: Spot::Door,
            since_ms: 0,
            emote: None,
            petted_dog_at_ms: None,
        },
        nightcap: None,
        street,
    }
}

fn view(entries: &[(Uuid, u16, u16, bool)]) -> StreetView {
    entries
        .iter()
        .map(|&(user_id, x, y, present)| (user_id, StreetRunner { x, y, present }))
        .collect()
}

#[test]
fn only_sessions_that_went_down_stand_on_the_street() {
    let (ann, bob) = (Uuid::now_v7(), Uuid::now_v7());
    let mut runner = StreetPresence::new();
    runner.sync(10, 5, true, 1_000);
    assert_eq!(runner.stand(), None, "nothing before the descent");

    runner.descend();
    runner.sync(10, 5, true, 1_000);
    let records = vec![record(ann, runner.stand()), record(bob, None)];
    assert_eq!(street_view(&records), view(&[(ann, 10, 5, true)]));

    runner.leave();
    runner.sync(11, 5, true, 2_000);
    assert_eq!(
        runner.stand(),
        None,
        "a leaver is off until the next descent"
    );
}

#[test]
fn one_user_with_two_sessions_is_one_runner_where_they_last_moved() {
    let ann = Uuid::now_v7();
    let mut laptop = StreetPresence::new();
    let mut phone = StreetPresence::new();
    laptop.descend();
    phone.descend();
    laptop.sync(10, 5, true, 1_000);
    phone.sync(30, 5, true, 2_000);
    // The laptop looks away: dim there, but no newer than the phone's move.
    laptop.sync(10, 5, false, 3_000);
    let (laptop_record, phone_record) = (record(ann, laptop.stand()), record(ann, phone.stand()));
    assert_eq!(
        street_view(&[laptop_record.clone(), phone_record.clone()]),
        view(&[(ann, 30, 5, true)])
    );

    // The phone looks away too: the runner stays at the phone's spot, dim.
    phone.sync(30, 5, false, 4_000);
    let phone_record = PresenceRecord {
        street: phone.stand(),
        ..phone_record
    };
    assert_eq!(
        street_view(&[laptop_record.clone(), phone_record.clone()]),
        view(&[(ann, 30, 5, false)])
    );

    // The laptop steps: the latest mover wins.
    laptop.sync(11, 5, true, 5_000);
    let laptop_record = PresenceRecord {
        street: laptop.stand(),
        ..laptop_record
    };
    assert_eq!(
        street_view(&[laptop_record, phone_record]),
        view(&[(ann, 11, 5, true)])
    );
}
