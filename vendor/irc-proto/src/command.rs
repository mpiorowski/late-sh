//! Enumeration of all available client commands.
use std::str::FromStr;

use crate::chan::ChannelExt;
use crate::error::MessageParseError;
use crate::mode::{ChannelMode, Mode, UserMode};
use crate::response::Response;

/// List of all client commands as defined in [RFC 2812](http://tools.ietf.org/html/rfc2812). This
/// also includes commands from the
/// [capabilities extension](https://tools.ietf.org/html/draft-mitchell-irc-capabilities-01).
/// Additionally, this includes some common additional commands from popular IRCds.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    // 3.1 Connection Registration
    /// PASS :password
    PASS(String),
    /// NICK :nickname
    NICK(String),
    /// USER user mode * :realname
    USER(String, String, String),
    /// OPER name :password
    OPER(String, String),
    /// MODE nickname modes
    UserMODE(String, Vec<Mode<UserMode>>),
    /// SERVICE nickname reserved distribution type reserved :info
    SERVICE(String, String, String, String, String, String),
    /// QUIT :comment
    QUIT(Option<String>),
    /// SQUIT server :comment
    SQUIT(String, String),

    // 3.2 Channel operations
    /// JOIN chanlist [chankeys] :[Real name]
    JOIN(String, Option<String>, Option<String>),
    /// PART chanlist :[comment]
    PART(String, Option<String>),
    /// MODE channel [modes [modeparams]]
    ChannelMODE(String, Vec<Mode<ChannelMode>>),
    /// TOPIC channel :[topic]
    TOPIC(String, Option<String>),
    /// NAMES [chanlist :[target]]
    NAMES(Option<String>, Option<String>),
    /// LIST [chanlist :[target]]
    LIST(Option<String>, Option<String>),
    /// INVITE nickname channel
    INVITE(String, String),
    /// KICK chanlist userlist :[comment]
    KICK(String, String, Option<String>),

    // 3.3 Sending messages
    /// PRIVMSG msgtarget :message
    ///
    /// ## Responding to a `PRIVMSG`
    ///
    /// When responding to a message, it is not sufficient to simply copy the message target
    /// (msgtarget). This will work just fine for responding to messages in channels where the
    /// target is the same for all participants. However, when the message is sent directly to a
    /// user, this target will be that client's username, and responding to that same target will
    /// actually mean sending itself a response. In such a case, you should instead respond to the
    /// user sending the message as specified in the message prefix. Since this is a common
    /// pattern, there is a utility function
    /// [`Message::response_target`]
    /// which is used for this exact purpose.
    PRIVMSG(String, String),
    /// NOTICE msgtarget :message
    ///
    /// ## Responding to a `NOTICE`
    ///
    /// When responding to a notice, it is not sufficient to simply copy the message target
    /// (msgtarget). This will work just fine for responding to messages in channels where the
    /// target is the same for all participants. However, when the message is sent directly to a
    /// user, this target will be that client's username, and responding to that same target will
    /// actually mean sending itself a response. In such a case, you should instead respond to the
    /// user sending the message as specified in the message prefix. Since this is a common
    /// pattern, there is a utility function
    /// [`Message::response_target`]
    /// which is used for this exact purpose.
    NOTICE(String, String),

    // 3.4 Server queries and commands
    /// MOTD :[target]
    MOTD(Option<String>),
    /// LUSERS [mask :[target]]
    LUSERS(Option<String>, Option<String>),
    /// VERSION :[target]
    VERSION(Option<String>),
    /// STATS [query :[target]]
    STATS(Option<String>, Option<String>),
    /// LINKS [[remote server] server :mask]
    LINKS(Option<String>, Option<String>),
    /// TIME :[target]
    TIME(Option<String>),
    /// CONNECT target server port :[remote server]
    CONNECT(String, String, Option<String>),
    /// TRACE :[target]
    TRACE(Option<String>),
    /// ADMIN :[target]
    ADMIN(Option<String>),
    /// INFO :[target]
    INFO(Option<String>),

    // 3.5 Service Query and Commands
    /// SERVLIST [mask :[type]]
    SERVLIST(Option<String>, Option<String>),
    /// SQUERY servicename text
    SQUERY(String, String),

    // 3.6 User based queries
    /// WHO [mask ["o"]]
    WHO(Option<String>, Option<bool>),
    /// WHOIS [target] masklist
    WHOIS(Option<String>, String),
    /// WHOWAS nicklist [count :[target]]
    WHOWAS(String, Option<String>, Option<String>),

    // 3.7 Miscellaneous messages
    /// KILL nickname :comment
    KILL(String, String),
    /// PING server1 :[server2]
    PING(String, Option<String>),
    /// PONG server :[server2]
    PONG(String, Option<String>),
    /// ERROR :message
    ERROR(String),

    // 4 Optional Features
    /// AWAY :[message]
    AWAY(Option<String>),
    /// REHASH
    REHASH,
    /// DIE
    DIE,
    /// RESTART
    RESTART,
    /// SUMMON user [target :[channel]]
    SUMMON(String, Option<String>, Option<String>),
    /// USERS :[target]
    USERS(Option<String>),
    /// WALLOPS :Text to be sent
    WALLOPS(String),
    /// USERHOST space-separated nicklist
    USERHOST(Vec<String>),
    /// ISON space-separated nicklist
    ISON(Vec<String>),

    // Non-RFC commands from InspIRCd
    /// SAJOIN nickname channel
    SAJOIN(String, String),
    /// SAMODE target modes [modeparams]
    SAMODE(String, String, Option<String>),
    /// SANICK old nickname new nickname
    SANICK(String, String),
    /// SAPART nickname :comment
    SAPART(String, String),
    /// SAQUIT nickname :comment
    SAQUIT(String, String),
    /// NICKSERV message
    NICKSERV(Vec<String>),
    /// CHANSERV message
    CHANSERV(String),
    /// OPERSERV message
    OPERSERV(String),
    /// BOTSERV message
    BOTSERV(String),
    /// HOSTSERV message
    HOSTSERV(String),
    /// MEMOSERV message
    MEMOSERV(String),

    // IRCv3 support
    /// CAP [*] COMMAND [*] :[param]
    CAP(
        Option<String>,
        CapSubCommand,
        Option<String>,
        Option<String>,
    ),

    // IRCv3.1 extensions
    /// AUTHENTICATE data
    AUTHENTICATE(String),
    /// ACCOUNT [account name]
    ACCOUNT(String),
    // AWAY is already defined as a send-only message.
    // AWAY(Option<String>),
    // JOIN is already defined.
    // JOIN(String, Option<String>, Option<String>),

    // IRCv3.2 extensions
    /// METADATA target COMMAND [params] :[param]
    METADATA(String, Option<MetadataSubCommand>, Option<Vec<String>>),
    /// MONITOR command [nicklist]
    MONITOR(String, Option<String>),
    /// BATCH (+/-)reference-tag [type [params]]
    BATCH(String, Option<BatchSubCommand>, Option<Vec<String>>),
    /// CHGHOST user host
    CHGHOST(String, String),

    // Default option.
    /// An IRC response code with arguments and optional suffix.
    Response(Response, Vec<String>),
    /// A raw IRC command unknown to the crate.
    Raw(String, Vec<String>),
}

