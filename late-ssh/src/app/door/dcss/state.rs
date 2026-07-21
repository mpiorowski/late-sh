use std::sync::Arc;

use ratatui::layout::Rect;

use super::proxy::{DcssProcess, ProcessConfig, ProxyStatus};
use crate::app::door::arcade::{ArcadeHandleService, HandleFlow, HandleKeyResult};
use crate::render_signal::RenderSignal;

// The launcher UI renders straight off the shared flow's status.
pub use crate::app::door::arcade::HandleStatus;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Launcher,
    Running,
}

/// Ticks to swallow launcher input after a game exits. At the 66ms world tick
/// this is ~0.7s, enough to absorb the player's trailing key-mashes (clearing
/// crawl's end-of-game "goodbye" / character-dump prompts) so a stray `q` cannot
/// reach the launcher's global quit and drop the whole SSH session.
const EXIT_GRACE_TICKS: u8 = 10;

pub struct State {
    host: String,
    port: u16,
    secret: String,
    /// Feature flag: when false the door is reachable but launching is a no-op
    /// and the Launcher shows an "unavailable" message.
    enabled: bool,
    mode: Mode,
    proxy: Option<DcssProcess>,
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
    /// The shared arcade-handle launcher flow (lookup, claim prompt, launch
    /// intent); the claimed handle becomes crawl's `-name`.
    handle: HandleFlow,
}

impl State {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        user_id: uuid::Uuid,
        host: String,
        port: u16,
        secret: String,
        term: String,
        enabled: bool,
        repaint: Option<Arc<RenderSignal>>,
        handle_svc: Option<ArcadeHandleService>,
    ) -> Self {
        Self {
            host,
            port,
            secret,
            enabled,
            mode: Mode::Launcher,
            proxy: None,
            viewport: Rect::new(0, 0, 80, 24),
            term,
            // A disabled door never looks the handle up.
            handle: HandleFlow::new(
                user_id,
                if enabled { handle_svc } else { None },
                repaint.clone(),
            ),
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
        let Some(playname) = self.handle.claimed() else {
            // Handle not known yet: remember the intent (no-op if the prompt
            // or retry hint is on screen); tick() launches when the in-flight
            // lookup or claim lands on Claimed.
            self.handle.request_launch();
            return;
        };
        self.proxy = Some(DcssProcess::spawn(ProcessConfig {
            host: self.host.clone(),
            port: self.port,
            secret: self.secret.clone(),
            playname,
            cols: self.viewport.width.max(1),
            rows: self.viewport.height.max(1),
            term: self.term.clone(),
            repaint: self.repaint.clone(),
        }));
        self.mode = Mode::Running;
        self.exit_grace = 0;
    }

    /// Called every app tick: if the process closed (clean save, death, quit, or
    /// crash), return to the Launcher. Treats all exits identically. Also
    /// completes a pending launch once the arcade-handle lookup or claim lands.
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
                // crawl's end-of-game prompts, and those trailing keys must not
                // reach the launcher's global `q` = quit-the-app handler.
                self.exit_grace = EXIT_GRACE_TICKS;
            }
            return;
        }
        if self.exit_grace > 0 {
            self.exit_grace -= 1;
        }
        if self.handle.take_ready_launch() {
            self.connect();
        }
    }

    /// Snapshot of the arcade-handle lifecycle for the launcher UI.
    pub fn handle_status(&self) -> HandleStatus {
        self.handle.status()
    }

    /// Whether the launcher still owes the player handle work (lookup, claim
    /// prompt, retry). Keeps the DCSS screen up while no game is running;
    /// once the handle is claimed an idle launcher bounces back to the Games
    /// hub as before.
    pub fn awaiting_handle(&self) -> bool {
        self.enabled && self.handle.awaiting()
    }

    /// The claim prompt's compose buffer, for rendering.
    pub fn entry_input(&self) -> &str {
        self.handle.entry_input()
    }

    /// Whether the one-time arcade-name claim modal is on screen.
    pub fn name_modal_visible(&self) -> bool {
        self.enabled && self.mode == Mode::Launcher && self.handle.modal_visible()
    }

    /// Close the claim modal (Esc); Enter or another launch attempt reopens it.
    pub fn dismiss_name_modal(&mut self) {
        self.handle.dismiss_modal();
    }

    /// Handle a Launcher-mode key byte. Returns true when consumed; unconsumed
    /// keys fall through to the global keymap (tab switching, quit).
    pub fn launcher_key(&mut self, byte: u8) -> bool {
        if !self.enabled || self.mode == Mode::Running {
            return false;
        }
        match self.handle.key(byte) {
            HandleKeyResult::Launch => {
                self.connect();
                true
            }
            HandleKeyResult::Consumed => true,
            HandleKeyResult::Ignored => false,
        }
    }

    /// Whether the launcher should currently swallow input because a game just
    /// exited and the player's trailing keystrokes are still arriving. Stops a
    /// stray `q` from falling through to the global quit and dropping the
    /// session.
    pub fn in_exit_grace(&self) -> bool {
        self.exit_grace > 0
    }

    pub fn proxy(&self) -> Option<&DcssProcess> {
        self.proxy.as_ref()
    }

    /// Intercept the F1 key before it reaches crawl. Returns true when the
    /// input was consumed and must NOT be forwarded as-is.
    ///
    /// F1 is remapped to crawl's own `?` help menu: it is the conventional help
    /// key, and intercepting it also stops the raw F1 escape (`ESC O P`) from
    /// leaking into the game as stray commands. late.sh keeps no help UI of its
    /// own; `?` and F1 both open crawl's in-game help.
    pub fn intercept_input(&self, data: &[u8]) -> bool {
        if is_f1(data) {
            self.forward_input(b"?");
            return true;
        }
        false
    }

    /// Forward client bytes to crawl, minus mouse and bracketed-paste reports.
    /// The crawl console build is keyboard-driven, but late.sh keeps any-event
    /// mouse tracking (`?1003h`) on for its own UI, so the client streams motion
    /// reports whose leading `ESC` would cancel crawl's menus and prompts.
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

/// Drop terminal reports crawl must never see: SGR mouse (`ESC [ < … M/m`),
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
#[path = "state_test.rs"]
mod state_test;
