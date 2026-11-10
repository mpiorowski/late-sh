use super::*;

#[test]
fn the_line_names_the_help_topic_and_the_listen_page_without_its_scheme() {
    assert_eq!(
        line("https://late.sh/listen"),
        "Plain ssh plays nothing: install late (? Pair) or open late.sh/listen"
    );
}
