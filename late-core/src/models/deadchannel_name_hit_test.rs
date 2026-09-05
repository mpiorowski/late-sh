use uuid::Uuid;

use super::{NameHitSignal, parse_name_hit_payload};

fn signal() -> NameHitSignal {
    NameHitSignal {
        message_id: Uuid::now_v7(),
        room_id: Uuid::now_v7(),
        user_id: Uuid::now_v7(),
        seed: 0xDEAD_BEEF_CAFE_F00D,
    }
}

#[test]
fn a_beat_survives_the_wire() {
    let sent = signal();
    assert_eq!(parse_name_hit_payload(&sent.to_payload()), Some(sent));
}

#[test]
fn junk_on_the_channel_is_not_a_beat() {
    let sent = signal();
    let payload = sent.to_payload();
    for junk in [
        "",
        "not-a-uuid:x:y:1",
        // A field short, and one too many: both are somebody else writing
        // on our channel, and neither may paint a name.
        &payload[..payload.rfind(':').unwrap()],
        &format!("{payload}:extra"),
    ] {
        assert_eq!(parse_name_hit_payload(junk), None, "payload: {junk}");
    }
}
