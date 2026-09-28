use late_core::models::deadchannel_street::{Stand, StreetBatch};
use uuid::Uuid;

use super::{HEARD_TTL_MS, Street, StreetRunner, StreetView};

fn view(entries: &[(Uuid, u16, u16, bool)]) -> StreetView {
    entries
        .iter()
        .map(|&(user_id, x, y, present)| (user_id, StreetRunner { x, y, present }))
        .collect()
}

#[test]
fn a_walk_on_one_replica_lands_on_the_other() {
    let (replica_a, replica_b) = (Uuid::now_v7(), Uuid::now_v7());
    let mut a = Street::new(replica_a);
    let mut b = Street::new(replica_b);
    let (ann_session, ann) = (Uuid::now_v7(), Uuid::now_v7());
    let (bob_session, bob) = (Uuid::now_v7(), Uuid::now_v7());

    a.stand(ann_session, ann, 10, 5, true, 1_000);
    a.stand(ann_session, ann, 11, 5, true, 1_100);
    b.stand(bob_session, bob, 40, 6, true, 1_000);
    let from_a = a.take_changes();
    let from_b = b.take_changes();
    assert!(a.hear(from_b.clone(), 1_200));
    assert!(b.hear(from_a.clone(), 1_200));
    // Each replica's own batch comes back over the wire and changes nothing.
    assert!(!a.hear(from_a, 1_200));
    assert!(!b.hear(from_b, 1_200));

    let both = view(&[(ann, 11, 5, true), (bob, 40, 6, true)]);
    assert_eq!(a.view(), both);
    assert_eq!(b.view(), both);
    // Nothing moved since, so nothing goes out.
    assert!(a.take_changes().is_empty());
}

#[test]
fn a_leaver_goes_at_once_and_a_dead_replica_goes_when_its_heartbeats_stop() {
    let mut here = Street::new(Uuid::now_v7());
    let mut leaving = Street::new(Uuid::now_v7());
    let mut dying = Street::new(Uuid::now_v7());
    let (ann_session, ann) = (Uuid::now_v7(), Uuid::now_v7());
    let (bob_session, bob) = (Uuid::now_v7(), Uuid::now_v7());
    leaving.stand(ann_session, ann, 10, 5, true, 0);
    dying.stand(bob_session, bob, 20, 5, false, 0);
    here.hear(leaving.take_heartbeat(), 0);
    here.hear(dying.take_heartbeat(), 0);

    assert!(leaving.leave(ann_session));
    assert!(!leaving.leave(ann_session));
    let goodbye = leaving.take_changes();
    assert_eq!(goodbye.stands, Vec::<Stand>::new());
    assert_eq!(goodbye.left, vec![ann_session]);
    here.hear(goodbye, 1_000);
    assert_eq!(here.view(), view(&[(bob, 20, 5, false)]));

    // The dying replica says nothing more; its runner holds for the TTL.
    assert_eq!(here.expire(HEARD_TTL_MS - 1), 0);
    assert_eq!(here.view(), view(&[(bob, 20, 5, false)]));
    assert_eq!(here.expire(HEARD_TTL_MS), 1);
    assert_eq!(here.view(), StreetView::new());
}

#[test]
fn one_user_on_two_replicas_is_one_runner_where_they_last_moved() {
    let mut laptop_side = Street::new(Uuid::now_v7());
    let mut phone_side = Street::new(Uuid::now_v7());
    let ann = Uuid::now_v7();
    let (laptop, phone) = (Uuid::now_v7(), Uuid::now_v7());

    laptop_side.stand(laptop, ann, 10, 5, true, 1_000);
    phone_side.stand(phone, ann, 30, 5, true, 2_000);
    // The laptop looks away: dim there, but no newer than the phone's move.
    laptop_side.stand(laptop, ann, 10, 5, false, 3_000);
    phone_side.hear(laptop_side.take_changes(), 3_000);
    assert_eq!(phone_side.view(), view(&[(ann, 30, 5, true)]));

    // The phone looks away too: the runner stays at the phone's spot, dim.
    phone_side.stand(phone, ann, 30, 5, false, 4_000);
    assert_eq!(phone_side.view(), view(&[(ann, 30, 5, false)]));

    // The laptop steps: the latest mover wins.
    laptop_side.stand(laptop, ann, 11, 5, true, 5_000);
    phone_side.hear(laptop_side.take_changes(), 5_000);
    assert_eq!(phone_side.view(), view(&[(ann, 11, 5, true)]));
}

#[test]
fn a_heartbeat_repeats_every_stand_and_carries_an_unpublished_leaver() {
    let replica_id = Uuid::now_v7();
    let mut street = Street::new(replica_id);
    let (ann_session, ann) = (Uuid::now_v7(), Uuid::now_v7());
    let (bob_session, bob) = (Uuid::now_v7(), Uuid::now_v7());
    let cat_session = Uuid::now_v7();
    street.stand(ann_session, ann, 10, 5, true, 1_000);
    street.stand(bob_session, bob, 20, 5, false, 1_000);
    street.stand(cat_session, Uuid::now_v7(), 30, 5, true, 1_000);
    street.take_changes();
    street.leave(cat_session);

    let mut heartbeat = street.take_heartbeat();
    heartbeat.stands.sort_by_key(|stand| stand.session_id);
    let mut stands = vec![
        Stand {
            session_id: ann_session,
            user_id: ann,
            x: 10,
            y: 5,
            present: true,
            moved_at_ms: 1_000,
        },
        Stand {
            session_id: bob_session,
            user_id: bob,
            x: 20,
            y: 5,
            present: false,
            moved_at_ms: 1_000,
        },
    ];
    stands.sort_by_key(|stand| stand.session_id);
    assert_eq!(
        heartbeat,
        StreetBatch {
            replica_id,
            stands,
            left: vec![cat_session],
        }
    );
    assert_eq!(street.local_count(), 2);
    assert!(street.take_changes().is_empty());
}
