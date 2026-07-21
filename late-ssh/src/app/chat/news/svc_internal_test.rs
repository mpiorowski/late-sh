use super::{
    ArticleExtraction, display_author, encode_ascii_payload, extraction_looks_not_found,
    is_twitter_url, is_youtube_url, sanitize_payload_field, truncate_for_chat,
};
use std::collections::HashMap;
use uuid::Uuid;

#[test]
fn youtube_url_detection_covers_common_hosts() {
    assert!(is_youtube_url("https://www.youtube.com/watch?v=abc"));
    assert!(is_youtube_url("https://youtu.be/abc"));
    assert!(is_youtube_url("https://m.youtube.com/watch?v=abc"));
    assert!(!is_youtube_url("https://vimeo.com/123"));
}

#[test]
fn not_found_detection_flags_low_confidence_ai_output() {
    let extraction = ArticleExtraction {
        title: "Video Not Found".to_string(),
        image_url: None,
        summary: "• No content details are available to generate a summary.".to_string(),
    };
    assert!(extraction_looks_not_found(&extraction));
}

#[test]
fn not_found_detection_allows_normal_extractions() {
    let extraction = ArticleExtraction {
        title: "Never Run claude /init".to_string(),
        image_url: Some("https://i.ytimg.com/vi/abc/default.jpg".to_string()),
        summary: "• Explains tradeoffs of generated context files.".to_string(),
    };
    assert!(!extraction_looks_not_found(&extraction));
}

#[test]
fn display_author_prefers_username() {
    let user_id = Uuid::now_v7();
    let mut usernames = HashMap::new();
    usernames.insert(user_id, "mat".to_string());
    assert_eq!(display_author(&usernames, user_id), "mat");
}

#[test]
fn display_author_falls_back_to_short_id() {
    let user_id = Uuid::now_v7();
    let usernames = HashMap::new();
    assert_eq!(
        display_author(&usernames, user_id),
        user_id.to_string()[..8]
    );
}

#[test]
fn encode_summary_bullets_preserves_all_bullets() {
    let summary = "• first point\n• second point\n• third point";
    assert_eq!(
        super::encode_summary_bullets(summary),
        "first point\\nsecond point\\nthird point"
    );
}

#[test]
fn encode_summary_bullets_empty_input() {
    assert_eq!(super::encode_summary_bullets(""), "");
}

#[test]
fn encode_summary_bullets_skips_no_content_lines() {
    let summary = "• No content details are available.\n• Actual point";
    assert_eq!(super::encode_summary_bullets(summary), "Actual point");
}

// --- truncate_for_chat ---

#[test]
fn truncate_for_chat_returns_short_string_unchanged() {
    assert_eq!(truncate_for_chat("hello", 10), "hello");
}

#[test]
fn truncate_for_chat_at_exact_limit() {
    assert_eq!(truncate_for_chat("abcde", 5), "abcde");
}

#[test]
fn truncate_for_chat_adds_ellipsis_when_over_limit() {
    assert_eq!(truncate_for_chat("abcdefghij", 7), "abcd...");
}

// --- sanitize_payload_field ---

#[test]
fn sanitize_payload_field_replaces_separator() {
    let input = format!("before{}after", super::NEWS_SEPARATOR);
    assert_eq!(sanitize_payload_field(&input), "before | after");
}

#[test]
fn sanitize_payload_field_replaces_newlines() {
    assert_eq!(sanitize_payload_field("a\nb\rc"), "a b c");
}

// --- encode_ascii_payload ---

#[test]
fn encode_ascii_payload_encodes_newlines() {
    assert_eq!(encode_ascii_payload("a\nb"), "a\\nb");
}

#[test]
fn encode_ascii_payload_escapes_backslashes() {
    assert_eq!(encode_ascii_payload("a\\b"), "a\\\\b");
}

#[test]
fn encode_ascii_payload_handles_both() {
    assert_eq!(encode_ascii_payload("a\\b\nc"), "a\\\\b\\nc");
}

// --- edge cases for existing functions ---

#[test]
fn display_author_ignores_whitespace_only_username() {
    let user_id = Uuid::now_v7();
    let mut usernames = HashMap::new();
    usernames.insert(user_id, "   ".to_string());
    assert_eq!(
        display_author(&usernames, user_id),
        user_id.to_string()[..8]
    );
}

#[test]
fn is_youtube_url_detects_nocookie_domain() {
    assert!(is_youtube_url("https://www.youtube-nocookie.com/embed/abc"));
}

#[test]
fn is_youtube_url_rejects_invalid_url() {
    assert!(!is_youtube_url("not a url at all"));
}

#[test]
fn twitter_url_detection_covers_common_hosts() {
    assert!(is_twitter_url("https://twitter.com/user/status/123"));
    assert!(is_twitter_url("https://x.com/user/status/123"));
    assert!(is_twitter_url("https://mobile.twitter.com/user/status/123"));
    assert!(!is_twitter_url("https://youtube.com/watch?v=abc"));
    assert!(!is_twitter_url("not a url at all"));
}

#[test]
fn build_news_chat_announcement_is_compact_and_branded() {
    let msg = super::build_news_chat_announcement(
        "A very cool post title",
        "• one interesting summary point\n• another point",
        "https://example.com/article",
        ".:-\n+*#",
    );
    assert!(msg.starts_with(super::NEWS_MARKER));
    assert!(msg.contains(super::NEWS_SEPARATOR));
    assert!(msg.contains("A very cool post title"));
    assert!(msg.contains("one interesting summary point"));
    assert!(msg.contains("\\n"));
}
