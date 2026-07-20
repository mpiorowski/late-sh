use std::sync::Arc;

use ratatui::layout::Rect;

use super::proxy::{ProcessConfig, ProxyStatus, UsurperProcess};
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
/// the game's goodbye screens) so a stray `q` cannot reach the launcher's
/// global quit and drop the whole SSH session.
const EXIT_GRACE_TICKS: u8 = 10;

pub struct State {
    host: String,
    port: u16,
    secret: String,
    /// Feature flag: when false the door is reachable but launching is a no-op
    /// and the Launcher shows an "unavailable" message.
    enabled: bool,
    mode: Mode,
    proxy: Option<UsurperProcess>,
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
    /// intent); the claimed handle becomes the game's player identity via the
    /// host-written DOOR32.SYS.
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
            viewport: Rect::new(0, 0, 80, 25),
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
        self.proxy = Some(UsurperProcess::spawn(ProcessConfig {
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

    /// Called every app tick: if the process closed (in-game quit, time-out, or
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
                // the game's goodbye prompts, and those trailing keys must not
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
    /// prompt, retry). Keeps the Usurper screen up while no game is running;
    /// once the handle is claimed an idle launcher bounces back to the Games
    /// hub as usual.
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

    pub fn proxy(&self) -> Option<&UsurperProcess> {
        self.proxy.as_ref()
    }

    /// Test hook: enter Running with no proxy, as if a game just exited.
    #[cfg(test)]
    pub(super) fn force_running_for_test(&mut self) {
        self.mode = Mode::Running;
    }

    /// Forward client bytes to the game, minus mouse/paste reports and the
    /// function keys. There is no F1 help remap here (the game has no
    /// universal help key; its menus are self-describing), so unlike
    /// nethack/dcss nothing is intercepted, only stripped.
    ///
    /// This strip is best-effort noise reduction. The authoritative F-key
    /// filter is on the host (late-usurper `input_filter`), which is
    /// stateful across chunk boundaries and cannot be bypassed by a raw SSH
    /// client to the host or by splitting a sequence across two chunks.
    pub fn forward_input(&self, data: &[u8]) {
        if let Some(proxy) = &self.proxy {
            let filtered = strip_input_noise(data);
            if !filtered.is_empty() {
                proxy.send_input(filtered);
            }
        }
    }
}

/// Drop terminal reports and keys the game must never see:
/// - SGR mouse (`ESC [ < ... M/m`), legacy X10 mouse (`ESC [ M b x y`), and
///   bracketed-paste markers, exactly like the other doors (late.sh keeps
///   any-event mouse tracking on for its own UI).
/// - The function keys F1-F12, in both the SS3 (`ESC O P..S`) and CSI
///   (`ESC [ 11~ .. 24~`) encodings plus the linux-console `ESC [ [ A..E`
///   form. In DOOR32 local mode the player's keyboard IS the game's sysop
///   console, and DDPlus binds F2/F7/F8/F10 to sysop functions (chat window,
///   time adjustment, eject); letting those through would hand every player
///   the sysop panel. Navigation keys (arrows, Home/End/PgUp/PgDn) pass
///   through untouched.
///
/// A sequence truncated at the chunk boundary falls through unchanged rather
/// than swallowing a following keystroke.
pub(super) fn strip_input_noise(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] == 0x1b && i + 1 < data.len() {
            // SS3 F1-F4: ESC O P/Q/R/S
            if data[i + 1] == b'O'
                && i + 2 < data.len()
                && matches!(data[i + 2], b'P' | b'Q' | b'R' | b'S')
            {
                i += 3;
                continue;
            }
            if data[i + 1] == b'[' {
                let rest = &data[i + 2..];
                // SGR mouse: ESC [ < ... (M|m)
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
                // Linux-console F1-F5: ESC [ [ A..E
                if rest.first() == Some(&b'[') && rest.len() >= 2 && matches!(rest[1], b'A'..=b'E')
                {
                    i += 4;
                    continue;
                }
                // CSI F-keys: ESC [ <code> ~ with code in the F1-F12 set
                // (11-15, 17-21, 23, 24). Other codes (1-8: Home/Ins/Del/End/
                // PgUp/PgDn) pass through.
                if rest.len() >= 3
                    && rest[2] == b'~'
                    && rest[0].is_ascii_digit()
                    && rest[1].is_ascii_digit()
                {
                    let code = (rest[0] - b'0') * 10 + (rest[1] - b'0');
                    if matches!(code, 11..=15 | 17..=21 | 23 | 24) {
                        i += 5;
                        continue;
                    }
                }
            }
        }
        out.push(data[i]);
        i += 1;
    }
    out
}
