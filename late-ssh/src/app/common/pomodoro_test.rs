use super::*;

fn at(now: DateTime<Utc>, minutes: i64) -> PomodoroTimer {
    PomodoroTimer {
        label: "deep work".to_string(),
        ends_at: now + chrono::Duration::minutes(minutes),
    }
}

#[test]
fn owner_badge_counts_down_in_whole_seconds() {
    let now = Utc::now();
    let timer = at(now, 25);
    // Rounded up, so the first frame after `/pomodoro 25` reads the duration
    // the user asked for rather than one second less.
    assert_eq!(timer.remaining_secs(now), 25 * 60);
    assert_eq!(timer.badge(now), "25:00 deep work");
    assert_eq!(
        timer.badge(now + chrono::Duration::milliseconds(500)),
        "25:00 deep work"
    );
    assert_eq!(
        timer.badge(now + chrono::Duration::seconds(1)),
        "24:59 deep work"
    );
    assert_eq!(
        timer.badge(now + chrono::Duration::minutes(24)),
        "01:00 deep work"
    );
}

/// Expiry is cleared on a 1Hz edge in `tick.rs`, so the badge has to hold at
/// zero for the frames in between instead of going negative.
#[test]
fn owner_badge_floors_at_zero_after_expiry() {
    let now = Utc::now();
    let timer = PomodoroTimer {
        label: "Pomodoro".to_string(),
        ends_at: now - chrono::Duration::seconds(30),
    };
    assert_eq!(timer.remaining_secs(now), 0);
    assert_eq!(timer.badge(now), "00:00 Pomodoro");
}

/// Peers see whole minutes, rounded up, so a fresh 25 minute block never reads
/// `24m` and the last partial minute never reads `0m`.
#[test]
fn peer_badge_rounds_minutes_up_and_expires() {
    let now = Utc::now();
    assert_eq!(
        peer_badge(now + chrono::Duration::minutes(25), now).as_deref(),
        Some("🍅25m")
    );
    assert_eq!(
        peer_badge(now + chrono::Duration::seconds(24 * 60 + 1), now).as_deref(),
        Some("🍅25m"),
        "a part-used minute still counts"
    );
    assert_eq!(
        peer_badge(now + chrono::Duration::seconds(1), now).as_deref(),
        Some("🍅1m"),
        "the final minute is not rounded away"
    );
    assert_eq!(
        peer_badge(now, now),
        None,
        "an elapsed timer has no badge, which is how entries retire"
    );
    assert_eq!(peer_badge(now - chrono::Duration::hours(2), now), None);
}

/// The whole point of storing only `ends_at`: what a peer can render carries
/// no trace of the label its owner typed.
#[test]
fn directory_publishes_the_deadline_without_the_label() {
    let now = Utc::now();
    let directory = new_directory();
    let user = Uuid::from_u128(1);

    set_user(&directory, user, Some(&at(now, 25)));
    let entries = snapshot(&directory);
    assert_eq!(
        entries.get(&user).copied(),
        Some(now + chrono::Duration::minutes(25))
    );

    let badges = resolve_all(&entries, now);
    assert_eq!(badges.get(&user).map(String::as_str), Some("🍅25m"));
    assert!(
        !badges.values().any(|badge| badge.contains("deep work")),
        "a peer badge must never carry the owner's label"
    );
}

#[test]
fn directory_clears_on_stop_and_skips_stale_entries() {
    let now = Utc::now();
    let directory = new_directory();
    let running = Uuid::from_u128(1);
    let stale = Uuid::from_u128(2);

    set_user(&directory, running, Some(&at(now, 5)));
    set_user(&directory, stale, Some(&at(now, -5)));
    let badges = resolve_all(&snapshot(&directory), now);
    assert!(badges.contains_key(&running));
    assert!(
        !badges.contains_key(&stale),
        "an elapsed entry resolves to no badge"
    );

    // `/pomodoro stop` and session teardown both clear through the same path.
    set_user(&directory, running, None);
    assert!(snapshot(&directory).get(&running).is_none());
}

/// Readers hold an `Arc` of the map, so a write during a frame must not be
/// visible to the snapshot that frame already took.
#[test]
fn snapshot_is_isolated_from_later_writes() {
    let now = Utc::now();
    let directory = new_directory();
    let user = Uuid::from_u128(1);
    set_user(&directory, user, Some(&at(now, 25)));

    let taken = snapshot(&directory);
    set_user(&directory, user, None);
    assert!(
        taken.contains_key(&user),
        "the already-taken snapshot keeps its entry"
    );
    assert!(
        snapshot(&directory).is_empty(),
        "the next read sees the clear"
    );
}
