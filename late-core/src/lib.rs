pub mod api_types;
pub mod ascii;
pub mod audio;
pub mod audio_config;
pub mod db;
#[cfg(test)]
mod db_test;
pub mod icecast;
pub mod model;
#[cfg(test)]
mod model_test;
pub mod models;
pub mod nonogram;
pub mod rate_limit;
pub mod shutdown;
pub mod telemetry;

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
