use crate::terminal_size::*;

#[test]
fn terminal_size_accepts_normal_dimensions() {
    assert_eq!(
        clamp_terminal_size(120, 40),
        TerminalSize {
            cols: 120,
            rows: 40,
            clamped: false,
        }
    );
}

#[test]
fn terminal_size_clamps_zero_and_oversized_dimensions() {
    assert_eq!(
        clamp_terminal_size(0, u32::MAX),
        TerminalSize {
            cols: 1,
            rows: MAX_TERMINAL_ROWS,
            clamped: true,
        }
    );
}
