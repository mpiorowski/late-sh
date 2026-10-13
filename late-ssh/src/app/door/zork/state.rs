use super::protocol::{
    Action, Availability, Catalogue, EXIT_BUSY, EXIT_QUIT, EXIT_RETURNED, EXIT_SAVE_REJECTED,
    Edition, EditionSlots,
};
use super::proxy::{self, Connection, Ending, Process, ProxyStatus};
use super::suggest::Suggestion;
use crate::app::door::arcade::{ArcadeHandleService, HandleFlow, HandleKeyResult, HandleStatus};
use crate::app::door::keys;
use crate::metrics::{self, ZorkCatalogueRead, ZorkSessionEnd, ZorkSuggestion, ZorkSwitchResult};
use crate::render_signal::RenderSignal;
use futures_util::FutureExt;
use ratatui::layout::Rect;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

const IDLE_SHUTDOWN: Duration = Duration::from_secs(20 * 60);
const RETURN_TIMEOUT: Duration = Duration::from_secs(8);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Editions,
    Actions,
    Confirm(Action),
    Running,
}
pub struct StateConfig {
    pub user_id: uuid::Uuid,
    pub host: String,
    pub port: u16,
    pub secret: String,
    pub term: String,
    pub enabled: bool,
    pub repaint: Option<Arc<RenderSignal>>,
    /// Accessor for the account's arcade handle. `None` on headless/test
    /// paths.
    pub handle_svc: Option<ArcadeHandleService>,
}
pub struct State {
    connection: Connection,
    /// The shared arcade-handle flow (lookup, the one-time claim prompt,
    /// launch intent), as on the roguelikes: every game is played, and
    /// listed for watchers, under the claimed handle.
    handle: HandleFlow,
    /// The edition and action that opened the claim prompt, launched once
    /// the claim lands so the player's one Enter still starts the game. The
    /// edition is the one it was chosen on: the player may browse another
    /// meanwhile, and a New confirmed for one must not start on the other.
    after_claim: Option<(Edition, Action)>,
    /// The watcher suggestion F3 last typed, so it is not offered again.
    typed_suggestion: Option<uuid::Uuid>,
    term: String,
    enabled: bool,
    viewport: Rect,
    mode: Mode,
    selected: usize,
    edition: Edition,
    action: usize,
    catalogue: Option<Catalogue>,
    lookup: Option<JoinHandle<anyhow::Result<Catalogue>>>,
    proxy: Option<Process>,
    live: Option<Edition>,
    pending: Option<(Edition, Action, Instant)>,
    last_input: Instant,
    exit_grace: u8,
    message: String,
    input: InputFilter,
}
impl Drop for State {
    fn drop(&mut self) {
        if let Some(task) = &self.lookup {
            task.abort();
        }
    }
}
impl State {
    pub fn new(cfg: StateConfig) -> Self {
        Self {
            // A disabled door never looks the handle up.
            handle: HandleFlow::new(
                cfg.user_id,
                if cfg.enabled { cfg.handle_svc } else { None },
                cfg.repaint.clone(),
            ),
            after_claim: None,
            typed_suggestion: None,
            connection: Connection {
                host: cfg.host,
                port: cfg.port,
                secret: cfg.secret,
                user_id: cfg.user_id,
                repaint: cfg.repaint,
            },
            term: cfg.term,
            enabled: cfg.enabled,
            viewport: Rect::new(0, 0, 80, 24),
            mode: Mode::Editions,
            selected: 0,
            edition: Edition::Zork1,
            action: 0,
            catalogue: None,
            lookup: None,
            proxy: None,
            live: None,
            pending: None,
            last_input: Instant::now(),
            exit_grace: 0,
            message: String::new(),
            input: InputFilter::default(),
        }
    }
    pub fn open_menu(&mut self) {
        self.mode = Mode::Editions;
        self.refresh();
    }
    pub fn resume_live(&mut self) {
        if self.live.is_some() {
            self.mode = Mode::Running;
        }
    }
    pub fn mode(&self) -> Mode {
        self.mode
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
    pub fn edition(&self) -> Edition {
        self.edition
    }
    pub fn action(&self) -> usize {
        self.action
    }
    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn loading(&self) -> bool {
        self.lookup.is_some()
    }
    pub fn is_running(&self) -> bool {
        self.live.is_some()
    }
    pub fn game_visible(&self) -> bool {
        self.mode == Mode::Running
    }
    pub fn proxy(&self) -> Option<&Process> {
        self.proxy.as_ref()
    }
    pub fn live(&self) -> Option<Edition> {
        self.live
    }
    /// Snapshot of the arcade-handle lifecycle, for the claim modal and the
    /// player's own watch chat (`spectate::chat`).
    pub fn handle_status(&self) -> HandleStatus {
        self.handle.status()
    }
    /// The claim prompt's compose buffer, for rendering.
    pub fn entry_input(&self) -> &str {
        self.handle.entry_input()
    }
    /// Whether the one-time arcade-name claim modal is over the menus: the
    /// account has no handle yet, as on the roguelikes' launchers.
    pub fn name_modal_visible(&self) -> bool {
        self.enabled && !self.game_visible() && self.handle.modal_visible()
    }
    /// Close the claim modal (Esc); choosing a game reopens it.
    pub fn dismiss_name_modal(&mut self) {
        self.handle.dismiss_modal();
    }
    /// A key byte for the claim modal. Returns whether it was consumed.
    pub fn name_key(&mut self, byte: u8) -> bool {
        match self.handle.key(byte) {
            HandleKeyResult::Launch | HandleKeyResult::Consumed => true,
            HandleKeyResult::Ignored => false,
        }
    }
    /// Stand the handle on a claimed name without a lookup, for tests that
    /// launch a game.
    #[cfg(test)]
    pub fn force_handle_for_test(&mut self, handle: &str) {
        self.handle.force_claimed_for_test(handle);
    }
    pub fn typed_suggestion(&self) -> Option<uuid::Uuid> {
        self.typed_suggestion
    }
    pub fn slots(&self, edition: Edition) -> Option<&EditionSlots> {
        self.catalogue
            .as_ref()?
            .editions
            .iter()
            .find(|slots| slots.edition == edition)
    }
    pub fn set_viewport(&mut self, area: Rect) {
        let cols = area.width.clamp(20, 255);
        let rows = area.height.clamp(4, 255);
        if (self.viewport.width, self.viewport.height) != (cols, rows)
            && let Some(proxy) = &self.proxy
        {
            proxy.resize(cols, rows);
        }
        self.viewport = Rect::new(area.x, area.y, cols, rows);
    }
    fn refresh(&mut self) {
        if !self.enabled || self.lookup.is_some() {
            return;
        }
        let cfg = self.connection.clone();
        self.lookup = Some(tokio::spawn(async move {
            let result = proxy::catalogue(cfg.clone()).await;
            if let Some(repaint) = cfg.repaint {
                repaint.wake();
            }
            result
        }));
    }
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        if self
            .input
            .waiting_since
            .is_some_and(|since| since.elapsed() >= Duration::from_millis(100))
        {
            let bytes = self.input.flush();
            self.send_game_input(bytes);
        }
        self.exit_grace = self.exit_grace.saturating_sub(1);
        // The handle lookup or claim landed on a launch the player asked for
        // while it was in flight: start the action they chose, on the
        // edition they chose it on.
        if self.handle.take_ready_launch() {
            changed = true;
            if let Some((edition, action)) = self.after_claim.take() {
                self.edition = edition;
                self.launch(action);
            }
        }
        if self.lookup.as_ref().is_some_and(|task| task.is_finished()) {
            changed = true;
            match self
                .lookup
                .take()
                .expect("finished lookup")
                .now_or_never()
                .expect("finished lookup result")
            {
                Ok(Ok(catalogue)) => {
                    metrics::record_zork_catalogue_read(ZorkCatalogueRead::Read);
                    self.catalogue = Some(catalogue);
                }
                error => {
                    metrics::record_zork_catalogue_read(ZorkCatalogueRead::Failed);
                    tracing::warn!(?error, "zork save catalogue unavailable");
                    self.catalogue = None;
                    self.message = "Unable to read saves. Press r to retry.".into();
                }
            }
        }
        if self
            .proxy
            .as_ref()
            .is_some_and(|proxy| proxy.status() == ProxyStatus::Closed)
        {
            changed = true;
            let outcome = self.proxy.take().expect("closed proxy").outcome();
            let previous = self
                .live
                .take()
                .expect("a proxy always runs a live edition");
            metrics::record_zork_session_end(previous, session_end(outcome.ending));
            if let Some((edition, action, _)) = self.pending.take() {
                if outcome.ending == Ending::Exited(EXIT_RETURNED) {
                    metrics::record_zork_switch(ZorkSwitchResult::Switched);
                    self.start(edition, action);
                    return true;
                }
                metrics::record_zork_switch(ZorkSwitchResult::SaveFailed);
                self.message =
                    "The current game did not finish saving. Choose Continue before trying again."
                        .into();
            } else if !outcome.message.is_empty() {
                self.message = outcome.message;
            } else if !matches!(outcome.ending, Ending::Exited(EXIT_QUIT | EXIT_RETURNED)) {
                self.message =
                    "Zork closed unexpectedly. Continue uses the last completed autosave.".into();
            }
            self.edition = previous;
            self.mode = Mode::Actions;
            self.exit_grace = 10;
            self.refresh();
        }
        if self
            .pending
            .is_some_and(|(_, _, started)| started.elapsed() >= RETURN_TIMEOUT)
        {
            changed = true;
            metrics::record_zork_switch(ZorkSwitchResult::TimedOut);
            self.pending = None;
            self.mode = Mode::Actions;
            if let Some(edition) = self.live {
                self.edition = edition;
            }
            self.message = "The current game could not finish saving. It is still open; try again after the next prompt.".into();
        }
        if let Some(edition) = self.live
            && self.last_input.elapsed() >= IDLE_SHUTDOWN
        {
            changed = true;
            metrics::record_zork_session_end(edition, ZorkSessionEnd::Idle);
            self.proxy = None;
            self.live = None;
            self.pending = None;
            self.mode = Mode::Actions;
            self.message =
                "Closed after 20 minutes without game input. Continue resumes the last autosave."
                    .into();
            self.refresh();
        }
        changed
    }
    pub fn available(&self, action: Action) -> bool {
        if !self.enabled || self.pending.is_some() {
            return false;
        }
        match action {
            Action::New => true,
            Action::Auto if self.live == Some(self.edition) => true,
            Action::Auto => self
                .slots(self.edition)
                .is_some_and(|slots| slots.autosave.status == Availability::Ready),
            Action::Manual => self
                .slots(self.edition)
                .is_some_and(|slots| slots.manual.status == Availability::Ready),
        }
    }
    fn launch(&mut self, action: Action) {
        if !self.available(action) {
            return;
        }
        if action == Action::Auto && self.live == Some(self.edition) {
            self.mode = Mode::Running;
            return;
        }
        // Every game runs under the claimed handle. Without one this opens
        // the claim prompt, and the claim landing launches the same action
        // (`tick`).
        if self.handle.claimed().is_none() {
            self.after_claim = Some((self.edition, action));
            self.handle.request_launch();
            // A confirmation already answered stays answered: the menu
            // under the prompt is the action list again.
            self.mode = Mode::Actions;
            return;
        }
        if let Some(proxy) = &self.proxy {
            self.pending = Some((self.edition, action, Instant::now()));
            proxy.return_to_menu();
            self.mode = Mode::Running;
            self.message = "Saving the current game before switching…".into();
        } else {
            self.start(self.edition, action);
        }
    }
    fn start(&mut self, edition: Edition, action: Action) {
        metrics::record_zork_launch(edition, action);
        let playname = self
            .handle
            .claimed()
            .expect("a game only starts under a claimed handle");
        self.proxy = Some(Process::spawn(
            self.connection.clone(),
            edition,
            action,
            &playname,
            self.viewport.width,
            self.viewport.height,
            self.term.clone(),
        ));
        self.live = Some(edition);
        self.edition = edition;
        self.mode = Mode::Running;
        self.last_input = Instant::now();
        self.message.clear();
        self.exit_grace = 0;
        self.input = InputFilter::default();
    }
    /// Returns true when Back should leave the trilogy for the Games hub.
    pub fn back(&mut self) -> bool {
        match self.mode {
            Mode::Editions => true,
            Mode::Actions => {
                self.mode = Mode::Editions;
                false
            }
            Mode::Confirm(_) => {
                self.mode = Mode::Actions;
                false
            }
            Mode::Running => false,
        }
    }
    pub fn menu_key(&mut self, byte: u8) -> bool {
        if self.exit_grace > 0 {
            return true;
        }
        if let Mode::Confirm(action) = self.mode {
            match byte {
                b'y' | b'Y' => self.launch(action),
                b'n' | b'N' | b'\r' | b'\n' | 0x1b => self.mode = Mode::Actions,
                _ => {}
            }
            return true;
        }
        match byte {
            b'j' => self.step(true),
            b'k' => self.step(false),
            b'r' => {
                self.message.clear();
                self.refresh();
            }
            b'\r' | b'\n' => match self.mode {
                Mode::Editions => {
                    self.edition = Edition::ALL[self.selected];
                    self.action = 0;
                    self.mode = Mode::Actions;
                }
                Mode::Actions => match self.action {
                    0 => self.launch(Action::Auto),
                    1 if self.available(Action::Manual) => {
                        self.mode = Mode::Confirm(Action::Manual)
                    }
                    2 if self.available(Action::New) => self.mode = Mode::Confirm(Action::New),
                    3 => self.mode = Mode::Editions,
                    _ => {}
                },
                _ => {}
            },
            _ => return false,
        }
        true
    }
    pub fn step(&mut self, down: bool) {
        let (index, count) = match self.mode {
            Mode::Editions => (&mut self.selected, 3),
            Mode::Actions => (&mut self.action, 4),
            _ => return,
        };
        *index = (*index + if down { 1 } else { count - 1 }) % count;
    }
    pub fn in_exit_grace(&self) -> bool {
        self.exit_grace > 0
    }
    pub fn forward_input(&mut self, data: &[u8]) {
        let bytes = self.input.feed(data);
        self.send_game_input(bytes);
    }
    /// F3: type the waiting watcher suggestion onto the story's prompt,
    /// without Enter, so the player still reads and sends it. Only at an
    /// empty prompt (`suggest::at_empty_prompt`); anywhere else nothing is
    /// typed.
    pub fn type_suggestion(&mut self, suggestion: Option<Suggestion>) {
        let at_prompt = self
            .proxy
            .as_ref()
            .is_some_and(|proxy| proxy.with_screen(super::suggest::at_empty_prompt));
        let result = match (suggestion, at_prompt) {
            (None, _) => ZorkSuggestion::NoneWaiting,
            (Some(_), false) => ZorkSuggestion::NotAtPrompt,
            (Some(suggestion), true) => {
                self.typed_suggestion = Some(suggestion.message_id);
                self.send_game_input(suggestion.command.into_bytes());
                ZorkSuggestion::Typed
            }
        };
        metrics::record_zork_suggestion(result);
    }
    /// Test-only: fabricate a live game around a proxy pointed at a dead
    /// address, so detach and reap paths can be exercised without a host.
    /// Needs a Tokio runtime (the proxy spawns its bridge task).
    #[cfg(test)]
    pub fn force_running_for_test(&mut self) {
        self.handle.force_claimed_for_test("tester");
        self.start(Edition::Zork1, Action::New);
    }
    fn send_game_input(&mut self, bytes: Vec<u8>) {
        if let Some(proxy) = &self.proxy {
            let bytes = if proxy.with_screen(|screen| screen.application_cursor()) {
                keys::to_application_cursor(&bytes)
            } else {
                bytes
            };
            if !bytes.is_empty() {
                proxy.send_input(bytes);
                self.last_input = Instant::now();
            }
        }
    }
}
/// The telemetry tag for a closed session, from the host's exit statuses.
fn session_end(ending: Ending) -> ZorkSessionEnd {
    match ending {
        Ending::Exited(EXIT_QUIT) => ZorkSessionEnd::Quit,
        Ending::Exited(EXIT_RETURNED) => ZorkSessionEnd::Returned,
        Ending::Exited(EXIT_SAVE_REJECTED) => ZorkSessionEnd::SaveRejected,
        Ending::Exited(EXIT_BUSY) => ZorkSessionEnd::Busy,
        Ending::Exited(_) | Ending::Closed => ZorkSessionEnd::Crashed,
        Ending::Failed => ZorkSessionEnd::ConnectFailed,
    }
}

