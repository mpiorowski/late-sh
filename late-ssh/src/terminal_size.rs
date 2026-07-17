pub(crate) const MAX_TERMINAL_COLS: u16 = 500;
pub(crate) const MAX_TERMINAL_ROWS: u16 = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
    pub clamped: bool,
}

pub(crate) fn clamp_terminal_size(cols: u32, rows: u32) -> TerminalSize {
    let clamped_cols = cols.clamp(1, u32::from(MAX_TERMINAL_COLS)) as u16;
    let clamped_rows = rows.clamp(1, u32::from(MAX_TERMINAL_ROWS)) as u16;

    TerminalSize {
        cols: clamped_cols,
        rows: clamped_rows,
        clamped: cols != u32::from(clamped_cols) || rows != u32::from(clamped_rows),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
