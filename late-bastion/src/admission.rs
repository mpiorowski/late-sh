//! Per-IP admission before SSH negotiation, after trusted PROXY resolution.

use late_core::{MutexRecover, rate_limit::IpRateLimiter};
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub(crate) struct IpLimits {
    attempts: IpRateLimiter,
    counts: Arc<Mutex<HashMap<IpAddr, usize>>>,
    max_connections: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Reject {
    RateLimited,
    ConnectionLimit,
}

#[derive(Debug)]
pub(crate) struct IpPermit {
    ip: IpAddr,
    counts: Arc<Mutex<HashMap<IpAddr, usize>>>,
}

impl IpLimits {
    pub(crate) fn new(max_connections: usize, max_attempts: usize, window_secs: u64) -> Self {
        Self {
            attempts: IpRateLimiter::new(max_attempts, window_secs),
            counts: Arc::default(),
            max_connections,
        }
    }

    pub(crate) fn admit(&self, ip: IpAddr) -> Result<IpPermit, Reject> {
        if !self.attempts.allow(ip) {
            return Err(Reject::RateLimited);
        }
        let mut counts = self.counts.lock_recover();
        if counts.get(&ip).copied().unwrap_or_default() >= self.max_connections {
            return Err(Reject::ConnectionLimit);
        }
        *counts.entry(ip).or_default() += 1;
        Ok(IpPermit {
            ip,
            counts: Arc::clone(&self.counts),
        })
    }

    pub(crate) fn cleanup(&self) {
        self.attempts.cleanup();
    }
}

impl Drop for IpPermit {
    fn drop(&mut self) {
        let mut counts = self.counts.lock_recover();
        if let Some(count) = counts.get_mut(&self.ip) {
            if *count <= 1 {
                counts.remove(&self.ip);
            } else {
                *count -= 1;
            }
        }
    }
}
