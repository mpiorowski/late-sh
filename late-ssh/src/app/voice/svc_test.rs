use super::*;
use serde_json::Value;

const ROOM: Uuid = Uuid::from_u128(0x1234);

fn enabled_service() -> VoiceService {
    VoiceService::new(
        VoiceConfig::enabled(
            "ws://localhost:7880".to_string(),
            "devkey".to_string(),
            "secret".to_string(),
            "late-voice".to_string(),
        )
        .expect("voice config"),
    )
}

fn claims_from_token(token: &str) -> Value {
    let payload = token.split('.').nth(1).expect("jwt payload");
    let bytes = URL_SAFE_NO_PAD
        .decode(payload.as_bytes())
        .expect("decode payload");
    serde_json::from_slice(&bytes).expect("claims json")
}

#[test]
fn join_ticket_targets_the_rooms_livekit_channel() {
    let service = enabled_service();
    let ticket = service
        .join_ticket(ROOM, Uuid::from_u128(1), "alice", true, false)
        .expect("join ticket");
    let claims = claims_from_token(&ticket.token);

    assert_eq!(ticket.room, format!("late-voice-{ROOM}"));
    assert_eq!(claims["video"]["room"], ticket.room);
    assert_eq!(claims["video"]["roomCreate"], false);
    assert_eq!(claims["video"]["roomJoin"], true);
    assert_eq!(claims["video"]["canPublish"], true);
    assert_eq!(claims["video"]["canSubscribe"], true);
}

#[test]
fn round_trips_the_room_id_through_the_livekit_name() {
    let service = enabled_service();
    let name = service.livekit_room_name(ROOM);
    assert_eq!(service.room_id_from_livekit(&name), Some(ROOM));
    assert_eq!(service.room_id_from_livekit("some-other-room"), None);
}

#[test]
fn presence_is_keyed_per_room() {
    let service = enabled_service();
    let _rx = service.subscribe();
    let room_a = Uuid::from_u128(0xa);
    let room_b = Uuid::from_u128(0xb);
    let user = Uuid::from_u128(1);

    service.update_local_state(room_a, user, "ali".to_string(), false, false, true);
    assert!(service.snapshot().participant(room_a, user).is_some());
    assert!(service.snapshot().participant(room_b, user).is_none());

    // Joining another room moves the user, never duplicates them.
    service.update_local_state(room_b, user, "ali".to_string(), false, false, true);
    assert!(service.snapshot().participant(room_a, user).is_none());
    assert!(service.snapshot().participant(room_b, user).is_some());
    assert_eq!(service.snapshot().current_room(user), Some(room_b));
}

#[test]
fn kicked_user_is_denied_a_join_ticket_until_allowed() {
    let service = enabled_service();
    let user = Uuid::from_u128(7);

    assert!(
        service
            .join_ticket(ROOM, user, "spammer", true, false)
            .is_ok()
    );
    assert!(service.kick(user).changed);
    assert!(service.is_blocked(user));
    // The token gate is one layer: no new ticket means no fresh LiveKit access.
    assert!(
        service
            .join_ticket(ROOM, user, "spammer", true, false)
            .is_err()
    );

    assert!(service.allow(user));
    assert!(!service.is_blocked(user));
    assert!(
        service
            .join_ticket(ROOM, user, "spammer", true, false)
            .is_ok()
    );
}

#[test]
fn kick_removes_a_present_participant_and_reports_their_room() {
    let service = enabled_service();
    let _rx = service.subscribe();
    let user = Uuid::from_u128(9);

    service.update_local_state(ROOM, user, "noisy".to_string(), false, false, true);
    assert!(service.snapshot().participant(ROOM, user).is_some());

    let outcome = service.kick(user);
    assert!(outcome.changed);
    // The reported room lets the caller force-disconnect via the server API.
    assert_eq!(outcome.livekit_room, Some(service.livekit_room_name(ROOM)));
    assert!(service.snapshot().participant(ROOM, user).is_none());

    // A blocked client that keeps reporting presence is dropped, not re-added.
    service.update_local_state(ROOM, user, "noisy".to_string(), false, false, true);
    assert!(service.snapshot().participant(ROOM, user).is_none());
}

#[test]
fn livekit_http_base_maps_ws_schemes() {
    assert_eq!(
        livekit_http_base("ws://localhost:7880").unwrap(),
        "http://localhost:7880"
    );
    assert_eq!(
        livekit_http_base("wss://lk.example.com/").unwrap(),
        "https://lk.example.com"
    );
    assert_eq!(
        livekit_http_base("https://lk.example.com").unwrap(),
        "https://lk.example.com"
    );
    assert!(livekit_http_base("ftp://nope").is_err());
}
