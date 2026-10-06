use crate::ircd::replies::*;
use irc_proto::{Command, Response};

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

#[test]
fn topic_is_332_when_set_and_331_when_not() {
    assert_eq!(
        topic("alice", "#books", Some("what we are reading"))
            .to_string()
            .trim_end(),
        ":irc.late.sh 332 alice #books :what we are reading"
    );
    assert_eq!(
        topic("alice", "#books", None).to_string().trim_end(),
        ":irc.late.sh 331 alice #books :No topic is set"
    );
    assert_eq!(
        topic("alice", "#books", Some("   ")).to_string().trim_end(),
        ":irc.late.sh 331 alice #books :No topic is set",
        "a blank topic is no topic"
    );
}
