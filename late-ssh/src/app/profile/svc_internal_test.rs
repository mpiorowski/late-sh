use super::*;

#[test]
fn profile_snapshot_default_is_empty() {
    let snapshot = ProfileSnapshot::default();
    assert_eq!(snapshot.user_id, None);
    assert!(snapshot.profile.is_none());
    assert!(snapshot.bonsai.is_none());
}

#[test]
fn should_prune_when_only_one_receiver_remains() {
    let (tx, _rx) = watch::channel(ProfileSnapshot::default());
    assert!(should_prune_snapshot_sender(&tx));
}

#[test]
fn should_not_prune_when_multiple_receivers_exist() {
    let (tx, _rx1) = watch::channel(ProfileSnapshot::default());
    let _rx2 = tx.subscribe();
    assert!(!should_prune_snapshot_sender(&tx));
}

#[test]
fn should_prune_when_channel_is_closed() {
    let (tx, rx) = watch::channel(ProfileSnapshot::default());
    drop(rx);
    assert!(should_prune_snapshot_sender(&tx));
}
