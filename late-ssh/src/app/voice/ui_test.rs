use super::*;
use chrono::Utc;
use uuid::Uuid;

fn participant(muted: bool, deafened: bool, speaking: bool) -> VoiceParticipant {
    VoiceParticipant {
        user_id: Uuid::nil(),
        username: "tester".to_string(),
        muted,
        deafened,
        speaking,
        updated_at: Utc::now(),
    }
}

#[test]
fn presence_priority_is_deafened_then_muted_then_speaking() {
    // Deafened outranks everything, even an erroneously-set speaking flag.
    assert_eq!(
        Presence::of(&participant(true, true, true)),
        Presence::Deafened
    );
    // Muted outranks speaking.
    assert_eq!(
        Presence::of(&participant(true, false, true)),
        Presence::Muted
    );
    // Speaking shows over plain listening.
    assert_eq!(
        Presence::of(&participant(false, false, true)),
        Presence::Speaking
    );
    // Joined, mic on, silent => listening.
    assert_eq!(
        Presence::of(&participant(false, false, false)),
        Presence::Listening
    );
}

#[test]
fn every_presence_has_a_distinct_icon_and_label() {
    let all = [
        Presence::Speaking,
        Presence::Listening,
        Presence::Muted,
        Presence::Deafened,
    ];
    for (i, a) in all.iter().enumerate() {
        for b in all.iter().skip(i + 1) {
            assert_ne!(a.icon(), b.icon(), "icons must be distinct");
            assert_ne!(a.label(), b.label(), "labels must be distinct");
        }
    }
}

#[test]
fn global_voice_badge_uses_current_room_and_status() {
    let room_id = Uuid::from_u128(42);
    let user_id = Uuid::from_u128(7);
    let snapshot = VoiceSnapshot {
        enabled: true,
        livekit_url: Some("wss://voice.example".to_string()),
        rooms: [(
            room_id,
            vec![VoiceParticipant {
                user_id,
                username: "tester".to_string(),
                muted: true,
                deafened: false,
                speaking: false,
                updated_at: Utc::now(),
            }],
        )]
        .into_iter()
        .collect(),
    };

    let badge = global_voice_badge(&snapshot, user_id, |_| Some("#lounge".to_string()));
    assert_eq!(badge.as_deref(), Some(" mic #lounge [muted] "));
}

#[test]
fn voice_row_is_one_line_with_the_keys_flushed_right() {
    let room_id = Uuid::from_u128(42);
    let snapshot = VoiceSnapshot {
        enabled: true,
        livekit_url: Some("wss://voice.example".to_string()),
        rooms: std::collections::HashMap::new(),
    };
    let view = VoiceRoomView {
        snapshot: &snapshot,
        room_id,
        current_user_id: Uuid::from_u128(7),
        paired_cli_supports_voice: true,
    };

    let line = voice_strip_line(&view, 70);
    let rendered: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert!(
        rendered.starts_with("No one is in voice yet."),
        "who is here reads from the left: {rendered}"
    );
    assert!(
        rendered.trim_end().ends_with("/voice"),
        "the keys sit at the right edge: {rendered}"
    );
    assert_eq!(
        unicode_width::UnicodeWidthStr::width(rendered.as_str()),
        70,
        "the row fills the width exactly, so the hint lands on the edge"
    );

    // Too narrow for both: the status wins and the hint drops rather than wrap.
    let narrow: String = voice_strip_line(&view, 30)
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();
    assert_eq!(narrow, "No one is in voice yet.");
}
