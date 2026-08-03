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
fn unauthorized_detection_matches_only_the_auth_code() {
    let unauthorized = CsApiError::Api {
        code: "UNAUTHORIZED".to_string(),
        message: "Missing or invalid token".to_string(),
    };
    let forbidden = CsApiError::Api {
        code: "FORBIDDEN".to_string(),
        message: "Not allowed".to_string(),
    };
    assert!(unauthorized.is_unauthorized());
    assert!(!forbidden.is_unauthorized());
    assert!(!CsApiError::Transport("boom".to_string()).is_unauthorized());
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
