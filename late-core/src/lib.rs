pub mod api_types;
#[cfg(test)]
mod api_types_test;
pub mod ascii;
#[cfg(test)]
mod ascii_test;
pub mod audio;
pub mod audio_config;
#[cfg(test)]
mod audio_config_test;
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
#[cfg(test)]
mod rate_limit_test;
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
