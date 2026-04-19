use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::IsTerminal;

pub(super) struct RawModeGuard(bool);

impl RawModeGuard {
    pub(super) fn enable_if_tty() -> Self {
        if !std::io::stdin().is_terminal() {
            return Self(false);
        }
        match enable_raw_mode() {
            Ok(()) => Self(true),
            Err(err) => {
                eprintln!("warning: failed to enable raw mode: {err}");
                Self(false)
            }
        }
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.0 {
            let _ = disable_raw_mode();
        }
    }
}
