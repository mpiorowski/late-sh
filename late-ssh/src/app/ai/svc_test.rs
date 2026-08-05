use super::*;

#[test]
fn first_text_reads_the_reply() {
    let body = r#"{
        "candidates": [
            {"content": {"parts": [{"text": "{\"title\": \"a post\"}"}]}}
        ]
    }"#;

    assert_eq!(
        first_text("test", body).unwrap(),
        Some("{\"title\": \"a post\"}".to_string())
    );
}

/// The shape the news pipeline has been getting since the model moved to 3.6:
/// a perfectly valid 200 whose candidate carries no text at all. It still comes
/// back as `None` (callers are unchanged for now), but it must no longer pass
/// silently — the raw body is the only record of why.
#[test]
fn first_text_is_none_for_a_candidate_with_no_content() {
    let body = r#"{"candidates": [{"finishReason": "MAX_TOKENS"}]}"#;

    assert_eq!(first_text("test", body).unwrap(), None);
}

#[test]
fn first_text_is_none_for_a_blocked_prompt() {
    let body = r#"{"candidates": [], "promptFeedback": {"blockReason": "SAFETY"}}"#;

    assert_eq!(first_text("test", body).unwrap(), None);
}

#[test]
fn first_text_errors_on_a_body_that_is_not_gemini_json() {
    assert!(first_text("test", "<html>502 Bad Gateway</html>").is_err());
}
