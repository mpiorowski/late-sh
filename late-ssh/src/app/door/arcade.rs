// The arcade handle: one immutable public name per account, shared by door
// games whose upstream binaries key saves and public score files by player
// name (DCSS today; NetHack may adopt it later). Validation and uniqueness
// live in late-core (`models::arcade_handle`); this is the session-side
// accessor, cloned into each connection like the other door services.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use late_core::db::Db;
use late_core::models::arcade_handle::{self, ArcadeHandle, ClaimOutcome};
use uuid::Uuid;

use crate::render_signal::RenderSignal;

/// Thin async accessor for the account's arcade handle.
#[derive(Clone)]
pub struct ArcadeHandleService {
    db: Db,
}

impl ArcadeHandleService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// The account's claimed handle, if any.
    pub async fn get(&self, user_id: Uuid) -> Result<Option<String>> {
        let client = self.db.get().await?;
        ArcadeHandle::find_by_user_id(&client, user_id).await
    }

    /// Claim a handle for the account (first claim wins; immutable after).
    /// The caller pre-validates shape and reserved names.
    pub async fn claim(&self, user_id: Uuid, handle: &str) -> Result<ClaimOutcome> {
        let client = self.db.get().await?;
        ArcadeHandle::claim(&client, user_id, handle).await
    }
}

/// Where the account stands with its arcade handle. Written by the background
/// lookup/claim tasks, read by a door's launcher UI, so [`HandleFlow`] keeps it
/// behind an `Arc<Mutex<..>>` like the doors' proxy status flags.
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

/// What a launcher should do with a key byte it fed to [`HandleFlow::key`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleKeyResult {
    /// The flow consumed the byte (prompt editing, submit, retry, debounce).
    Consumed,
    /// Enter with a claimed handle: the door should connect now.
    Launch,
    /// Not the flow's byte; fall through to the global keymap.
    Ignored,
}

/// The launcher-side lifecycle of the arcade handle, shared by every door that
/// keys saves by it (DCSS, NetHack): background lookup on entry, a one-time
/// claim prompt with a compose buffer, and launch intent carried across the
/// async gaps so the hub's single Enter still starts the game.
pub struct HandleFlow {
    /// `None` on headless/test paths (the prompt then reports the name service
    /// as unavailable).
    svc: Option<ArcadeHandleService>,
    user_id: Uuid,
    /// Render-loop wakeup so async results repaint promptly.
    repaint: Option<Arc<RenderSignal>>,
    status: Arc<Mutex<HandleStatus>>,
    /// Compose buffer for the claim prompt. Only the foreground touches it.
    entry: String,
    /// The player asked to launch before the handle was known (hub Enter races
    /// the lookup; a claim is a launch intent too). The door's `tick()` drains
    /// it via [`HandleFlow::take_ready_launch`].
    launch_pending: bool,
    /// The player closed the claim modal with Esc. The landing then shows a
    /// hint instead, and Enter (or another launch attempt) reopens the modal.
    dismissed: bool,
}

impl HandleFlow {
    /// Build the flow and, when a service is present, start the background
    /// lookup immediately (the caller gates this on the door being enabled).
    pub fn new(
        user_id: Uuid,
        svc: Option<ArcadeHandleService>,
        repaint: Option<Arc<RenderSignal>>,
    ) -> Self {
        let flow = Self {
            svc,
            user_id,
            repaint,
            status: Arc::new(Mutex::new(HandleStatus::Missing { error: None })),
            entry: String::new(),
            launch_pending: false,
            dismissed: false,
        };
        if flow.svc.is_some() {
            flow.spawn_load();
        }
        flow
    }

    /// Snapshot of the handle lifecycle for the launcher UI.
    pub fn status(&self) -> HandleStatus {
        self.status.lock().expect("handle mutex").clone()
    }

    /// The claimed handle, if the account has one.
    pub fn claimed(&self) -> Option<String> {
        match self.status() {
            HandleStatus::Claimed(name) => Some(name),
            HandleStatus::Loading
            | HandleStatus::Missing { .. }
            | HandleStatus::Claiming
            | HandleStatus::Failed => None,
        }
    }

    /// Whether the flow still owes the player handle work (lookup, claim
    /// prompt, retry). Doors use this to hold their screen up while no game is
    /// running.
    pub fn awaiting(&self) -> bool {
        self.claimed().is_none()
    }

    /// The claim prompt's compose buffer, for rendering.
    pub fn entry_input(&self) -> &str {
        &self.entry
    }

