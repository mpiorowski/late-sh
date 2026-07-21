use crate::app::common::mentions::*;
use crate::app::common::theme;
use ratatui::style::{Modifier, Style};

#[test]
fn extract_single_mention() {
    assert_eq!(extract_mentions("hey @alice"), vec!["alice"]);
}

#[test]
fn extract_multiple_mentions() {
    let result = extract_mentions("hey @alice and @Bob");
    assert_eq!(result, vec!["alice", "bob"]);
}

#[test]
fn extract_deduplicates() {
    let result = extract_mentions("@alice @Alice @ALICE");
    assert_eq!(result, vec!["alice"]);
}

#[test]
fn extract_ignores_email() {
    assert!(extract_mentions("mail me at hi@example.com").is_empty());
}

#[test]
fn extract_ignores_bare_at() {
    assert!(extract_mentions("just @ here").is_empty());
}

#[test]
fn extract_stops_at_punctuation() {
    let result = extract_mentions("@alice, nice one");
    assert_eq!(result, vec!["alice"]);
}

#[test]
fn extract_handles_mention_with_special_chars() {
    let result = extract_mentions("hi @night-owl_123");
    assert_eq!(result, vec!["night-owl_123"]);
}

#[test]
fn extract_ignores_mentions_inside_inline_code() {
    assert!(extract_mentions("ping `@alice` later").is_empty());
    assert_eq!(extract_mentions("ping `@alice` then @bob"), vec!["bob"]);
}

#[test]
fn mention_spans_highlight_mentions() {
    let spans = mention_spans("hey @alice and @bob", Style::default());
    assert_eq!(spans.len(), 4);
    assert_eq!(spans[0].content.as_ref(), "hey ");
    assert_eq!(spans[1].content.as_ref(), "@alice");
    assert_eq!(spans[2].content.as_ref(), " and ");
    assert_eq!(spans[3].content.as_ref(), "@bob");
    assert_eq!(spans[1].style.fg, Some(theme::MENTION()));
    assert_eq!(spans[3].style.fg, Some(theme::MENTION()));
    assert!(spans[1].style.add_modifier.contains(Modifier::BOLD));
}

#[test]
fn mention_spans_ignore_email_addresses() {
    let spans = mention_spans("mail me at hi@example.com", Style::default());
    assert_eq!(spans.len(), 1);
    assert_eq!(spans[0].content.as_ref(), "mail me at hi@example.com");
    assert_eq!(spans[0].style.fg, None);
}

#[test]
fn mention_spans_stop_before_trailing_punctuation() {
    let spans = mention_spans("@alice, nice one", Style::default());
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].content.as_ref(), "@alice");
    assert_eq!(spans[1].content.as_ref(), ", nice one");
}

#[test]
fn mention_spans_ignore_mentions_inside_inline_code() {
    let spans = mention_spans("`@alice` and @bob", Style::default());
    assert_eq!(spans.len(), 2);
    assert_eq!(spans[0].content.as_ref(), "`@alice` and ");
    assert_eq!(spans[1].content.as_ref(), "@bob");
}
