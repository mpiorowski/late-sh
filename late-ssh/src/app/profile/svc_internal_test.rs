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

fn day(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
}

#[test]
fn no_friend_birthdays_yields_no_alert() {
    assert_eq!(build_birthday_alert(&[], day(2026, 5, 20)), None);
    let none_soon = vec![("zoe".to_string(), "11-30".to_string())];
    assert_eq!(build_birthday_alert(&none_soon, day(2026, 5, 20)), None);
}

#[test]
fn today_birthday_is_called_out_first() {
    let b = vec![
        ("ada".to_string(), "05-20".to_string()),
        ("bo".to_string(), "05-23".to_string()),
    ];
    let msg = build_birthday_alert(&b, day(2026, 5, 20)).unwrap();
    assert!(msg.starts_with("ada — birthday today!"), "{msg}");
    assert!(msg.contains("bo's birthday in 3 days"), "{msg}");
}

#[test]
fn tomorrow_is_phrased_specially_and_sorted_by_proximity() {
    let b = vec![
        ("far".to_string(), "05-27".to_string()),
        ("near".to_string(), "05-21".to_string()),
    ];
    let msg = build_birthday_alert(&b, day(2026, 5, 20)).unwrap();
    assert_eq!(msg, "near's birthday tomorrow · far's birthday in 7 days");
}

#[test]
fn eight_days_out_is_outside_the_window() {
    let b = vec![("late".to_string(), "05-28".to_string())];
    assert_eq!(build_birthday_alert(&b, day(2026, 5, 20)), None);
}
