use late_core::models::chat_message_gild::GildTier;
use uuid::Uuid;

/// The message a gild is being bought for, captured when the picker opens.
/// Holding the author's name and a body preview here means the modal renders
/// what the buyer selected even if the room scrolls underneath it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GildTarget {
    pub message_id: Uuid,
    pub author_username: String,
    pub preview: String,
}

/// What `Enter` on the picker asks the service for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GildSubmit {
    pub message_id: Uuid,
    pub tier: GildTier,
}

/// The three-row tier picker. Pure selection state: what it costs and what
/// the author gets are read off [`GildTier`], and the balance is read off the
/// app at draw time, so nothing here can go stale.
#[derive(Debug, Default)]
pub(crate) struct GildModalState {
    target: Option<GildTarget>,
    selected: usize,
}

impl GildModalState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn open(&mut self, target: GildTarget) {
        self.target = Some(target);
        self.selected = 0;
    }

    pub(crate) fn close(&mut self) {
        self.target = None;
        self.selected = 0;
    }

    pub(crate) fn target(&self) -> Option<&GildTarget> {
        self.target.as_ref()
    }

    pub(crate) fn selected_tier(&self) -> GildTier {
        GildTier::ALL[self.selected.min(GildTier::ALL.len() - 1)]
    }

    /// Move the cursor without wrapping: three rows priced 100x apart, and a
    /// wrap from Bronze to Gold is exactly the keystroke nobody wants to
    /// discover after `Enter`.
    pub(crate) fn move_selection(&mut self, delta: isize) {
        let last = GildTier::ALL.len() as isize - 1;
        let next = (self.selected as isize + delta).clamp(0, last);
        self.selected = next as usize;
    }

    pub(crate) fn select_index(&mut self, index: usize) {
        if index < GildTier::ALL.len() {
            self.selected = index;
        }
    }

    /// The purchase to send, or `None` when the picker is not open.
    pub(crate) fn submit(&self) -> Option<GildSubmit> {
        self.target.as_ref().map(|target| GildSubmit {
            message_id: target.message_id,
            tier: self.selected_tier(),
        })
    }
}
