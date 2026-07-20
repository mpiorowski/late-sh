use crate::rate_limit::*;
use std::net::IpAddr;
use tokio::time::{Duration, sleep};

#[test]
fn allows_when_limit_is_zero() {
    let limiter = IpRateLimiter::new(0, 60);
    let ip: IpAddr = "127.0.0.1".parse().expect("parse ip");
    for _ in 0..10 {
        assert!(limiter.allow(ip));
    }
}

#[test]
fn denies_after_limit_within_window() {
    let limiter = IpRateLimiter::new(2, 60);
    let ip: IpAddr = "127.0.0.1".parse().expect("parse ip");
    assert!(limiter.allow(ip));
    assert!(limiter.allow(ip));
    assert!(!limiter.allow(ip));
}

#[test]
fn tracks_each_ip_independently() {
    let limiter = IpRateLimiter::new(1, 60);
    let ip_a: IpAddr = "127.0.0.1".parse().expect("parse ip a");
    let ip_b: IpAddr = "127.0.0.2".parse().expect("parse ip b");

    assert!(limiter.allow(ip_a));
    assert!(!limiter.allow(ip_a));
    assert!(limiter.allow(ip_b));
    assert!(!limiter.allow(ip_b));
}

#[tokio::test]
async fn allows_again_after_window_expires() {
    let limiter = IpRateLimiter::new(1, 1);
    let ip: IpAddr = "127.0.0.1".parse().expect("parse ip");

    assert!(limiter.allow(ip));
    assert!(!limiter.allow(ip));
    sleep(Duration::from_millis(1100)).await;
    assert!(limiter.allow(ip));
}

#[tokio::test]
async fn cleanup_removes_expired_ip_entries() {
    let limiter = IpRateLimiter::new(1, 1);
    let ip: IpAddr = "10.0.0.1".parse().expect("parse ip");

    limiter.allow(ip);
    assert_eq!(limiter.entry_count(), 1);

    sleep(Duration::from_millis(1100)).await;
    limiter.cleanup();
    assert_eq!(limiter.entry_count(), 0);
}

#[tokio::test]
async fn cleanup_retains_ips_with_active_timestamps() {
    let limiter = IpRateLimiter::new(5, 1);
    let stale_ip: IpAddr = "10.0.0.1".parse().expect("parse stale ip");
    let active_ip: IpAddr = "10.0.0.2".parse().expect("parse active ip");

    limiter.allow(stale_ip);
    sleep(Duration::from_millis(1100)).await;
    limiter.allow(active_ip);

    limiter.cleanup();
    assert_eq!(limiter.entry_count(), 1);
}
