use std::sync::{Arc, Mutex};

use late_core::models::arcade_handle::{self, ClaimOutcome};
use ratatui::layout::Rect;

use super::proxy::{DcssProcess, ProcessConfig, ProxyStatus};
use crate::app::door::arcade::ArcadeHandleService;
use crate::render_signal::RenderSignal;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Launcher,
    Running,
}

/// Where the account stands with its arcade handle (the crawl `-name`).
/// Written by the background lookup/claim tasks, read by the launcher UI, so
/// it lives behind an `Arc<Mutex<..>>` like the proxy status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandleStatus {
    /// The initial lookup is in flight.
    Loading,
    /// No handle claimed yet: the launcher shows the claim prompt. `error`
    /// carries the last refusal (bad shape, reserved, taken, db failure).
    Missing { error: Option<String> },
    /// A claim is in flight.
    Claiming,
    /// The account's immutable handle; launching is possible.
    Claimed(String),
    /// The initial lookup failed (db unreachable); Enter retries.
    Failed,
}

/// Ticks to swallow launcher input after a game exits. At the 66ms world tick
/// this is ~0.7s, enough to absorb the player's trailing key-mashes (clearing
/// crawl's end-of-game "goodbye" / character-dump prompts) so a stray `q` cannot
/// reach the launcher's global quit and drop the whole SSH session.
const EXIT_GRACE_TICKS: u8 = 10;

pub struct State {
    user_id: uuid::Uuid,
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
    /// Accessor for the account's arcade handle. `None` on headless/test paths
    /// (the prompt then reports the name service as unavailable).
    handle_svc: Option<ArcadeHandleService>,
    /// The handle lifecycle, shared with the background lookup/claim tasks.
    handle: Arc<Mutex<HandleStatus>>,
    /// Compose buffer for the claim prompt. Only the foreground touches it.
    entry: String,
    /// The player asked to launch before the handle was known (hub Enter races
    /// the lookup; a claim is a launch intent too). `tick()` connects as soon
    /// as the status lands on `Claimed`.
    launch_pending: bool,
}

