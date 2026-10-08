//! Golden frames from ascii.rest's own `lava-lamp.ts` (node, `frame(t)` at
//! t = 0, 1 and 2.5): the port draws exactly what the original does.

use super::{COLS, ROWS, frame};

#[test]
fn frames_match_ascii_rest() {
    for (t, golden) in [
        (0.0, include_str!("fixtures/lava_lamp_t0.txt")),
        (1.0, include_str!("fixtures/lava_lamp_t1.txt")),
        (2.5, include_str!("fixtures/lava_lamp_t2_5.txt")),
    ] {
        let drawn = frame(t);
        assert_eq!((drawn.cols, drawn.rows), (COLS, ROWS));
        assert_eq!(
            format!("{}\n", drawn.to_text()),
            golden,
            "lava lamp at t = {t}"
        );
    }
}
