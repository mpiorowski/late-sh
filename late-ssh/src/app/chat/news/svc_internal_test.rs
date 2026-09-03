use super::{
    TweetIdentity, display_author, encode_ascii_payload, handle_from_author_url,
    is_ai_blocklisted_url, is_tweet_url, is_youtube_url, sanitize_payload_field, truncate_for_chat,
    tweet_date_from_oembed_html, tweet_status_id, tweet_summary, tweet_text_from_oembed_html,
    tweet_title,
};
use std::collections::HashMap;
use uuid::Uuid;

/// The exact `html` X returns for a post with a video attached, entities and
/// all. Everything the card shows is parsed back out of this one string.
const OEMBED_HTML: &str = "<blockquote class=\"twitter-tweet\" data-dnt=\"true\"><p lang=\"en\" dir=\"ltr\">This is GPT-6 Astra.<br><br>Anything you can do on a computer, Astra can do for you. Fast. <a href=\"https://t.co/gDd0IsewJw\">pic.twitter.com/gDd0IsewJw</a></p>&mdash; OpenAI (@OpenAI) <a href=\"https://x.com/OpenAI/status/2095595741528125780?ref_src=twsrc%5Etfw\">September 3, 2026</a></blockquote>";

#[test]
fn youtube_url_detection_covers_common_hosts() {
    assert!(is_youtube_url("https://www.youtube.com/watch?v=abc"));
    assert!(is_youtube_url("https://youtu.be/abc"));
    assert!(is_youtube_url("https://m.youtube.com/watch?v=abc"));
    assert!(!is_youtube_url("https://vimeo.com/123"));
}

#[test]
fn ai_blocklist_covers_cyberspace_and_nothing_else() {
    assert!(is_ai_blocklisted_url("https://cyberspace.online/odd/post"));
    assert!(is_ai_blocklisted_url(
        "https://api.cyberspace.online/v1/posts"
    ));
    assert!(!is_ai_blocklisted_url(
        "https://example.com/cyberspace.online"
    ));
    assert!(!is_ai_blocklisted_url("https://notcyberspace.online/post"));
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
fn tweet_url_detection_covers_post_shapes_only() {
    assert!(is_tweet_url("https://twitter.com/user/status/123"));
    assert!(is_tweet_url("https://x.com/user/status/123"));
    assert!(is_tweet_url("https://mobile.twitter.com/user/status/123"));
    assert!(is_tweet_url("https://x.com/i/web/status/123"));
    assert!(!is_tweet_url("https://youtube.com/watch?v=abc"));
    assert!(!is_tweet_url("not a url at all"));
}

/// A profile, search, or list URL has no post for oEmbed to resolve, so it
/// has to stay on the generic AI path rather than fail the share outright.
#[test]
fn non_post_x_urls_stay_off_the_tweet_path() {
    assert!(!is_tweet_url("https://x.com/OpenAI"));
    assert!(!is_tweet_url("https://x.com/search?q=rust"));
    assert!(!is_tweet_url("https://x.com/i/lists/123"));
    assert!(!is_tweet_url("https://x.com/OpenAI/status/not-an-id"));
}

#[test]
fn tweet_status_id_ignores_trailing_segments_and_tracking_params() {
    assert_eq!(
        tweet_status_id("https://x.com/OpenAI/status/2095595741528125780?s=20"),
        Some("2095595741528125780".to_string())
    );
    assert_eq!(
        tweet_status_id("https://x.com/OpenAI/status/2095595741528125780/video/1"),
        Some("2095595741528125780".to_string())
    );
}

#[test]
fn oembed_html_yields_the_posts_own_words() {
    assert_eq!(
        tweet_text_from_oembed_html(OEMBED_HTML),
        "This is GPT-6 Astra.\nAnything you can do on a computer, Astra can do for you. Fast."
    );
}

#[test]
fn oembed_html_yields_the_post_date() {
    assert_eq!(
        tweet_date_from_oembed_html(OEMBED_HTML),
        Some("September 3, 2026".to_string())
    );
}

/// A link the author typed is part of what they said; `pic.twitter.com/...`
/// is X's own media shortlink and only adds noise to the card.
#[test]
fn post_text_keeps_authored_links_and_drops_media_shortlinks() {
    let html = "<blockquote><p lang=\"en\">Read <a href=\"https://t.co/x\">example.com/post</a> now <a href=\"https://t.co/y\">pic.x.com/abc</a></p>&mdash; A (@a) <a href=\"https://x.com/a/status/1\">May 1, 2026</a></blockquote>";
    assert_eq!(
        tweet_text_from_oembed_html(html),
        "Read example.com/post now"
    );
}

#[test]
fn post_text_decodes_entities_without_double_decoding() {
    let html = "<blockquote><p lang=\"en\">Rust &amp; C&#39;s &lt;stdio.h&gt; &amp;lt;stays&amp;gt;</p></blockquote>";
    assert_eq!(
        tweet_text_from_oembed_html(html),
        "Rust & C's <stdio.h> &lt;stays&gt;"
    );
}

#[test]
fn post_without_text_yields_nothing_rather_than_a_blank_card() {
    let profile_html =
        "<a class=\"twitter-timeline\" href=\"https://x.com/OpenAI\">Posts by OpenAI</a>";
    assert!(tweet_text_from_oembed_html(profile_html).is_empty());
    assert_eq!(tweet_date_from_oembed_html(profile_html), None);
}

#[test]
fn handle_comes_from_the_structured_author_url() {
    assert_eq!(handle_from_author_url("https://x.com/OpenAI"), "OpenAI");
    assert_eq!(handle_from_author_url("not a url"), "");
}

#[test]
fn post_card_leads_with_the_author_and_the_opening_line() {
    assert_eq!(
        tweet_title("OpenAI", "This is GPT-6 Astra."),
        "OpenAI on X: This is GPT-6 Astra."
    );
}

#[test]
fn post_card_summary_is_the_post_then_the_attribution() {
    let identity = TweetIdentity {
        author_name: "OpenAI".to_string(),
        handle: "OpenAI".to_string(),
        text: String::new(),
        date: Some("September 3, 2026".to_string()),
    };
    let lines = vec!["This is GPT-6 Astra.", "Anything you can do, Astra can do."];

    assert_eq!(
        tweet_summary(&identity, &lines),
        "• This is GPT-6 Astra.\n• Anything you can do, Astra can do.\n• Posted by OpenAI (@OpenAI) on X, September 3, 2026."
    );
}

/// A post that oEmbed answers for but whose author or date did not parse
/// still has to produce a readable card rather than dangling punctuation.
#[test]
fn post_card_survives_missing_author_and_date() {
    let identity = TweetIdentity {
        author_name: String::new(),
        handle: String::new(),
        text: String::new(),
        date: None,
    };

    assert_eq!(tweet_title("", ""), "Post on X");
    assert_eq!(
        tweet_summary(&identity, &["hello"]),
        "• hello\n• Posted by an X account on X."
    );
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
