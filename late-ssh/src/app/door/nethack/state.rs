use std::sync::Arc;

use ratatui::layout::Rect;

use super::proxy::{NethackProcess, ProcessConfig, ProxyStatus, nethack_playname};
use crate::render_signal::RenderSignal;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Launcher,
    Running,
}

/// Ticks to swallow launcher input after a game exits. At the 66ms world tick
/// this is ~0.7s, enough to absorb the player's trailing key-mashes (clearing
/// nethack's end-of-game `--More--`/disclosure prompts) so a stray `q` cannot
/// reach the launcher's global quit and drop the whole SSH session.
const EXIT_GRACE_TICKS: u8 = 10;

pub struct State {
    user_id: uuid::Uuid,
    bin: String,
    data_dir: String,
    /// Feature flag: when false the door is reachable but launching is a no-op
    /// and the Launcher shows an "unavailable" message.
    enabled: bool,
    mode: Mode,
    proxy: Option<NethackProcess>,
    /// Inner viewport (below the top bar) from the last render, used for PTY
    /// sizing.
    viewport: Rect,
    term: String,
    /// Render-loop wakeup (from the transport). Passed to the proxy so new
    /// output repaints promptly. `None` on headless/test paths.
    repaint: Option<Arc<RenderSignal>>,
    /// Ticks remaining in the post-exit input grace. Counts down in `tick()`
    /// while in the Launcher; while non-zero the launcher swallows input so a
    /// game's trailing keystrokes can't fall through to the global quit.
    exit_grace: u8,
}

impl State {
    pub fn new(
        user_id: uuid::Uuid,
        bin: String,
        data_dir: String,
        term: String,
        enabled: bool,
        repaint: Option<Arc<RenderSignal>>,
    ) -> Self {
        Self {
            user_id,
            bin,
            data_dir,
            enabled,
            mode: Mode::Launcher,
            proxy: None,
            viewport: Rect::new(0, 0, 80, 24),
            term,
            repaint,
            exit_grace: 0,
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
            playname: nethack_playname(self.user_id),
            cols: self.viewport.width.max(1),
            rows: self.viewport.height.max(1),
            term: self.term.clone(),
            repaint: self.repaint.clone(),
        }));
        self.mode = Mode::Running;
        self.exit_grace = 0;
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
                // Open the input grace: the player is usually still clearing
                // nethack's end-of-game prompts, and those trailing keys must
                // not reach the launcher's global `q` = quit-the-app handler.
                self.exit_grace = EXIT_GRACE_TICKS;
            }
        } else if self.exit_grace > 0 {
            self.exit_grace -= 1;
        }
    }

    /// Whether the launcher should currently swallow input because a game just
    /// exited and the player's trailing keystrokes are still arriving. Stops a
    /// stray `q` from falling through to the global quit and dropping the
    /// session.
    pub fn in_exit_grace(&self) -> bool {
        self.exit_grace > 0
    }

    pub fn proxy(&self) -> Option<&NethackProcess> {
        self.proxy.as_ref()
    }

    /// Intercept the F1 key before it reaches nethack. Returns true when the
    /// input was consumed and must NOT be forwarded as-is.
    ///
    /// F1 is remapped to NetHack's own `?` help menu: it is the conventional
    /// help key, and intercepting it also stops the raw F1 escape (`ESC O P`)
    /// from leaking into the game as stray commands. late.sh keeps no help UI
    /// of its own; `?` and F1 both open NetHack's in-game help.
    pub fn intercept_input(&self, data: &[u8]) -> bool {
        if is_f1(data) {
            self.forward_input(b"?");
            return true;
        }
        false
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
    fn f1_is_consumed_and_other_keys_pass_through() {
        let state = disabled_state();
        // F1 (both encodings) is consumed: late.sh remaps it to nethack's `?`
        // help, so it must not also be forwarded as the raw escape.
        assert!(state.intercept_input(b"\x1bOP"));
        assert!(state.intercept_input(b"\x1b[11~"));
        // Everything else falls through to be forwarded to nethack verbatim,
        // including a literal `?` (nethack's own help key).
        assert!(!state.intercept_input(b"?"));
        assert!(!state.intercept_input(b"hjkl"));
    }

    #[test]
    fn exit_grace_opens_on_close_and_counts_down() {
        let mut state = disabled_state();
        // Simulate a game that has exited: in Running with no proxy, the next
        // tick returns to the Launcher and opens the input grace.
        state.mode = Mode::Running;
        assert!(!state.in_exit_grace());
        state.tick();
        assert_eq!(state.mode(), Mode::Launcher);
        assert!(state.in_exit_grace());
        // The grace counts down once per tick and eventually clears, so the
        // launcher does not swallow input forever.
        for _ in 0..EXIT_GRACE_TICKS {
            assert!(state.in_exit_grace());
            state.tick();
        }
        assert!(!state.in_exit_grace());
    }

    #[test]
    fn is_f1_matches_both_encodings() {
        assert!(is_f1(b"\x1bOP"));
        assert!(is_f1(b"\x1b[11~"));
        assert!(!is_f1(b"\x1b[A"));
        assert!(!is_f1(b"?"));
    }
}