/// SSH data packets need not coincide with terminal escape sequences. Hold
/// incomplete escapes briefly, so mouse/paste wrappers cannot leak into a
/// command and application-cursor translation sees whole arrow sequences.
#[derive(Default)]
struct InputFilter {
    pending: Vec<u8>,
    waiting_since: Option<Instant>,
}
impl InputFilter {
    fn feed(&mut self, data: &[u8]) -> Vec<u8> {
        self.pending.extend_from_slice(data);
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.pending.len() {
            let rest = &self.pending[i..];
            if rest[0] != 0x1b {
                out.push(rest[0]);
                i += 1;
                continue;
            }
            if rest.len() < 2 {
                break;
            }
            if rest[1] == b'O' {
                if rest.len() < 3 {
                    break;
                }
                out.extend_from_slice(&rest[..3]);
                i += 3;
                continue;
            }
            if rest[1] != b'[' {
                out.push(rest[0]);
                i += 1;
                continue;
            }
            if rest.starts_with(b"\x1b[M") {
                if rest.len() < 6 {
                    break;
                }
                i += 6;
                continue;
            }
            let Some(end) = rest[2..].iter().position(|b| (0x40..=0x7e).contains(b)) else {
                // Bound malformed escape buffering independently of SSH chunks.
                if rest.len() > 64 {
                    i = self.pending.len();
                }
                break;
            };
            let len = end + 3;
            let sequence = &rest[..len];
            if !sequence.starts_with(b"\x1b[<")
                && sequence != b"\x1b[200~"
                && sequence != b"\x1b[201~"
            {
                out.extend_from_slice(sequence);
            }
            i += len;
        }
        self.pending.drain(..i);
        self.waiting_since = if self.pending.is_empty() {
            None
        } else {
            Some(Instant::now())
        };
        out
    }
    fn flush(&mut self) -> Vec<u8> {
        self.waiting_since = None;
        let bytes = std::mem::take(&mut self.pending);
        // An isolated Escape must still cancel SAVE/RESTORE. Incomplete noise
        // gets discarded rather than becoming characters at a story prompt.
        if bytes.starts_with(b"\x1b[<") || bytes.starts_with(b"\x1b[M") {
            Vec::new()
        } else {
            bytes
        }
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
