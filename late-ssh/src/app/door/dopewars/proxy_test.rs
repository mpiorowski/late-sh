use crate::app::door::dopewars::proxy::*;

#[test]
fn session_label_is_account_derived_and_safe() {
    let id = uuid::Uuid::from_u128(0x1234_5678_9abc_def0_1122_3344_5566_7788);
    let label = dopewars_session_label(id);
    assert!(label.starts_with("late_"));
    assert!(label.ends_with(&id.simple().to_string()[8..]));
    assert!(label.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
}

#[test]
fn session_label_is_stable_per_account() {
    let id = uuid::Uuid::from_u128(0x1234_5678_9abc_def0_1122_3344_5566_7788);
    assert_eq!(dopewars_session_label(id), dopewars_session_label(id));
}

#[test]
fn session_label_distinguishes_accounts() {
    let a = uuid::Uuid::from_u128(1);
    let b = uuid::Uuid::from_u128(2);
    assert_ne!(dopewars_session_label(a), dopewars_session_label(b));
}
