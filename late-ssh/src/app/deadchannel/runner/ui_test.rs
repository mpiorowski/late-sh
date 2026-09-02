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
