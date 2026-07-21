use super::{ComposerField, clamp_index, looks_like_url, move_index};

#[test]
fn field_cycles_forward_and_back() {
    assert_eq!(ComposerField::Title.next(), ComposerField::Url);
    assert_eq!(ComposerField::Description.next(), ComposerField::Title);
    assert_eq!(ComposerField::Title.prev(), ComposerField::Description);
}

#[test]
fn url_validation_requires_scheme() {
    assert!(looks_like_url("https://late.sh"));
    assert!(looks_like_url("http://example.com"));
    assert!(!looks_like_url("late.sh"));
    assert!(!looks_like_url("ftp://x"));
}

#[test]
fn clamp_index_handles_empty_list() {
    assert_eq!(clamp_index(4, 0), 0);
    assert_eq!(clamp_index(9, 3), 2);
}

#[test]
fn move_index_clamps_at_edges() {
    assert_eq!(move_index(0, -1, 5), 0);
    assert_eq!(move_index(4, 1, 5), 4);
    assert_eq!(move_index(2, 2, 5), 4);
}
