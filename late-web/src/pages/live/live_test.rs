use super::valid_capability_id;

#[test]
fn capability_ids_are_hex_dash_tokens_only() {
    assert!(valid_capability_id("0198a2f4c3f07f4e8a7bde12ab34cd56"));
    assert!(valid_capability_id("abc-123"));

    // Anything that could reshape the proxied internal path is rejected.
    assert!(!valid_capability_id(""));
    assert!(!valid_capability_id(".."));
    assert!(!valid_capability_id("a/b"));
    assert!(!valid_capability_id("a?b=1"));
    assert!(!valid_capability_id("a%2e%2e"));
    assert!(!valid_capability_id(&"a".repeat(65)));
}

#[test]
fn watch_and_golive_pages_render_with_the_id_embedded() {
    use askama::Template;

    let watch = super::WatchPage {
        stream_id: "abc123",
    }
    .render()
    .expect("watch page renders");
    assert!(watch.contains("abc123"));
    assert!(
        watch.contains("muted"),
        "the watch page must be born silent"
    );

    let golive = super::GoLivePage {
        publish_token: "tok456",
    }
    .render()
    .expect("golive page renders");
    assert!(golive.contains("tok456"));
    assert!(
        golive.contains("mic: off"),
        "the go-live page mic starts off"
    );
}
