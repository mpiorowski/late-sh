use super::*;

#[test]
fn parse_envelope_reads_documented_post_shape() {
    let body = r#"{
        "data": [
            {
                "postId": "abc123",
                "authorId": "uid",
                "authorUsername": "someone",
                "content": "markdown content",
                "title": "Optional Title",
                "slug": "optional-title",
                "topics": ["music", "linux"],
                "repliesCount": 5,
                "bookmarksCount": 2,
                "isPublic": false,
                "isNSFW": false,
                "attachments": [],
                "createdAt": "2026-03-27T10:12:01.516Z",
                "deleted": false
            }
        ],
        "cursor": "xyz789"
    }"#;
    let posts: Vec<CsPost> = parse_envelope(200, body).expect("parse feed");
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].post_id, "abc123");
    assert_eq!(posts[0].author_username, "someone");
    assert_eq!(posts[0].title.as_deref(), Some("Optional Title"));
    assert_eq!(posts[0].topics, vec!["music", "linux"]);
    assert_eq!(posts[0].replies_count, 5);
    assert!(posts[0].created_at.is_some());
}

#[test]
fn parse_envelope_tolerates_minimal_posts() {
    let body = r#"{ "data": [ { "postId": "p1" } ] }"#;
    let posts: Vec<CsPost> = parse_envelope(200, body).expect("parse minimal");
    assert_eq!(posts[0].post_id, "p1");
    assert!(posts[0].title.is_none());
    assert!(posts[0].topics.is_empty());
}

#[test]
fn parse_envelope_surfaces_api_errors() {
    let body = r#"{ "error": { "code": "RATE_LIMITED", "message": "Too many requests" } }"#;
    let result: Result<Vec<CsPost>, CsApiError> = parse_envelope(429, body);
    match result {
        Err(CsApiError::Api { code, message }) => {
            assert_eq!(code, "RATE_LIMITED");
            assert_eq!(message, "Too many requests");
        }
        other => panic!("expected api error, got {other:?}"),
    }
}

#[test]
fn parse_envelope_maps_garbage_to_transport() {
    let result: Result<Vec<CsPost>, CsApiError> = parse_envelope(502, "<html>bad gateway</html>");
    match result {
        Err(CsApiError::Transport(message)) => assert!(message.contains("502")),
        other => panic!("expected transport error, got {other:?}"),
    }
}

#[test]
fn notifications_name_the_post_they_are_about() {
    // Shapes taken from a live payload. A reply notification carries the
    // *post* id in targetId (the reply's own id lives in metadata), which is
    // what makes one jump work for both kinds.
    let body = r#"{
        "data": [
            {
                "id": "n1",
                "type": "reply",
                "actorUsername": "genghis_khan",
                "targetId": "3fpV5ovHddHqXS2BAhfj",
                "targetType": "reply",
                "metadata": { "replyId": "G3Om85TkG4YHWEpfKLmf" },
                "read": true
            },
            {
                "id": "n2",
                "type": "bookmark",
                "targetId": "3fpV5ovHddHqXS2BAhfj",
                "targetType": "post"
            },
            { "id": "n3", "type": "new_follower", "targetId": "some-user", "targetType": "user" },
            { "id": "n4", "type": "poke" }
        ]
    }"#;
    let notifications: Vec<CsNotification> = parse_envelope(200, body).expect("notifications");
    assert_eq!(notifications[0].post_id(), Some("3fpV5ovHddHqXS2BAhfj"));
    assert_eq!(notifications[1].post_id(), Some("3fpV5ovHddHqXS2BAhfj"));
    // A follow targets a person, and a poke targets nothing at all.
    assert_eq!(notifications[2].post_id(), None);
    assert_eq!(notifications[3].post_id(), None);
}

