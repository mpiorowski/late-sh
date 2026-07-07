//! A module defining an API for IRC user and channel modes.
use std::fmt;

use crate::command::Command;
use crate::error::MessageParseError;

/// A marker trait for different kinds of Modes.
pub trait ModeType: fmt::Display + fmt::Debug + Clone + PartialEq {
    /// Creates a command of this kind.
    fn mode(target: &str, modes: &[Mode<Self>]) -> Command;

    /// Returns true if this mode takes an argument, and false otherwise.
    fn takes_arg(&self) -> bool;

    /// Creates a Mode from a given char.
    fn from_char(c: char) -> Self;
}

/// User modes for the MODE command.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum UserMode {
    /// a - user is flagged as away
    Away,
    /// i - marks a users as invisible
    Invisible,
    /// w - user receives wallops
    Wallops,
    /// r - restricted user connection
    Restricted,
    /// o - operator flag
    Oper,
    /// O - local operator flag
    LocalOper,
    /// s - marks a user for receipt of server notices
    ServerNotices,
    /// x - masked hostname
    MaskedHost,

    /// Any other unknown-to-the-crate mode.
    Unknown(char),
}

impl ModeType for UserMode {
    fn mode(target: &str, modes: &[Mode<Self>]) -> Command {
        Command::UserMODE(target.to_owned(), modes.to_owned())
    }

    fn takes_arg(&self) -> bool {
        false
    }

    fn from_char(c: char) -> UserMode {
        use self::UserMode::*;

        match c {
            'a' => Away,
            'i' => Invisible,
            'w' => Wallops,
            'r' => Restricted,
            'o' => Oper,
            'O' => LocalOper,
            's' => ServerNotices,
            'x' => MaskedHost,
            _ => Unknown(c),
        }
    }
}

impl fmt::Display for UserMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::UserMode::*;

        write!(
            f,
            "{}",
            match *self {
                Away => 'a',
                Invisible => 'i',
                Wallops => 'w',
                Restricted => 'r',
                Oper => 'o',
                LocalOper => 'O',
                ServerNotices => 's',
                MaskedHost => 'x',
                Unknown(c) => c,
            }
        )
    }
}

/// Channel modes for the MODE command.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ChannelMode {
    /// b - ban the user from joining or speaking in the channel
    Ban,
    /// e - exemptions from bans
    Exception,
    /// l - limit the maximum number of users in a channel
    Limit,
    /// i - channel becomes invite-only
    InviteOnly,
    /// I - exception to invite-only rule
    InviteException,
    /// k - specify channel key
    Key,
    /// m - channel is in moderated mode
    Moderated,
    /// r - entry for registered users only
    RegisteredOnly,
    /// s - channel is hidden from listings
    Secret,
    /// t - require permissions to edit topic
    ProtectedTopic,
    /// n - users must join channels to message them
    NoExternalMessages,

    /// q - user gets founder permission
    Founder,
    /// a - user gets admin or protected permission
    Admin,
    /// o - user gets oper permission
    Oper,
    /// h - user gets halfop permission
    Halfop,
    /// v - user gets voice permission
    Voice,

    /// Any other unknown-to-the-crate mode.
    Unknown(char),
}

impl ModeType for ChannelMode {
    fn mode(target: &str, modes: &[Mode<Self>]) -> Command {
        Command::ChannelMODE(target.to_owned(), modes.to_owned())
    }

    fn takes_arg(&self) -> bool {
        use self::ChannelMode::*;

        matches!(
            *self,
            Ban | Exception
                | Limit
                | InviteException
                | Key
                | Founder
                | Admin
                | Oper
                | Halfop
                | Voice
        )
    }

    fn from_char(c: char) -> ChannelMode {
        use self::ChannelMode::*;

        match c {
            'b' => Ban,
            'e' => Exception,
            'l' => Limit,
            'i' => InviteOnly,
            'I' => InviteException,
            'k' => Key,
            'm' => Moderated,
            'r' => RegisteredOnly,
            's' => Secret,
            't' => ProtectedTopic,
            'n' => NoExternalMessages,
            'q' => Founder,
            'a' => Admin,
            'o' => Oper,
            'h' => Halfop,
            'v' => Voice,
            _ => Unknown(c),
        }
    }
}

impl fmt::Display for ChannelMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::ChannelMode::*;

        write!(
            f,
            "{}",
            match *self {
                Ban => 'b',
                Exception => 'e',
                Limit => 'l',
                InviteOnly => 'i',
                InviteException => 'I',
                Key => 'k',
                Moderated => 'm',
                RegisteredOnly => 'r',
                Secret => 's',
                ProtectedTopic => 't',
                NoExternalMessages => 'n',
                Founder => 'q',
                Admin => 'a',
                Oper => 'o',
                Halfop => 'h',
                Voice => 'v',
                Unknown(c) => c,
            }
        )
    }
}

