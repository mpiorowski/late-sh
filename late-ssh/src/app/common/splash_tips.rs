use std::sync::LazyLock;

use rand_core::{OsRng, RngCore};

const DEFAULT_SPLASH_TIP: &str = "Press ? outside of chat to see available hotkeys";

static NEW_AND_RETURNING_TIPS: LazyLock<Vec<String>> = LazyLock::new(|| {
    parse_tip_pool(include_str!(
        "../../../assets/splash_tips/new_and_returning_users_tip_pool.json"
    ))
});

static RETURNING_USER_TIPS: LazyLock<Vec<String>> = LazyLock::new(|| {
    parse_tip_pool(include_str!(
        "../../../assets/splash_tips/returning_users_tip_pool.json"
    ))
});

fn parse_tip_pool(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw)
        .expect("splash tip pool json is malformed")
        .into_iter()
        .map(|tip| tip.trim().to_string())
        .filter(|tip| !tip.is_empty())
        .collect()
}

pub(crate) fn tip_candidates(is_new_user: bool) -> Vec<&'static str> {
    let mut candidates: Vec<&'static str> =
        NEW_AND_RETURNING_TIPS.iter().map(String::as_str).collect();
    if !is_new_user {
        candidates.extend(RETURNING_USER_TIPS.iter().map(String::as_str));
    }
    candidates
}

pub(crate) fn choose_splash_hint(is_new_user: bool) -> String {
    let candidates = tip_candidates(is_new_user);
    if candidates.is_empty() {
        return DEFAULT_SPLASH_TIP.to_string();
    }

    let idx = (OsRng.next_u64() as usize) % candidates.len();
    candidates[idx].to_string()
}

#[cfg(test)]
mod tests {
    use super::{NEW_AND_RETURNING_TIPS, RETURNING_USER_TIPS, choose_splash_hint, tip_candidates};

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
}
