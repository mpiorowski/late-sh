use super::*;

#[test]
fn new_users_only_receive_shared_tip_candidates() {
    let candidates = tip_candidates(true);

    assert_eq!(candidates.len(), NEW_AND_RETURNING_TIPS.len());
    for tip in &candidates {
        assert!(
            NEW_AND_RETURNING_TIPS
                .iter()
                .any(|candidate| candidate == tip)
        );
    }
    for tip in &candidates {
        assert!(!RETURNING_USER_TIPS.iter().any(|candidate| candidate == tip));
    }
}

#[test]
fn returning_users_receive_combined_tip_candidates() {
    let candidates = tip_candidates(false);

    assert_eq!(
        candidates.len(),
        NEW_AND_RETURNING_TIPS.len() + RETURNING_USER_TIPS.len()
    );
    for tip in NEW_AND_RETURNING_TIPS
        .iter()
        .chain(RETURNING_USER_TIPS.iter())
    {
        assert!(
            candidates
                .iter()
                .any(|candidate| candidate == &tip.as_str())
        );
    }
}

#[test]
fn splash_hint_selection_uses_pool_values_when_available() {
    let tip = choose_splash_hint(true);

    assert!(
        tip_candidates(true)
            .iter()
            .any(|candidate| candidate == &tip)
    );
}
