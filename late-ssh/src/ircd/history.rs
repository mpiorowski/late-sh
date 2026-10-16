//! IRCv3 CHATHISTORY syntax and batch framing; no database or socket work.

use chrono::{DateTime, Utc};
use irc_proto::{BatchSubCommand, Command, Message, message::Tag};
use late_core::models::irc_history::{MAX_MESSAGES, Reference, Selection};
use uuid::Uuid;

use super::replies;

pub const JOIN_MESSAGES: usize = 25;

#[derive(Debug, PartialEq, Eq)]
pub enum Request {
    Messages {
        target: String,
        selection: Selection,
        limit: usize,
    },
    Targets {
        first: DateTime<Utc>,
        second: DateTime<Utc>,
        limit: usize,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct RequestError {
    pub code: &'static str,
    pub description: &'static str,
}

fn invalid(description: &'static str) -> RequestError {
    RequestError {
        code: "INVALID_PARAMS",
        description,
    }
}

fn timestamp(value: &str) -> Result<DateTime<Utc>, RequestError> {
    if value.len() != 24 {
        return Err(invalid("Expected timestamp=YYYY-MM-DDThh:mm:ss.sssZ"));
    }
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|_| invalid("Invalid timestamp"))?
        .with_timezone(&Utc);
    // Match the server-time format and reject offsets or submillisecond values.
    if parsed.to_rfc3339_opts(chrono::SecondsFormat::Millis, true) != value {
        return Err(invalid("Expected timestamp=YYYY-MM-DDThh:mm:ss.sssZ"));
    }
    Ok(parsed)
}

fn reference(value: &str) -> Result<Reference, RequestError> {
    if let Some(value) = value.strip_prefix("timestamp=") {
        return timestamp(value).map(Reference::Timestamp);
    }
    if let Some(value) = value.strip_prefix("msgid=") {
        return Uuid::parse_str(value)
            .map(Reference::MessageId)
            .map_err(|_| invalid("Invalid message ID"));
    }
    Err(RequestError {
        code: "INVALID_MSGREFTYPE",
        description: "Expected timestamp= or msgid=",
    })
}

pub fn parse(args: &[String]) -> Result<Request, RequestError> {
    let command = args
        .first()
        .ok_or_else(|| invalid("Missing subcommand"))?
        .to_ascii_uppercase();
    let expected = match command.as_str() {
        "LATEST" | "BEFORE" | "AFTER" | "AROUND" | "TARGETS" => 4,
        "BETWEEN" => 5,
        _ => return Err(invalid("Unknown subcommand")),
    };
    if args.len() != expected {
        return Err(invalid("Incorrect parameter count"));
    }
    let raw_limit = &args[expected - 1];
    if raw_limit.is_empty() || !raw_limit.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid("Limit must be a positive integer"));
    }
    let limit = raw_limit
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| invalid("Limit must be a positive integer"))?
        .min(MAX_MESSAGES as u64) as usize;
    if command == "TARGETS" {
        let (Reference::Timestamp(first), Reference::Timestamp(second)) =
            (reference(&args[1])?, reference(&args[2])?)
        else {
            return Err(invalid("TARGETS requires timestamps"));
        };
        return Ok(Request::Targets {
            first,
            second,
            limit,
        });
    }
    if args[1].is_empty() {
        return Err(invalid("Missing target"));
    }
    let selection = match command.as_str() {
        "LATEST" if args[2] == "*" => Selection::Latest(None),
        "LATEST" => Selection::Latest(Some(reference(&args[2])?)),
        "BEFORE" => Selection::Before(reference(&args[2])?),
        "AFTER" => Selection::After(reference(&args[2])?),
        "AROUND" => Selection::Around(reference(&args[2])?),
        "BETWEEN" => Selection::Between(reference(&args[2])?, reference(&args[3])?),
        _ => unreachable!("validated subcommand"),
    };
    Ok(Request::Messages {
        target: args[1].clone(),
        selection,
        limit,
    })
}

pub fn fail(code: &str, context: &[String], description: &str) -> Message {
    let mut args = vec!["CHATHISTORY".to_string(), code.to_string()];
    args.extend(context.iter().cloned());
    args.push(description.to_string());
    replies::server_msg(Command::Raw("FAIL".to_string(), args))
}

/// Construct the whole response only after the read succeeds. Batch IDs are
/// opaque, connection-independent UUIDs; no shared state is needed.
pub fn batch(
    mut messages: Vec<Message>,
    target: Option<&str>,
    enabled: bool,
    end_tag: bool,
) -> Vec<Message> {
    if !enabled {
        return messages;
    }
    let id = Uuid::new_v4().simple().to_string();
    let kind = if target.is_some() {
        "chathistory"
    } else {
        "draft/chathistory-targets"
    };
    let mut start = replies::server_msg(Command::BATCH(
        format!("+{id}"),
        Some(BatchSubCommand::CUSTOM(kind.to_string())),
        target.map(|name| vec![name.to_string()]),
    ));
    if end_tag {
        start.tags = Some(vec![Tag("draft/chathistory-end".to_string(), None)]);
    }
    for message in &mut messages {
        message
            .tags
            .get_or_insert_default()
            .push(Tag("batch".to_string(), Some(id.clone())));
    }
    let mut result = Vec::with_capacity(messages.len() + 2);
    result.push(start);
    result.extend(messages);
    result.push(replies::server_msg(Command::BATCH(
        format!("-{id}"),
        None,
        None,
    )));
    result
}
