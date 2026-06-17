//! Message construction helpers for server-originated IRC lines.

use irc_proto::{Command, Message, Prefix, Response, message::Tag};

/// Name this server identifies as in prefixes and numerics.
pub const SERVER_NAME: &str = "irc.late.sh";
/// NETWORK= value advertised in ISUPPORT.
pub const NETWORK_NAME: &str = "late.sh";
/// Synthetic hostname used in all user prefixes; never expose client IPs.
pub const USER_HOSTNAME: &str = "late.sh";
/// VERSION reply string.
pub const VERSION_STRING: &str = concat!("late-ssh-ircd-", env!("CARGO_PKG_VERSION"));

pub fn server_msg(command: Command) -> Message {
    Message {
        tags: None,
        prefix: Some(Prefix::ServerName(SERVER_NAME.to_string())),
        command,
    }
}

/// A numeric reply; the client's nick is always the first parameter.
pub fn numeric(nick: &str, response: Response, rest: Vec<String>) -> Message {
    let mut args = Vec::with_capacity(rest.len() + 1);
    args.push(nick.to_string());
    args.extend(rest);
    server_msg(Command::Response(response, args))
}

pub fn user_prefix(nick: &str) -> Prefix {
    Prefix::Nickname(
        nick.to_string(),
        nick.to_string(),
        USER_HOSTNAME.to_string(),
    )
}

/// A message that appears to come from a user (JOIN/PART/PRIVMSG/KICK...).
pub fn from_user(nick: &str, command: Command) -> Message {
    Message {
        tags: None,
        prefix: Some(user_prefix(nick)),
        command,
    }
}

pub fn from_user_with_tags(nick: &str, command: Command, tags: Vec<Tag>) -> Message {
    Message {
        tags: (!tags.is_empty()).then_some(tags),
        prefix: Some(user_prefix(nick)),
        command,
    }
}

pub fn server_notice(nick: &str, text: impl Into<String>) -> Message {
    server_msg(Command::NOTICE(nick.to_string(), text.into()))
}

/// ERROR line sent immediately before closing a connection.
pub fn error(text: impl Into<String>) -> Message {
    Message {
        tags: None,
        prefix: None,
        command: Command::ERROR(text.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_puts_nick_first_and_prefixes_server() {
        let msg = numeric(
            "alice",
            Response::RPL_WELCOME,
            vec!["Welcome to late.sh, alice".to_string()],
        );
        assert_eq!(
            msg.to_string().trim_end(),
            ":irc.late.sh 001 alice :Welcome to late.sh, alice"
        );
    }

    #[test]
    fn from_user_builds_full_prefix() {
        let msg = from_user("alice", Command::JOIN("#lounge".to_string(), None, None));
        assert_eq!(
            msg.to_string().trim_end(),
            ":alice!alice@late.sh JOIN #lounge"
        );
    }

    #[test]
    fn error_has_no_prefix() {
        assert_eq!(
            error("Closing Link").to_string().trim_end(),
            "ERROR :Closing Link"
        );
    }
}
