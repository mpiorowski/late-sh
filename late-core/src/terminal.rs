//! Terminal mode restoration shared by direct sessions and the SSH frontend.

/// End a partial terminal string, disable paste and mouse reporting, reset
/// colors and cursor shape, and show the cursor. Does not clear either screen
/// or leave the alternate screen, so callers can preserve their cleanup order.
pub const RESET_MODES: &str = concat!(
    "\x1b\\",
    "\x1b[?2004l\x1b[?1006l\x1b[?1003l\x1b[?1000l",
    "\x1b]111\x1b\\",
    "\x1b[0m\x1b[0 q\x1b[?25h",
);