    /// Whether the one-time claim modal should be on screen: the account has
    /// no handle yet (or the lookup failed) and the player hasn't waved it
    /// away with Esc. Loading stays modal-free so a fast lookup never flashes.
    pub fn modal_visible(&self) -> bool {
        if self.dismissed {
            return false;
        }
        matches!(
            self.status(),
            HandleStatus::Missing { .. } | HandleStatus::Claiming | HandleStatus::Failed
        )
    }

    /// Close the claim modal without claiming; the landing shows a reopen hint
    /// until the next launch attempt.
    pub fn dismiss_modal(&mut self) {
        self.dismissed = true;
    }

    /// Record launch intent. While the lookup or a claim is in flight the
    /// intent is remembered and `take_ready_launch` fires it once the handle
    /// lands; in the settled no-handle states it reopens the claim modal
    /// instead (the player wants to play, and the name is what's in the way).
    pub fn request_launch(&mut self) {
        match self.status() {
            HandleStatus::Loading | HandleStatus::Claiming => self.launch_pending = true,
            HandleStatus::Missing { .. } | HandleStatus::Failed => self.dismissed = false,
            HandleStatus::Claimed(_) => {}
        }
    }

    /// Drain pending launch intent: true exactly once when a launch was
    /// requested and the handle has since landed on Claimed. Voids the intent
    /// when the flow settled without a handle instead.
    pub fn take_ready_launch(&mut self) -> bool {
        if !self.launch_pending {
            return false;
        }
        match self.status() {
            HandleStatus::Claimed(_) => {
                self.launch_pending = false;
                true
            }
            HandleStatus::Missing { .. } | HandleStatus::Failed => {
                self.launch_pending = false;
                false
            }
            HandleStatus::Loading | HandleStatus::Claiming => false,
        }
    }

    /// Feed a launcher-mode key byte through the flow. While the claim prompt
    /// is open every printable is consumed, so a typed `q` cannot fall through
    /// to the global quit mid-word.
    pub fn key(&mut self, byte: u8) -> HandleKeyResult {
        let is_enter = matches!(byte, b'\r' | b'\n');
        match self.status() {
            HandleStatus::Claimed(_) => {
                if is_enter {
                    HandleKeyResult::Launch
                } else {
                    HandleKeyResult::Ignored
                }
            }
            // Work in flight; swallow Enter so it can't double-submit.
            HandleStatus::Loading | HandleStatus::Claiming => {
                if is_enter {
                    HandleKeyResult::Consumed
                } else {
                    HandleKeyResult::Ignored
                }
            }
            HandleStatus::Failed => {
                if is_enter {
                    self.dismissed = false;
                    self.spawn_load();
                    HandleKeyResult::Consumed
                } else {
                    HandleKeyResult::Ignored
                }
            }
            // Modal waved away: only Enter is ours, and it reopens the modal
            // rather than submitting the (hidden) buffer.
            HandleStatus::Missing { .. } if self.dismissed => {
                if is_enter {
                    self.dismissed = false;
                    HandleKeyResult::Consumed
                } else {
                    HandleKeyResult::Ignored
                }
            }
            HandleStatus::Missing { .. } => match byte {
                // Esc closes the modal when it arrives as a raw byte (it
                // usually comes via the global escape dispatch instead, which
                // calls `dismiss_modal` directly).
                0x1b => {
                    self.dismissed = true;
                    HandleKeyResult::Consumed
                }
                b'\r' | b'\n' => {
                    self.submit_entry();
                    HandleKeyResult::Consumed
                }
                0x08 | 0x7f => {
                    self.entry.pop();
                    HandleKeyResult::Consumed
                }
                0x20..=0x7e => {
                    if (byte.is_ascii_alphanumeric() || byte == b'_')
                        && self.entry.len() < arcade_handle::HANDLE_MAX_LEN
                    {
                        self.entry.push(byte as char);
                    }
                    HandleKeyResult::Consumed
                }
                _ => HandleKeyResult::Ignored,
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
        let Some(svc) = self.svc.clone() else {
            self.set_entry_error("The name service is unavailable.");
            return;
        };
        *self.status.lock().expect("handle mutex") = HandleStatus::Claiming;
        self.launch_pending = true;
        let slot = self.status.clone();
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
        *self.status.lock().expect("handle mutex") = HandleStatus::Missing {
            error: Some(message.to_string()),
        };
    }

    /// Look up the account's handle in the background; the launcher shows
    /// Loading until the result lands.
    fn spawn_load(&self) {
        let Some(svc) = self.svc.clone() else {
            *self.status.lock().expect("handle mutex") = HandleStatus::Missing { error: None };
            return;
        };
        *self.status.lock().expect("handle mutex") = HandleStatus::Loading;
        let slot = self.status.clone();
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
}
