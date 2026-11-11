use super::spoiler::{has_spoiler, mask_spoilers, unwrap_spoilers};

#[test]
fn masks_each_spoiler_to_its_display_width() {
    assert_eq!(
        mask_spoilers("today is ||CRANE|| and ||日本||!"),
        "today is ▒▒▒▒▒ and ▒▒▒▒!"
    );
}

#[test]
fn unwrap_drops_markers_and_keeps_text() {
    assert_eq!(unwrap_spoilers("it was ||CRANE||"), "it was CRANE");
}

#[test]
fn plain_pipes_are_not_spoilers() {
    for body in [
        "a || b || c",
        "||||",
        "|| padded||",
        "||open only",
        "||across\nlines||",
        "x | y",
    ] {
        assert!(!has_spoiler(body), "{body}");
        assert_eq!(mask_spoilers(body), body, "{body}");
    }
}

#[test]
fn a_rejected_opener_does_not_swallow_a_later_spoiler() {
    assert_eq!(mask_spoilers("a || b ||c||"), "a || b ▒");
}
