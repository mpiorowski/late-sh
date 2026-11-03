//! Pure room↔channel projection helpers (no I/O).

use std::collections::HashMap;

use irc_proto::message::Tag;
use late_core::models::chat_room::ChatRoom;
use uuid::Uuid;

/// Max bytes of message body per PRIVMSG line. Conservative: leaves room for
/// `:nick!nick@late.sh PRIVMSG #channel :` plus CRLF inside the 512-byte
/// line limit.
pub const PRIVMSG_CHUNK_BYTES: usize = 400;

pub fn msgid(message_id: Uuid) -> String {
    message_id.to_string()
}

pub fn server_time(created: chrono::DateTime<chrono::Utc>) -> String {
    created.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReplyTagError {
    MissingValue,
    MalformedValue,
    ConflictingValues,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactionTagAction {
    React,
    Unreact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactionTag {
    pub reply_to_message_id: Uuid,
    pub action: ReactionTagAction,
    pub icon: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReactionTagError {
    MissingReply,
    InvalidReply(ReplyTagError),
    MissingReaction,
    ConflictingReactions,
    MissingValue,
}

pub fn reply_tag(tags: Option<&[Tag]>) -> Result<Option<Uuid>, ReplyTagError> {
    let Some(tags) = tags else {
        return Ok(None);
    };
    let mut reply_to = None;
    for tag in tags
        .iter()
        .filter(|tag| matches!(tag.0.as_str(), "+reply" | "+draft/reply"))
    {
        let value = tag.1.as_deref().ok_or(ReplyTagError::MissingValue)?;
        let id = Uuid::parse_str(value).map_err(|_| ReplyTagError::MalformedValue)?;
        match reply_to {
            Some(existing) if existing != id => return Err(ReplyTagError::ConflictingValues),
            Some(_) => {}
            None => reply_to = Some(id),
        }
    }
    Ok(reply_to)
}

pub fn reaction_tag(tags: Option<&[Tag]>) -> Result<Option<ReactionTag>, ReactionTagError> {
    let Some(tags) = tags else {
        return Ok(None);
    };

    let mut reaction = None;
    for tag in tags {
        let action = match tag.0.as_str() {
            "+draft/react" => ReactionTagAction::React,
            "+draft/unreact" => ReactionTagAction::Unreact,
            _ => continue,
        };
        let value = tag.1.as_deref().ok_or(ReactionTagError::MissingValue)?;
        if reaction.is_some() {
            return Err(ReactionTagError::ConflictingReactions);
        }
        reaction = Some((action, value.to_string()));
    }

    let Some((action, icon)) = reaction else {
        return Ok(None);
    };
    let Some(reply_to_message_id) =
        reply_tag(Some(tags)).map_err(ReactionTagError::InvalidReply)?
    else {
        return Err(ReactionTagError::MissingReply);
    };

    Ok(Some(ReactionTag {
        reply_to_message_id,
        action,
        icon,
    }))
}

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
    let prefix = if room.visibility == "private" {
        "##"
    } else {
        "#"
    };
    room.slug.as_deref().map(|slug| format!("{prefix}{slug}"))
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

/// Visibility and slug for a client-supplied channel name.
/// `#foo` addresses the public room; `##foo` addresses the private room.
pub fn channel_parts(name: &str) -> Option<(&'static str, &str)> {
    let rest = name.strip_prefix('#')?;
    let (visibility, slug) = match rest.strip_prefix('#') {
        Some(slug) => ("private", slug),
        None => ("public", rest),
    };
    (!slug.is_empty() && !slug.contains('#')).then_some((visibility, slug))
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

/// A stored chat body as IRC delivers it. A `/me` line travels as a CTCP
/// ACTION, so clients draw `* nick waves` themselves instead of a plain
/// PRIVMSG that repeats the nick the prefix already carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrcBody {
    Text(String),
    Action(String),
}

const CTCP_ACTION_PREFIX: &str = "\u{1}ACTION ";
const CTCP_ACTION_SUFFIX: &str = "\u{1}";

/// Bytes the CTCP ACTION wrapper adds to every line it wraps.
pub const CTCP_ACTION_OVERHEAD: usize = CTCP_ACTION_PREFIX.len() + CTCP_ACTION_SUFFIX.len();

/// Wrap one already-split line as a CTCP ACTION.
pub fn ctcp_action(line: &str) -> String {
    format!("{CTCP_ACTION_PREFIX}{line}{CTCP_ACTION_SUFFIX}")
}

/// Render a stored chat body for IRC delivery.
pub fn body_for_irc(body: &str) -> IrcBody {
    match crate::app::chat::action::parse_action_body(body) {
        Some(action) => IrcBody::Action(action.to_string()),
        None => IrcBody::Text(body.to_string()),
    }
}

/// A stored channel message body as IRC delivers it, live or replayed: the
/// `/me` shape from `body_for_irc`, then a plain body's leading `@username`
/// mention rewritten to the matching IRC nick when the username is known.
pub fn channel_body_for_irc(body: &str, directory: &HashMap<Uuid, String>) -> IrcBody {
    match body_for_irc(body) {
        IrcBody::Text(text) => IrcBody::Text(rewrite_leading_mention_for_irc(&text, |username| {
            directory
                .values()
                .find(|candidate| candidate.eq_ignore_ascii_case(username))
                .cloned()
        })),
        IrcBody::Action(action) => IrcBody::Action(action),
    }
}

/// Extract CTCP ACTION text from a PRIVMSG body, if present.
pub fn parse_ctcp_action(text: &str) -> Option<&str> {
    text.strip_prefix(CTCP_ACTION_PREFIX)
        .map(|rest| rest.trim_end_matches(CTCP_ACTION_SUFFIX))
}
