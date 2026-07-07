//! Enumeration of all the possible server responses.
#![allow(non_camel_case_types)]
use std::str::FromStr;

macro_rules! make_response {
    ($($(#[$attr:meta])+ $variant:ident = $value:expr),+) => {
        /// List of all server responses as defined in
        /// [RFC 2812](http://tools.ietf.org/html/rfc2812) and
        /// [Modern docs](https://modern.ircdocs.horse/#numerics) (henceforth referred to as
        /// Modern). All commands are documented with their expected form from the RFC, and any
        /// useful, additional information about the response code.
        #[derive(Clone, Copy, Debug, PartialEq)]
        #[repr(u16)]
        pub enum Response {
            $($(#[$attr])+ $variant = $value),+
        }

        impl Response {
            /// Generates a Response from a u16.
            fn from_u16(val: u16) -> Option<Response> {
                match val {
                    $($value => Some(Response::$variant),)+
                    _ => None
                }
            }
        }
    }
}

make_response! {
    // Expected replies
    /// `001 Welcome to the Internet Relay Network <nick>!<user>@<host>` (Source: RFC2812)
    RPL_WELCOME         =   1,
    /// `002 Your host is <servername>, running version <ver>` (Source: RFC2812)
    RPL_YOURHOST        =   2,
    /// `003 This server was created <date>` (Source: RFC2812)
    RPL_CREATED         =   3,
    /// `004 <servername> <version> <available user modes> <available channel modes>` (Source:
    /// RFC2812)
    ///
    /// Various IRCds may choose to include additional arguments to `RPL_MYINFO`, and it's best to
    /// check for certain what the servers you're targeting do. Typically, there are additional
    /// parameters at the end for modes that have parameters, and server modes.
    RPL_MYINFO          =   4,
    /// `005 <servername> *(<feature>(=<value>)) :are supported by this server` (Source: Modern)
    ///
    /// [RPL_ISUPPORT](https://modern.ircdocs.horse/#rplisupport-005) replaces RPL_BOUNCE from
    /// RFC2812, but does so consistently in modern IRCd implementations. RPL_BOUNCE has been moved
    /// to `010`.
    RPL_ISUPPORT        =   5,
    /// `010 Try server <server name>, port <port number>` (Source: Modern)
    RPL_BOUNCE          =  10,
    /// Undefined format. (Source: Modern)
    ///
    /// RPL_NONE is a dummy numeric. It does not have a defined use nor format.
    RPL_NONE            = 300,
    /// `302 :*1<reply> *( " " <reply> )` (Source: RFC2812)
    RPL_USERHOST        = 302,
    /// `303 :*1<nick> *( " " <nick> )` (Source: RFC2812)
    RPL_ISON            = 303,
    /// `301 <nick> :<away message>` (Source: RFC2812)
    RPL_AWAY            = 301,
    /// `305 :You are no longer marked as being away` (Source: RFC2812)
    RPL_UNAWAY          = 305,
    /// `306 :You have been marked as being away` (Source: RFC2812)
    RPL_NOWAWAY         = 306,
    /// `311 <nick> <user> <host> * :<real name>` (Source: RFC2812)
    RPL_WHOISUSER       = 311,
    /// `312 <nick> <server> :<server info>` (Source: RFC2812)
    RPL_WHOISSERVER     = 312,
    /// `313 <nick> :is an IRC operator` (Source: RFC2812)
    RPL_WHOISOPERATOR   = 313,
    /// `317 <nick> <integer> :seconds idle` (Source: RFC2812)
    RPL_WHOISIDLE       = 317,
    /// `318 <nick> :End of WHOIS list` (Source: RFC2812)
    RPL_ENDOFWHOIS      = 318,
    /// `319 <nick> :*( ( "@" / "+" ) <channel> " " )` (Source: RFC2812)
    RPL_WHOISCHANNELS   = 319,
    /// `314 <nick> <user> <host> * :<real name>` (Source: RFC2812)
    RPL_WHOWASUSER      = 314,
    /// `369 <nick> :End of WHOWAS` (Source: RFC2812)
    RPL_ENDOFWHOWAS     = 369,
    /// Obsolete. Not used. (Source: RFC2812)
    RPL_LISTSTART       = 321,
    /// `322 <channel> <# visible> :<topic>` (Source: RFC2812)
    RPL_LIST            = 322,
    /// `323 :End of LIST (Source: RFC2812)
    RPL_LISTEND         = 323,
    /// `325 <channel> <nickname>` (Source: RFC2812)
    RPL_UNIQOPIS        = 325,
    /// `324 <channel> <mode> <mode params>` (Source: RFC2812)
    RPL_CHANNELMODEIS   = 324,
    /// `331 <channel> :No topic is set` (Source: RFC2812)
    RPL_NOTOPIC         = 331,
    /// `332 <channel> :<topic>` (Source: RFC2812)
    RPL_TOPIC           = 332,
    /// `333 <channel> <nick>!<user>@<host> <unix timestamp>` (Source: RFC2812)
    RPL_TOPICWHOTIME    = 333,
    /// `341 <channel> <nick>` (Source: RFC2812)
    RPL_INVITING        = 341,
    /// `342 <user> :Summoning user to IRC` (Source: RFC2812)
    ///
    /// According to Modern, this response is rarely implemented. In practice, people simply message
    /// one another in a channel with their specified username in the message, rather than use the
    /// `SUMMON` command.
    RPL_SUMMONING       = 342,
    /// `346 <channel> <invitemask>` (Source: RFC2812)
    RPL_INVITELIST      = 346,
    /// `347 <channel> :End of channel invite list` (Source: RFC2812)
    ///
    /// According to Modern, `RPL_ENDOFEXCEPTLIST` (349) is frequently deployed for this same
    /// purpose and the difference will be noted in channel mode and the statement in the suffix.
    RPL_ENDOFINVITELIST = 347,
    /// `348 <channel> <exceptionmask>` (Source: RFC2812)
    RPL_EXCEPTLIST      = 348,
    /// `349 <channel> :End of channel exception list` (Source: RFC2812)
    RPL_ENDOFEXCEPTLIST = 349,
    /// `351 <version> <server> :<comments>` (Source: RFC2812/Modern)
    RPL_VERSION         = 351,
    /// `352 <channel> <user> <host> <server> <nick> ( "H" / "G" > ["*"] [ ( "@" / "+" ) ]
    /// :<hopcount> <real name>` (Source: RFC2812)
    RPL_WHOREPLY        = 352,
    /// `315 <name> :End of WHO list` (Source: RFC2812)
    RPL_ENDOFWHO        = 315,
    /// `353 ( "=" / "*" / "@" ) <channel> :[ "@" / "+" ] <nick> *( " " [ "@" / "+" ] <nick> )`
    /// (Source: RFC2812)
    RPL_NAMREPLY        = 353,
    /// `366 <channel> :End of NAMES list` (Source: RFC2812)
    RPL_ENDOFNAMES      = 366,
    /// `364 <mask> <server> :<hopcount> <server info>` (Source: RFC2812)
    RPL_LINKS           = 364,
    /// `365 <mask> :End of LINKS list` (Source: RFC2812)
    RPL_ENDOFLINKS      = 365,
    /// `367 <channel> <banmask>` (Source: RFC2812)
    RPL_BANLIST         = 367,
    /// `368 <channel> :End of channel ban list` (Source: RFC2812)
    RPL_ENDOFBANLIST    = 368,
    /// `371 :<string>` (Source: RFC2812)
    RPL_INFO            = 371,
    /// `374 :End of INFO list` (Source: RFC2812)
    RPL_ENDOFINFO       = 374,
    /// `375 :- <server> Message of the day -` (Source: RFC2812)
    RPL_MOTDSTART       = 375,
    /// `372 :- <text>` (Source: RFC2812)
    RPL_MOTD            = 372,
    /// `376 :End of MOTD command` (Source: RFC2812)
    RPL_ENDOFMOTD       = 376,
    /// `381 :You are now an IRC operator` (Source: RFC2812)
    RPL_YOUREOPER       = 381,
    /// `382 <config file> :Rehashing` (Source: RFC2812)
    RPL_REHASHING       = 382,
    /// `383 You are service <servicename>` (Source: RFC2812)
    RPL_YOURESERVICE    = 383,
    /// `391 <server> :<string showing server's local time>` (Source: RFC2812)
    RPL_TIME            = 391,
    /// `392 :UserID   Terminal  Host` (Source: RFC2812)
    RPL_USERSSTART      = 392,
    /// `393 :<username> <ttyline> <hostname>` (Source: RFC2812)
    RPL_USERS           = 393,
    /// `394 :End of users` (Source: RFC2812)
    RPL_ENDOFUSERS      = 394,
    /// `395 :Nobody logged in` (Source: RFC2812)
    RPL_NOUSERS         = 395,
    /// `396 <nickname> <host> :is now your displayed host` (Source: InspIRCd)
    ///
    /// This response code is sent after a user enables the user mode +x (host masking), and it is
    /// successfully enabled. The particular format described above is from InspIRCd, but the
    /// response code should be common amongst servers that support host masks.
    RPL_HOSTHIDDEN      = 396,
    /// `200 Link <version & debug level> <destination> <next server> V<protocol version>
    /// <link uptime in seconds> <backstream sendq> <upstream sendq>` (Source: RFC2812)
    RPL_TRACELINK       = 200,
    /// `201 Try. <class> <server>` (Source: RFC2812)
    RPL_TRACECONNECTING = 201,
    /// `202 H.S. <class> <server>` (Source: RFC2812)
    RPL_TRACEHANDSHAKE  = 202,
    /// `203 ???? <class> [<client IP address in dot form>]` (Source: RFC2812)
    RPL_TRACEUKNOWN     = 203,
    /// `204 Oper <class> <nick>` (Source: RFC2812)
    RPL_TRACEOPERATOR   = 204,
    /// `205 User <class> <nick>` (Source: RFC2812)
    RPL_TRACEUSER       = 205,
    /// `206 Serv <class> <int>S <int>C <server> <nick!user|*!*>@<host|server> V<protocol version>`
    /// (Source: RFC2812)
    RPL_TRACESERVER     = 206,
    /// `207 Service <class> <name> <type> <active type>` (Source: RFC2812)
    RPL_TRACESERVICE    = 207,
    /// `208 <newtype> 0 <client name>` (Source: RFC2812)
    RPL_TRACENEWTYPE    = 208,
    /// `209 Class <class> <count>` (Source: RFC2812)
    RPL_TRACECLASS      = 209,
    /// Unused. (Source: RFC2812)
    RPL_TRACERECONNECT  = 210,
    /// `261 File <logfile> <debug level>` (Source: RFC2812)
    RPL_TRACELOG        = 261,
    /// `262 <server name> <version & debug level> :End of TRACE` (Source: RFC2812)
    RPL_TRACEEND        = 262,
    /// `211 <linkname> <sendq> <sent messages> <sent Kbytes> <received messages> <received Kbytes>
    /// <time open>` (Source: RFC2812)
    RPL_STATSLINKINFO   = 211,
    /// `212 <command> <count> <byte count> <remote count>` (Source: RFC2812)
    RPL_STATSCOMMANDS   = 212,
    /// `219 <stats letter> :End of STATS report` (Source: RFC2812)
    RPL_ENDOFSTATS      = 219,
    /// `242 :Server Up %d days %d:%02d:%02d` (Source: RFC2812)
    RPL_STATSUPTIME     = 242,
    /// `243 O <hostmask> * <name>` (Source: RFC2812)
    RPL_STATSOLINE      = 243,
    /// `221 <user mode string>` (Source: RFC2812)
    RPL_UMODEIS         = 221,
    /// `234 <name> <server> <mask> <type> <hopcount> <info>` (Source: RFC2812)
    RPL_SERVLIST        = 234,
    /// `235 <mask> <type> :End of service listing` (Source: RFC2812)
    RPL_SERVLISTEND     = 235,
    /// `251 :There are <int> users and <int> services on <int> servers` (Source: RFC2812)
    RPL_LUSERCLIENT     = 251,
    /// `252 <integer> :operator(s) online` (Source: RFC2812)
    RPL_LUSEROP         = 252,
    /// `253 <integer> :unknown connection(s)` (Source: RFC2812)
    RPL_LUSERUNKNOWN    = 253,
    /// `254 <integer> :channels formed` (Source: RFC2812)
    RPL_LUSERCHANNELS   = 254,
    /// `255 :I have <integer> clients and <integer> servers` (Source: RFC2812)
    RPL_LUSERME         = 255,
    /// `256 <server> :Administrative info` (Source: RFC2812)
    RPL_ADMINME         = 256,
    /// `257 :<admin info>` (Source: RFC2812)
    RPL_ADMINLOC1       = 257,
    /// `258 :<admin info>` (Source: RFC2812)
    RPL_ADMINLOC2       = 258,
    /// `259 :<admin info>` (Source: RFC2812)
    RPL_ADMINEMAIL      = 259,
    /// `263 <command> :Please wait a while and try again.` (Source: RFC2812)
    RPL_TRYAGAIN        = 263,
    /// `265 <client> [<u> <m>] :Current local users <u>, max <m>` (Source: Modern)
    RPL_LOCALUSERS      = 265,
    /// `266 <client> [<u> <m>] :Current local users <u>, max <m>` (Source: Modern)
    RPL_GLOBALUSERS     = 266,
    /// `276 <client> <nick> :has client certificate fingerprint <fingerprint>` (Source: Modern)
    RPL_WHOISCERTFP     = 276,
    /// `730 <nick> :target[,target2]*` (Source: RFC2812)
    RPL_MONONLINE       = 730,
    /// `731 <nick> :target[,target2]*` (Source: RFC2812)
    RPL_MONOFFLINE      = 731,
    /// `732 <nick> :target[,target2]*` (Source: RFC2812)
    RPL_MONLIST         = 732,
    /// `733 <nick> :End of MONITOR list` (Source: RFC2812)
    RPL_ENDOFMONLIST    = 733,
    /// `760 <target> <key> <visibility> :<value>` (Source: RFC2812)
    RPL_WHOISKEYVALUE   = 760,
    /// `761 <target> <key> <visibility> :[<value>]` (Source: RFC2812)
    RPL_KEYVALUE        = 761,
    /// `762 :end of metadata` (Source: RFC2812)
    RPL_METADATAEND     = 762,
    /// `900 <nick> <nick>!<ident>@<host> <account> :You are now logged in as <user>` (Source:
    /// IRCv3)
    RPL_LOGGEDIN        = 900,
    /// `901 <nick> <nick>!<ident>@<host> :You are now logged out` (Source: IRCv3)
    RPL_LOGGEDOUT       = 901,
    /// `903 <nick> :SASL authentication successful` (Source: IRCv3)
    RPL_SASLSUCCESS     = 903,
    /// `908 <nick> <mechanisms> :are available SASL mechanisms` (Source: IRCv3)
    RPL_SASLMECHS       = 908,

    // Error replies
    /// `400 <client> <command>{ <subcommand>} :<info>` (Source: Modern)
    ///
    /// According to Modern, this error will be returned when the given command/subcommand could not
    /// be processed. It's a very general error, and should only be used when more specific numerics
    /// do not suffice.
    ERR_UNKNOWNERROR        = 400,
    /// `401 <nickname> :No such nick/channel` (Source: RFC2812)
    ERR_NOSUCHNICK          = 401,
    /// `402 <server name> :No such server` (Source: RFC2812)
    ERR_NOSUCHSERVER        = 402,
    /// `403 <channel name> :No such channel` (Source: RFC2812)
    ERR_NOSUCHCHANNEL       = 403,
    /// `404 <channel name> :Cannot send to channel` (Source: RFC2812)
    ERR_CANNOTSENDTOCHAN    = 404,
    /// `405 <channel name> :You have joined too many channels` (Source: RFC2812)
    ERR_TOOMANYCHANNELS     = 405,
    /// `406 <nickname> :There was no such nickname` (Source: RFC2812)
    ERR_WASNOSUCHNICK       = 406,
    /// `407 <target> :<error code> recipients. <abort message>` (Source: RFC2812)
    ERR_TOOMANYTARGETS      = 407,
    /// `408 <service name> :No such service` (Source: RFC2812)
    ERR_NOSUCHSERVICE       = 408,
    /// `409 :No origin specified` (Source: RFC2812)
    ERR_NOORIGIN            = 409,
    /// `411 :No recipient given (<command>)` (Source: RFC2812)
    ERR_NORECIPIENT         = 411,
    /// `412 :No text to send` (Source: RFC2812)
    ERR_NOTEXTTOSEND        = 412,
    /// `413 <mask> :No toplevel domain specified` (Source: RFC2812)
    ERR_NOTOPLEVEL          = 413,
    /// `414 <mask> :Wildcard in toplevel domain` (Source: RFC2812)
    ERR_WILDTOPLEVEL        = 414,
    /// `415 <mask> :Bad Server/host mask` (Source: RFC2812)
    ERR_BADMASK             = 415,
    /// `421 <command> :Unknown command` (Source: RFC2812)
    ERR_UNKNOWNCOMMAND      = 421,
    /// `422 :MOTD File is missing` (Source: RFC2812)
    ERR_NOMOTD              = 422,
    /// `423 <server> :No administrative info available` (Source: RFC2812)
    ERR_NOADMININFO         = 423,
    /// `424 :File error doing <file op> on <file>` (Source: RFC2812)
    ERR_FILEERROR           = 424,
    /// `431 :No nickname given` (Source: RFC2812)
    ERR_NONICKNAMEGIVEN     = 431,
    /// `432 <nick> :Erroneous nickname"` (Source: RFC2812)
    ERR_ERRONEOUSNICKNAME   = 432,
    /// `433 <nick> :Nickname is already in use` (Source: RFC2812)
    ERR_NICKNAMEINUSE       = 433,
    /// `436 <nick> :Nickname collision KILL from <user>@<host>` (Source: RFC2812)
    ERR_NICKCOLLISION       = 436,
    /// `437 <nick/channel> :Nick/channel is temporarily unavailable` (Source: RFC2812)
    ERR_UNAVAILRESOURCE     = 437,
    /// `441 <nick> <channel> :They aren't on that channel` (Source: RFC2812)
    ERR_USERNOTINCHANNEL    = 441,
    /// `442 <channel> :You're not on that channel` (Source: RFC2812)
    ERR_NOTONCHANNEL        = 442,
    /// `443 <user> <channel> :is already on channel` (Source: RFC2812)
    ERR_USERONCHANNEL       = 443,
    /// `444 <user> :User not logged in` (Source: RFC2812)
    ERR_NOLOGIN             = 444,
    /// `445 :SUMMON has been disabled` (Source: RFC2812)
    ERR_SUMMONDISABLED      = 445,
    /// `446 :USERS has been disabled` (Source: RFC2812)
    ERR_USERSDISABLED       = 446,
    /// `451 :You have not registered` (Source: RFC2812)
    ERR_NOTREGISTERED       = 451,
    /// `461 <command> :Not enough parameters` (Source: RFC2812)
    ERR_NEEDMOREPARAMS      = 461,
    /// `462 :Unauthorized command (already registered)` (Source: RFC2812)
    ERR_ALREADYREGISTRED    = 462,
    /// `463 :Your host isn't among the privileged` (Source: RFC2812)
    ERR_NOPERMFORHOST       = 463,
    /// `464 :Password incorrect` (Source: RFC2812)
    ERR_PASSWDMISMATCH      = 464,
    /// `465 :You are banned from this server` (Source: RFC2812)
    ERR_YOUREBANNEDCREEP    = 465,
    /// `466` (Source: RFC2812)
    ERR_YOUWILLBEBANNED     = 466,
    /// `467 <channel> :Channel key already set` (Source: RFC2812)
    ERR_KEYSET              = 467,
    /// `471 <channel> :Cannot join channel (+l)` (Source: RFC2812)
    ERR_CHANNELISFULL       = 471,
    /// `472 <char> :is unknown mode char to me for <channel>` (Source: RFC2812)
    ERR_UNKNOWNMODE         = 472,
    /// `473 <channel> :Cannot join channel (+i)` (Source: RFC2812)
    ERR_INVITEONLYCHAN      = 473,
    /// `474 <channel> :Cannot join channel (+b)` (Source: RFC2812)
    ERR_BANNEDFROMCHAN      = 474,
    /// `475 <channel> :Cannot join channel (+k)` (Source: RFC2812)
    ERR_BADCHANNELKEY       = 475,
    /// `476 <channel> :Bad Channel Mask` (Source: RFC2812)
    ERR_BADCHANMASK         = 476,
    /// `477 <channel> :Channel doesn't support modes` (Source: RFC2812)
    ERR_NOCHANMODES         = 477,
    /// `478 <channel> <char> :Channel list is full` (Source: RFC2812)
    ERR_BANLISTFULL         = 478,
    /// `481 :Permission Denied- You're not an IRC operator` (Source: RFC2812)
    ERR_NOPRIVILEGES        = 481,
    /// `482 <channel> :You're not channel operator` (Source: RFC2812)
    ERR_CHANOPRIVSNEEDED    = 482,
    /// `483 :You can't kill a server!` (Source: RFC2812)
    ERR_CANTKILLSERVER      = 483,
    /// `484 :Your connection is restricted!` (Source: RFC2812)
    ERR_RESTRICTED          = 484,
    /// `485 :You're not the original channel operator` (Source: RFC2812)
    ERR_UNIQOPPRIVSNEEDED   = 485,
    /// `491 :No O-lines for your host` (Source: RFC2812)
    ERR_NOOPERHOST          = 491,
    /// `501 :Unknown MODE flag` (Source: RFC2812)
    ERR_UMODEUNKNOWNFLAG    = 501,
    /// `502 :Cannot change mode for other users` (Source: RFC2812)
    ERR_USERSDONTMATCH      = 502,
    /// `723 <client> <priv> :Insufficient oper privileges.` (Source: Modern)
    ///
    /// Sent to an operator to indicate that they don't have the specific privileges to perform the
    /// desired action. The format and meaning of the privilege string is server-defined.
    ERR_NOPRIVS             = 723,
    /// `734 <nick> <limit> <targets> :Monitor list is full.` (Source: RFC2812)
    ERR_MONLISTFULL         = 734,
    /// `764 <target> :metadata limit reached` (Source: RFC2812)
    ERR_METADATALIMIT       = 764,
    /// `765 <target> :invalid metadata target` (Source: RFC2812)
    ERR_TARGETINVALID       = 765,
    /// `766 <key> :no matching key` (Source: RFC2812)
    ERR_NOMATCHINGKEY       = 766,
    /// `767 <key> :invalid metadata key` (Source: RFC2812)
    ERR_KEYINVALID          = 767,
    /// `768 <target> <key> :key not set` (Source: RFC2812)
    ERR_KEYNOTSET           = 768,
    /// `769 <target> <key> :permission denied` (Source: RFC2812)
    ERR_KEYNOPERMISSION     = 769,
    /// `902 <nick> :You must use a nick assigned to you.` (Source: IRCv3)
    ERR_NICKLOCKED          = 902,
    /// `904 <nick> :SASL authentication failed` (Source: IRCv3)
    ERR_SASLFAIL            = 904,
    /// `905 <nick> :SASL message too long` (Source: IRCv3)
    ERR_SASLTOOLONG         = 905,
    /// `906 <nick> :SASL authentication aborted` (Source: IRCv3)
    ERR_SASLABORT           = 906,
    /// `907 <nick> :You have already authenticated using SASL` (Source: IRCv3)
    ERR_SASLALREADY         = 907
}

impl Response {
    /// Determines whether or not this response is an error response.
    ///
    /// This error consideration is according to RFC2812, but is rather simplistic. It considers all
    /// response codes above 400 to be errors, which misclassifies some extensions (e.g. from IRCv3)
    /// that add responses and errors both in the same range (typically 700s or 900s).
    pub fn is_error(&self) -> bool {
        *self as u16 >= 400
    }
}

impl FromStr for Response {
    type Err = &'static str;
    fn from_str(s: &str) -> Result<Response, &'static str> {
        if let Ok(rc) = s.parse() {
            match Response::from_u16(rc) {
                Some(r) => Ok(r),
                None => Err("Failed to parse due to unknown response code."),
            }
        } else {
            Err("Failed to parse response code.")
        }
    }
}

#[cfg(test)]
mod test {
    use super::Response;

    #[test]
    fn is_error() {
        assert!(!Response::RPL_NAMREPLY.is_error());
        assert!(Response::ERR_NICKNAMEINUSE.is_error());
    }
}
