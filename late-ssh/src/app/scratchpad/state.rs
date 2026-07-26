use late_core::MutexRecover;
use ratatui_textarea::{CursorMove, TextArea, WrapMode};
use uuid::Uuid;

use crate::app::common::composer::new_themed_textarea;

use super::registry::{SharedScratchpad, SharedScratchpadRegistry};

/// Small on purpose: a scratchpad, not a file editor (no DB persistence, no
/// syntax highlighting, no more than two participants; see module scope).
const MAX_CHARS: usize = 8_000;

/// Per-session view onto a shared scratchpad pairing. `shared` is the same
/// `Arc<Mutex<..>>` the partner's session holds; `editor` is this session's
/// local `TextArea`, resynced from `shared` whenever the partner publishes.
pub(crate) struct ScratchpadState {
    registry: SharedScratchpadRegistry,
    shared: SharedScratchpad,
    own_user_id: Uuid,
    pub(crate) partner_id: Uuid,
    pub(crate) partner_username: String,
    pub(crate) editor: TextArea<'static>,
    /// The shared buffer's `revision` as of our last read or write. Sync
    /// only pulls when this is stale, so publishing our own edit doesn't
    /// immediately bounce back and clobber our local cursor.
    last_seen_revision: u64,
    /// Partner's last-known cursor, presence-only (never used to merge
    /// edits), refreshed on every `sync_from_shared`.
    pub(crate) partner_cursor: (usize, usize),
}

impl ScratchpadState {
    pub(crate) fn new(
        registry: SharedScratchpadRegistry,
        shared: SharedScratchpad,
        own_user_id: Uuid,
        partner_id: Uuid,
        partner_username: String,
    ) -> Self {
        let mut editor = new_themed_textarea("", WrapMode::None, true);
        let (last_seen_revision, partner_cursor) = {
            let mut buffer = shared.lock_recover();
            // Tells the registry this side actually opened the editor, so a
            // pairing whose other session died is torn down whole instead of
            // stranding them as permanently paired.
            buffer.mark_joined(own_user_id);
            editor.insert_str(&buffer.content);
            (buffer.revision, buffer.cursor_for(partner_id))
        };
        Self {
            registry,
            shared,
            own_user_id,
            partner_id,
            partner_username,
            editor,
            last_seen_revision,
            partner_cursor,
        }
    }

    /// Pull the partner's latest edit into the local editor, if anything
    /// changed since we last looked. Skips content we just published
    /// ourselves. Returns true when something render-visible changed.
    ///
    /// The local cursor jumps to the end of the newly-adopted content rather
    /// than trying to preserve its old position: with no OT/CRDT merge, an
    /// old position is measured against content that no longer exists, and
    /// restoring it verbatim silently inserts the next keystroke wherever
    /// that stale offset now lands (e.g. index 0 for a side that hadn't
    /// typed yet), scrambling the buffer instead of appending after the
    /// partner's edit.
    ///
    /// The yank register is saved across the replace: `cut` is the only bulk
    /// delete the textarea exposes, and it would otherwise overwrite whatever
    /// the user had copied with the entire buffer every time the partner
    /// typed.
    pub(crate) fn sync_from_shared(&mut self) -> bool {
        let (content, partner_cursor) = {
            let buffer = self.shared.lock_recover();
            if buffer.revision == self.last_seen_revision {
                return false;
            }
            self.last_seen_revision = buffer.revision;
            (buffer.content.clone(), buffer.cursor_for(self.partner_id))
        };
        self.partner_cursor = partner_cursor;
        if self.editor.lines().join("\n") == content {
            return true;
        }
        let yank = self.editor.yank_text();
        self.editor.select_all();
        self.editor.cut();
        self.editor.insert_str(&content);
        self.editor
            .move_cursor(CursorMove::Jump(u16::MAX, u16::MAX));
        self.editor.set_yank_text(yank);
        true
    }

    /// Push our local edits to the shared buffer and bump the revision.
    pub(crate) fn publish(&mut self) {
        let cursor = self.editor.cursor();
        let mut buffer = self.shared.lock_recover();
        buffer.content = self.editor.lines().join("\n");
        buffer.set_cursor_for(self.own_user_id, (cursor.0, cursor.1));
        buffer.revision += 1;
        self.last_seen_revision = buffer.revision;
    }

    pub(crate) fn partner_left(&self) -> bool {
        self.shared.lock_recover().left.is_some()
    }

    pub(crate) fn max_chars(&self) -> usize {
        MAX_CHARS
    }
}

impl Drop for ScratchpadState {
    fn drop(&mut self) {
        self.registry.leave(self.own_user_id);
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