/// A mode argument for the MODE command.
#[derive(Clone, Debug, PartialEq)]
pub enum Mode<T>
where
    T: ModeType,
{
    /// Adding the specified mode, optionally with an argument.
    Plus(T, Option<String>),
    /// Removing the specified mode, optionally with an argument.
    Minus(T, Option<String>),
    /// No prefix mode, used to query ban list on channel join.
    NoPrefix(T),
}

impl<T> Mode<T>
where
    T: ModeType,
{
    /// Creates a plus mode with an `&str` argument.
    pub fn plus(inner: T, arg: Option<&str>) -> Mode<T> {
        Mode::Plus(inner, arg.map(|s| s.to_owned()))
    }

    /// Creates a minus mode with an `&str` argument.
    pub fn minus(inner: T, arg: Option<&str>) -> Mode<T> {
        Mode::Minus(inner, arg.map(|s| s.to_owned()))
    }

    /// Create a no prefix mode with an `&str` argument.
    pub fn no_prefix(inner: T) -> Mode<T> {
        Mode::NoPrefix(inner)
    }

    /// Gets the mode flag associated with this mode with a + or - prefix as needed.
    pub fn flag(&self) -> String {
        match self {
            Mode::Plus(mode, _) => format!("+{}", mode),
            Mode::Minus(mode, _) => format!("-{}", mode),
            Mode::NoPrefix(mode) => mode.to_string(),
        }
    }

    /// Gets the arg associated with this mode, if any. Only some channel modes support arguments,
    /// e.g. b (ban) or o (oper).
    pub fn arg(&self) -> Option<&str> {
        match self {
            Mode::Plus(_, arg) | Mode::Minus(_, arg) => arg.as_deref(),
            _ => None,
        }
    }
}

impl<T> fmt::Display for Mode<T>
where
    T: ModeType,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Mode::Plus(_, Some(ref arg)) | Mode::Minus(_, Some(ref arg)) => {
                write!(f, "{} {}", self.flag(), arg)
            }
            _ => write!(f, "{}", self.flag()),
        }
    }
}

enum PlusMinus {
    Plus,
    Minus,
    NoPrefix,
}

// MODE user [modes]
impl Mode<UserMode> {
    // TODO: turning more edge cases into errors.
    /// Parses the specified mode string as user modes.
    pub fn as_user_modes(pieces: &[&str]) -> Result<Vec<Mode<UserMode>>, MessageParseError> {
        parse_modes(pieces)
    }
}

// MODE channel [modes [modeparams]]
impl Mode<ChannelMode> {
    // TODO: turning more edge cases into errors.
    /// Parses the specified mode string as channel modes.
    pub fn as_channel_modes(pieces: &[&str]) -> Result<Vec<Mode<ChannelMode>>, MessageParseError> {
        parse_modes(pieces)
    }
}

fn parse_modes<T>(pieces: &[&str]) -> Result<Vec<Mode<T>>, MessageParseError>
where
    T: ModeType,
{
    use self::PlusMinus::*;

    let mut res = vec![];

    if let Some((first, rest)) = pieces.split_first() {
        let mut modes = first.chars();
        let mut args = rest.iter();

        let mut cur_mod = match modes.next() {
            Some('+') => Plus,
            Some('-') => Minus,
            Some(_) => {
                // rewind modes
                modes = first.chars();
                NoPrefix
            }
            None => {
                // No modifier
                return Ok(res);
            }
        };

        for c in modes {
            match c {
                '+' => cur_mod = Plus,
                '-' => cur_mod = Minus,
                _ => {
                    let mode = T::from_char(c);
                    let arg = if mode.takes_arg() {
                        // TODO: if there's no arg, this should error
                        args.next()
                    } else {
                        None
                    };
                    res.push(match cur_mod {
                        Plus => Mode::Plus(mode, arg.map(|s| s.to_string())),
                        Minus => Mode::Minus(mode, arg.map(|s| s.to_string())),
                        NoPrefix => Mode::NoPrefix(mode),
                    })
                }
            }
        }

        // TODO: if there are extra args left, this should error
    } else {
        // No modifier
    };

    Ok(res)
}

#[cfg(test)]
mod test {
    use super::{ChannelMode, Mode};
    use crate::Command;
    use crate::Message;

    #[test]
    fn parse_channel_mode() {
        let cmd = "MODE #foo +r".parse::<Message>().unwrap().command;
        assert_eq!(
            Command::ChannelMODE(
                "#foo".to_string(),
                vec![Mode::Plus(ChannelMode::RegisteredOnly, None)]
            ),
            cmd
        );
    }

    #[test]
    fn parse_no_mode() {
        let cmd = "MODE #foo".parse::<Message>().unwrap().command;
        assert_eq!(Command::ChannelMODE("#foo".to_string(), vec![]), cmd);
    }

    #[test]
    fn parse_no_plus() {
        let cmd = "MODE #foo b".parse::<Message>().unwrap().command;
        assert_eq!(
            Command::ChannelMODE("#foo".to_string(), vec![Mode::NoPrefix(ChannelMode::Ban)]),
            cmd
        );
    }
}
