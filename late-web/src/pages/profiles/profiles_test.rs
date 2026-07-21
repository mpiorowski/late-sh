use super::{
    dash_or, parse_contacts, render_markdown, split_paragraphs, status_id, status_label,
    status_priority, summary_preview,
};

#[test]
fn paragraphs_drop_empty_and_trim() {
    let para = split_paragraphs("hello\n\n  world  \n");
    assert_eq!(para, vec!["hello", "world"]);
}

#[test]
fn status_helpers_map_known_values() {
    assert_eq!(status_id("open"), "open");
    assert_eq!(status_id("nope"), "unknown");
    assert_eq!(status_label("not-looking"), "not looking");
}

#[test]
fn dash_or_handles_blank() {
    assert_eq!(dash_or(None), "—");
    assert_eq!(dash_or(Some("   ")), "—");
    assert_eq!(dash_or(Some(" rust ")), "rust");
}

#[test]
fn status_priority_orders_open_first() {
    let mut statuses = vec!["not-looking", "open", "casual", "weird"];
    statuses.sort_by_key(|s| status_priority(s));
    assert_eq!(statuses, vec!["open", "casual", "not-looking", "weird"]);
}

#[test]
fn summary_preview_collapses_whitespace_and_truncates_on_word() {
    assert_eq!(
        summary_preview("hello\n\n  there  friend", 80),
        "hello there friend"
    );
    let preview = summary_preview("alpha beta gamma delta epsilon zeta", 18);
    assert_eq!(preview, "alpha beta gamma…");
}

#[test]
fn render_markdown_renders_headings_and_lists() {
    let html = render_markdown("# Hi\n\n- one\n- two");
    assert!(html.contains("<h1>Hi</h1>"));
    assert!(html.contains("<li>one</li>"));
}

#[test]
fn render_markdown_strips_raw_html() {
    // Raw HTML in source must not survive — author content is untrusted.
    let html = render_markdown("hello <script>alert(1)</script> world");
    assert!(!html.contains("<script>"));
    assert!(html.contains("hello"));
    assert!(html.contains("world"));
}

#[test]
fn render_markdown_empty_returns_empty() {
    assert_eq!(render_markdown(""), "");
    assert_eq!(render_markdown("   \n  "), "");
}

#[test]
fn parse_contacts_splits_on_commas_and_trims() {
    let items = parse_contacts("foo@bar.com, DM on late.sh ,  ");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].value, "foo@bar.com");
    assert_eq!(items[0].href, "mailto:foo@bar.com");
    assert_eq!(items[1].value, "DM on late.sh");
    assert_eq!(items[1].href, "");
}

#[test]
fn parse_contacts_links_urls_but_not_bare_text() {
    let items = parse_contacts("https://t.me/me, just say hi");
    assert_eq!(items[0].href, "https://t.me/me");
    assert_eq!(items[1].href, "");
}

#[test]
fn parse_contacts_empty_input_yields_empty_list() {
    assert!(parse_contacts("").is_empty());
    assert!(parse_contacts("   ,  , ").is_empty());
}
