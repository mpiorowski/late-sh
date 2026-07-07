//! A module providing a data structure for messages to and from IRC servers.
use std::borrow::ToOwned;
use std::fmt::{Display, Formatter, Result as FmtResult, Write};
use std::str::FromStr;

use crate::chan::ChannelExt;
use crate::command::Command;
use crate::error;
use crate::error::{MessageParseError, ProtocolError};
use crate::prefix::Prefix;

/// A data structure representing an IRC message according to the protocol specification. It
/// consists of a collection of IRCv3 tags, a prefix (describing the source of the message), and
/// the protocol command. If the command is unknown, it is treated as a special raw command that
/// consists of a collection of arguments and the special suffix argument. Otherwise, the command
/// is parsed into a more useful form as described in [`Command`].
#[derive(Clone, PartialEq, Debug)]
pub struct Message {
    /// Message tags as defined by [IRCv3.2](http://ircv3.net/specs/core/message-tags-3.2.html).
    /// These tags are used to add extended information to the given message, and are commonly used
    /// in IRCv3 extensions to the IRC protocol.
    pub tags: Option<Vec<Tag>>,
    /// The message prefix (or source) as defined by [RFC 2812](http://tools.ietf.org/html/rfc2812).
    pub prefix: Option<Prefix>,
    /// The IRC command, parsed according to the known specifications. The command itself and its
    /// arguments (including the special suffix argument) are captured in this component.
    pub command: Command,
}

impl Message {
    /// Creates a new message from the given components.
    ///
    /// # Example
    /// ```
    /// # extern crate irc_proto;
    /// # use irc_proto::Message;
    /// # fn main() {
    /// let message = Message::new(
    ///     Some("nickname!username@hostname"), "JOIN", vec!["#channel"]
    /// ).unwrap();
    /// # }
    /// ```
    pub fn new(
        prefix: Option<&str>,
        command: &str,
        args: Vec<&str>,
    ) -> Result<Message, MessageParseError> {
        Message::with_tags(None, prefix, command, args)
    }

    /// Creates a new IRCv3.2 message from the given components, including message tags. These tags
    /// are used to add extended information to the given message, and are commonly used in IRCv3
    /// extensions to the IRC protocol.
    pub fn with_tags(
        tags: Option<Vec<Tag>>,
        prefix: Option<&str>,
        command: &str,
        args: Vec<&str>,
    ) -> Result<Message, error::MessageParseError> {
        Ok(Message {
            tags,
            prefix: prefix.map(|p| p.into()),
            command: Command::new(command, args)?,
        })
    }

    /// Gets the nickname of the message source, if it exists.
    ///
    /// # Example
    /// ```
    /// # extern crate irc_proto;
    /// # use irc_proto::Message;
    /// # fn main() {
    /// let message = Message::new(
    ///     Some("nickname!username@hostname"), "JOIN", vec!["#channel"]
    /// ).unwrap();
    /// assert_eq!(message.source_nickname(), Some("nickname"));
    /// # }
    /// ```
    pub fn source_nickname(&self) -> Option<&str> {
        // <prefix> ::= <servername> | <nick> [ '!' <user> ] [ '@' <host> ]
        // <servername> ::= <host>
        self.prefix.as_ref().and_then(|p| match p {
            Prefix::Nickname(name, _, _) => Some(&name[..]),
            _ => None,
        })
    }

    /// Gets the likely intended place to respond to this message.
    /// If the type of the message is a `PRIVMSG` or `NOTICE` and the message is sent to a channel,
    /// the result will be that channel. In all other cases, this will call `source_nickname`.
    ///
    /// # Example
    /// ```
    /// # extern crate irc_proto;
    /// # use irc_proto::Message;
    /// # fn main() {
    /// let msg1 = Message::new(
    ///     Some("ada"), "PRIVMSG", vec!["#channel", "Hi, everyone!"]
    /// ).unwrap();
    /// assert_eq!(msg1.response_target(), Some("#channel"));
    /// let msg2 = Message::new(
    ///     Some("ada"), "PRIVMSG", vec!["betsy", "betsy: hi"]
    /// ).unwrap();
    /// assert_eq!(msg2.response_target(), Some("ada"));
    /// # }
    /// ```
    pub fn response_target(&self) -> Option<&str> {
        match self.command {
            Command::PRIVMSG(ref target, _) if target.is_channel_name() => Some(target),
            Command::NOTICE(ref target, _) if target.is_channel_name() => Some(target),
            _ => self.source_nickname(),
        }
    }
}

