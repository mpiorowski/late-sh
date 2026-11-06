use super::text::{escape_html, split_paragraphs};

#[test]
fn paragraphs_drop_empty_lines_and_trim() {
    assert_eq!(
        split_paragraphs("  one \n\n two\n   \nthree"),
        vec!["one", "two", "three"]
    );
    assert!(split_paragraphs("\n  \n").is_empty());
}

#[test]
fn escape_html_covers_the_five_markup_characters() {
    assert_eq!(
        escape_html(r#"<a href="x">Tom & Jerry's</a>"#),
        "&lt;a href=&quot;x&quot;&gt;Tom &amp; Jerry&#39;s&lt;/a&gt;"
    );
    assert_eq!(escape_html("plain"), "plain");
}
