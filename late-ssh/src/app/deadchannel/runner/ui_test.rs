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

/// The badge is the mark and the level as one token, and its color is the
/// newest tint the level unlocked at the tailor: nothing past the top of
/// the ladder paints anything but white.
#[test]
fn the_badge_is_the_mark_and_the_level_in_the_levels_band() {
    let look = Look::parse(&serde_json::json!({
        "hood": {"piece": "hood.cross", "tint": "amber"},
        "eyes": {"piece": "eyes.gem", "tint": "white"},
        "coat": {"piece": "coat.heavy", "tint": "static"},
        "mark": {"glyph": "▚"}
    }))
    .expect("parse");

    let entry = |level, marks| RunnerEntry {
        look,
        level,
        peak_level: 15,
        marks,
    };
    assert_eq!(badge_text(&entry(7, 0)), "▚7");
    assert_eq!(badge_text(&entry(15, 0)), "▚15");
    assert_eq!(badge_text(&entry(3, 2)), "▚3╬2", "the marks ride behind the Signal's glyph");
    let colors = (1..=16).map(level_color).collect::<Vec<_>>();
    let expected = [
        Tint::Static,
        Tint::Static,
        Tint::Static,
        Tint::Phosphor,
        Tint::Phosphor,
        Tint::Phosphor,
        Tint::Cyan,
        Tint::Cyan,
        Tint::Cyan,
        Tint::Magenta,
        Tint::Magenta,
        Tint::Magenta,
        Tint::Red,
        Tint::Red,
        Tint::White,
        Tint::White,
    ]
    .map(tint_color);
    assert_eq!(colors, expected);
}