impl From<Command> for Message {
    fn from(cmd: Command) -> Message {
        Message {
            tags: None,
            prefix: None,
            command: cmd,
        }
    }
}

impl FromStr for Message {
    type Err = ProtocolError;

    fn from_str(s: &str) -> Result<Message, Self::Err> {
        if s.is_empty() {
            return Err(ProtocolError::InvalidMessage {
                string: s.to_owned(),
                cause: MessageParseError::EmptyMessage,
            });
        }

        let mut state = s;

        let tags = if state.starts_with('@') {
            let tags = state.find(' ').map(|i| &state[1..i]);
            state = state.find(' ').map_or("", |i| &state[i + 1..]);
            tags.map(|ts| {
                ts.split(';')
                    .filter(|s| !s.is_empty())
                    .map(|s: &str| {
                        let mut iter = s.splitn(2, '=');
                        let (fst, snd) = (iter.next(), iter.next());
                        let snd = snd.map(unescape_tag_value);
                        Tag(fst.unwrap_or("").to_owned(), snd)
                    })
                    .collect::<Vec<_>>()
            })
        } else {
            None
        };

        let prefix = if state.starts_with(':') {
            let prefix = state.find(' ').map(|i| &state[1..i]);
            state = state.find(' ').map_or("", |i| &state[i + 1..]);
            prefix
        } else {
            None
        };

        let line_ending_len = if state.ends_with("\r\n") {
            "\r\n"
        } else if state.ends_with('\r') {
            "\r"
        } else if state.ends_with('\n') {
            "\n"
        } else {
            ""
        }
        .len();

        let suffix = if state.contains(" :") {
            let suffix = state
                .find(" :")
                .map(|i| &state[i + 2..state.len() - line_ending_len]);
            state = state.find(" :").map_or("", |i| &state[..i + 1]);
            suffix
        } else {
            state = &state[..state.len() - line_ending_len];
            None
        };

        let command = match state.find(' ').map(|i| &state[..i]) {
            Some(cmd) => {
                state = state.find(' ').map_or("", |i| &state[i + 1..]);
                cmd
            }
            // If there's no arguments but the "command" starts with colon, it's not a command.
            None if state.starts_with(':') => {
                return Err(ProtocolError::InvalidMessage {
                    string: s.to_owned(),
                    cause: MessageParseError::InvalidCommand,
                })
            }
            // If there's no arguments following the command, the rest of the state is the command.
            None => {
                let cmd = state;
                state = "";
                cmd
            }
        };

        let mut args: Vec<_> = state.splitn(14, ' ').filter(|s| !s.is_empty()).collect();
        if let Some(suffix) = suffix {
            args.push(suffix);
        }

        Message::with_tags(tags, prefix, command, args).map_err(|e| ProtocolError::InvalidMessage {
            string: s.to_owned(),
            cause: e,
        })
    }
}

impl<'a> From<&'a str> for Message {
    fn from(s: &'a str) -> Message {
        s.parse().unwrap()
    }
}

impl Display for Message {
    /// Converts a Message into a String according to the IRC protocol.
    ///
    /// # Example
    /// ```
    /// # extern crate irc_proto;
    /// # use irc_proto::Message;
    /// # fn main() {
    /// let msg = Message::new(
    ///     Some("ada"), "PRIVMSG", vec!["#channel", "Hi, everyone!"]
    /// ).unwrap();
    /// assert_eq!(msg.to_string(), ":ada PRIVMSG #channel :Hi, everyone!\r\n");
    /// # }
    /// ```
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        if let Some(ref tags) = self.tags {
            f.write_char('@')?;
            for (i, tag) in tags.iter().enumerate() {
                if i > 0 {
                    f.write_char(';')?;
                }
                f.write_str(&tag.0)?;
                if let Some(ref value) = tag.1 {
                    f.write_char('=')?;
                    escape_tag_value(f, value)?;
                }
            }
            f.write_char(' ')?;
        }
        if let Some(ref prefix) = self.prefix {
            write!(f, ":{} ", prefix)?
        }
        write!(f, "{}\r\n", String::from(&self.command))
    }
}

