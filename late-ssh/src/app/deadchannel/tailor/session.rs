//! A session's side of the tailor: the draft in the mirror, what the row
//! wears as far as this session knows, the tailor's last word, and the
//! one write that may be out. Decides nothing: a key press moves the
//! draft, `wear` asks the service, and the answer sets the word.

use tokio::sync::mpsc;
use uuid::Uuid;

use super::state::Draft;
use super::svc::{TailorOutcome, TailorService};
use crate::app::deadchannel::runner::state::Look;

pub(crate) struct TailorSession {
    /// The look being tried on while the panel is open; `None` when it
    /// is shut, or open with no runner to dress.
    pub draft: Option<Draft>,
    /// What the row wears: the look the panel opened on, then each look
    /// the service confirmed.
    pub worn: Option<Look>,
    /// The tailor's last word: the answer to `wear`.
    pub word: Option<String>,
    /// A write is out and its answer has not landed.
    pub saving: bool,
    user_id: Uuid,
    svc: TailorService,
    outcome_tx: mpsc::UnboundedSender<TailorOutcome>,
    outcome_rx: mpsc::UnboundedReceiver<TailorOutcome>,
}

impl TailorSession {
    pub(crate) fn new(user_id: Uuid, svc: TailorService) -> Self {
        let (outcome_tx, outcome_rx) = mpsc::unbounded_channel();
        Self {
            draft: None,
            worn: None,
            word: None,
            saving: false,
            user_id,
            svc,
            outcome_tx,
            outcome_rx,
        }
    }

    /// Step up to the mirror wearing `look` (the directory's copy; `None`
    /// when the directory has no runner for this user yet).
    pub(crate) fn open(&mut self, look: Option<Look>) {
        self.draft = look.map(Draft::new);
        self.worn = look;
        self.word = None;
    }

    /// Back to the street. A draft not worn is dropped.
    pub(crate) fn close(&mut self) {
        self.draft = None;
        self.word = None;
    }

    /// Whether the draft differs from what the row wears.
    pub(crate) fn changed(&self) -> bool {
        match (&self.draft, &self.worn) {
            (Some(draft), Some(worn)) => draft.look != *worn,
            _ => false,
        }
    }

    /// Ask the service to put the draft on the row. One write out at a
    /// time; a draft equal to what is worn asks for nothing.
    pub(crate) fn wear(&mut self) {
        if self.saving {
            return;
        }
        let Some(draft) = self.draft else {
            return;
        };
        if !self.changed() {
            self.word = Some("you are wearing that already.".to_string());
            return;
        }
        self.saving = true;
        self.svc
            .wear_task(self.user_id, draft.look, self.outcome_tx.clone());
    }

    /// Drain answers. Returns true when the mirror moved and the frame
    /// needs repainting.
    pub(crate) fn tick(&mut self) -> bool {
        let mut changed = false;
        while let Ok(outcome) = self.outcome_rx.try_recv() {
            changed = true;
            self.saving = false;
            match outcome {
                TailorOutcome::Worn(look) => {
                    self.worn = Some(look);
                    self.word = Some("the tailor turns the mirror. that is you now.".to_string());
                }
                TailorOutcome::NoRunner => {
                    self.word = Some("nothing looks back. you have no runner here.".to_string());
                }
                TailorOutcome::Failed => {
                    self.word = Some("the tailor is not answering. try again.".to_string());
                }
            }
        }
        changed
    }
}
