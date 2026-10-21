//! A session's side of PRINTS: what the bench holds (shut, no runner, the
//! wall being read, a draft), whether a poster hangs as far as this session
//! knows, the printer's last word, and the one write that may be out.
//! Decides nothing: a key press moves the draft, `save` and `burn` ask the
//! service, and the answers set the word.

use dartboard_core::Canvas;
use tokio::sync::mpsc;
use uuid::Uuid;

use super::state::Draft;
use super::svc::{PosterOutcome, PosterService};
use crate::app::deadchannel::runner::svc::RunnerEntry;

/// What the panel has in front of it.
#[derive(Debug)]
pub(crate) enum Bench {
    /// The panel is shut.
    Shut,
    /// No standing runner in the directory: nothing to paint for.
    NoRunner,
    /// The wall is being read; the brush will be cut to `marks`.
    Reading {
        marks: i32,
    },
    /// The wall could not be read: nothing to paint on until the panel
    /// opens again.
    Unread,
    Open(Draft),
}

/// The write that is out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Write {
    Save,
    Burn,
}

pub(crate) struct PosterSession {
    pub bench: Bench,
    /// A poster hangs on the wall: read when the panel opened, then each
    /// save and burn the service confirmed.
    pub hung: bool,
    /// The printer's last word: the answer to `save` or `burn`.
    pub word: Option<String>,
    /// A write is out and its answer has not landed.
    pub writing: Option<Write>,
    user_id: Uuid,
    username: String,
    svc: PosterService,
    outcome_tx: mpsc::UnboundedSender<PosterOutcome>,
    outcome_rx: mpsc::UnboundedReceiver<PosterOutcome>,
}

impl PosterSession {
    pub(crate) fn new(user_id: Uuid, username: String, svc: PosterService) -> Self {
        let (outcome_tx, outcome_rx) = mpsc::unbounded_channel();
        Self {
            bench: Bench::Shut,
            hung: false,
            word: None,
            writing: None,
            user_id,
            username,
            svc,
            outcome_tx,
            outcome_rx,
        }
    }

    /// Step up to the press as `runner` (the directory's copy; `None` when
    /// the directory has no runner for this user). The wall is read, and
    /// the draft opens on it a tick later.
    pub(crate) fn open(&mut self, runner: Option<RunnerEntry>) {
        self.word = None;
        self.bench = match runner {
            Some(entry) => {
                self.svc
                    .read_task(self.user_id, self.username.clone(), self.outcome_tx.clone());
                Bench::Reading { marks: entry.marks }
            }
            None => Bench::NoRunner,
        };
    }

    /// Back to the street. A draft not saved is dropped.
    pub(crate) fn close(&mut self) {
        self.bench = Bench::Shut;
        self.word = None;
    }

    pub(crate) fn draft(&self) -> Option<&Draft> {
        match &self.bench {
            Bench::Open(draft) => Some(draft),
            Bench::Shut | Bench::NoRunner | Bench::Reading { .. } | Bench::Unread => None,
        }
    }

    pub(crate) fn draft_mut(&mut self) -> Option<&mut Draft> {
        match &mut self.bench {
            Bench::Open(draft) => Some(draft),
            Bench::Shut | Bench::NoRunner | Bench::Reading { .. } | Bench::Unread => None,
        }
    }

    /// Hang the draft. One write out at a time; a draft equal to what
    /// hangs asks for nothing.
    pub(crate) fn save(&mut self) {
        if self.writing.is_some() {
            return;
        }
        let Some(draft) = self.draft() else {
            return;
        };
        if !draft.dirty() {
            self.word = Some("that is what hangs already.".to_string());
            return;
        }
        let canvas = draft.canvas.clone();
        self.writing = Some(Write::Save);
        self.svc.save_task(
            self.user_id,
            self.username.clone(),
            canvas,
            self.outcome_tx.clone(),
        );
    }

    /// Burn what hangs. One write out at a time.
    pub(crate) fn burn(&mut self) {
        if self.writing.is_some() {
            return;
        }
        self.writing = Some(Write::Burn);
        self.svc
            .burn_task(self.user_id, self.username.clone(), self.outcome_tx.clone());
    }

    /// Drain answers. Returns true when the panel moved and the frame needs
    /// repainting.
    pub(crate) fn tick(&mut self) -> bool {
        let mut changed = false;
        while let Ok(outcome) = self.outcome_rx.try_recv() {
            changed = true;
            match outcome {
                PosterOutcome::Hung(stored) => self.hung_read(stored),
                PosterOutcome::Unread => {
                    if let Bench::Reading { .. } = self.bench {
                        self.bench = Bench::Unread;
                    }
                }
                PosterOutcome::Saved(canvas) => {
                    self.writing = None;
                    self.hung = true;
                    if let Some(draft) = self.draft_mut() {
                        draft.saved(canvas);
                    }
                    self.word = Some("the press runs. it hangs on your profile now.".to_string());
                }
                PosterOutcome::Burned => {
                    self.writing = None;
                    self.hung = false;
                    if let Some(draft) = self.draft_mut() {
                        draft.burned();
                    }
                    self.word = Some("it burns. the wall is bare.".to_string());
                }
                PosterOutcome::NoRunner => {
                    self.writing = None;
                    self.word = Some("the printer shrugs. you have no runner here.".to_string());
                }
                PosterOutcome::Failed => {
                    self.writing = None;
                    self.word = Some("the press jams. try again.".to_string());
                }
            }
        }
        changed
    }

    /// The wall read for the panel: the draft opens on it, unless the
    /// panel was shut (or opened again) before the answer landed.
    fn hung_read(&mut self, stored: Option<Canvas>) {
        let Bench::Reading { marks } = self.bench else {
            return;
        };
        self.hung = stored.is_some();
        self.bench = Bench::Open(Draft::new(stored, marks));
    }
}
