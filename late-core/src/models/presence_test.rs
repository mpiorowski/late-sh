use uuid::Uuid;

use super::{
    ClubhouseStand, Emote, MAX_PAYLOAD_BYTES, NightcapStand, PresenceBatch, PresenceRecord, Spot,
    StreetStand, parse_presence_payload,
};

fn record(n: u16) -> PresenceRecord {
    PresenceRecord {
        session_id: Uuid::now_v7(),
        user_id: Uuid::now_v7(),
        // The longest name the site allows, so the split is tested at its
        // worst.
        username: "x".repeat(32),
        clubhouse: ClubhouseStand {
            spot: Spot::Walking { x: n, y: 7 },
            since_ms: 1_790_000_000_123,
            emote: Some((Emote::Dance, 1_790_000_000_456)),
            petted_dog_at_ms: Some(1_790_000_000_789),
        },
        nightcap: Some(NightcapStand {
            stool: 5,
            sat_at_ms: 1_790_000_000_000,
            drinks: 3,
        }),
        street: Some(StreetStand {
            x: 400,
            y: 12,
            present: false,
            moved_at_ms: 1_790_000_000_999,
        }),
    }
}

#[test]
fn a_batch_survives_the_wire() {
    let mut seated = record(1);
    seated.clubhouse.spot = Spot::Seat { index: 4 };
    seated.nightcap = None;
    seated.street = None;
    let mut standing = record(2);
    standing.clubhouse.spot = Spot::Standing { index: 1 };
    let mut door = record(3);
    door.clubhouse.spot = Spot::Door;
    door.clubhouse.emote = None;
    let sent = PresenceBatch {
        replica_id: Uuid::now_v7(),
        records: vec![record(0), seated, standing, door],
        left: vec![Uuid::now_v7()],
    };
    assert_eq!(parse_presence_payload(&sent.to_payload()), Some(sent));
}

#[test]
fn junk_on_the_channel_is_not_a_batch() {
    let payload = PresenceBatch {
        replica_id: Uuid::now_v7(),
        records: vec![record(1)],
        left: Vec::new(),
    }
    .to_payload();
    for junk in [
        "",
        "{}",
        "not json",
        &payload[..payload.len() - 1],
        &payload.replace("\"walking\"", "\"flying\""),
    ] {
        assert_eq!(parse_presence_payload(junk), None, "payload: {junk}");
    }
}

#[test]
fn a_big_batch_splits_into_notifies_that_fit_and_keep_every_entry_in_order() {
    let replica_id = Uuid::now_v7();
    let records: Vec<PresenceRecord> = (0..60).map(record).collect();
    let left: Vec<Uuid> = (0..300).map(|_| Uuid::now_v7()).collect();
    let batch = PresenceBatch {
        replica_id,
        records: records.clone(),
        left: left.clone(),
    };

    let notifies = batch.into_notifies();

    assert!(notifies.len() > 2, "{} notifies", notifies.len());
    for notify in &notifies {
        assert_eq!(notify.replica_id, replica_id);
        assert!(
            notify.to_payload().len() <= MAX_PAYLOAD_BYTES,
            "{} bytes",
            notify.to_payload().len()
        );
    }
    let rejoined_records: Vec<PresenceRecord> =
        notifies.iter().flat_map(|n| n.records.clone()).collect();
    let rejoined_left: Vec<Uuid> = notifies.iter().flat_map(|n| n.left.clone()).collect();
    assert_eq!(rejoined_records, records);
    assert_eq!(rejoined_left, left);
}

#[test]
fn an_empty_batch_sends_nothing() {
    let batch = PresenceBatch {
        replica_id: Uuid::now_v7(),
        records: Vec::new(),
        left: Vec::new(),
    };
    assert!(batch.into_notifies().is_empty());
}
