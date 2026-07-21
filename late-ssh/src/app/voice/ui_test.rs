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
