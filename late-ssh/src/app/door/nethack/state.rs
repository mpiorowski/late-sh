use std::sync::Arc;

use ratatui::layout::Rect;

use super::proxy::{NethackProcess, ProcessConfig, ProxyStatus, sanitize_playname};
use crate::render_signal::RenderSignal;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Launcher,
    Running,
}

pub struct State {
    user_id: uuid::Uuid,
    username: String,
    bin: String,
    data_dir: String,
    /// Feature flag: when false the door is reachable but launching is a no-op
    /// and the Launcher shows an "unavailable" message.
    enabled: bool,
    mode: Mode,
    /// late.sh-side keybinding cheat sheet, toggled with F1 while in-game.
    /// Purely a render overlay; nethack never sees these keys.
    help_open: bool,
    proxy: Option<NethackProcess>,
    /// Inner viewport (below the top bar) from the last render, used for PTY
    /// sizing.
    viewport: Rect,
    term: String,
    /// Render-loop wakeup (from the transport). Passed to the proxy so new
    /// output repaints promptly. `None` on headless/test paths.
    repaint: Option<Arc<RenderSignal>>,
}

impl State {
    pub fn new(
        user_id: uuid::Uuid,
        username: String,
        bin: String,
        data_dir: String,
        term: String,
        enabled: bool,
        repaint: Option<Arc<RenderSignal>>,
    ) -> Self {
        Self {
            user_id,
            username,
            bin,
            data_dir,
            enabled,
            mode: Mode::Launcher,
            help_open: false,
            proxy: None,
            viewport: Rect::new(0, 0, 80, 24),
            term,
            repaint,
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Whether the door is enabled (launchable). When false the Launcher shows
    /// an "unavailable" message and `connect` is a no-op.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn is_running(&self) -> bool {
        matches!(self.mode, Mode::Running)
    }

    pub fn set_viewport(&mut self, area: Rect) {
        let resized = self.viewport.width != area.width || self.viewport.height != area.height;
        self.viewport = area;
        if resized && let Some(p) = &self.proxy {
            p.resize(area.width, area.height);
        }
    }

    pub fn connect(&mut self) {
        if !self.enabled || self.proxy.is_some() {
            return;
        }
        self.proxy = Some(NethackProcess::spawn(ProcessConfig {
            bin: self.bin.clone(),
            data_dir: self.data_dir.clone(),
            playname: sanitize_playname(&self.username, self.user_id),
            cols: self.viewport.width.max(1),
            rows: self.viewport.height.max(1),
            term: self.term.clone(),
            repaint: self.repaint.clone(),
        }));
        self.mode = Mode::Running;
    }

    /// Called every app tick: if the process closed (clean quit, death, or
    /// crash), return to the Launcher. Treats all exits identically.
    pub fn tick(&mut self) {
        if self.mode == Mode::Running {
            let closed = self
                .proxy
                .as_ref()
                .is_none_or(|p| p.status() == ProxyStatus::Closed);
            if closed {
                self.proxy = None;
                self.mode = Mode::Launcher;
                self.help_open = false;
            }
        }
    }

    pub fn proxy(&self) -> Option<&NethackProcess> {
        self.proxy.as_ref()
    }

    /// Whether the late.sh cheat-sheet overlay is currently showing.
    pub fn help_open(&self) -> bool {
        self.help_open
    }

    /// Intercept late.sh-side overlay keys before anything reaches nethack.
    /// Returns true when the input was consumed and must NOT be forwarded.
    ///
    /// F1 toggles the cheat sheet. While it is open, the next keypress just
    /// dismisses it (and is swallowed so it can't nudge the hero around).
    pub fn intercept_input(&mut self, data: &[u8]) -> bool {
        if is_f1(data) {
            self.help_open = !self.help_open;
            self.wake();
            return true;
        }
        if self.help_open {
            // A real keypress dismisses, but ignore the mouse-motion flood so
            // the overlay does not vanish the instant the pointer twitches.
            if !strip_input_noise(data).is_empty() {
                self.help_open = false;
                self.wake();
            }
            return true;
        }
        false
    }

    fn wake(&self) {
        if let Some(sig) = &self.repaint {
            sig.wake();
        }
    }

    /// Forward client bytes to nethack, minus mouse and bracketed-paste reports.
    /// NetHack is a keyboard-only tty game, but late.sh keeps any-event mouse
    /// tracking (`?1003h`) on for its own UI, so the client streams motion
    /// reports whose leading `ESC` cancels every nethack menu (notably `?`).
    /// Stripping them is what makes in-game `?` actually work.
    pub fn forward_input(&self, data: &[u8]) {
        if let Some(proxy) = &self.proxy {
            let filtered = strip_input_noise(data);
            if !filtered.is_empty() {
                proxy.send_input(filtered);
            }
        }
    }
}

/// F1 as sent by the common terminals: SS3 form (`ESC O P`, xterm/most) and the
/// CSI form (`ESC [ 1 1 ~`, linux/screen/some tmux setups).
fn is_f1(data: &[u8]) -> bool {
    data == b"\x1bOP" || data == b"\x1b[11~"
}

/// Drop terminal reports nethack must never see: SGR mouse (`ESC [ < … M/m`),
/// legacy X10 mouse (`ESC [ M b x y`), and bracketed-paste markers (`ESC [
/// 200~` / `ESC [ 201~`). Everything else, including real keys and arrow-key
/// escapes, passes through verbatim. A sequence truncated at the chunk boundary
/// falls through unchanged rather than swallowing a following keystroke.
fn strip_input_noise(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b'[' {
            let rest = &data[i + 2..];
            // SGR mouse: ESC [ < … (M|m)
            if rest.first() == Some(&b'<')
                && let Some(end) = rest.iter().position(|&b| b == b'M' || b == b'm')
            {
                i += 2 + end + 1;
                continue;
            }
            // Legacy X10 mouse: ESC [ M b x y (three bytes after M)
            if rest.first() == Some(&b'M') && rest.len() >= 4 {
                i += 2 + 4;
                continue;
            }
            // Bracketed-paste markers.
            if rest.starts_with(b"200~") || rest.starts_with(b"201~") {
                i += 2 + 4;
                continue;
            }
        }
        out.push(data[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disabled_state() -> State {
        State::new(
            uuid::Uuid::nil(),
            "tester".to_string(),
            "/usr/games/nethack".to_string(),
            "/var/lib/late-nethack".to_string(),
            "xterm".to_string(),
            false,
            None,
        )
    }

    #[test]
    fn connect_is_a_no_op_when_disabled() {
        let mut state = disabled_state();
        assert!(!state.is_enabled());
        state.connect();
        assert!(state.proxy().is_none());
        assert_eq!(state.mode(), Mode::Launcher);
    }

    #[test]
    fn forward_input_without_proxy_is_a_no_op() {
        let state = disabled_state();
        // Must not panic when nothing is running.
        state.forward_input(b"hjkl");
    }

    #[test]
    fn strip_input_noise_drops_mouse_keeps_keys() {
        // The `?` survives a motion report glued to it, which is exactly the
        // case that used to cancel the help menu.
        assert_eq!(strip_input_noise(b"\x1b[<35;10;5M?"), b"?");
        assert_eq!(strip_input_noise(b"?\x1b[<35;10;5m"), b"?");
        // Legacy X10 mouse and paste markers go too.
        assert_eq!(strip_input_noise(b"a\x1b[Mabcb"), b"ab");
        assert_eq!(strip_input_noise(b"\x1b[200~hi\x1b[201~"), b"hi");
    }

    #[test]
    fn strip_input_noise_passes_keys_and_arrows() {
        assert_eq!(strip_input_noise(b"hjkl"), b"hjkl");
        // Arrow keys (ESC [ A …) must not be mistaken for mouse.
        assert_eq!(strip_input_noise(b"\x1b[A\x1b[B"), b"\x1b[A\x1b[B");
    }

    #[test]
    fn f1_toggles_help_and_mouse_noise_does_not_dismiss_it() {
        let mut state = disabled_state();
        assert!(!state.help_open());
        // F1 opens.
        assert!(state.intercept_input(b"\x1bOP"));
        assert!(state.help_open());
        // A mouse motion report is swallowed but keeps the overlay open.
        assert!(state.intercept_input(b"\x1b[<35;1;1M"));
        assert!(state.help_open());
        // A real keypress dismisses it.
        assert!(state.intercept_input(b" "));
        assert!(!state.help_open());
    }

    #[test]
    fn is_f1_matches_both_encodings() {
        assert!(is_f1(b"\x1bOP"));
        assert!(is_f1(b"\x1b[11~"));
        assert!(!is_f1(b"\x1b[A"));
        assert!(!is_f1(b"?"));
    }
}
