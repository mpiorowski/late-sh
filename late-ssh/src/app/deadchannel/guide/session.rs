//! A session's side of the guide: the view state, and the first-descent
//! claim that opens it by itself once. A key press moves the state; the
//! guide opens on its own only when the service's answer says this was
//! the first time down.

use tokio::sync::mpsc;
use uuid::Uuid;

use super::state::State;
use super::svc::{GuideOutcome, GuideService};

pub(crate) struct GuideSession {
    pub state: State,
    user_id: Uuid,
    svc: GuideService,
    outcome_tx: mpsc::UnboundedSender<GuideOutcome>,
    outcome_rx: mpsc::UnboundedReceiver<GuideOutcome>,
}

impl GuideSession {
    pub(crate) fn new(user_id: Uuid, svc: GuideService) -> Self {
        let (outcome_tx, outcome_rx) = mpsc::unbounded_channel();
        Self {
            state: State::new(),
            user_id,
            svc,
            outcome_tx,
            outcome_rx,
        }
    }

    /// The descent: ask whether it is the first. The answer lands on a
    /// later tick, so the street shows first and the guide comes up over
    /// it.
    pub(crate) fn descend(&mut self) {
        self.svc
            .claim_first_descent_task(self.user_id, self.outcome_tx.clone());
    }

    /// Drain answers. Returns true when the guide opened.
    pub(crate) fn tick(&mut self) -> bool {
        let mut changed = false;
        while let Ok(outcome) = self.outcome_rx.try_recv() {
            match outcome {
                GuideOutcome::FirstDescent => {
                    self.state.open();
                    changed = true;
                }
                GuideOutcome::SeenBefore => {}
            }
        }
        changed
    }
}
