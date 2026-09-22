use late_core::models::work_profile::WorkStatus;

use super::{dash_or, parse_contacts, render_markdown, split_paragraphs, status_label};

#[test]
fn paragraphs_drop_empty_and_trim() {
    let para = split_paragraphs("hello\n\n  world  \n");
    assert_eq!(para, vec!["hello", "world"]);
}

#[test]
fn status_label_spells_out_every_status() {
    assert_eq!(status_label(WorkStatus::Open), "open to work");
    assert_eq!(status_label(WorkStatus::Casual), "casually listening");
    assert_eq!(status_label(WorkStatus::NotLooking), "not looking");
}

#[test]
fn dash_or_handles_blank() {
    assert_eq!(dash_or(None), "—");
    assert_eq!(dash_or(Some("   ")), "—");
    assert_eq!(dash_or(Some(" rust ")), "rust");
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
