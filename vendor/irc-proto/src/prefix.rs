//! A module providing an enum for a message prefix.
use std::fmt;
use std::str::FromStr;

/// The Prefix indicates "the true origin of the message", according to the server.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Prefix {
    /// servername, e.g. collins.mozilla.org
    ServerName(String),
    /// nickname [ ["!" username] "@" hostname ]
    /// i.e. Nickname(nickname, username, hostname)
    /// Any of the strings may be ""
    Nickname(String, String, String),
}

impl Prefix {
    /// Creates a prefix by parsing a string.
    ///
    /// # Example
    /// ```
    /// # extern crate irc_proto;
    /// # use irc_proto::Prefix;
    /// # fn main() {
    /// Prefix::new_from_str("nickname!username@hostname");
    /// Prefix::new_from_str("example.com");
    /// # }
    /// ```
    pub fn new_from_str(s: &str) -> Prefix {
        #[derive(Copy, Clone, Eq, PartialEq)]
        enum Active {
            Name,
            User,
            Host,
        }

        let mut name = String::new();
        let mut user = String::new();
        let mut host = String::new();
        let mut active = Active::Name;
        let mut is_server = false;

        for c in s.chars() {
            if c == '.' && active == Active::Name {
                // We won't return Nickname("nick", "", "") but if @ or ! are
                // encountered, then we set this back to false
                is_server = true;
            }

            match c {
                '!' if active == Active::Name => {
                    is_server = false;
                    active = Active::User;
                }

                '@' if active != Active::Host => {
                    is_server = false;
                    active = Active::Host;
                }

                _ => {
                    // Push onto the active buffer
                    match active {
                        Active::Name => &mut name,
                        Active::User => &mut user,
                        Active::Host => &mut host,
                    }
                    .push(c)
                }
            }
        }

        if is_server {
            Prefix::ServerName(name)
        } else {
            Prefix::Nickname(name, user, host)
        }
    }
}

/// This implementation never returns an error and is isomorphic with `Display`.
impl FromStr for Prefix {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Prefix::new_from_str(s))
    }
}

/// This is isomorphic with `FromStr`
impl fmt::Display for Prefix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Prefix::ServerName(name) => write!(f, "{}", name),
            Prefix::Nickname(name, user, host) => match (&name[..], &user[..], &host[..]) {
                ("", "", "") => write!(f, ""),
                (name, "", "") => write!(f, "{}", name),
                (name, user, "") => write!(f, "{}!{}", name, user),
                (name, "", host) => write!(f, "{}@{}", name, host),
                (name, user, host) => write!(f, "{}!{}@{}", name, user, host),
            },
        }
    }
}

impl From<&str> for Prefix {
    fn from(s: &str) -> Self {
        Prefix::new_from_str(s)
    }
}

#[cfg(test)]
mod test {
    use super::Prefix::{self, Nickname, ServerName};

    // Checks that str -> parsed -> Display doesn't lose data
    fn test_parse(s: &str) -> Prefix {
        let prefix = Prefix::new_from_str(s);
        let s2 = format!("{}", prefix);
        assert_eq!(s, &s2);
        prefix
    }

    #[test]
    fn print() {
        let s = format!("{}", Nickname("nick".into(), "".into(), "".into()));
        assert_eq!(&s, "nick");
        let s = format!("{}", Nickname("nick".into(), "user".into(), "".into()));
        assert_eq!(&s, "nick!user");
        let s = format!("{}", Nickname("nick".into(), "user".into(), "host".into()));
        assert_eq!(&s, "nick!user@host");
    }

    #[test]
    fn parse_word() {
        assert_eq!(
            test_parse("only_nick"),
            Nickname("only_nick".into(), String::new(), String::new())
        )
    }

    #[test]
    fn parse_host() {
        assert_eq!(test_parse("host.tld"), ServerName("host.tld".into()))
    }

    #[test]
    fn parse_nick_user() {
        assert_eq!(
            test_parse("test!nick"),
            Nickname("test".into(), "nick".into(), String::new())
        )
    }

    #[test]
    fn parse_nick_user_host() {
        assert_eq!(
            test_parse("test!nick@host"),
            Nickname("test".into(), "nick".into(), "host".into())
        )
    }

    #[test]
    fn parse_dot_and_symbols() {
        assert_eq!(
            test_parse("test.net@something"),
            Nickname("test.net".into(), "".into(), "something".into())
        )
    }

    #[test]
    fn parse_danger_cases() {
        assert_eq!(
            test_parse("name@name!user"),
            Nickname("name".into(), "".into(), "name!user".into())
        );
        assert_eq!(
            // can't reverse the parse
            "name!@".parse::<Prefix>().unwrap(),
            Nickname("name".into(), "".into(), "".into())
        );
        assert_eq!(
            // can't reverse the parse
            "name!@hostname".parse::<Prefix>().unwrap(),
            Nickname("name".into(), "".into(), "hostname".into())
        );
        assert_eq!(
            test_parse("name!.user"),
            Nickname("name".into(), ".user".into(), "".into())
        );
        assert_eq!(
            test_parse("name!user.user"),
            Nickname("name".into(), "user.user".into(), "".into())
        );
        assert_eq!(
            test_parse("name!user@host.host"),
            Nickname("name".into(), "user".into(), "host.host".into())
        );
        assert_eq!(
            test_parse("!user"),
            Nickname("".into(), "user".into(), "".into())
        );
        assert_eq!(
            "!@host.host".parse::<Prefix>().unwrap(),
            Nickname("".into(), "".into(), "host.host".into())
        );
    }
}