fn stringify(cmd: &str, args: &[&str]) -> String {
    match args.split_last() {
        Some((suffix, args)) => {
            let args = args.join(" ");
            let sp = if args.is_empty() { "" } else { " " };
            let co = if suffix.is_empty() || suffix.contains(' ') || suffix.starts_with(':') {
                ":"
            } else {
                ""
            };
            format!("{}{}{} {}{}", cmd, sp, args, co, suffix)
        }
        None => cmd.to_string(),
    }
}

impl<'a> From<&'a Command> for String {
    fn from(cmd: &'a Command) -> String {
        match *cmd {
            Command::PASS(ref p) => stringify("PASS", &[p]),
            Command::NICK(ref n) => stringify("NICK", &[n]),
            Command::USER(ref u, ref m, ref r) => stringify("USER", &[u, m, "*", r]),
            Command::OPER(ref u, ref p) => stringify("OPER", &[u, p]),
            Command::UserMODE(ref u, ref m) => {
                // User modes never have arguments.
                m.iter().fold(format!("MODE {u} "), |mut acc, m| {
                    acc.push_str(&m.flag());
                    acc
                })
            }
            Command::SERVICE(ref nick, ref r0, ref dist, ref typ, ref r1, ref info) => {
                stringify("SERVICE", &[nick, r0, dist, typ, r1, info])
            }
            Command::QUIT(Some(ref m)) => stringify("QUIT", &[m]),
            Command::QUIT(None) => stringify("QUIT", &[]),
            Command::SQUIT(ref s, ref c) => stringify("SQUIT", &[s, c]),
            Command::JOIN(ref c, Some(ref k), Some(ref n)) => stringify("JOIN", &[c, k, n]),
            Command::JOIN(ref c, Some(ref k), None) => stringify("JOIN", &[c, k]),
            Command::JOIN(ref c, None, Some(ref n)) => stringify("JOIN", &[c, n]),
            Command::JOIN(ref c, None, None) => stringify("JOIN", &[c]),
            Command::PART(ref c, Some(ref m)) => stringify("PART", &[c, m]),
            Command::PART(ref c, None) => stringify("PART", &[c]),
            Command::ChannelMODE(ref c, ref m) => {
                let cmd = m.iter().fold(format!("MODE {c} "), |mut acc, m| {
                    acc.push_str(&m.flag());
                    acc
                });
                m.iter().filter_map(|m| m.arg()).fold(cmd, |mut acc, arg| {
                    acc.push(' ');
                    acc.push_str(arg);
                    acc
                })
            }
            Command::TOPIC(ref c, Some(ref t)) => stringify("TOPIC", &[c, t]),
            Command::TOPIC(ref c, None) => stringify("TOPIC", &[c]),
            Command::NAMES(Some(ref c), Some(ref t)) => stringify("NAMES", &[c, t]),
            Command::NAMES(Some(ref c), None) => stringify("NAMES", &[c]),
            Command::NAMES(None, _) => stringify("NAMES", &[]),
            Command::LIST(Some(ref c), Some(ref t)) => stringify("LIST", &[c, t]),
            Command::LIST(Some(ref c), None) => stringify("LIST", &[c]),
            Command::LIST(None, _) => stringify("LIST", &[]),
            Command::INVITE(ref n, ref c) => stringify("INVITE", &[n, c]),
            Command::KICK(ref c, ref n, Some(ref r)) => stringify("KICK", &[c, n, r]),
            Command::KICK(ref c, ref n, None) => stringify("KICK", &[c, n]),
            Command::PRIVMSG(ref t, ref m) => stringify("PRIVMSG", &[t, m]),
            Command::NOTICE(ref t, ref m) => stringify("NOTICE", &[t, m]),
            Command::MOTD(Some(ref t)) => stringify("MOTD", &[t]),
            Command::MOTD(None) => stringify("MOTD", &[]),
            Command::LUSERS(Some(ref m), Some(ref t)) => stringify("LUSERS", &[m, t]),
            Command::LUSERS(Some(ref m), None) => stringify("LUSERS", &[m]),
            Command::LUSERS(None, _) => stringify("LUSERS", &[]),
            Command::VERSION(Some(ref t)) => stringify("VERSION", &[t]),
            Command::VERSION(None) => stringify("VERSION", &[]),
            Command::STATS(Some(ref q), Some(ref t)) => stringify("STATS", &[q, t]),
            Command::STATS(Some(ref q), None) => stringify("STATS", &[q]),
            Command::STATS(None, _) => stringify("STATS", &[]),
            Command::LINKS(Some(ref r), Some(ref s)) => stringify("LINKS", &[r, s]),
            Command::LINKS(None, Some(ref s)) => stringify("LINKS", &[s]),
            Command::LINKS(_, None) => stringify("LINKS", &[]),
            Command::TIME(Some(ref t)) => stringify("TIME", &[t]),
            Command::TIME(None) => stringify("TIME", &[]),
            Command::CONNECT(ref t, ref p, Some(ref r)) => stringify("CONNECT", &[t, p, r]),
            Command::CONNECT(ref t, ref p, None) => stringify("CONNECT", &[t, p]),
            Command::TRACE(Some(ref t)) => stringify("TRACE", &[t]),
            Command::TRACE(None) => stringify("TRACE", &[]),
            Command::ADMIN(Some(ref t)) => stringify("ADMIN", &[t]),
            Command::ADMIN(None) => stringify("ADMIN", &[]),
            Command::INFO(Some(ref t)) => stringify("INFO", &[t]),
            Command::INFO(None) => stringify("INFO", &[]),
            Command::SERVLIST(Some(ref m), Some(ref t)) => stringify("SERVLIST", &[m, t]),
            Command::SERVLIST(Some(ref m), None) => stringify("SERVLIST", &[m]),
            Command::SERVLIST(None, _) => stringify("SERVLIST", &[]),
            Command::SQUERY(ref s, ref t) => stringify("SQUERY", &[s, t]),
            Command::WHO(Some(ref s), Some(true)) => stringify("WHO", &[s, "o"]),
            Command::WHO(Some(ref s), _) => stringify("WHO", &[s]),
            Command::WHO(None, _) => stringify("WHO", &[]),
            Command::WHOIS(Some(ref t), ref m) => stringify("WHOIS", &[t, m]),
            Command::WHOIS(None, ref m) => stringify("WHOIS", &[m]),
            Command::WHOWAS(ref n, Some(ref c), Some(ref t)) => stringify("WHOWAS", &[n, c, t]),
            Command::WHOWAS(ref n, Some(ref c), None) => stringify("WHOWAS", &[n, c]),
            Command::WHOWAS(ref n, None, _) => stringify("WHOWAS", &[n]),
            Command::KILL(ref n, ref c) => stringify("KILL", &[n, c]),
            Command::PING(ref s, Some(ref t)) => stringify("PING", &[s, t]),
            Command::PING(ref s, None) => stringify("PING", &[s]),
            Command::PONG(ref s, Some(ref t)) => stringify("PONG", &[s, t]),
            Command::PONG(ref s, None) => stringify("PONG", &[s]),
            Command::ERROR(ref m) => stringify("ERROR", &[m]),
            Command::AWAY(Some(ref m)) => stringify("AWAY", &[m]),
            Command::AWAY(None) => stringify("AWAY", &[]),
            Command::REHASH => stringify("REHASH", &[]),
            Command::DIE => stringify("DIE", &[]),
            Command::RESTART => stringify("RESTART", &[]),
            Command::SUMMON(ref u, Some(ref t), Some(ref c)) => stringify("SUMMON", &[u, t, c]),
            Command::SUMMON(ref u, Some(ref t), None) => stringify("SUMMON", &[u, t]),
            Command::SUMMON(ref u, None, _) => stringify("SUMMON", &[u]),
            Command::USERS(Some(ref t)) => stringify("USERS", &[t]),
            Command::USERS(None) => stringify("USERS", &[]),
            Command::WALLOPS(ref t) => stringify("WALLOPS", &[t]),
            Command::USERHOST(ref u) => {
                stringify("USERHOST", &u.iter().map(|s| &s[..]).collect::<Vec<_>>())
            }
            Command::ISON(ref u) => {
                stringify("ISON", &u.iter().map(|s| &s[..]).collect::<Vec<_>>())
            }

            Command::SAJOIN(ref n, ref c) => stringify("SAJOIN", &[n, c]),
            Command::SAMODE(ref t, ref m, Some(ref p)) => stringify("SAMODE", &[t, m, p]),
            Command::SAMODE(ref t, ref m, None) => stringify("SAMODE", &[t, m]),
            Command::SANICK(ref o, ref n) => stringify("SANICK", &[o, n]),
            Command::SAPART(ref c, ref r) => stringify("SAPART", &[c, r]),
            Command::SAQUIT(ref c, ref r) => stringify("SAQUIT", &[c, r]),

            Command::NICKSERV(ref p) => {
                stringify("NICKSERV", &p.iter().map(|s| &s[..]).collect::<Vec<_>>())
            }
            Command::CHANSERV(ref m) => stringify("CHANSERV", &[m]),
            Command::OPERSERV(ref m) => stringify("OPERSERV", &[m]),
            Command::BOTSERV(ref m) => stringify("BOTSERV", &[m]),
            Command::HOSTSERV(ref m) => stringify("HOSTSERV", &[m]),
            Command::MEMOSERV(ref m) => stringify("MEMOSERV", &[m]),

            Command::CAP(None, ref s, None, Some(ref p)) => stringify("CAP", &[s.to_str(), p]),
            Command::CAP(None, ref s, None, None) => stringify("CAP", &[s.to_str()]),
            Command::CAP(Some(ref k), ref s, None, Some(ref p)) => {
                stringify("CAP", &[k, s.to_str(), p])
            }
            Command::CAP(Some(ref k), ref s, None, None) => stringify("CAP", &[k, s.to_str()]),
            Command::CAP(None, ref s, Some(ref c), Some(ref p)) => {
                stringify("CAP", &[s.to_str(), c, p])
            }
            Command::CAP(None, ref s, Some(ref c), None) => stringify("CAP", &[s.to_str(), c]),
            Command::CAP(Some(ref k), ref s, Some(ref c), Some(ref p)) => {
                stringify("CAP", &[k, s.to_str(), c, p])
            }
            Command::CAP(Some(ref k), ref s, Some(ref c), None) => {
                stringify("CAP", &[k, s.to_str(), c])
            }

            Command::AUTHENTICATE(ref d) => stringify("AUTHENTICATE", &[d]),
            Command::ACCOUNT(ref a) => stringify("ACCOUNT", &[a]),

            Command::METADATA(ref t, Some(ref c), None) => {
                stringify("METADATA", &[&t[..], c.to_str()])
            }
            Command::METADATA(ref t, Some(ref c), Some(ref a)) => stringify(
                "METADATA",
                &[t, &c.to_str().to_owned()]
                    .iter()
                    .map(|s| &s[..])
                    .chain(a.iter().map(|s| &s[..]))
                    .collect::<Vec<_>>(),
            ),

            // Note that it shouldn't be possible to have a later arg *and* be
            // missing an early arg, so in order to serialize this as valid, we
            // return it as just the command.
            Command::METADATA(ref t, None, _) => stringify("METADATA", &[t]),

            Command::MONITOR(ref c, Some(ref t)) => stringify("MONITOR", &[c, t]),
            Command::MONITOR(ref c, None) => stringify("MONITOR", &[c]),
            Command::BATCH(ref t, Some(ref c), Some(ref a)) => stringify(
                "BATCH",
                &[t, &c.to_str().to_owned()]
                    .iter()
                    .map(|s| &s[..])
                    .chain(a.iter().map(|s| &s[..]))
                    .collect::<Vec<_>>(),
            ),
            Command::BATCH(ref t, Some(ref c), None) => stringify("BATCH", &[t, c.to_str()]),
            Command::BATCH(ref t, None, Some(ref a)) => stringify(
                "BATCH",
                &[t].iter()
                    .map(|s| &s[..])
                    .chain(a.iter().map(|s| &s[..]))
                    .collect::<Vec<_>>(),
            ),
            Command::BATCH(ref t, None, None) => stringify("BATCH", &[t]),
            Command::CHGHOST(ref u, ref h) => stringify("CHGHOST", &[u, h]),

            Command::Response(ref resp, ref a) => stringify(
                &format!("{:03}", *resp as u16),
                &a.iter().map(|s| &s[..]).collect::<Vec<_>>(),
            ),
            Command::Raw(ref c, ref a) => {
                stringify(c, &a.iter().map(|s| &s[..]).collect::<Vec<_>>())
            }
        }
    }
}

