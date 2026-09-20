use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use std::io::{self, IsTerminal, Write};

pub(super) struct RawModeGuard {
    raw_enabled: bool,
    #[cfg(windows)]
    input_mode: Option<windows_console::ConsoleModeGuard>,
}

/// Turns off the terminal modes a late.sh session turns on (mouse reporting
/// 1000/1003/1006, bracketed paste 2004), resets the themed background
/// (OSC 111), and shows the cursor. The server
/// sends the same on a clean exit, but an idle timeout or a dropped link
/// ends the session without it, and the shell is left printing mouse
/// reports. Sent on every session end, so it has to be harmless twice:
/// that is why it never leaves the alternate screen (`?1049l` a second
/// time restores a stale cursor and the prompt overwrites the goodbye).
const SESSION_MODES_OFF: &[u8] =
    b"\x1b[?1000l\x1b[?1003l\x1b[?1006l\x1b[?2004l\x1b]111\x1b\\\x1b[?25h";

pub(super) fn write_session_modes_off(out: &mut impl Write) -> io::Result<()> {
    out.write_all(SESSION_MODES_OFF)?;
    out.flush()
}

/// Writes [`SESSION_MODES_OFF`] to the terminal when the session ends,
/// however it ends. Inert when stdout is not a VT-capable terminal.
pub(super) struct SessionModesGuard {
    enabled: bool,
}

impl SessionModesGuard {
    pub(super) fn enable_if_tty() -> Self {
        Self {
            enabled: std::io::stdout().is_terminal() && supports_vt_output(),
        }
    }
}

impl Drop for SessionModesGuard {
    fn drop(&mut self) {
        if self.enabled {
            let _ = write_session_modes_off(&mut std::io::stdout());
        }
    }
}

#[cfg(windows)]
fn supports_vt_output() -> bool {
    crossterm::ansi_support::supports_ansi()
}

#[cfg(not(windows))]
fn supports_vt_output() -> bool {
    true
}

pub(super) fn enable_ansi_output_if_tty() {
    if !std::io::stdout().is_terminal() {
        return;
    }
    enable_ansi_output();
}

#[cfg(windows)]
fn enable_ansi_output() {
    if !crossterm::ansi_support::supports_ansi() {
        eprintln!(
            "warning: failed to enable ANSI escape support for this Windows terminal; \
             use Windows Terminal or another VT-compatible console"
        );
    }
}

#[cfg(not(windows))]
fn enable_ansi_output() {}

impl RawModeGuard {
    pub(super) fn enable_if_tty() -> Self {
        if !std::io::stdin().is_terminal() {
            return Self::disabled();
        }
        #[cfg(windows)]
        let input_mode = match windows_console::ConsoleModeGuard::capture_input() {
            Ok(mode) => Some(mode),
            Err(err) => {
                eprintln!("warning: failed to read Windows console input mode: {err}");
                None
            }
        };
        match enable_raw_mode() {
            Ok(()) => {
                #[cfg(windows)]
                if let Some(mode) = input_mode.as_ref()
                    && let Err(err) = mode.enable_virtual_terminal_input()
                {
                    eprintln!("warning: failed to enable Windows virtual terminal input: {err}");
                }

                Self {
                    raw_enabled: true,
                    #[cfg(windows)]
                    input_mode,
                }
            }
            Err(err) => {
                eprintln!("warning: failed to enable raw mode: {err}");
                Self::disabled()
            }
        }
    }

    fn disabled() -> Self {
        Self {
            raw_enabled: false,
            #[cfg(windows)]
            input_mode: None,
        }
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        if self.raw_enabled {
            let _ = disable_raw_mode();
        }
        #[cfg(windows)]
        if let Some(mut input_mode) = self.input_mode.take() {
            let _ = input_mode.restore();
        }
    }
}

#[cfg(windows)]
mod windows_console {
    use std::{ffi::c_void, io};

    const STD_INPUT_HANDLE: u32 = -10i32 as u32;
    const ENABLE_VIRTUAL_TERMINAL_INPUT: u32 = 0x0200;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetStdHandle(n_std_handle: u32) -> *mut c_void;
        fn GetConsoleMode(console_handle: *mut c_void, mode: *mut u32) -> i32;
        fn SetConsoleMode(console_handle: *mut c_void, mode: u32) -> i32;
    }

    pub(super) struct ConsoleModeGuard {
        handle: *mut c_void,
        original_mode: u32,
        restored: bool,
    }

    impl ConsoleModeGuard {
        pub(super) fn capture_input() -> io::Result<Self> {
            let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
            if handle.is_null() || handle as isize == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self {
                handle,
                original_mode: get_console_mode(handle)?,
                restored: false,
            })
        }

        pub(super) fn enable_virtual_terminal_input(&self) -> io::Result<()> {
            let mode = get_console_mode(self.handle)?;
            if mode & ENABLE_VIRTUAL_TERMINAL_INPUT != 0 {
                return Ok(());
            }
            set_console_mode(self.handle, mode | ENABLE_VIRTUAL_TERMINAL_INPUT)
        }

        pub(super) fn restore(&mut self) -> io::Result<()> {
            if self.restored {
                return Ok(());
            }
            self.restored = true;
            set_console_mode(self.handle, self.original_mode)
        }
    }

    impl Drop for ConsoleModeGuard {
        fn drop(&mut self) {
            let _ = self.restore();
        }
    }

    fn get_console_mode(handle: *mut c_void) -> io::Result<u32> {
        let mut mode = 0;
        if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(mode)
        }
    }

    fn set_console_mode(handle: *mut c_void, mode: u32) -> io::Result<()> {
        if unsafe { SetConsoleMode(handle, mode) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
#[path = "raw_mode_test.rs"]
mod raw_mode_test;
