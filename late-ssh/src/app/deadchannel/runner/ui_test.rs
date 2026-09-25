use super::super::state::{Look, Tint};
use super::*;

#[test]
fn a_portrait_is_the_three_worn_rows_in_their_tints() {
    let look = Look::parse(&serde_json::json!({
        "hood": {"piece": "hood.cross", "tint": "amber"},
        "eyes": {"piece": "eyes.gem", "tint": "white"},
        "coat": {"piece": "coat.heavy", "tint": "static"},
        "mark": {"glyph": "▚"}
    }))
    .expect("parse");

    let spans = portrait_spans(&look);
    let rows = spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(rows, vec![" ╬═╬ ", "▐◈ ◈▌", " ▟▓▙ "]);
    assert_eq!(spans[0].style.fg, Some(tint_color(Tint::Amber)));
    assert_eq!(spans[1].style.fg, Some(tint_color(Tint::White)));
    assert_eq!(spans[2].style.fg, Some(tint_color(Tint::Static)));
}

/// The badge is the mark and the level as one token, and its color climbs
/// the bands the portrait tints come from: nothing past the top of the
/// ladder paints anything but white.
#[test]
fn the_badge_is_the_mark_and_the_level_in_the_levels_band() {
    let look = Look::parse(&serde_json::json!({
        "hood": {"piece": "hood.cross", "tint": "amber"},
        "eyes": {"piece": "eyes.gem", "tint": "white"},
        "coat": {"piece": "coat.heavy", "tint": "static"},
        "mark": {"glyph": "▚"}
    }))
    .expect("parse");

    assert_eq!(badge_text(&RunnerEntry { look, level: 7 }), "▚7");
    assert_eq!(badge_text(&RunnerEntry { look, level: 15 }), "▚15");
    assert_eq!(level_color(1), tint_color(Tint::Static));
    assert_eq!(level_color(4), tint_color(Tint::Static));
    assert_eq!(level_color(5), tint_color(Tint::Amber));
    assert_eq!(level_color(9), tint_color(Tint::Amber));
    assert_eq!(level_color(10), tint_color(Tint::Phosphor));
    assert_eq!(level_color(14), tint_color(Tint::Phosphor));
    assert_eq!(level_color(15), tint_color(Tint::White));
    assert_eq!(level_color(40), tint_color(Tint::White));
}
