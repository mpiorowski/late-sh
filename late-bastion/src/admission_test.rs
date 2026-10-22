use crate::admission::{IpLimits, Reject};

#[test]
fn per_ip_slots_are_shared_and_released_independently() {
    let limits = IpLimits::new(2, 30, 60);
    let ip = "203.0.113.1".parse().unwrap();
    let first = limits.admit(ip).unwrap();
    let second = limits.clone().admit(ip).unwrap();
    assert_eq!(limits.admit(ip).unwrap_err(), Reject::ConnectionLimit);
    let other = limits.admit("203.0.113.2".parse().unwrap()).unwrap();
    drop(first);
    let replacement = limits.admit(ip).unwrap();
    drop((second, replacement, other));
    assert!(limits.admit(ip).is_ok());
}

#[test]
fn reconnecting_does_not_reset_the_attempt_limit() {
    let limits = IpLimits::new(1, 2, 60);
    let ip = "203.0.113.1".parse().unwrap();
    drop(limits.admit(ip).unwrap());
    drop(limits.admit(ip).unwrap());
    assert_eq!(limits.admit(ip).unwrap_err(), Reject::RateLimited);
    assert!(limits.admit("203.0.113.2".parse().unwrap()).is_ok());
}

#[tokio::test]
async fn cancelling_a_connection_releases_its_ip_slot() {
    let limits = IpLimits::new(1, 30, 60);
    let ip = "203.0.113.1".parse().unwrap();
    let permit = limits.admit(ip).unwrap();
    let task = tokio::spawn(async move {
        let _permit = permit;
        std::future::pending::<()>().await;
    });
    task.abort();
    let _ = task.await;
    assert!(limits.admit(ip).is_ok());
}
