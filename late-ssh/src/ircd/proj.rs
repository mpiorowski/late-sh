//! Pure room↔channel projection helpers (no I/O).

use late_core::models::chat_room::ChatRoom;

/// Max bytes of message body per PRIVMSG line. Conservative: leaves room for
/// `:nick!nick@late.sh PRIVMSG #channel :` plus CRLF inside the 512-byte
/// line limit.
pub const PRIVMSG_CHUNK_BYTES: usize = 400;

/// Channel name for a room, if the room is exposed over IRC.
pub fn channel_name(room: &ChatRoom) -> Option<String> {
    if !is_irc_channel_kind(room) {
        return None;
    }
    room.slug.as_deref().map(|slug| format!("#{slug}"))
}

/// Whether a room kind/visibility combination is exposed as an IRC channel.
/// Game-room chat and DMs are not (FRD §6.1).
pub fn is_irc_channel_kind(room: &ChatRoom) -> bool {
    match room.kind.as_str() {
        "lounge" | "language" => true,
        "topic" => room.visibility == "public" || room.visibility == "private",
        _ => false,
    }
}

pub fn is_lounge(room: &ChatRoom) -> bool {
    room.kind == "lounge"
}

/// Normalize a channel name for case-insensitive matching (CASEMAPPING=ascii).
pub fn normalize_channel(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// Slug for a client-supplied channel name (`#foo` → `foo`).
pub fn slug_for_channel(name: &str) -> Option<&str> {
    name.strip_prefix('#').filter(|slug| !slug.is_empty())
}

/// Split a message body into IRC-sized lines: hard line breaks first, then
/// chunk long lines at UTF-8 character boundaries.
pub fn split_body(body: &str, max_bytes: usize) -> Vec<String> {
    let mut out = Vec::new();
    for line in body.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let mut rest = line;
        while !rest.is_empty() {
            if rest.len() <= max_bytes {
                out.push(rest.to_string());
                break;
            }
            let mut cut = max_bytes;
            while !rest.is_char_boundary(cut) {
                cut -= 1;
            }
            let (head, tail) = rest.split_at(cut);
            out.push(head.to_string());
            rest = tail;
        }
    }
    out
}

/// Render inbound CTCP ACTION text as the shared chat action marker.
pub fn action_to_body(action: &str) -> String {
    crate::app::chat::action::encode_action_body(action)
        .unwrap_or_else(|| action.trim().to_string())
}

/// Render a stored chat body for IRC delivery.
pub fn body_for_irc(body: &str, author: &str) -> String {
    crate::app::chat::action::parse_action_body(body)
        .map(|action| format!("* {author} {action}"))
        .unwrap_or_else(|| body.to_string())
}

/// Extract CTCP ACTION text from a PRIVMSG body, if present.
pub fn parse_ctcp_action(text: &str) -> Option<&str> {
    text.strip_prefix("\u{1}ACTION ")
        .map(|rest| rest.trim_end_matches('\u{1}'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use late_core::models::chat_room::ChatRoom;

    fn room(kind: &str, visibility: &str, slug: Option<&str>) -> ChatRoom {
        ChatRoom {
            id: uuid::Uuid::new_v4(),
            created: chrono::Utc::now(),
            updated: chrono::Utc::now(),
            kind: kind.to_string(),
            visibility: visibility.to_string(),
            auto_join: false,
            permanent: false,
            slug: slug.map(str::to_string),
            language_code: None,
            dm_user_a: None,
            dm_user_b: None,
        }
    }

    #[test]
    fn channel_names_follow_room_kinds() {
        assert_eq!(
            channel_name(&room("lounge", "public", Some("lounge"))).as_deref(),
            Some("#lounge")
        );
        assert_eq!(
            channel_name(&room("topic", "private", Some("sekrit"))).as_deref(),
            Some("#sekrit")
        );
        assert_eq!(channel_name(&room("game", "public", Some("poker-1"))), None);
        assert_eq!(channel_name(&room("dm", "dm", None)), None);
        assert_eq!(channel_name(&room("lounge", "public", None)), None);
    }

    #[test]
    fn split_body_respects_newlines_and_utf8_boundaries() {
        assert_eq!(split_body("a\nb", 400), vec!["a", "b"]);
        assert_eq!(split_body("", 400), Vec::<String>::new());
        // multi-byte chars must not be split mid-codepoint
        let body = "é".repeat(300); // 600 bytes
        let lines = split_body(&body, 400);
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|l| l.len() <= 400));
        assert_eq!(lines.join(""), body);
    }

    #[test]
    fn ctcp_action_round_trip() {
        assert_eq!(parse_ctcp_action("\u{1}ACTION waves\u{1}"), Some("waves"));
        assert_eq!(parse_ctcp_action("hello"), None);
        assert_eq!(action_to_body("waves"), "\u{1}ACTION waves\u{1}");
    }

    #[test]
    fn channel_lookup_helpers() {
        assert_eq!(slug_for_channel("#lounge"), Some("lounge"));
        assert_eq!(slug_for_channel("lounge"), None);
        assert_eq!(slug_for_channel("#"), None);
        assert_eq!(normalize_channel("#LOUNGE"), "#lounge");
    }
}
