use super::*;

#[test]
fn terminal_size_default_fallback_is_sane() {
    let (cols, rows) = terminal_size_or_default();
    assert!(cols > 0);
    assert!(rows > 0);
}

#[cfg(unix)]
#[test]
fn pty_winsize_maps_rows_and_cols() {
    let winsize = pty_winsize(120, 40);
    assert_eq!(winsize.ws_col, 120);
    assert_eq!(winsize.ws_row, 40);
}
