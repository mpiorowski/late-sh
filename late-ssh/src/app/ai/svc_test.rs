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

/// The shape the Flash models actually return on the grounded JSON path: told
/// to emit bare JSON, they fence it anyway (seen on 3.6-flash).
#[test]
fn extract_json_object_unwraps_a_fenced_reply() {
    let fenced = "```json\n{\n  \"summary\": \"a video\"\n}\n```";

    assert_eq!(
        extract_json_object(fenced),
        "{\n  \"summary\": \"a video\"\n}"
    );
}

#[test]
fn extract_json_object_unwraps_a_fence_with_no_language_tag() {
    assert_eq!(extract_json_object("```\n{\"a\": 1}\n```"), "{\"a\": 1}");
}

#[test]
fn extract_json_object_leaves_bare_json_untouched() {
    assert_eq!(
        extract_json_object("{\"summary\": \"x\"}"),
        "{\"summary\": \"x\"}"
    );
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

/// A fake Gemini on a local port: records the body and the key header of
/// every call, answers `reply` as the first candidate's text.
async fn fake_gemini(
    reply: &'static str,
) -> (
    String,
    std::sync::Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>>,
) {
    use axum::extract::State;
    use axum::http::HeaderMap;

    type Calls = std::sync::Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>>;
    let calls: Calls = Default::default();
    async fn answer(
        State((calls, reply)): State<(Calls, &'static str)>,
        headers: HeaderMap,
        axum::Json(body): axum::Json<serde_json::Value>,
    ) -> axum::Json<serde_json::Value> {
        let key = headers
            .get("x-goog-api-key")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        calls.lock().expect("calls").push((key, body));
        axum::Json(serde_json::json!({
            "candidates": [{"content": {"parts": [{"text": reply}]}}]
        }))
    }
    let app = axum::Router::new()
        .route("/v1beta/models/{call}", axum::routing::post(answer))
        .with_state((calls.clone(), reply));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    (format!("http://{addr}"), calls)
}

/// The pictures go first as inline PNGs, the prompt after them, the
/// schema enforced, no temperature, and the cap that leaves thinking over
/// images room to finish.
#[tokio::test]
async fn generate_json_with_images_sends_the_pictures_before_the_prompt() {
    use base64::Engine;

    let (endpoint, calls) = fake_gemini(r#"{"alt": "a cat"}"#).await;
    let ai = AiService::at_endpoint(true, Some("test-key".to_string()), format!("{endpoint}/"));
    let schema = serde_json::json!({"type": "object"});

    let reply = ai
        .generate_json_with_images(
            "a-model",
            "you look at pictures",
            &[b"first".as_slice(), b"second".as_slice()],
            "what is this?",
            schema.clone(),
        )
        .await
        .expect("call");

    assert_eq!(reply, Some(r#"{"alt": "a cat"}"#.to_string()));
    let calls = calls.lock().expect("calls");
    assert_eq!(calls.len(), 1);
    let (key, body) = &calls[0];
    assert_eq!(key, "test-key");
    let encode = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
    assert_eq!(
        body["contents"][0]["parts"],
        serde_json::json!([
            {"inlineData": {"mimeType": "image/png", "data": encode(b"first")}},
            {"inlineData": {"mimeType": "image/png", "data": encode(b"second")}},
            {"text": "what is this?"},
        ])
    );
    assert_eq!(
        body["systemInstruction"]["parts"],
        serde_json::json!([{"text": "you look at pictures"}])
    );
    assert_eq!(
        body["generationConfig"],
        serde_json::json!({
            "maxOutputTokens": 16384,
            "responseMimeType": "application/json",
            "responseSchema": schema,
        })
    );
}

#[tokio::test]
async fn generate_json_with_images_is_none_with_ai_off() {
    let (endpoint, calls) = fake_gemini("{}").await;
    let ai = AiService::at_endpoint(false, Some("test-key".to_string()), endpoint);

    let reply = ai
        .generate_json_with_images(
            "a-model",
            "system",
            &[b"png".as_slice()],
            "prompt",
            serde_json::json!({}),
        )
        .await
        .expect("call");

    assert_eq!(reply, None);
    assert!(calls.lock().expect("calls").is_empty());
}
