//! A session's side of the bonsai: a mirror of the stored tree to draw
//! from, and the requests that ask `BonsaiService` to change it. Nothing
//! here decides anything. A key press becomes a command; the tree on screen
//! moves when the service's answer (or another session's change) arrives.

use late_core::models::{bonsai::Tree, bonsai_decay_protection::BonsaiDecayProtection};
use tokio::sync::{broadcast, mpsc};
use uuid::Uuid;

use crate::app::bonsai::state::{BonsaiAction, BonsaiCommand, BonsaiState};
use crate::app::bonsai::svc::{BonsaiOutcome, BonsaiService};

pub(crate) struct BonsaiSession {
    /// The mirror. Only the selection cursor and `message` are this
    /// session's own; everything else is a copy of the row.
    pub tree: BonsaiState,
    user_id: Uuid,
    svc: BonsaiService,
    outcome_tx: mpsc::UnboundedSender<BonsaiOutcome>,
    outcome_rx: mpsc::UnboundedReceiver<BonsaiOutcome>,
    changes_rx: broadcast::Receiver<Uuid>,
}

impl BonsaiSession {
    pub(crate) fn new(user_id: Uuid, svc: BonsaiService, tree: BonsaiState) -> Self {
        let (outcome_tx, outcome_rx) = mpsc::unbounded_channel();
        let changes_rx = svc.subscribe_changes();
        Self {
            tree,
            user_id,
            svc,
            outcome_tx,
            outcome_rx,
            changes_rx,
        }
    }

    /// Ask the service to run `action` on the branch under this session's
    /// cursor. The mirror does not move until the answer comes back.
    pub(crate) fn request(&mut self, action: BonsaiAction) {
        let command = BonsaiCommand {
            selected_branch_id: self.tree.selected_branch_id,
            action,
        };
        self.svc
            .act_task(self.user_id, command, self.outcome_tx.clone());
    }

    /// Drain answers and change notices. Returns true when the mirror or
    /// its message moved and the frame needs repainting.
    pub(crate) fn tick(&mut self) -> bool {
        if self.own_tree_changed() {
            self.svc.reload_task(self.user_id, self.outcome_tx.clone());
        }
        let mut changed = false;
        while let Ok(outcome) = self.outcome_rx.try_recv() {
            changed = true;
            match outcome {
                BonsaiOutcome::Acted {
                    tree,
                    decay_protection,
                    message,
                    selected_branch_id,
                } => {
                    self.install(tree, decay_protection);
                    self.tree.select_or_keep(selected_branch_id);
                    self.tree.message = message;
                }
                BonsaiOutcome::Reloaded {
                    tree,
                    decay_protection,
                } => self.install(tree, decay_protection),
                BonsaiOutcome::ActionFailed => {
                    self.tree.message = Some("Bonsai is unreachable; try again".to_string());
                }
            }
        }
        changed
    }

    /// Whether any drained change notice names this session's user. A
    /// lagged receiver may have dropped one that did, so it counts as yes.
    fn own_tree_changed(&mut self) -> bool {
        let mut own = false;
        loop {
            match self.changes_rx.try_recv() {
                Ok(user_id) => own |= user_id == self.user_id,
                Err(broadcast::error::TryRecvError::Lagged(_)) => own = true,
                Err(broadcast::error::TryRecvError::Empty)
                | Err(broadcast::error::TryRecvError::Closed) => return own,
            }
        }
    }

    /// Replace the mirror with a stored tree, unless the mirror already
    /// shows a newer one: a reload and an action answer race each other
    /// and can arrive out of order. The cursor and the message stay.
    fn install(&mut self, tree: Tree, decay_protection: Option<BonsaiDecayProtection>) {
        if tree.state_revision < self.tree.revision {
            return;
        }
        let selected_branch_id = self.tree.selected_branch_id;
        let message = self.tree.message.take();
        self.tree = BonsaiState::view_only(tree, decay_protection);
        self.tree.select_or_keep(selected_branch_id);
        self.tree.message = message;
    }
}

#[cfg(test)]
#[path = "session_test.rs"]
mod session_test;
