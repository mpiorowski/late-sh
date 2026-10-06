use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use super::SessionLease;

fn active() -> Arc<Mutex<HashSet<String>>> {
    Arc::new(Mutex::new(HashSet::new()))
}

/// Every player's progress lives in one shared HOME keyed by playname, so a
/// second concurrent session under the same name would run a second child
/// holding its own copy of that progress and overwrite the first's save. The
/// second claim must fail so `shell_request` can refuse it.
#[test]
fn a_second_claim_on_a_live_playname_is_refused() {
    let active = active();
    let _first = SessionLease::claim("mateu".to_string(), active.clone()).expect("first claims");

    assert!(SessionLease::claim("mateu".to_string(), active.clone()).is_none());
    // A different player is unaffected: the guard is per playname, not global.
    assert!(SessionLease::claim("tasmania".to_string(), active).is_some());
}

/// Dropping the lease (the bridge task ending, however it ended) frees the
/// name, so the player can reconnect immediately instead of being locked out
/// of their own save until the process restarts.
#[test]
fn dropping_a_lease_frees_the_playname_for_the_next_session() {
    let active = active();
    let first = SessionLease::claim("mateu".to_string(), active.clone()).expect("first claims");

    drop(first);

    let second = SessionLease::claim("mateu".to_string(), active.clone()).expect("name is free");
    assert_eq!(active.lock().expect("mutex").len(), 1);

    drop(second);
    assert!(active.lock().expect("mutex").is_empty());
}
