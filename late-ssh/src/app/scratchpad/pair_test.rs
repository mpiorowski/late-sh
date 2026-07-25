use std::sync::{Arc, Mutex};
use std::time::Instant;

use uuid::Uuid;

use super::*;
use crate::state::ActiveUser;

fn active_users_with(entries: &[(Uuid, &str)]) -> ActiveUsers {
    let mut users = std::collections::HashMap::new();
    for (id, username) in entries {
        users.insert(
            *id,
            ActiveUser {
                username: username.to_string(),
                fingerprint: None,
                peer_ip: None,
                audio_source: late_core::models::user::AudioSource::Icecast,
                sessions: Vec::new(),
                connection_count: 1,
                last_login_at: Instant::now(),
            },
        );
    }
    Arc::new(Mutex::new(users))
}

#[test]
fn finds_by_case_insensitive_username() {
    let alice = Uuid::from_u128(1);
    let active = active_users_with(&[(alice, "Alice")]);
    assert_eq!(
        find_active_user_by_username(Some(&active), "alice"),
        Some(alice)
    );
    assert_eq!(
        find_active_user_by_username(Some(&active), "ALICE"),
        Some(alice)
    );
}

#[test]
fn returns_none_for_unknown_username() {
    let active = active_users_with(&[(Uuid::from_u128(1), "Alice")]);
    assert_eq!(find_active_user_by_username(Some(&active), "bob"), None);
}

#[test]
fn returns_none_without_an_active_users_map() {
    assert_eq!(find_active_user_by_username(None, "alice"), None);
}
