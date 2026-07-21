use super::{display_author, parse_tags};
use std::collections::HashMap;
use uuid::Uuid;

#[test]
fn parse_tags_normalizes_and_dedupes() {
    let tags = parse_tags("Rust, CLI rust, web-dev");
    assert_eq!(tags, vec!["rust", "cli", "web-dev"]);
}

#[test]
fn parse_tags_strips_hash_and_filters_invalid() {
    let tags = parse_tags("#rust, !!!, ok");
    assert_eq!(tags, vec!["rust", "ok"]);
}

#[test]
fn parse_tags_caps_count() {
    let raw = (0..20)
        .map(|i| format!("tag{i}"))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(parse_tags(&raw).len(), 8);
}

#[test]
fn parse_tags_empty_input() {
    assert!(parse_tags("").is_empty());
    assert!(parse_tags("   ,  ").is_empty());
}

#[test]
fn display_author_prefers_username() {
    let id = Uuid::now_v7();
    let mut map = HashMap::new();
    map.insert(id, "alice".to_string());
    assert_eq!(display_author(&map, id), "alice");
}

#[test]
fn display_author_falls_back_to_short_id() {
    let id = Uuid::now_v7();
    let map = HashMap::new();
    assert_eq!(display_author(&map, id), id.to_string()[..8]);
}