impl Command {
    /// Constructs a new Command.
    pub fn new(cmd: &str, args: Vec<&str>) -> Result<Command, MessageParseError> {
        Ok(if cmd.eq_ignore_ascii_case("PASS") {
            if args.len() != 1 {
                raw(cmd, args)
            } else {
                Command::PASS(args[0].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("NICK") {
            if args.len() != 1 {
                raw(cmd, args)
            } else {
                Command::NICK(args[0].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("USER") {
            if args.len() != 4 {
                raw(cmd, args)
            } else {
                Command::USER(args[0].to_owned(), args[1].to_owned(), args[3].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("OPER") {
            if args.len() != 2 {
                raw(cmd, args)
            } else {
                Command::OPER(args[0].to_owned(), args[1].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("MODE") {
            if args.is_empty() {
                raw(cmd, args)
            } else if args[0].is_channel_name() {
                Command::ChannelMODE(args[0].to_owned(), Mode::as_channel_modes(&args[1..])?)
            } else {
                Command::UserMODE(args[0].to_owned(), Mode::as_user_modes(&args[1..])?)
            }
        } else if cmd.eq_ignore_ascii_case("SERVICE") {
            if args.len() != 6 {
                raw(cmd, args)
            } else {
                Command::SERVICE(
                    args[0].to_owned(),
                    args[1].to_owned(),
                    args[2].to_owned(),
                    args[3].to_owned(),
                    args[4].to_owned(),
                    args[5].to_owned(),
                )
            }
        } else if cmd.eq_ignore_ascii_case("QUIT") {
            if args.is_empty() {
                Command::QUIT(None)
            } else if args.len() == 1 {
                Command::QUIT(Some(args[0].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("SQUIT") {
            if args.len() != 2 {
                raw(cmd, args)
            } else {
                Command::SQUIT(args[0].to_owned(), args[1].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("JOIN") {
            if args.len() == 1 {
                Command::JOIN(args[0].to_owned(), None, None)
            } else if args.len() == 2 {
                Command::JOIN(args[0].to_owned(), Some(args[1].to_owned()), None)
            } else if args.len() == 3 {
                Command::JOIN(
                    args[0].to_owned(),
                    Some(args[1].to_owned()),
                    Some(args[2].to_owned()),
                )
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("PART") {
            if args.len() == 1 {
                Command::PART(args[0].to_owned(), None)
            } else if args.len() == 2 {
                Command::PART(args[0].to_owned(), Some(args[1].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("TOPIC") {
            if args.len() == 1 {
                Command::TOPIC(args[0].to_owned(), None)
            } else if args.len() == 2 {
                Command::TOPIC(args[0].to_owned(), Some(args[1].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("NAMES") {
            if args.is_empty() {
                Command::NAMES(None, None)
            } else if args.len() == 1 {
                Command::NAMES(Some(args[0].to_owned()), None)
            } else if args.len() == 2 {
                Command::NAMES(Some(args[0].to_owned()), Some(args[1].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("LIST") {
            if args.is_empty() {
                Command::LIST(None, None)
            } else if args.len() == 1 {
                Command::LIST(Some(args[0].to_owned()), None)
            } else if args.len() == 2 {
                Command::LIST(Some(args[0].to_owned()), Some(args[1].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("INVITE") {
            if args.len() != 2 {
                raw(cmd, args)
            } else {
                Command::INVITE(args[0].to_owned(), args[1].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("KICK") {
            if args.len() == 3 {
                Command::KICK(
                    args[0].to_owned(),
                    args[1].to_owned(),
                    Some(args[2].to_owned()),
                )
            } else if args.len() == 2 {
                Command::KICK(args[0].to_owned(), args[1].to_owned(), None)
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("PRIVMSG") {
            if args.len() != 2 {
                raw(cmd, args)
            } else {
                Command::PRIVMSG(args[0].to_owned(), args[1].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("NOTICE") {
            if args.len() != 2 {
                raw(cmd, args)
            } else {
                Command::NOTICE(args[0].to_owned(), args[1].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("MOTD") {
            if args.is_empty() {
                Command::MOTD(None)
            } else if args.len() == 1 {
                Command::MOTD(Some(args[0].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("LUSERS") {
            if args.is_empty() {
                Command::LUSERS(None, None)
            } else if args.len() == 1 {
                Command::LUSERS(Some(args[0].to_owned()), None)
            } else if args.len() == 2 {
                Command::LUSERS(Some(args[0].to_owned()), Some(args[1].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("VERSION") {
            if args.is_empty() {
                Command::VERSION(None)
            } else if args.len() == 1 {
                Command::VERSION(Some(args[0].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("STATS") {
            if args.is_empty() {
                Command::STATS(None, None)
            } else if args.len() == 1 {
                Command::STATS(Some(args[0].to_owned()), None)
            } else if args.len() == 2 {
                Command::STATS(Some(args[0].to_owned()), Some(args[1].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("LINKS") {
            if args.is_empty() {
                Command::LINKS(None, None)
            } else if args.len() == 1 {
                Command::LINKS(Some(args[0].to_owned()), None)
            } else if args.len() == 2 {
                Command::LINKS(Some(args[0].to_owned()), Some(args[1].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("TIME") {
            if args.is_empty() {
                Command::TIME(None)
            } else if args.len() == 1 {
                Command::TIME(Some(args[0].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("CONNECT") {
            if args.len() != 2 {
                raw(cmd, args)
            } else {
                Command::CONNECT(args[0].to_owned(), args[1].to_owned(), None)
            }
        } else if cmd.eq_ignore_ascii_case("TRACE") {
            if args.is_empty() {
                Command::TRACE(None)
            } else if args.len() == 1 {
                Command::TRACE(Some(args[0].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("ADMIN") {
            if args.is_empty() {
                Command::ADMIN(None)
            } else if args.len() == 1 {
                Command::ADMIN(Some(args[0].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("INFO") {
            if args.is_empty() {
                Command::INFO(None)
            } else if args.len() == 1 {
                Command::INFO(Some(args[0].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("SERVLIST") {
            if args.is_empty() {
                Command::SERVLIST(None, None)
            } else if args.len() == 1 {
                Command::SERVLIST(Some(args[0].to_owned()), None)
            } else if args.len() == 2 {
                Command::SERVLIST(Some(args[0].to_owned()), Some(args[1].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("SQUERY") {
            if args.len() != 2 {
                raw(cmd, args)
            } else {
                Command::SQUERY(args[0].to_owned(), args[1].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("WHO") {
            if args.is_empty() {
                Command::WHO(None, None)
            } else if args.len() == 1 {
                Command::WHO(Some(args[0].to_owned()), None)
            } else if args.len() == 2 {
                Command::WHO(Some(args[0].to_owned()), Some(args[1] == "o"))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("WHOIS") {
            if args.len() == 1 {
                Command::WHOIS(None, args[0].to_owned())
            } else if args.len() == 2 {
                Command::WHOIS(Some(args[0].to_owned()), args[1].to_owned())
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("WHOWAS") {
            if args.len() == 1 {
                Command::WHOWAS(args[0].to_owned(), None, None)
            } else if args.len() == 2 {
                Command::WHOWAS(args[0].to_owned(), None, Some(args[1].to_owned()))
            } else if args.len() == 3 {
                Command::WHOWAS(
                    args[0].to_owned(),
                    Some(args[1].to_owned()),
                    Some(args[2].to_owned()),
                )
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("KILL") {
            if args.len() != 2 {
                raw(cmd, args)
            } else {
                Command::KILL(args[0].to_owned(), args[1].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("PING") {
            if args.len() == 1 {
                Command::PING(args[0].to_owned(), None)
            } else if args.len() == 2 {
                Command::PING(args[0].to_owned(), Some(args[1].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("PONG") {
            if args.len() == 1 {
                Command::PONG(args[0].to_owned(), None)
            } else if args.len() == 2 {
                Command::PONG(args[0].to_owned(), Some(args[1].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("ERROR") {
            if args.len() != 1 {
                raw(cmd, args)
            } else {
                Command::ERROR(args[0].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("AWAY") {
            if args.is_empty() {
                Command::AWAY(None)
            } else if args.len() == 1 {
                Command::AWAY(Some(args[0].to_owned()))
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("REHASH") {
            if args.is_empty() {
                Command::REHASH
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("DIE") {
            if args.is_empty() {
                Command::DIE
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("RESTART") {
            if args.is_empty() {
                Command::RESTART
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("SUMMON") {
            if args.len() == 1 {
                Command::SUMMON(args[0].to_owned(), None, None)
            } else if args.len() == 2 {
                Command::SUMMON(args[0].to_owned(), Some(args[1].to_owned()), None)
            } else if args.len() == 3 {
                Command::SUMMON(
                    args[0].to_owned(),
                    Some(args[1].to_owned()),
                    Some(args[2].to_owned()),
                )
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("USERS") {
            if args.len() != 1 {
                raw(cmd, args)
            } else {
                Command::USERS(Some(args[0].to_owned()))
            }
        } else if cmd.eq_ignore_ascii_case("WALLOPS") {
            if args.len() != 1 {
                raw(cmd, args)
            } else {
                Command::WALLOPS(args[0].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("USERHOST") {
            Command::USERHOST(args.into_iter().map(|s| s.to_owned()).collect())
        } else if cmd.eq_ignore_ascii_case("ISON") {
            Command::USERHOST(args.into_iter().map(|s| s.to_owned()).collect())
        } else if cmd.eq_ignore_ascii_case("SAJOIN") {
            if args.len() != 2 {
                raw(cmd, args)
            } else {
                Command::SAJOIN(args[0].to_owned(), args[1].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("SAMODE") {
            if args.len() == 2 {
                Command::SAMODE(args[0].to_owned(), args[1].to_owned(), None)
            } else if args.len() == 3 {
                Command::SAMODE(
                    args[0].to_owned(),
                    args[1].to_owned(),
                    Some(args[2].to_owned()),
                )
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("SANICK") {
            if args.len() != 2 {
                raw(cmd, args)
            } else {
                Command::SANICK(args[0].to_owned(), args[1].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("SAPART") {
            if args.len() != 2 {
                raw(cmd, args)
            } else {
                Command::SAPART(args[0].to_owned(), args[1].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("SAQUIT") {
            if args.len() != 2 {
                raw(cmd, args)
            } else {
                Command::SAQUIT(args[0].to_owned(), args[1].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("NICKSERV") {
            if args.len() != 1 {
                raw(cmd, args)
            } else {
                Command::NICKSERV(args[1..].iter().map(|s| s.to_string()).collect())
            }
        } else if cmd.eq_ignore_ascii_case("CHANSERV") {
            if args.len() != 1 {
                raw(cmd, args)
            } else {
                Command::CHANSERV(args[0].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("OPERSERV") {
            if args.len() != 1 {
                raw(cmd, args)
            } else {
                Command::OPERSERV(args[0].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("BOTSERV") {
            if args.len() != 1 {
                raw(cmd, args)
            } else {
                Command::BOTSERV(args[0].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("HOSTSERV") {
            if args.len() != 1 {
                raw(cmd, args)
            } else {
                Command::HOSTSERV(args[0].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("MEMOSERV") {
            if args.len() != 1 {
                raw(cmd, args)
            } else {
                Command::MEMOSERV(args[0].to_owned())
            }
        } else if cmd.eq_ignore_ascii_case("CAP") {
            if args.len() == 1 {
                if let Ok(cmd) = args[0].parse() {
                    Command::CAP(None, cmd, None, None)
                } else {
                    raw(cmd, args)
                }
            } else if args.len() == 2 {
                if let Ok(cmd) = args[0].parse() {
                    Command::CAP(None, cmd, Some(args[1].to_owned()), None)
                } else if let Ok(cmd) = args[1].parse() {
                    Command::CAP(Some(args[0].to_owned()), cmd, None, None)
                } else {
                    raw(cmd, args)
                }
            } else if args.len() == 3 {
                if let Ok(cmd) = args[0].parse() {
                    Command::CAP(
                        None,
                        cmd,
                        Some(args[1].to_owned()),
                        Some(args[2].to_owned()),
                    )
                } else if let Ok(cmd) = args[1].parse() {
                    Command::CAP(
                        Some(args[0].to_owned()),
                        cmd,
                        Some(args[2].to_owned()),
                        None,
                    )
                } else {
                    raw(cmd, args)
                }
            } else if args.len() == 4 {
                if let Ok(cmd) = args[1].parse() {
                    Command::CAP(
                        Some(args[0].to_owned()),
                        cmd,
                        Some(args[2].to_owned()),
                        Some(args[3].to_owned()),
                    )
                } else {
                    raw(cmd, args)
                }
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("AUTHENTICATE") {
            if args.len() == 1 {
                Command::AUTHENTICATE(args[0].to_owned())
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("ACCOUNT") {
            if args.len() == 1 {
                Command::ACCOUNT(args[0].to_owned())
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("METADATA") {
            match args.len() {
                2 => match args[1].parse() {
                    Ok(c) => Command::METADATA(args[0].to_owned(), Some(c), None),
                    Err(_) => raw(cmd, args),
                },
                3.. => match args[1].parse() {
                    Ok(c) => Command::METADATA(
                        args[0].to_owned(),
                        Some(c),
                        Some(args.into_iter().skip(1).map(|s| s.to_owned()).collect()),
                    ),
                    Err(_) => {
                        if args.len() == 3 {
                            Command::METADATA(
                                args[0].to_owned(),
                                None,
                                Some(args.into_iter().skip(1).map(|s| s.to_owned()).collect()),
                            )
                        } else {
                            raw(cmd, args)
                        }
                    }
                },
                _ => raw(cmd, args),
            }
        } else if cmd.eq_ignore_ascii_case("MONITOR") {
            if args.len() == 2 {
                Command::MONITOR(args[0].to_owned(), Some(args[1].to_owned()))
            } else if args.len() == 1 {
                Command::MONITOR(args[0].to_owned(), None)
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("BATCH") {
            if args.len() == 1 {
                Command::BATCH(args[0].to_owned(), None, None)
            } else if args.len() == 2 {
                match args[1].parse() {
                    Ok(subcmd) => Command::BATCH(args[0].to_owned(), Some(subcmd), None),
                    Err(_) => raw(cmd, args),
                }
            } else if args.len() > 2 {
                match args[1].parse() {
                    Ok(subcmd) => Command::BATCH(
                        args[0].to_owned(),
                        Some(subcmd),
                        Some(args.iter().skip(2).map(|&s| s.to_owned()).collect()),
                    ),
                    Err(_) => raw(cmd, args),
                }
            } else {
                raw(cmd, args)
            }
        } else if cmd.eq_ignore_ascii_case("CHGHOST") {
            if args.len() == 2 {
                Command::CHGHOST(args[0].to_owned(), args[1].to_owned())
            } else {
                raw(cmd, args)
            }
        } else if let Ok(resp) = cmd.parse() {
            Command::Response(resp, args.into_iter().map(|s| s.to_owned()).collect())
        } else {
            raw(cmd, args)
        })
    }
}

/// Makes a raw message from the specified command, arguments, and suffix.
fn raw(cmd: &str, args: Vec<&str>) -> Command {
    Command::Raw(
        cmd.to_owned(),
        args.into_iter().map(|s| s.to_owned()).collect(),
    )
}

/// A list of all of the subcommands for the capabilities extension.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CapSubCommand {
    /// Requests a list of the server's capabilities.
    LS,
    /// Requests a list of the server's capabilities.
    LIST,
    /// Requests specific capabilities blindly.
    REQ,
    /// Acknowledges capabilities.
    ACK,
    /// Does not acknowledge certain capabilities.
    NAK,
    /// Ends the capability negotiation before registration.
    END,
    /// Signals that new capabilities are now being offered.
    NEW,
    /// Signasl that the specified capabilities are cancelled and no longer available.
    DEL,
}

impl CapSubCommand {
    /// Gets the string that corresponds to this subcommand.
    pub fn to_str(&self) -> &str {
        match *self {
            CapSubCommand::LS => "LS",
            CapSubCommand::LIST => "LIST",
            CapSubCommand::REQ => "REQ",
            CapSubCommand::ACK => "ACK",
            CapSubCommand::NAK => "NAK",
            CapSubCommand::END => "END",
            CapSubCommand::NEW => "NEW",
            CapSubCommand::DEL => "DEL",
        }
    }
}

impl FromStr for CapSubCommand {
    type Err = MessageParseError;

    fn from_str(s: &str) -> Result<CapSubCommand, Self::Err> {
        if s.eq_ignore_ascii_case("LS") {
            Ok(CapSubCommand::LS)
        } else if s.eq_ignore_ascii_case("LIST") {
            Ok(CapSubCommand::LIST)
        } else if s.eq_ignore_ascii_case("REQ") {
            Ok(CapSubCommand::REQ)
        } else if s.eq_ignore_ascii_case("ACK") {
            Ok(CapSubCommand::ACK)
        } else if s.eq_ignore_ascii_case("NAK") {
            Ok(CapSubCommand::NAK)
        } else if s.eq_ignore_ascii_case("END") {
            Ok(CapSubCommand::END)
        } else if s.eq_ignore_ascii_case("NEW") {
            Ok(CapSubCommand::NEW)
        } else if s.eq_ignore_ascii_case("DEL") {
            Ok(CapSubCommand::DEL)
        } else {
            Err(MessageParseError::InvalidSubcommand {
                cmd: "CAP",
                sub: s.to_owned(),
            })
        }
    }
}

/// A list of all the subcommands for the
/// [metadata extension](http://ircv3.net/specs/core/metadata-3.2.html).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MetadataSubCommand {
    /// Looks up the value for some keys.
    GET,
    /// Lists all of the metadata keys and values.
    LIST,
    /// Sets the value for some key.
    SET,
    /// Removes all metadata.
    CLEAR,
}

impl MetadataSubCommand {
    /// Gets the string that corresponds to this subcommand.
    pub fn to_str(&self) -> &str {
        match *self {
            MetadataSubCommand::GET => "GET",
            MetadataSubCommand::LIST => "LIST",
            MetadataSubCommand::SET => "SET",
            MetadataSubCommand::CLEAR => "CLEAR",
        }
    }
}

impl FromStr for MetadataSubCommand {
    type Err = MessageParseError;

    fn from_str(s: &str) -> Result<MetadataSubCommand, Self::Err> {
        if s.eq_ignore_ascii_case("GET") {
            Ok(MetadataSubCommand::GET)
        } else if s.eq_ignore_ascii_case("LIST") {
            Ok(MetadataSubCommand::LIST)
        } else if s.eq_ignore_ascii_case("SET") {
            Ok(MetadataSubCommand::SET)
        } else if s.eq_ignore_ascii_case("CLEAR") {
            Ok(MetadataSubCommand::CLEAR)
        } else {
            Err(MessageParseError::InvalidSubcommand {
                cmd: "METADATA",
                sub: s.to_owned(),
            })
        }
    }
}

/// [batch extension](http://ircv3.net/specs/extensions/batch-3.2.html).
#[derive(Clone, Debug, PartialEq)]
pub enum BatchSubCommand {
    /// [NETSPLIT](http://ircv3.net/specs/extensions/batch/netsplit.html)
    NETSPLIT,
    /// [NETJOIN](http://ircv3.net/specs/extensions/batch/netsplit.html)
    NETJOIN,
    /// Vendor-specific BATCH subcommands.
    CUSTOM(String),
}

impl BatchSubCommand {
    /// Gets the string that corresponds to this subcommand.
    pub fn to_str(&self) -> &str {
        match *self {
            BatchSubCommand::NETSPLIT => "NETSPLIT",
            BatchSubCommand::NETJOIN => "NETJOIN",
            BatchSubCommand::CUSTOM(ref s) => s,
        }
    }
}

impl FromStr for BatchSubCommand {
    type Err = MessageParseError;

    fn from_str(s: &str) -> Result<BatchSubCommand, Self::Err> {
        if s.eq_ignore_ascii_case("NETSPLIT") {
            Ok(BatchSubCommand::NETSPLIT)
        } else if s.eq_ignore_ascii_case("NETJOIN") {
            Ok(BatchSubCommand::NETJOIN)
        } else {
            Ok(BatchSubCommand::CUSTOM(s.to_uppercase()))
        }
    }
}

#[cfg(test)]
mod test {
    use super::Command;
    use super::Response;
    use crate::Message;

    #[test]
    fn format_response() {
        assert!(
            String::from(&Command::Response(
                Response::RPL_WELCOME,
                vec!["foo".into()],
            )) == "001 foo"
        );
    }

    #[test]
    fn user_round_trip() {
        let cmd = Command::USER("a".to_string(), "b".to_string(), "c".to_string());
        let line = Message::from(cmd.clone()).to_string();
        let returned_cmd = line.parse::<Message>().unwrap().command;
        assert_eq!(cmd, returned_cmd);
    }

    #[test]
    fn parse_user_message() {
        let cmd = "USER a 0 * b".parse::<Message>().unwrap().command;
        assert_eq!(
            Command::USER("a".to_string(), "0".to_string(), "b".to_string()),
            cmd
        );
    }
}
