use super::{
    history::{self, Request},
    replies,
};
use irc_proto::{Command, message::Tag};
use late_core::models::irc_history::{Reference, Selection};
use uuid::Uuid;

fn parse(command: &str) -> Result<Request, history::RequestError> {
    history::parse(
        &command
            .split_whitespace()
            .map(str::to_string)
            .collect::<Vec<_>>(),
    )
}

#[test]
fn history_parser_selectors_and_limits() {
    let id = Uuid::new_v4();
    assert_eq!(
        parse("latest #room * 999").unwrap(),
        Request::Messages {
            target: "#room".into(),
            selection: Selection::Latest(None),
            limit: 300,
        }
    );
    for (name, selection) in [
        ("BEFORE", Selection::Before(Reference::MessageId(id))),
        ("AFTER", Selection::After(Reference::MessageId(id))),
        ("AROUND", Selection::Around(Reference::MessageId(id))),
        ("LATEST", Selection::Latest(Some(Reference::MessageId(id)))),
    ] {
        assert_eq!(
            parse(&format!("{name} ##room msgid={id} 7")).unwrap(),
            Request::Messages {
                target: "##room".into(),
                selection,
                limit: 7,
            }
        );
    }
    assert!(matches!(
        parse(&format!(
            "BETWEEN #room msgid={id} timestamp=2026-10-13T12:00:00.000Z 10"
        ))
        .unwrap(),
        Request::Messages {
            selection: Selection::Between(_, _),
            ..
        }
    ));
    assert!(matches!(
        parse("TARGETS timestamp=2026-10-01T00:00:00.000Z timestamp=2026-11-01T00:00:00.000Z 4")
            .unwrap(),
        Request::Targets { limit: 4, .. }
    ));
}

#[test]
fn history_parser_rejects_invalid_requests() {
    for command in [
        "",
        "NOPE #room * 2",
        "LATEST #room *",
        "LATEST #room * 2 extra",
        "LATEST #room * 0",
        "LATEST #room * -1",
        "LATEST #room * +1",
        "LATEST #room * 18446744073709551616",
        "LATEST #room * 1.5",
        "BEFORE #room msgid=garbage 1",
        "AFTER #room timestamp=garbage 2",
        "AFTER #room timestamp=2026-10-13T00:00:00Z 2",
        "AFTER #room timestamp=2026-10-13T00:00:00.000001Z 2",
        "AFTER #room timestamp=2026-10-13T00:00:00.000+00:00 2",
        "TARGETS msgid=00000000-0000-0000-0000-000000000001 timestamp=2026-10-13T00:00:00.000Z 2",
    ] {
        assert_eq!(
            parse(command).unwrap_err().code,
            "INVALID_PARAMS",
            "{command}"
        );
    }
    assert_eq!(
        parse("BEFORE #room other=foo 5").unwrap_err().code,
        "INVALID_MSGREFTYPE"
    );
    assert!(parse("BEFORE #room * 5").is_err());
}

#[test]
fn history_batches_are_balanced_and_tag_every_line() {
    let messages = vec![replies::from_user_with_tags(
        "alice",
        Command::PRIVMSG("#room".into(), "hello world".into()),
        vec![Tag("time".into(), Some("2026-10-13T00:00:00.000Z".into()))],
    )];
    let batch = history::batch(messages.clone(), Some("#room"), true, true);
    let Command::BATCH(start, Some(kind), Some(target)) = &batch[0].command else {
        panic!("batch start")
    };
    assert_eq!(kind.to_str(), "chathistory");
    assert_eq!(target, &["#room"]);
    let id = start.strip_prefix('+').unwrap();
    assert!(
        batch[0]
            .tags
            .as_ref()
            .unwrap()
            .iter()
            .any(|tag| tag.0 == "draft/chathistory-end")
    );
    assert!(
        batch[1]
            .tags
            .as_ref()
            .unwrap()
            .contains(&Tag("batch".into(), Some(id.into())))
    );
    assert!(
        batch[1]
            .tags
            .as_ref()
            .unwrap()
            .iter()
            .any(|tag| tag.0 == "time")
    );
    assert_eq!(
        batch[2].command,
        Command::BATCH(format!("-{id}"), None, None)
    );
    assert_eq!(
        history::batch(messages.clone(), Some("#room"), false, true),
        messages
    );
    let empty = history::batch(Vec::new(), None, true, false);
    assert_eq!(empty.len(), 2);
    assert!(empty[0].to_string().contains("draft/chathistory-targets"));
    assert!(empty[0].tags.is_none());
}
