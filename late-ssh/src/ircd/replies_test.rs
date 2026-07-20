use irc_proto::{Command, Response};
use crate::ircd::replies::*;

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
