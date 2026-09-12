use super::*;

fn timed(now: DateTime<Utc>, status: Status, minutes: i64) -> SessionStatus {
    SessionStatus {
        status,
        ends_at: Some(now + chrono::Duration::minutes(minutes)),
    }
}

fn open(status: Status) -> SessionStatus {
    SessionStatus {
        status,
        ends_at: None,
    }
}

/// Every glyph and word is distinct, so no two statuses read the same in a
/// chat line and no typed word is ambiguous.
#[test]
fn statuses_are_distinct_and_parse_by_word() {
    let mut glyphs: Vec<&str> = Status::ALL.into_iter().map(Status::glyph).collect();
    glyphs.sort_unstable();
    let count = glyphs.len();
    glyphs.dedup();
    assert_eq!(glyphs.len(), count, "two statuses share a glyph");

    for status in Status::ALL {
        assert_eq!(Status::parse(status.word()), Some(status));
    }
    assert_eq!(Status::parse("WORKING"), Some(Status::Working));
    assert_eq!(
        Status::parse("deep work"),
        None,
        "free text is not a status any more"
    );
    assert_eq!(Status::parse(""), None);
}

/// No status glyph may carry a variation selector. Ratatui's crossterm backend
/// miscounts the cursor after a wide VS16 grapheme, so glyphs after one drift
/// until a resize (`chat/CONTEXT.md`, message rendering). A status badge
/// prints on every author line of everyone who set one, which makes it the
/// widest place for that bug to show.
#[test]
fn no_status_glyph_needs_a_variation_selector() {
    for status in Status::ALL {
        assert!(
            !status.glyph().contains('\u{FE0F}'),
            "status {} uses the VS16 glyph {}",
            status.word(),
            status.glyph(),
        );
    }
}

/// The reason the set is closed at all: a status glyph must never be
/// something a player can buy, or a badge owner would look like they set a
/// status they never set. Scans the seed migrations, which is how every
/// purchasable badge enters the catalog today.
#[test]
fn no_status_glyph_collides_with_a_purchasable_badge() {
    // Compare with the variation selector stripped: a seed that writes a
    // status glyph's VS16 spelling still collides visually with ours.
    let bare = |text: &str| text.replace('\u{FE0F}', "");
    let migrations = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../late-core/migrations")
        .canonicalize()
        .expect("migrations directory");

    let mut checked = 0usize;
    for entry in std::fs::read_dir(&migrations).expect("read migrations") {
        let path = entry.expect("migration entry").path();
        if path.extension().is_none_or(|ext| ext != "sql") {
            continue;
        }
        let sql = bare(&std::fs::read_to_string(&path).expect("read migration"));
        // Only the seeds that put a glyph beside a name can collide.
        if !sql.contains("beside your chat name") && !sql.contains("emoji") {
            continue;
        }
        checked += 1;
        for status in Status::ALL {
            assert!(
                !sql.contains(&bare(status.glyph())),
                "status {} uses {}, already seeded by {}",
                status.word(),
                status.glyph(),
                path.file_name().unwrap_or_default().to_string_lossy(),
            );
        }
    }
    assert!(checked > 0, "no badge seed migrations were scanned");
}

/// The owner's HUD reads seconds while a countdown runs and falls back to the
/// glyph when there is nothing to count.
#[test]
fn hud_badge_counts_down_and_floors_at_zero() {
    let now = Utc::now();
    let focus = timed(now, Status::Focus, 25);
    assert_eq!(focus.remaining_secs(now), Some(25 * 60));
    assert_eq!(focus.hud_badge(now), "25:00 focus");
    assert_eq!(
        focus.hud_badge(now + chrono::Duration::milliseconds(500)),
        "25:00 focus"
    );
    assert_eq!(
        focus.hud_badge(now + chrono::Duration::seconds(1)),
        "24:59 focus"
    );

    // Expiry is cleared on a 1Hz edge in `tick.rs`, so the badge holds at zero
    // for the frames in between instead of going negative.
    let elapsed = timed(now, Status::Focus, -1);
    assert_eq!(elapsed.remaining_secs(now), Some(0));
    assert_eq!(elapsed.hud_badge(now), "00:00 focus");

    assert_eq!(open(Status::Away).hud_badge(now), "💤 away");
    assert_eq!(open(Status::Away).remaining_secs(now), None);
}

/// Peers see whole minutes, rounded up, so a fresh 25 minute block never
/// reads `24m` and the last partial minute never reads `0m`.
#[test]
fn peer_badge_rounds_minutes_up_and_expires() {
    let now = Utc::now();
    assert_eq!(
        timed(now, Status::Focus, 25).peer_badge(now).as_deref(),
        Some("🍅 25m focus")
    );
    assert_eq!(
        SessionStatus {
            status: Status::Focus,
            ends_at: Some(now + chrono::Duration::seconds(24 * 60 + 1)),
        }
        .peer_badge(now)
        .as_deref(),
        Some("🍅 25m focus"),
        "a part-used minute still counts"
    );
    assert_eq!(
        SessionStatus {
            status: Status::Focus,
            ends_at: Some(now + chrono::Duration::seconds(1)),
        }
        .peer_badge(now)
        .as_deref(),
        Some("🍅 1m focus"),
        "the final minute is not rounded away"
    );
    assert_eq!(
        timed(now, Status::Focus, 0).peer_badge(now),
        None,
        "an elapsed countdown has no badge, which is how entries retire"
    );
}

