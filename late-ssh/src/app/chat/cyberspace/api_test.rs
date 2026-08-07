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
