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