/// A message tag as defined by [IRCv3.2](http://ircv3.net/specs/core/message-tags-3.2.html).
/// It consists of a tag key, and an optional value for the tag. Each message can contain a number
/// of tags (in the string format, they are separated by semicolons). Tags are used to add extended
/// information to a message under IRCv3.
#[derive(Clone, PartialEq, Debug)]
pub struct Tag(pub String, pub Option<String>);

fn escape_tag_value(f: &mut dyn Write, value: &str) -> FmtResult {
    for c in value.chars() {
        match c {
            ';' => f.write_str("\\:")?,
            ' ' => f.write_str("\\s")?,
            '\\' => f.write_str("\\\\")?,
            '\r' => f.write_str("\\r")?,
            '\n' => f.write_str("\\n")?,
            c => f.write_char(c)?,
        }
    }
    Ok(())
}

fn unescape_tag_value(value: &str) -> String {
    let mut unescaped = String::with_capacity(value.len());
    let mut iter = value.chars();
    while let Some(c) = iter.next() {
        let r = if c == '\\' {
            match iter.next() {
                Some(':') => ';',
                Some('s') => ' ',
                Some('\\') => '\\',
                Some('r') => '\r',
                Some('n') => '\n',
                Some(c) => c,
                None => break,
            }
        } else {
            c
        };
        unescaped.push(r);
    }
    unescaped
}

#[cfg(test)]
mod test {
    use super::{Message, Tag};
    use crate::command::Command::{Raw, PRIVMSG, QUIT};

    #[test]
    fn new() {
        let message = Message {
            tags: None,
            prefix: None,
            command: PRIVMSG("test".to_string(), "Testing!".to_string()),
        };
        assert_eq!(
            Message::new(None, "PRIVMSG", vec!["test", "Testing!"]).unwrap(),
            message
        )
    }

    #[test]
    fn source_nickname() {
        assert_eq!(
            Message::new(None, "PING", vec!["data"])
                .unwrap()
                .source_nickname(),
            None
        );

        assert_eq!(
            Message::new(Some("irc.test.net"), "PING", vec!["data"])
                .unwrap()
                .source_nickname(),
            None
        );

        assert_eq!(
            Message::new(Some("test!test@test"), "PING", vec!["data"])
                .unwrap()
                .source_nickname(),
            Some("test")
        );

        assert_eq!(
            Message::new(Some("test@test"), "PING", vec!["data"])
                .unwrap()
                .source_nickname(),
            Some("test")
        );

        assert_eq!(
            Message::new(Some("test!test@irc.test.com"), "PING", vec!["data"])
                .unwrap()
                .source_nickname(),
            Some("test")
        );

        assert_eq!(
            Message::new(Some("test!test@127.0.0.1"), "PING", vec!["data"])
                .unwrap()
                .source_nickname(),
            Some("test")
        );

        assert_eq!(
            Message::new(Some("test@test.com"), "PING", vec!["data"])
                .unwrap()
                .source_nickname(),
            Some("test")
        );

        assert_eq!(
            Message::new(Some("test"), "PING", vec!["data"])
                .unwrap()
                .source_nickname(),
            Some("test")
        );
    }

    #[test]
    fn to_string() {
        let message = Message {
            tags: None,
            prefix: None,
            command: PRIVMSG("test".to_string(), "Testing!".to_string()),
        };
        assert_eq!(&message.to_string()[..], "PRIVMSG test Testing!\r\n");
        let message = Message {
            tags: None,
            prefix: Some("test!test@test".into()),
            command: PRIVMSG("test".to_string(), "Still testing!".to_string()),
        };
        assert_eq!(
            &message.to_string()[..],
            ":test!test@test PRIVMSG test :Still testing!\r\n"
        );
    }

    #[test]
    fn from_string() {
        let message = Message {
            tags: None,
            prefix: None,
            command: PRIVMSG("test".to_string(), "Testing!".to_string()),
        };
        assert_eq!(
            "PRIVMSG test :Testing!\r\n".parse::<Message>().unwrap(),
            message
        );
        let message = Message {
            tags: None,
            prefix: Some("test!test@test".into()),
            command: PRIVMSG("test".to_string(), "Still testing!".to_string()),
        };
        assert_eq!(
            ":test!test@test PRIVMSG test :Still testing!\r\n"
                .parse::<Message>()
                .unwrap(),
            message
        );
        let message = Message {
            tags: Some(vec![
                Tag("aaa".to_string(), Some("bbb".to_string())),
                Tag("ccc".to_string(), None),
                Tag("example.com/ddd".to_string(), Some("eee".to_string())),
            ]),
            prefix: Some("test!test@test".into()),
            command: PRIVMSG("test".to_string(), "Testing with tags!".to_string()),
        };
        assert_eq!(
            "@aaa=bbb;ccc;example.com/ddd=eee :test!test@test PRIVMSG test :Testing with \
             tags!\r\n"
                .parse::<Message>()
                .unwrap(),
            message
        )
    }