impl State {
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
        let state = Self {
            user_id,
            host,
            port,
            secret,
            enabled,
            mode: Mode::Launcher,
            proxy: None,
            viewport: Rect::new(0, 0, 80, 24),
            term,
            repaint,
            exit_grace: 0,
            handle_svc,
            handle: Arc::new(Mutex::new(HandleStatus::Missing { error: None })),
            entry: String::new(),
            launch_pending: false,
        };
        if state.enabled && state.handle_svc.is_some() {
            state.spawn_load();
        }
        state
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
        let status = self.handle.lock().expect("handle mutex").clone();
        match status {
            HandleStatus::Claimed(playname) => {
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
                self.launch_pending = false;
            }
            // Handle not known yet: remember the intent; tick() launches when
            // the in-flight lookup or claim lands on Claimed.
            HandleStatus::Loading | HandleStatus::Claiming => self.launch_pending = true,
            // The claim prompt (or the retry hint) is on screen; nothing to do.
            HandleStatus::Missing { .. } | HandleStatus::Failed => {}
        }
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
        if self.launch_pending {
            let status = self.handle.lock().expect("handle mutex").clone();
            match status {
                HandleStatus::Claimed(_) => self.connect(), // clears the flag
                // The lookup/claim came back without a handle: the prompt (or
                // retry hint) is on screen, the intent is void.
                HandleStatus::Missing { .. } | HandleStatus::Failed => {
                    self.launch_pending = false;
                }
                HandleStatus::Loading | HandleStatus::Claiming => {}
            }
        }
    }

    /// Snapshot of the arcade-handle lifecycle for the launcher UI.
    pub fn handle_status(&self) -> HandleStatus {
        self.handle.lock().expect("handle mutex").clone()
    }

    /// Whether the launcher still owes the player handle work (lookup, claim
    /// prompt, retry). Keeps the DCSS screen up while no game is running;
    /// once the handle is claimed an idle launcher bounces back to the Games
    /// hub as before.
    pub fn awaiting_handle(&self) -> bool {
        self.enabled && !matches!(self.handle_status(), HandleStatus::Claimed(_))
    }

    /// The claim prompt's compose buffer, for rendering.
    pub fn entry_input(&self) -> &str {
        &self.entry
    }

    /// Handle a Launcher-mode key byte. Returns true when consumed; unconsumed
    /// keys fall through to the global keymap (tab switching, quit). While the
    /// claim prompt is open every printable byte is consumed, so a typed `q`
    /// cannot fall through to the global quit mid-word.
    pub fn launcher_key(&mut self, byte: u8) -> bool {
        if !self.enabled || self.mode == Mode::Running {
            return false;
        }
        let is_enter = matches!(byte, b'\r' | b'\n');
        let status = self.handle.lock().expect("handle mutex").clone();
        match status {
            HandleStatus::Claimed(_) => {
                if is_enter {
                    self.connect();
                }
                is_enter
            }
            // Work in flight; swallow Enter so it can't double-submit.
            HandleStatus::Loading | HandleStatus::Claiming => is_enter,
            HandleStatus::Failed => {
                if is_enter {
                    self.spawn_load();
                }
                is_enter
            }
            HandleStatus::Missing { .. } => match byte {
                b'\r' | b'\n' => {
                    self.submit_entry();
                    true
                }
                0x08 | 0x7f => {
                    self.entry.pop();
                    true
                }
                0x20..=0x7e => {
                    if (byte.is_ascii_alphanumeric() || byte == b'_')
                        && self.entry.len() < arcade_handle::HANDLE_MAX_LEN
                    {
                        self.entry.push(byte as char);
                    }
                    true
                }
                _ => false,
            },
        }
    }

    /// Validate the compose buffer and, if it holds up, claim it in the
    /// background. A successful claim doubles as launch intent: the player
    /// typed the name because they want to play.
    fn submit_entry(&mut self) {
        let name = self.entry.clone();
        if !arcade_handle::handle_shape_valid(&name) {
            self.set_entry_error(
                "3-20 characters: letters, digits, underscore; starting with a letter.",
            );
            return;
        }
        if arcade_handle::handle_reserved(&name) {
            self.set_entry_error("That name is reserved.");
            return;
        }
        let Some(svc) = self.handle_svc.clone() else {
            self.set_entry_error("The name service is unavailable.");
            return;
        };
        *self.handle.lock().expect("handle mutex") = HandleStatus::Claiming;
        self.launch_pending = true;
        let slot = self.handle.clone();
        let repaint = self.repaint.clone();
        let user_id = self.user_id;
        tokio::spawn(async move {
            let next = match svc.claim(user_id, &name).await {
                Ok(ClaimOutcome::Claimed) => HandleStatus::Claimed(name),
                // A racing double-submit already claimed for this account;
                // whatever landed first is the handle.
                Ok(ClaimOutcome::AlreadyClaimed(existing)) => HandleStatus::Claimed(existing),
                Ok(ClaimOutcome::Taken) => HandleStatus::Missing {
                    error: Some("That name is already taken.".to_string()),
                },
                Err(e) => {
                    tracing::warn!(error = ?e, "arcade handle claim failed");
                    HandleStatus::Missing {
                        error: Some("Couldn't save the name. Try again.".to_string()),
                    }
                }
            };
            *slot.lock().expect("handle mutex") = next;
            if let Some(sig) = &repaint {
                sig.wake();
            }
        });
    }

    fn set_entry_error(&self, message: &str) {
        *self.handle.lock().expect("handle mutex") = HandleStatus::Missing {
            error: Some(message.to_string()),
        };
    }

    /// Look up the account's handle in the background; the launcher shows
    /// Loading until the result lands.
    fn spawn_load(&self) {
        let Some(svc) = self.handle_svc.clone() else {
            *self.handle.lock().expect("handle mutex") = HandleStatus::Missing { error: None };
            return;
        };
        *self.handle.lock().expect("handle mutex") = HandleStatus::Loading;
        let slot = self.handle.clone();
        let repaint = self.repaint.clone();
        let user_id = self.user_id;
        tokio::spawn(async move {
            let next = match svc.get(user_id).await {
                Ok(Some(name)) => HandleStatus::Claimed(name),
                Ok(None) => HandleStatus::Missing { error: None },
                Err(e) => {
                    tracing::warn!(error = ?e, "arcade handle lookup failed");
                    HandleStatus::Failed
                }
            };
            *slot.lock().expect("handle mutex") = next;
            if let Some(sig) = &repaint {
                sig.wake();
            }
        });
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
mod tests {
    use super::*;

    fn disabled_state() -> State {
        State::new(
            uuid::Uuid::nil(),
            "127.0.0.1".to_string(),
            2325,
            String::new(),
            "xterm".to_string(),
            false,
            None,
            None,
        )
    }

    /// Enabled but with no handle service (headless): the claim prompt is
    /// reachable and validation runs, while nothing can spawn tasks.
    fn promptable_state() -> State {
        State::new(
            uuid::Uuid::nil(),
            "127.0.0.1".to_string(),
            2325,
            String::new(),
            "xterm".to_string(),
            true,
            None,
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
        // case that would cancel the help menu.
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
        // F1 (both encodings) is consumed: late.sh remaps it to crawl's `?`
        // help, so it must not also be forwarded as the raw escape.
        assert!(state.intercept_input(b"\x1bOP"));
        assert!(state.intercept_input(b"\x1b[11~"));
        // Everything else falls through to be forwarded to crawl verbatim,
        // including a literal `?` (crawl's own help key).
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
    fn prompt_consumes_printables_and_builds_the_name() {
        let mut state = promptable_state();
        assert_eq!(state.handle_status(), HandleStatus::Missing { error: None });
        // Valid handle bytes accumulate; every printable is consumed so a
        // stray `q` can't fall through to the global quit mid-word.
        for b in b"Gnoll_Fan" {
            assert!(state.launcher_key(*b));
        }
        assert_eq!(state.entry_input(), "Gnoll_Fan");
        // Rejected chars are still consumed, but don't land in the buffer.
        assert!(state.launcher_key(b'?'));
        assert!(state.launcher_key(b' '));
        assert!(state.launcher_key(b'q'));
        assert_eq!(state.entry_input(), "Gnoll_Fanq");
        // Backspace edits.
        assert!(state.launcher_key(0x7f));
        assert_eq!(state.entry_input(), "Gnoll_Fan");
        // Escape sequences and control bytes fall through to global handling.
        assert!(!state.launcher_key(0x1b));
    }

    #[test]
    fn prompt_caps_the_buffer_at_max_len() {
        let mut state = promptable_state();
        for _ in 0..40 {
            state.launcher_key(b'a');
        }
        assert_eq!(
            state.entry_input().len(),
            late_core::models::arcade_handle::HANDLE_MAX_LEN
        );
    }

    #[test]
    fn submit_surfaces_validation_errors() {
        let mut state = promptable_state();
        // Too short.
        state.launcher_key(b'a');
        state.launcher_key(b'\r');
        let HandleStatus::Missing { error: Some(msg) } = state.handle_status() else {
            panic!("expected a shape error");
        };
        assert!(msg.contains("3-20"));
        // Reserved: the buffer survives so the player can edit it.
        let mut state = promptable_state();
        for b in b"late_abc" {
            state.launcher_key(*b);
        }
        state.launcher_key(b'\n');
        let HandleStatus::Missing { error: Some(msg) } = state.handle_status() else {
            panic!("expected a reserved error");
        };
        assert!(msg.contains("reserved"));
        assert_eq!(state.entry_input(), "late_abc");
    }

    #[test]
    fn launcher_keys_are_inert_when_disabled() {
        let mut state = disabled_state();
        assert!(!state.launcher_key(b'a'));
        assert!(!state.launcher_key(b'\r'));
        assert_eq!(state.entry_input(), "");
    }

    #[test]
    fn is_f1_matches_both_encodings() {
        assert!(is_f1(b"\x1bOP"));
        assert!(is_f1(b"\x1b[11~"));
        assert!(!is_f1(b"\x1b[A"));
        assert!(!is_f1(b"?"));
    }
}
