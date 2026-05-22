use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use late_core::MutexRecover;
use uuid::Uuid;

pub const CHESS_WIN_PAYOUT_COOLDOWN: Duration = Duration::from_secs(60 * 60);
pub const TRON_WIN_PAYOUT_COOLDOWN: Duration = Duration::from_secs(10 * 60);

#[derive(Clone)]
pub struct RoomWinPayoutLimiter {
    last_paid: Arc<Mutex<HashMap<Uuid, Instant>>>,
    cooldown: Duration,
}

impl RoomWinPayoutLimiter {
    pub fn new(cooldown: Duration) -> Self {
        Self {
            last_paid: Arc::new(Mutex::new(HashMap::new())),
            cooldown,
        }
    }

    pub fn allow(&self, user_id: Uuid, now: Instant) -> bool {
        let mut last_paid = self.last_paid.lock_recover();
        if let Some(last) = last_paid.get(&user_id)
            && now.saturating_duration_since(*last) < self.cooldown
        {
            return false;
        }
        last_paid.insert(user_id, now);
        last_paid.retain(|_, last| now.saturating_duration_since(*last) <= self.cooldown);
        true
    }
}

impl Default for RoomWinPayoutLimiter {
    fn default() -> Self {
        Self::new(CHESS_WIN_PAYOUT_COOLDOWN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_allows_first_payout_then_blocks_until_cooldown() {
        let limiter = RoomWinPayoutLimiter::new(Duration::from_secs(60));
        let user = Uuid::now_v7();
        let now = Instant::now();

        assert!(limiter.allow(user, now));
        assert!(!limiter.allow(user, now + Duration::from_secs(59)));
        assert!(limiter.allow(user, now + Duration::from_secs(60)));
    }
}
