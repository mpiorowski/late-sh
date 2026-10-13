use chrono::TimeZone;

use super::*;

const PLAYER: Uuid = Uuid::from_u128(1);
const ALICE: Uuid = Uuid::from_u128(7);

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap()
}

fn message(seconds_ago: i64, user_id: Uuid, body: &str) -> ChatMessage {
    let created = now() - Duration::seconds(seconds_ago);
    ChatMessage {
        id: Uuid::from_u128(1000 + seconds_ago as u128),
        created,
        updated: created,
        reply_to_message_id: None,
        reply_to_user_id: None,
        room_id: Uuid::from_u128(2),
        user_id,
        body: body.to_string(),
    }
}

#[test]
fn a_line_starting_with_a_prompt_is_a_command() {
    assert_eq!(command(">open mailbox"), Some("open mailbox".into()));
    assert_eq!(command("  >  go   north "), Some("go north".into()));
    assert_eq!(command("hello there"), None);
    assert_eq!(command("> "), None);
    assert_eq!(command("x > y"), None);
}

/// Only printable ASCII is typed: a control byte or an escape could reach
/// the game as a key (Esc cancels, Enter would send), so they become spaces.
#[test]
fn a_command_carries_nothing_the_game_could_read_as_a_key() {
    assert_eq!(command(">take\x1b[Alamp\r\n"), Some("take [Alamp".into()));
    assert_eq!(command(">eat \u{1f382} cake"), Some("eat cake".into()));
    let long = format!(">{}", "x".repeat(COMMAND_MAX_CHARS + 10));
    assert_eq!(
        command(&long).map(|c| c.chars().count()),
        Some(COMMAND_MAX_CHARS)
    );
}

/// The newest suggestion waits, even under chatter that came after it.
#[test]
fn the_newest_fresh_suggestion_from_a_watcher_waits() {
    let messages = [
        message(5, ALICE, "lol the grue"),
        message(10, ALICE, ">open mailbox"),
        message(20, ALICE, ">go north"),
    ];
    assert_eq!(
        latest(&messages, PLAYER, None, now()),
        Some(Suggestion {
            message_id: messages[1].id,
            command: "open mailbox".into(),
        })
    );
}

#[test]
fn the_players_own_lines_are_not_suggestions() {
    let messages = [
        message(5, PLAYER, ">read leaflet"),
        message(10, ALICE, ">open mailbox"),
    ];
    assert_eq!(
        latest(&messages, PLAYER, None, now()).map(|s| s.command),
        Some("open mailbox".into())
    );
}

/// Once typed, a suggestion is gone, and an older one does not take its
/// place; a stale one never waits.
#[test]
fn a_typed_or_stale_suggestion_does_not_wait() {
    let messages = [
        message(10, ALICE, ">open mailbox"),
        message(20, ALICE, ">go north"),
    ];
    assert_eq!(latest(&messages, PLAYER, Some(messages[0].id), now()), None);

    let stale = [message(11 * 60, ALICE, ">open mailbox")];
    assert_eq!(latest(&stale, PLAYER, None, now()), None);
}

fn screen(bytes: &[u8]) -> vt100::Parser {
    let mut parser = vt100::Parser::new(24, 80, 0);
    parser.process(bytes);
    parser
}

/// F3 types only onto the story's empty prompt; a half-typed command, a
/// pagination wait, or the RESTORE confirmation would read the letters as
/// something else.
#[test]
fn only_an_empty_prompt_takes_a_suggestion() {
    let story = b"\x1b[7m West of House   Score: 0   Moves: 1\x1b[0m\x1b[3;1HThere is a small mailbox here.\r\n\r\n";
    let at = |tail: &[u8]| at_empty_prompt(screen(&[story.as_slice(), tail].concat()).screen());
    assert!(at(b">"));
    assert!(!at(b">go no"), "the player is typing");
    assert!(!at(b"[MORE]"), "a pagination wait");
    assert!(!at(b"Restore your manual save? [y/N] "), "a confirmation");
    assert!(!at(b""), "no prompt yet");
}
