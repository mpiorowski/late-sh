use super::protocol::{Action, Availability, Catalogue, Edition, EditionSlots};
use super::proxy::{self, Connection, Process, ProxyStatus};
use crate::app::door::keys;
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
}
pub struct State {
    connection: Connection,
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
        if self.lookup.as_ref().is_some_and(|task| task.is_finished()) {
            changed = true;
            match self
                .lookup
                .take()
                .expect("finished lookup")
                .now_or_never()
                .expect("finished lookup result")
            {
                Ok(Ok(catalogue)) => self.catalogue = Some(catalogue),
                error => {
                    tracing::warn!(?error, "Zork save catalogue unavailable");
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
            let previous = self.live.take();
            if let Some((edition, action, _)) = self.pending.take() {
                if outcome.code == Some(20) {
                    self.start(edition, action);
                    return true;
                }
                self.message =
                    "The current game did not finish saving. Choose Continue before trying again."
                        .into();
            } else if !outcome.message.is_empty() {
                self.message = outcome.message;
            } else if !matches!(outcome.code, Some(0 | 20)) {
                self.message =
                    "Zork closed unexpectedly. Continue uses the last completed autosave.".into();
            }
            if let Some(edition) = previous {
                self.edition = edition;
            }
            self.mode = Mode::Actions;
            self.exit_grace = 10;
            self.refresh();
        }
        if self
            .pending
            .is_some_and(|(_, _, started)| started.elapsed() >= RETURN_TIMEOUT)
        {
            changed = true;
            self.pending = None;
            self.mode = Mode::Actions;
            if let Some(edition) = self.live {
                self.edition = edition;
            }
            self.message = "The current game could not finish saving. It is still open; try again after the next prompt.".into();
        }
        if self.live.is_some() && self.last_input.elapsed() >= IDLE_SHUTDOWN {
            changed = true;
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
        self.proxy = Some(Process::spawn(
            self.connection.clone(),
            edition,
            action,
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