#[test]
fn void_endpoints_accept_a_body_with_nothing_in_it() {
    // Reply and mark-all-read ignore their payload. A 2xx that carries no
    // `data` is a success, not a transport failure: treating it as one makes
    // a landed reply look failed and invites the user to send it twice.
    parse_void(200, r#"{"data":null}"#).expect("null data is still a success");
    parse_void(204, "").expect("an empty body is still a success");
    let error = parse_void(401, r#"{"error":{"code":"UNAUTHORIZED","message":"nope"}}"#)
        .expect_err("errors still surface");
    assert!(matches!(error, CsApiError::Api { .. }));
}

#[test]
fn login_tokens_parse_with_and_without_refresh_token() {
    let login = r#"{ "data": { "idToken": "id-1", "refreshToken": "r-1", "rtdbToken": "x", "rtdbUrl": "https://example" } }"#;
    let tokens: LoginTokens = parse_envelope(200, login).expect("login tokens");
    assert_eq!(tokens.id_token, "id-1");
    assert_eq!(tokens.refresh_token.as_deref(), Some("r-1"));

    let refresh =
        r#"{ "data": { "idToken": "id-2", "rtdbToken": "x", "rtdbUrl": "https://example" } }"#;
    let tokens: LoginTokens = parse_envelope(200, refresh).expect("refresh tokens");
    assert_eq!(tokens.id_token, "id-2");
    assert!(tokens.refresh_token.is_none());
}

#[test]
fn circ_message_parses_both_style_shapes() {
    let one: CircMessage =
        serde_json::from_str(r#"{"id":"m1","content":"hi","style":"rainbow"}"#).expect("one style");
    assert_eq!(one.styles, vec!["rainbow".to_string()]);

    let many: CircMessage =
        serde_json::from_str(r#"{"id":"m2","content":"hi","style":["rainbow","blink"]}"#)
            .expect("chained styles");
    assert_eq!(
        many.styles,
        vec!["rainbow".to_string(), "blink".to_string()]
    );

    let none: CircMessage =
        serde_json::from_str(r#"{"id":"m3","content":"hi"}"#).expect("no style");
    assert!(none.styles.is_empty());
}

#[test]
fn display_text_decodes_art_and_drops_duplicated_attachment_captions() {
    // `style: "art"` is the one style that changes how content reads.
    let art: CircMessage =
        serde_json::from_str(r#"{"id":"m1","content":"XF8o44OEKV8v","style":"art"}"#)
            .expect("art message");
    assert_eq!(art.display_text(), r"\_(ツ)_/");

    // A caption that is just the attachment's own URL would print twice.
    let captionless: CircMessage = serde_json::from_str(
        r#"{"id":"m2","content":"https://cdn.example/a.png","imageUrl":"https://cdn.example/a.png"}"#,
    )
    .expect("attachment message");
    assert_eq!(captionless.display_text(), "");
    assert_eq!(captionless.attachment_label(), Some("[image]"));

    // A real caption survives alongside its attachment.
    let captioned: CircMessage = serde_json::from_str(
        r#"{"id":"m3","content":"look at this","imageUrl":"https://cdn.example/a.png"}"#,
    )
    .expect("captioned message");
    assert_eq!(captioned.display_text(), "look at this");

    // A deleted message is a tombstone whatever it used to carry.
    let deleted: CircMessage = serde_json::from_str(
        r#"{"id":"m4","content":"[DELETED]","deleted":true,"imageUrl":"https://cdn.example/a.png"}"#,
    )
    .expect("deleted message");
    assert_eq!(deleted.display_text(), "[deleted]");
    assert_eq!(deleted.attachment_label(), None);
}

#[test]
fn stream_frames_carry_window_arrival_and_deletion() {
    // The opening frame is the whole window, keyed by message id.
    let window = parse_circ_stream_frame(
        "event: put\ndata: {\"path\":\"/\",\"data\":{\"m2\":{\"content\":\"second\",\"timestamp\":2},\"m1\":{\"content\":\"first\",\"timestamp\":1}}}",
    )
    .expect("window frame");
    match window {
        CircStreamEvent::Window(messages) => {
            // Sorted oldest-first, and the map key becomes the id.
            let ids: Vec<&str> = messages.iter().map(|m| m.id.as_str()).collect();
            assert_eq!(ids, vec!["m1", "m2"]);
        }
        other => panic!("expected a window, got {other:?}"),
    }

    let arrival = parse_circ_stream_frame(
        "event: put\ndata: {\"path\":\"/m3\",\"data\":{\"content\":\"hello\",\"timestamp\":3}}",
    )
    .expect("arrival frame");
    match arrival {
        CircStreamEvent::Upsert(message) => {
            assert_eq!(message.id, "m3");
            assert_eq!(message.content, "hello");
        }
        other => panic!("expected an upsert, got {other:?}"),
    }

    // A delete rewrites a message already on screen rather than adding one:
    // listening only for arrivals leaves the deletion invisible.
    let deletion = parse_circ_stream_frame(
        "event: patch\ndata: {\"path\":\"/m3\",\"data\":{\"content\":\"[DELETED]\",\"deleted\":true}}",
    )
    .expect("patch frame");
    match deletion {
        CircStreamEvent::Patch { id, deleted, .. } => {
            assert_eq!(id, "m3");
            assert!(deleted);
        }
        other => panic!("expected a patch, got {other:?}"),
    }

    let removal = parse_circ_stream_frame("event: put\ndata: {\"path\":\"/m3\",\"data\":null}")
        .expect("null");
    assert!(matches!(removal, CircStreamEvent::Removed(id) if id == "m3"));

    // Keep-alives and unmodelled paths say nothing about the room.
    assert!(parse_circ_stream_frame("event: keep-alive\ndata: null").is_none());
    assert!(
        parse_circ_stream_frame(
            "event: patch\ndata: {\"path\":\"/m3/content\",\"data\":\"edited\"}"
        )
        .is_none()
    );
}

#[test]
fn presence_heartbeat_is_floored_against_a_hot_loop() {
    // A misbehaving response naming a zero cadence must not turn the
    // presence loop into a hot cycle of authenticated POSTs.
    let hot: CircPresence =
        parse_envelope(200, r#"{ "data": { "heartbeatMs": 0, "idleAfterMs": 60000 } }"#)
            .expect("parse hot presence");
    assert_eq!(hot.heartbeat_ms, CIRC_PRESENCE_MIN_HEARTBEAT_MS);

    // The cadence they actually publish stays theirs, untouched.
    let sane: CircPresence = parse_envelope(200, r#"{ "data": { "heartbeatMs": 30000 } }"#)
        .expect("parse sane presence");
    assert_eq!(sane.heartbeat_ms, 30_000);
}

#[test]
fn stream_buffer_keeps_multibyte_chars_whole_across_chunk_splits() {
    let frame =
        "event: put\ndata: {\"path\":\"/m1\",\"data\":{\"content\":\"caffè 🦀\",\"timestamp\":1}}\n\n";
    // Cut inside the 4-byte crab: each half alone is invalid UTF-8, which is
    // exactly where a TCP chunk boundary is allowed to land.
    let split = frame.find('🦀').expect("crab in frame") + 2;
    let bytes = frame.as_bytes();

    let mut buffer = CircStreamBuffer::default();
    assert!(
        buffer.push(&bytes[..split]).is_empty(),
        "half a frame must wait, not decode"
    );
    assert_eq!(buffer.push(&bytes[split..]), vec![frame.to_string()]);
    assert_eq!(buffer.pending_len(), 0);
}

#[test]
fn stream_buffer_drains_every_completed_frame_and_keeps_the_tail() {
    let mut buffer = CircStreamBuffer::default();
    let frames = buffer.push(b"event: put\ndata: 1\n\nevent: put\ndata: 2\n\nevent: pu");
    assert_eq!(
        frames,
        vec![
            "event: put\ndata: 1\n\n".to_string(),
            "event: put\ndata: 2\n\n".to_string(),
        ]
    );
    assert_eq!(buffer.pending_len(), "event: pu".len());
}
