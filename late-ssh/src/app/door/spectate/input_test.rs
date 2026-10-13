use ratatui::layout::Rect;

use super::{is_f2, wants_own_chat};

#[test]
fn f2_matches_both_encodings() {
    assert!(is_f2(b"\x1bOQ"));
    assert!(is_f2(b"\x1b[12~"));
    assert!(!is_f2(b"\x1bOP"), "F1 stays the game's help key");
    assert!(!is_f2(b"Q"));
}

/// A left press inside the pane asks for the composer; presses outside it,
/// releases, drags, the wheel and the other buttons are the game's (which
/// drops them).
#[test]
fn a_left_click_on_the_pane_wants_the_composer() {
    let pane = Rect::new(120, 1, 40, 30);
    // SGR coordinates are 1-based: the pane's first cell (120, 1) is 121;2.
    assert!(wants_own_chat(b"\x1b[<0;121;2M", pane));
    assert!(
        wants_own_chat(b"\x1b[<0;160;31M", pane),
        "the pane's far corner"
    );
    assert!(wants_own_chat(b"\x1b[<4;130;5M", pane), "shift-click");
    assert!(
        !wants_own_chat(b"\x1b[<0;120;2M", pane),
        "one cell left of the pane"
    );
    assert!(!wants_own_chat(b"\x1b[<0;161;2M", pane), "one cell past it");
    assert!(!wants_own_chat(b"\x1b[<0;121;2m", pane), "a release");
    assert!(!wants_own_chat(b"\x1b[<32;121;2M", pane), "a drag");
    assert!(!wants_own_chat(b"\x1b[<64;121;2M", pane), "the wheel");
    assert!(!wants_own_chat(b"\x1b[<2;121;2M", pane), "the right button");
    // A press batched behind motion reports still counts.
    assert!(wants_own_chat(b"\x1b[<35;10;10M\x1b[<0;130;5M", pane));
    assert!(!wants_own_chat(b"\x1b[<0;130", pane), "a split report");
    assert!(!wants_own_chat(b"j", pane));
}
