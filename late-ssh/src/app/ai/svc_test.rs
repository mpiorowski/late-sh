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

/// The shape 3.6-flash actually returns on the grounded JSON path: told to
/// emit bare JSON, it fences it anyway.
#[test]
fn extract_json_object_unwraps_a_fenced_reply() {
    let fenced = "```json\n{\n  \"summary\": \"a video\"\n}\n```";

    assert_eq!(extract_json_object(fenced), "{\n  \"summary\": \"a video\"\n}");
}

#[test]
fn extract_json_object_unwraps_a_fence_with_no_language_tag() {
    assert_eq!(extract_json_object("```\n{\"a\": 1}\n```"), "{\"a\": 1}");
}

#[test]
fn extract_json_object_leaves_bare_json_untouched() {
    assert_eq!(extract_json_object("{\"summary\": \"x\"}"), "{\"summary\": \"x\"}");
}

/// Grounded replies also arrive with prose around the fence: a preamble,
/// trailing grounding notes, an uppercase language tag. Any remnant fails the
/// caller's parse and aborts the whole article share, so the JSON has to come
/// out of all of them.
#[test]
fn extract_json_object_survives_prose_around_the_fence() {
    let reply = "Here is the JSON:\n```JSON\n{\"summary\": \"x\"}\n```\nSources: example.com";

    assert_eq!(extract_json_object(reply), "{\"summary\": \"x\"}");
}
