use anyhow::{Context, Result};
#[cfg(unix)]
use nix::{libc, pty::Winsize};
#[cfg(unix)]
use std::{fs, io, os::fd::AsRawFd, sync::Arc};
use tracing::debug;

#[cfg(unix)]
#[derive(Clone)]
pub(super) struct PtyResizeHandle {
    pub(super) master: Arc<fs::File>,
}

#[cfg(unix)]
impl PtyResizeHandle {
    pub(super) fn resize_to_current(&self) -> Result<()> {
        let (cols, rows) = terminal_size_or_default();
        resize_pty(&self.master, cols, rows)
    }
}

pub(super) fn terminal_size_or_default() -> (u16, u16) {
    crossterm::terminal::size().unwrap_or((80, 24))
}

#[cfg(unix)]
pub(super) fn pty_winsize(cols: u16, rows: u16) -> Winsize {
    Winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    }
}

#[cfg(unix)]
fn resize_pty(master: &fs::File, cols: u16, rows: u16) -> Result<()> {
    let winsize = pty_winsize(cols, rows);
    let rc = unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ, &winsize) };
    if rc == -1 {
        return Err(io::Error::last_os_error()).context("failed to resize local ssh pty");
    }
    debug!(cols, rows, "resized local ssh pty");
    Ok(())
}

#[cfg(test)]
mod tests {
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
}
