use crate::app::common::emoji::expand_shortcodes;

#[test]
fn known_shortcodes_become_emoji() {
    assert_eq!(expand_shortcodes(":thumbsup:"), "👍");
    assert_eq!(expand_shortcodes("nice :fire: work"), "nice 🔥 work");
    assert_eq!(expand_shortcodes(":+1: :tada:"), "👍 🎉");
}

#[test]
fn unknown_shortcodes_are_left_as_typed() {
    assert_eq!(
        expand_shortcodes("shipped it :hammer_time:"),
        "shipped it :hammer_time:"
    );
    assert_eq!(expand_shortcodes("::"), "::");
}

/// The two shapes that would break if the scanner were naive about colons.
#[test]
fn colons_in_ordinary_text_are_not_shortcodes() {
    assert_eq!(
        expand_shortcodes("see https://late.sh/docs"),
        "see https://late.sh/docs"
    );
    assert_eq!(expand_shortcodes("meet at 10:30:00"), "meet at 10:30:00");
    assert_eq!(expand_shortcodes("ratio 3:1"), "ratio 3:1");
}

/// An unknown code's closing colon can be the next one's opener, so the scanner
/// must not skip past it.
#[test]
fn a_real_shortcode_after_an_unknown_one_still_resolves() {
    assert_eq!(expand_shortcodes(":nope:fire:"), ":nope🔥");
    assert_eq!(expand_shortcodes("::fire:"), ":🔥");
}

#[test]
fn text_without_colons_is_returned_untouched() {
    assert!(matches!(
        expand_shortcodes("plain message"),
        std::borrow::Cow::Borrowed("plain message")
    ));
}