/// The whole rule, at the level the rest of the app reads it: a countdown
/// survives posting and expires on its own, an open-ended status never
/// expires and is cleared by a message instead.
#[test]
fn only_open_ended_statuses_clear_on_post() {
    let now = Utc::now();
    let counting = timed(now, Status::Focus, 25);
    assert!(!counting.clears_on_post());
    assert!(!counting.is_expired(now));
    assert!(counting.is_expired(now + chrono::Duration::minutes(25)));

    let away = open(Status::Away);
    assert!(away.clears_on_post());
    assert!(
        !away.is_expired(now + chrono::Duration::days(1)),
        "nothing but a message clears an open-ended status"
    );
    assert_eq!(
        away.peer_badge(now + chrono::Duration::days(1)).as_deref(),
        Some("💤 away"),
        "and it never goes stale in the directory"
    );
}

#[test]
fn directory_publishes_clears_and_skips_stale_entries() {
    let now = Utc::now();
    let directory = new_directory();
    let running = Uuid::from_u128(1);
    let idle = Uuid::from_u128(2);
    let stale = Uuid::from_u128(3);

    set_user(&directory, running, Some(timed(now, Status::Focus, 5)));
    set_user(&directory, idle, Some(open(Status::Away)));
    set_user(&directory, stale, Some(timed(now, Status::Gaming, -5)));

    let badges = resolve_all(&snapshot(&directory), now);
    assert_eq!(
        badges.get(&running).map(String::as_str),
        Some("🍅 5m focus")
    );
    assert_eq!(badges.get(&idle).map(String::as_str), Some("💤 away"));
    assert!(
        !badges.contains_key(&stale),
        "an elapsed entry resolves to no badge"
    );

    // `/status off`, the clearing message, and session teardown all clear
    // through the same path.
    set_user(&directory, running, None);
    assert!(snapshot(&directory).get(&running).is_none());
}

/// The disconnect path in `ssh.rs`: it removes the leaving session from the
/// roster and publishes with no status of its own. The entry falls back to a
/// session that is still running one, and clears once the user has none left.
#[test]
fn publish_for_user_falls_back_to_a_remaining_sessions_status() {
    use crate::state::{ActiveSession, ActiveUser};

    let now = Utc::now();
    let user = Uuid::from_u128(1);
    let focus = timed(now, Status::Focus, 50);
    let session = |token: &str, status: Option<SessionStatus>| ActiveSession {
        token: token.to_string(),
        fingerprint: None,
        peer_ip: None,
        status,
    };
    let active_users: ActiveUsers = Arc::new(Mutex::new(
        [(
            user,
            ActiveUser {
                username: "alice".to_string(),
                fingerprint: None,
                audio_source: late_core::models::user::AudioSource::default(),
                sessions: vec![
                    session("desktop", Some(open(Status::Away))),
                    session("laptop", Some(focus)),
                ],
                connection_count: 2,
                last_login_at: std::time::Instant::now(),
            },
        )]
        .into(),
    ));
    let directory = new_directory();

    publish_for_user(&directory, &active_users, user, Some(open(Status::Away)));
    assert_eq!(
        snapshot(&directory).get(&user),
        Some(&open(Status::Away)),
        "the session that just changed wins"
    );

    // The desktop disconnects.
    active_users
        .lock_recover()
        .get_mut(&user)
        .expect("user")
        .sessions
        .retain(|session| session.token != "desktop");
    publish_for_user(&directory, &active_users, user, None);
    assert_eq!(
        snapshot(&directory).get(&user),
        Some(&focus),
        "the laptop's countdown survives the desktop leaving"
    );

    // The laptop was the last connection.
    active_users.lock_recover().remove(&user);
    publish_for_user(&directory, &active_users, user, None);
    assert_eq!(snapshot(&directory).get(&user), None);
}

/// Readers hold an `Arc` of the map, so a write during a frame must not be
/// visible to the snapshot that frame already took, and an unchanged write
/// must not swap the pointer (the chat row cache epoch keys off it).
#[test]
fn snapshot_is_isolated_from_later_writes() {
    let now = Utc::now();
    let directory = new_directory();
    let user = Uuid::from_u128(1);
    set_user(&directory, user, Some(timed(now, Status::Focus, 25)));

    let taken = snapshot(&directory);
    set_user(&directory, user, Some(timed(now, Status::Focus, 25)));
    assert!(
        Arc::ptr_eq(&taken, &snapshot(&directory)),
        "an unchanged write must not swap the pointer"
    );

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
