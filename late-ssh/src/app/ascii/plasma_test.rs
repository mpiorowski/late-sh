//! Golden frames from ascii.rest's own `plasma.ts` (node, `frame(t)` at
//! t = 0, 1 and 2.5, at its native 64x22): the port draws exactly what the
//! original does at that size.

use super::frame;

#[test]
fn frames_match_ascii_rest_at_its_native_size() {
    for (t, golden) in [
        (0.0, include_str!("fixtures/plasma_t0.txt")),
        (1.0, include_str!("fixtures/plasma_t1.txt")),
        (2.5, include_str!("fixtures/plasma_t2_5.txt")),
    ] {
        assert_eq!(
            format!("{}\n", frame(t, 64, 22).to_text()),
            golden,
            "plasma at t = {t}"
        );
    }
}

#[test]
fn fills_any_area_it_is_given() {
    let drawn = frame(3.0, 7, 3);
    assert_eq!((drawn.cols, drawn.rows), (7, 3));
    assert!(drawn.cells.iter().all(|glyph| *glyph != ' '));
}
