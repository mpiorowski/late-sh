use crate::MutexRecover;
use std::{
    collections::{HashMap, VecDeque},
    net::IpAddr,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct IpRateLimiter {
    max_attempts: usize,
    window: Duration,
    attempts_by_ip: Arc<Mutex<HashMap<IpAddr, VecDeque<Instant>>>>,
}

impl IpRateLimiter {
    pub fn new(max_attempts: usize, window_secs: u64) -> Self {
        Self {
            max_attempts,
            window: Duration::from_secs(window_secs),
            attempts_by_ip: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn max_attempts(&self) -> usize {
        self.max_attempts
    }

    pub fn window_secs(&self) -> u64 {
        self.window.as_secs()
    }

    pub fn entry_count(&self) -> usize {
        self.attempts_by_ip.lock_recover().len()
    }

    pub fn cleanup(&self) {
        let now = Instant::now();
        let mut attempts_by_ip = self.attempts_by_ip.lock_recover();
        attempts_by_ip.retain(|_, attempts| {
            while let Some(first) = attempts.front() {
                if now.duration_since(*first) <= self.window {
                    break;
                }
                attempts.pop_front();
            }
            !attempts.is_empty()
        });
    }

    pub fn allow(&self, ip: IpAddr) -> bool {
        if self.max_attempts == 0 {
            return true;
        }

        let now = Instant::now();
        let mut attempts_by_ip = self.attempts_by_ip.lock_recover();
        let attempts = attempts_by_ip.entry(ip).or_default();

        while let Some(first) = attempts.front() {
            if now.duration_since(*first) <= self.window {
                break;
            }
            attempts.pop_front();
        }

        if attempts.len() >= self.max_attempts {
            return false;
        }

        attempts.push_back(now);
        true
    }
}
