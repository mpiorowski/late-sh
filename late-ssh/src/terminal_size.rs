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
