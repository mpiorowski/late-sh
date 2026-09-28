use late_core::models::presence::{ClubhouseStand, PresenceBatch, PresenceRecord, Spot};
use uuid::Uuid;

use super::{HEARD_TTL_MS, Presence};

fn record(session_id: Uuid, user_id: Uuid, x: u16) -> PresenceRecord {
    PresenceRecord {
        session_id,
        user_id,
        username: format!("user{x}"),
        clubhouse: ClubhouseStand {
            spot: Spot::Walking { x, y: 5 },
            since_ms: 1_000,
            emote: None,
            petted_dog_at_ms: None,
        },
        nightcap: None,
        street: None,
    }
}

fn sorted(mut records: Vec<PresenceRecord>) -> Vec<PresenceRecord> {
    records.sort_by_key(|record| record.session_id);
    records
}

#[test]
fn a_change_on_one_replica_lands_on_the_other() {
    let mut a = Presence::new(Uuid::now_v7());
    let mut b = Presence::new(Uuid::now_v7());
    let ann = record(Uuid::now_v7(), Uuid::now_v7(), 10);
    let bob = record(Uuid::now_v7(), Uuid::now_v7(), 40);

    assert!(a.put(record(ann.session_id, ann.user_id, 9)));
    assert!(a.put(ann.clone()));
    assert!(!a.put(ann.clone()), "the same record again is no change");
    b.put(bob.clone());
    let from_a = a.take_changes();
    let from_b = b.take_changes();
    assert_eq!(from_a.records, vec![ann.clone()], "only the latest goes out");
    assert!(a.hear(from_b.clone(), 1_200));
    assert!(b.hear(from_a.clone(), 1_200));
    // Each replica's own batch comes back over the wire and changes nothing.
    assert!(!a.hear(from_a, 1_200));
    assert!(!b.hear(from_b, 1_200));

    let both = sorted(vec![ann, bob]);
    assert_eq!(a.records(), both);
    assert_eq!(b.records(), both);
    assert!(a.take_changes().is_empty(), "nothing changed since");
}

#[test]
fn a_leaver_goes_at_once_and_a_dead_replica_goes_when_its_heartbeats_stop() {
    let mut here = Presence::new(Uuid::now_v7());
    let mut leaving = Presence::new(Uuid::now_v7());
    let mut dying = Presence::new(Uuid::now_v7());
    let ann = record(Uuid::now_v7(), Uuid::now_v7(), 10);
    let bob = record(Uuid::now_v7(), Uuid::now_v7(), 20);
    leaving.put(ann.clone());
    dying.put(bob.clone());
    here.hear(leaving.take_heartbeat(), 0);
    here.hear(dying.take_heartbeat(), 0);

    assert!(leaving.leave(ann.session_id));
    assert!(!leaving.leave(ann.session_id));
    let goodbye = leaving.take_changes();
    assert_eq!(goodbye.records, Vec::<PresenceRecord>::new());
    assert_eq!(goodbye.left, vec![ann.session_id]);
    here.hear(goodbye, 1_000);
    assert_eq!(here.records(), vec![bob.clone()]);

    // The dying replica says nothing more; its record holds for the TTL.
    assert_eq!(here.expire(HEARD_TTL_MS - 1), 0);
    assert_eq!(here.records(), vec![bob]);
    assert_eq!(here.expire(HEARD_TTL_MS), 1);
    assert_eq!(here.records(), Vec::<PresenceRecord>::new());
}

#[test]
fn a_heartbeat_repeats_every_record_and_carries_an_unpublished_leaver() {
    let replica_id = Uuid::now_v7();
    let mut presence = Presence::new(replica_id);
    let ann = record(Uuid::now_v7(), Uuid::now_v7(), 10);
    let bob = record(Uuid::now_v7(), Uuid::now_v7(), 20);
    let cat = record(Uuid::now_v7(), Uuid::now_v7(), 30);
    presence.put(ann.clone());
    presence.put(bob.clone());
    presence.put(cat.clone());
    presence.take_changes();
    presence.leave(cat.session_id);

    let mut heartbeat = presence.take_heartbeat();
    heartbeat.records = sorted(heartbeat.records);
    assert_eq!(
        heartbeat,
        PresenceBatch {
            replica_id,
            records: sorted(vec![ann, bob]),
            left: vec![cat.session_id],
        }
    );
    assert!(presence.take_changes().is_empty());
}
