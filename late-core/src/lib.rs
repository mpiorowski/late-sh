pub mod api_types;
pub mod ascii;
pub mod audio;
pub mod audio_config;
pub mod db;
pub mod icecast;
pub mod model;
pub mod models;
pub mod nonogram;
pub mod proxy_protocol;
pub mod rate_limit;
pub mod shutdown;
pub mod telemetry;
pub mod tunnel_protocol;

#[cfg(feature = "testing")]
pub mod test_utils;

use std::sync::{Mutex, MutexGuard};

/// Extension trait for `Mutex` that recovers from poisoning instead of panicking.
pub trait MutexRecover<T> {
    fn lock_recover(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexRecover<T> for Mutex<T> {
    fn lock_recover(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|e| {
            tracing::warn!("mutex poisoned, recovering");
            e.into_inner()
        })
    }
}