    #[test]
    fn from_string_atypical_endings() {
        let message = Message {
            tags: None,
            prefix: None,
            command: PRIVMSG("test".to_string(), "Testing!".to_string()),
        };
        assert_eq!(
            "PRIVMSG test :Testing!\r".parse::<Message>().unwrap(),
            message
        );
        assert_eq!(
            "PRIVMSG test :Testing!\n".parse::<Message>().unwrap(),
            message
        );
        assert_eq!(
            "PRIVMSG test :Testing!".parse::<Message>().unwrap(),
            message
        );
    }

    #[test]
    fn from_and_to_string() {
        let message =
            "@aaa=bbb;ccc;example.com/ddd=eee :test!test@test PRIVMSG test :Testing with \
             tags!\r\n";
        assert_eq!(message.parse::<Message>().unwrap().to_string(), message);
    }

    #[test]
    fn to_message() {
        let message = Message {
            tags: None,
            prefix: None,
            command: PRIVMSG("test".to_string(), "Testing!".to_string()),
        };
        let msg: Message = "PRIVMSG test :Testing!\r\n".into();
        assert_eq!(msg, message);
        let message = Message {
            tags: None,
            prefix: Some("test!test@test".into()),
            command: PRIVMSG("test".to_string(), "Still testing!".to_string()),
        };
        let msg: Message = ":test!test@test PRIVMSG test :Still testing!\r\n".into();
        assert_eq!(msg, message);
    }

    #[test]
    fn to_message_with_colon_in_arg() {
        // Apparently, UnrealIRCd (and perhaps some others) send some messages that include
        // colons within individual parameters. So, let's make sure it parses correctly.
        let message = Message {
            tags: None,
            prefix: Some("test!test@test".into()),
            command: Raw(
                "COMMAND".to_string(),
                vec!["ARG:test".to_string(), "Testing!".to_string()],
            ),
        };
        let msg: Message = ":test!test@test COMMAND ARG:test :Testing!\r\n".into();
        assert_eq!(msg, message);
    }

    #[test]
    fn to_message_no_prefix_no_args() {
        let message = Message {
            tags: None,
            prefix: None,
            command: QUIT(None),
        };
        let msg: Message = "QUIT\r\n".into();
        assert_eq!(msg, message);
    }

    #[test]
    #[should_panic]
    fn to_message_invalid_format() {
        let _: Message = ":invalid :message".into();
    }

    #[test]
    fn to_message_tags_escapes() {
        let msg = "@tag=\\:\\s\\\\\\r\\n\\a\\ :test PRIVMSG #test :test\r\n"
            .parse::<Message>()
            .unwrap();
        let message = Message {
            tags: Some(vec![Tag("tag".to_string(), Some("; \\\r\na".to_string()))]),
            prefix: Some("test".into()),
            command: PRIVMSG("#test".to_string(), "test".to_string()),
        };
        assert_eq!(msg, message);
    }

    #[test]
    fn to_string_tags_escapes() {
        let msg = Message {
            tags: Some(vec![Tag("tag".to_string(), Some("; \\\r\na".to_string()))]),
            prefix: Some("test".into()),
            command: PRIVMSG("#test".to_string(), "test".to_string()),
        }
        .to_string();
        let message = "@tag=\\:\\s\\\\\\r\\na :test PRIVMSG #test test\r\n";
        assert_eq!(msg, message);
    }

    #[test]
    fn to_message_with_colon_in_suffix() {
        let msg = "PRIVMSG #test ::test".parse::<Message>().unwrap();
        let message = Message {
            tags: None,
            prefix: None,
            command: PRIVMSG("#test".to_string(), ":test".to_string()),
        };
        assert_eq!(msg, message);
    }

    #[test]
    fn to_string_with_colon_in_suffix() {
        let msg = Message {
            tags: None,
            prefix: None,
            command: PRIVMSG("#test".to_string(), ":test".to_string()),
        }
        .to_string();
        let message = "PRIVMSG #test ::test\r\n";
        assert_eq!(msg, message);
    }
}
