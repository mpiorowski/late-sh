use std::sync::LazyLock;

use late_core::models::media_queue_item::SONG_QUEUE_REWARD_CHIPS;
use late_core::radio::RadioStation;
use rand_core::{OsRng, RngCore};

/// "Did you know": the splash line on the first logins after the tour,
/// one per login, chosen by the account's login count and shown once each.
/// It sells the house to someone who came back, in the order that predicts
/// return; past the series the splash goes back to the random tip pools.
/// `pub` because `metrics` counts the line shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DidYouKnow {
    /// The radio: the stations and the YouTube booth, where they play.
    Radio,
    /// The rung is up and a daily win starts the streak.
    Rung,
    /// The people: the map and the profiles.
    People,
    /// The Music Booth queues a track for the whole house.
    Booth,
    /// The Artboard, one canvas for everyone.
    Artboard,
    /// Nightcap, the bar out back.
    Nightcap,
}

impl DidYouKnow {
    /// The line for this login, counted from the account's first. The first
    /// login is the tour's, and past the series there is none.
    pub(crate) fn for_login(login_count: i32) -> Option<Self> {
        match login_count {
            2 => Some(Self::Radio),
            3 => Some(Self::Rung),
            4 => Some(Self::People),
            5 => Some(Self::Booth),
            6 => Some(Self::Artboard),
            7 => Some(Self::Nightcap),
            _ => None,
        }
    }

    /// The splash line. `listen_url` is the public listen page.
    pub(crate) fn line(self, listen_url: &str) -> String {
        match self {
            Self::Radio => {
                let stations = RadioStation::enabled().count();
                format!(
                    "Did you know? {stations} radio stations and the YouTube booth play in the late CLI, or in a browser at {listen_url}"
                )
            }
            Self::Rung => "Did you know? Today's rung is up in The Arcade: any daily win starts your streak, the rung's adds a star".to_string(),
            Self::People => "Did you know? People (5) has a map of who is where, and p on any name in chat opens their profile".to_string(),
            Self::Booth => format!(
                "Did you know? v then v opens the Music Booth: queue a YouTube track for the whole house, it pays {SONG_QUEUE_REWARD_CHIPS} chips"
            ),
            Self::Artboard => "Did you know? Artboard (4) is one ASCII canvas the whole house draws on; the best of it hangs in the gallery".to_string(),
            Self::Nightcap => "Did you know? n in the Clubhouse opens Nightcap, the quiet bar out back: six stools, drinks on d, carve a line with c".to_string(),
        }
    }
}

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
#[path = "splash_tips_test.rs"]
mod splash_tips_test;
