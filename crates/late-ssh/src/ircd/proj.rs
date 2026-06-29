//! Pure room↔channel projection helpers (no I/O).

use late_core::models::chat_room::ChatRoom;

/// Max bytes of message body per PRIVMSG line. Conservative: leaves room for
/// `:nick!nick@late.sh PRIVMSG #channel :` plus CRLF inside the 512-byte
/// line limit.
pub const PRIVMSG_CHUNK_BYTES: usize = 400;

/// IRC-visible nick for a canonical late.sh username.
///
/// late.sh allows `.` in usernames, but IRC nicks do not. late.sh does not
/// allow `^`, so this is a reversible one-character projection.
pub fn nick_for_username(username: &str) -> String {
    username.replace('.', "^")
}

/// Canonical late.sh username implied by an IRC-visible nick.
pub fn username_for_nick(nick: &str) -> String {
    nick.replace('^', ".")
}

/// Rewrite a leading late.sh username mention in a chat payload to an IRC nick.
///
/// Only the first token is considered: optional leading whitespace, optional
/// `@`, 1-32 late.sh username characters, then whitespace, end, `:`, or `,`.
pub fn rewrite_leading_mention_for_irc(
    body: &str,
    mut canonical_username: impl FnMut(&str) -> Option<String>,
) -> String {
    rewrite_leading_mention(body, is_late_username_char, |maybe_username| {
        canonical_username(maybe_username).map(|username| nick_for_username(&username))
    })
}

/// Rewrite a leading IRC nick mention in a chat payload to a late.sh username.
pub fn rewrite_leading_mention_for_late(
    body: &str,
    mut canonical_username_for_nick: impl FnMut(&str) -> Option<String>,
) -> String {
    rewrite_leading_mention(body, is_irc_nick_char, |maybe_nick| {
        canonical_username_for_nick(maybe_nick)
    })
}

fn rewrite_leading_mention(
    body: &str,
    is_candidate_char: impl Fn(char) -> bool,
    mut replacement: impl FnMut(&str) -> Option<String>,
) -> String {
    let mut cursor = 0;
    for (idx, ch) in body.char_indices() {
        if ch.is_whitespace() {
            cursor = idx + ch.len_utf8();
        } else {
            break;
        }
    }

    if body[cursor..].starts_with('@') {
        cursor += 1;
    }

    let name_start = cursor;
    let mut name_end = cursor;
    let mut name_len = 0usize;
    for (offset, ch) in body[name_start..].char_indices() {
        if name_len >= 32 || !is_candidate_char(ch) {
            break;
        }
        name_len += 1;
        name_end = name_start + offset + ch.len_utf8();
    }

    if name_len == 0 {
        return body.to_string();
    }

    let delimiter_ok = body[name_end..]
        .chars()
        .next()
        .is_none_or(|ch| ch.is_whitespace() || matches!(ch, ':' | ','));
    if !delimiter_ok {
        return body.to_string();
    }

    let maybe_username = &body[name_start..name_end];
    let Some(replacement) = replacement(maybe_username) else {
        return body.to_string();
    };

    format!(
        "{}{}{}",
        &body[..name_start],
        replacement,
        &body[name_end..]
    )
}

fn is_late_username_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-')
}

fn is_irc_nick_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '^' | '_' | '-')
}

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

    #[test]
    fn nick_projection_substitutes_dots_reversibly() {
        assert_eq!(nick_for_username("alice.smith"), "alice^smith");
        assert_eq!(nick_for_username(".alice."), "^alice^");
        assert_eq!(username_for_nick("alice^smith"), "alice.smith");
        assert_eq!(username_for_nick("^alice^"), ".alice.");
    }

    #[test]
    fn leading_mentions_are_projected_to_irc_nicks() {
        let usernames = ["alice.smith", "Bob.Dot"];
        let lookup = |candidate: &str| {
            usernames
                .iter()
                .find(|username| username.eq_ignore_ascii_case(candidate))
                .map(|username| (*username).to_string())
        };

        assert_eq!(
            rewrite_leading_mention_for_irc("@alice.smith hello", lookup),
            "@alice^smith hello"
        );
        assert_eq!(
            rewrite_leading_mention_for_irc("  Bob.Dot: hello", lookup),
            "  Bob^Dot: hello"
        );
        assert_eq!(
            rewrite_leading_mention_for_irc("alice.smith, hello", lookup),
            "alice^smith, hello"
        );
    }

    #[test]
    fn leading_mentions_are_projected_to_late_usernames() {
        let usernames = ["alice.smith", "Bob.Dot"];
        let lookup = |candidate: &str| {
            usernames
                .iter()
                .find(|username| nick_for_username(username).eq_ignore_ascii_case(candidate))
                .map(|username| (*username).to_string())
        };

        assert_eq!(
            rewrite_leading_mention_for_late("@alice^smith hello", lookup),
            "@alice.smith hello"
        );
        assert_eq!(
            rewrite_leading_mention_for_late("  Bob^Dot: hello", lookup),
            "  Bob.Dot: hello"
        );
        assert_eq!(
            rewrite_leading_mention_for_late("alice^smith, hello", lookup),
            "alice.smith, hello"
        );
    }

    #[test]
    fn leading_mention_projection_ignores_non_matches() {
        let lookup = |candidate: &str| (candidate == "alice.smith").then(|| candidate.to_string());

        assert_eq!(
            rewrite_leading_mention_for_irc("hello @alice.smith", lookup),
            "hello @alice.smith"
        );
        assert_eq!(
            rewrite_leading_mention_for_irc("@alice.smithsonian hello", lookup),
            "@alice.smithsonian hello"
        );
        assert_eq!(
            rewrite_leading_mention_for_irc("@missing.user hello", lookup),
            "@missing.user hello"
        );
    }
}
