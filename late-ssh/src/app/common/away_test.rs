use std::sync::{Arc, Mutex};

use super::*;
use crate::state::ActiveSession;

fn session(token: &str, away: bool) -> ActiveSession {
    ActiveSession {
        token: token.to_string(),
        fingerprint: None,
        peer_ip: None,
        away,
    }
}

fn user(sessions: Vec<ActiveSession>) -> ActiveUser {
    ActiveUser {
        username: "alice".to_string(),
        fingerprint: None,
        audio_source: late_core::models::user::AudioSource::default(),
        connection_count: sessions.len().max(1),
        sessions,
        last_login_at: std::time::Instant::now(),
    }
}

#[test]
fn a_session_goes_away_after_the_threshold_or_by_hand() {
    assert!(!session_is_away(Duration::ZERO, false));
    assert!(!session_is_away(AWAY_AFTER - Duration::from_secs(1), false));
    assert!(session_is_away(AWAY_AFTER, false));
    assert!(
        session_is_away(Duration::ZERO, true),
        "/brb is away at once"
    );
}

/// The whole merge rule over a roster: every session away is away, one live
/// session keeps the user here, and a user with no sessions at all (the
/// always-on ghost bots) is never away.
#[test]
fn away_user_ids_needs_every_session_away_and_at_least_one() {
    let all_away = Uuid::from_u128(1);
    let one_here = Uuid::from_u128(2);
    let bot = Uuid::from_u128(3);
    let roster: HashMap<Uuid, ActiveUser> = [
        (
            all_away,
            user(vec![session("desktop", true), session("irc", true)]),
        ),
        (
            one_here,
            user(vec![session("desktop", true), session("laptop", false)]),
        ),
        (bot, user(Vec::new())),
    ]
    .into();

    assert_eq!(away_user_ids(&roster), HashSet::from([all_away]));
}

#[test]
fn set_session_away_marks_only_the_named_session() {
    let user_id = Uuid::from_u128(1);
    let active_users: ActiveUsers = Arc::new(Mutex::new(
        [(
            user_id,
            user(vec![session("desktop", false), session("laptop", false)]),
        )]
        .into(),
    ));

    set_session_away(&active_users, user_id, "desktop", true);
    assert!(!user_is_away(&active_users.lock_recover()[&user_id]));

    set_session_away(&active_users, user_id, "laptop", true);
    assert!(user_is_away(&active_users.lock_recover()[&user_id]));

    // A session that already left is not resurrected, and nothing panics.
    set_session_away(&active_users, user_id, "gone", false);
    set_session_away(&active_users, Uuid::from_u128(9), "desktop", false);
    assert!(user_is_away(&active_users.lock_recover()[&user_id]));
}

/// The away glyph prints on every author line of everyone who is away, so it
/// must not carry a variation selector: ratatui's crossterm backend miscounts
/// the cursor after a wide VS16 grapheme (`chat/CONTEXT.md`).
#[test]
fn away_glyph_needs_no_variation_selector() {
    assert!(!AWAY_GLYPH.contains('\u{FE0F}'));
}

/// Nobody may be able to buy the away glyph as a badge, or a buyer would read
/// as away while they are here. Scans the seed migrations, which is how every
/// purchasable badge enters the catalog.
#[test]
fn away_glyph_is_not_a_purchasable_badge() {
    // Compare with the variation selector stripped: a seed that writes the
    // glyph's VS16 spelling still collides visually with ours.
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
        assert!(
            !sql.contains(&bare(AWAY_GLYPH)),
            "{AWAY_GLYPH} is already seeded by {}",
            path.file_name().unwrap_or_default().to_string_lossy(),
        );
    }
    assert!(checked > 0, "no badge seed migrations were scanned");
}
