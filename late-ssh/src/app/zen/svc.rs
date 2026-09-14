//! The Pulse sampler: once a minute the process's human headcount lands in
//! the shared day of buckets every session's Pulse tile reads. In-memory
//! only, like presence itself: a restart starts a fresh day.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use late_core::MutexRecover;
use tokio_util::sync::CancellationToken;

use super::pulse::PulseHistory;
use crate::state::{ActiveUsers, online_human_count};

pub type SharedPulse = Arc<Mutex<PulseHistory>>;

const SAMPLE_EVERY: Duration = Duration::from_secs(60);

pub fn new_shared_pulse() -> SharedPulse {
    Arc::new(Mutex::new(PulseHistory::default()))
}

pub async fn run_pulse_sampler(
    pulse: SharedPulse,
    active_users: ActiveUsers,
    shutdown: CancellationToken,
) {
    let mut interval = tokio::time::interval(SAMPLE_EVERY);
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = interval.tick() => {
                let online = online_human_count(&active_users);
                pulse.lock_recover().record(chrono::Utc::now(), online);
            }
        }
    }
}
