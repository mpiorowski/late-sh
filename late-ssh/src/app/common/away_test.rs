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

#[test]
fn only_a_person_counts_as_presence() {
    let person = |data: &[u8]| match presence_input(data) {
        PresenceInput::Person { event_end, .. } => Some(event_end),
        PresenceInput::Nobody | PresenceInput::Partial { .. } => None,
    };
    // Keys, pastes, clicks, drags, and scrolls are a person; the first
    // thing they did ends where the next one starts.
    assert_eq!(person(b"j"), Some(1));
    assert_eq!(person(b"jk"), Some(1), "the second key is theirs too");
    assert_eq!(person(b"\xc3\xa9j"), Some(2), "one UTF-8 character");
    assert_eq!(person(b"\x1b"), Some(1), "Esc alone is the Esc key");
    assert_eq!(person(b"\x1bj"), Some(2), "Alt and a key");
    assert_eq!(person(b"\x1b[A"), Some(3));
    assert_eq!(person(b"\x1bOP"), Some(3), "F1");
    assert_eq!(person(b"\x1b[200~hello\x1b[201~x"), Some(17), "a whole paste");
    assert_eq!(person(b"\x1b[<0;20;5M"), Some(10), "left press");
    assert_eq!(person(b"\x1b[<0;20;5m"), Some(10), "left release");
    assert_eq!(person(b"\x1b[<32;21;5M"), Some(11), "left drag");
    assert_eq!(person(b"\x1b[<64;20;5M"), Some(11), "wheel up");
    // The pointer crossing the terminal (motion bit, no button; with or
    // without a modifier held), and focus reports, are not.
    assert_eq!(presence_input(b""), PresenceInput::Nobody);
    assert_eq!(presence_input(b"\x1b[<35;20;5M"), PresenceInput::Nobody);
    assert_eq!(
        presence_input(b"\x1b[<39;20;5M"),
        PresenceInput::Nobody,
        "shift held"
    );
    assert_eq!(presence_input(b"\x1b[I"), PresenceInput::Nobody);
    assert_eq!(presence_input(b"\x1b[O"), PresenceInput::Nobody);
    // A burst of moves in one chunk is still nobody; one key in it is a
    // person, whose event ends past the moves before it.
    assert_eq!(
        presence_input(b"\x1b[<35;20;5M\x1b[<35;21;5M\x1b[<35;22;6M"),
        PresenceInput::Nobody
    );
    assert_eq!(person(b"\x1b[<35;20;5M\x1b[<35;21;5Mx"), Some(23));
    assert_eq!(person(b"\x1b[<35;20;5M\x1b[<0;21;5M"), Some(21));
}

/// A chunk boundary can fall inside a report. The cut-off tail is held,
/// not classified: half a mouse move is not a person, and neither is its
/// other half once the two are read together.
#[test]
fn a_report_cut_in_two_is_read_whole() {
    assert_eq!(
        presence_input(b"\x1b[<35;2"),
        PresenceInput::Partial { held_from: 0 }
    );
    assert_eq!(
        presence_input(b"\x1b[<35;20;5M\x1b[<35;2"),
        PresenceInput::Partial { held_from: 11 }
    );
    assert_eq!(presence_input(b"\x1b["), PresenceInput::Partial { held_from: 0 });
    assert_eq!(presence_input(b"\x1bO"), PresenceInput::Partial { held_from: 0 });
    assert_eq!(
        presence_input(b"\x1bP>|kitty"),
        PresenceInput::Partial { held_from: 0 },
        "a reply to a query, cut before its terminator"
    );
    // Joined with the rest, the move is nobody and the click a person.
    assert_eq!(presence_input(b"\x1b[<35;20;5M"), PresenceInput::Nobody);
    assert_eq!(
        presence_input(b"\x1b[<0;20;5M"),
        PresenceInput::Person {
            event_end: 10,
            paste_open: false
        }
    );
    // A key before the cut-off tail is a person already; the tail is the
    // parser's to finish.
    assert_eq!(
        presence_input(b"x\x1b[<35;2"),
        PresenceInput::Person {
            event_end: 1,
            paste_open: false
        }
    );
}

/// A paste the chunk opens and does not close is a person's, and runs on:
/// `paste_end` finds where a later chunk closes it.
#[test]
fn a_paste_runs_on_past_the_chunk_that_opens_it() {
    assert_eq!(
        presence_input(b"\x1b[200~hel"),
        PresenceInput::Person {
            event_end: 9,
            paste_open: true
        }
    );
    assert_eq!(paste_end(b"lo\x1b[201~X"), Some(8));
    assert_eq!(paste_end(b"lo"), None);
}
