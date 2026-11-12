//! A hung piece's card in #lounge, on the session's side: which chat
//! messages are cards (the service's set, mirrored here), and the piece
//! Enter on one opens over the chat. `ChatState` holds this the way it
//! holds `news`: it asks which message is a card and leaves the rest here.

use std::collections::HashMap;

use late_core::models::artboard_piece::ApplauseOutcome;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::app::artboard::gallery::state::{
    applause_notice, applause_refusal, current_period_month, take_applause,
};
use crate::app::artboard::gallery::svc::{CardResult, GalleryPiece, GalleryService, LoungeCards};

pub(crate) struct LoungeCardState {
    service: GalleryService,
    viewer_id: Uuid,
    cards_rx: watch::Receiver<LoungeCards>,
    cards: LoungeCards,
    modal: Option<PieceModal>,
}

/// One piece over the chat. It opens on the card's own copy of the piece
/// and asks once how this viewer sees it, which is where their own
/// applause comes from.
pub(crate) struct PieceModal {
    piece: GalleryPiece,
    mine: bool,
    notice: Option<String>,
    /// An applause is on its way. One at a time: `v` is held until it
    /// lands, so answers apply in the order they were asked for.
    applauding: bool,
    /// `v` was pressed here at least once. The first read was asked for
    /// before that, so from then on it is older than what the applause
    /// answers say and is dropped when it lands.
    applauded_here: bool,
    /// This modal's own channel: an answer meant for a piece closed since
    /// has nowhere to land.
    results_tx: mpsc::UnboundedSender<CardResult>,
    results_rx: mpsc::UnboundedReceiver<CardResult>,
}

impl PieceModal {
    pub(crate) fn piece(&self) -> &GalleryPiece {
        &self.piece
    }

    /// Whether the viewer hung this piece: nobody applauds their own.
    pub(crate) fn mine(&self) -> bool {
        self.mine
    }

    pub(crate) fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }
}

/// What a tick changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CardTick {
    /// The set of cards, or what one of them shows: chat's rows are stale.
    pub(crate) cards_changed: bool,
    /// The open piece changed on screen.
    pub(crate) modal_changed: bool,
    /// The open piece turned out to be taken down, and the modal closed.
    pub(crate) taken_down: bool,
}

impl LoungeCardState {
    pub(crate) fn new(service: GalleryService, viewer_id: Uuid) -> Self {
        let cards_rx = service.subscribe_lounge_cards();
        let cards = cards_rx.borrow().clone();
        Self {
            service,
            viewer_id,
            cards_rx,
            cards,
            modal: None,
        }
    }

    /// The cards by chat message id: those messages draw as the piece.
    pub(crate) fn cards(&self) -> &HashMap<Uuid, GalleryPiece> {
        &self.cards
    }

    /// The piece `message_id` is the card of, if it is one.
    pub(crate) fn card(&self, message_id: Uuid) -> Option<&GalleryPiece> {
        self.cards.get(&message_id)
    }

    pub(crate) fn modal(&self) -> Option<&PieceModal> {
        self.modal.as_ref()
    }

    /// Open the piece behind the card `message_id` over the chat. False
    /// when the message is no card.
    pub(crate) fn open(&mut self, message_id: Uuid) -> bool {
        let Some(piece) = self.cards.get(&message_id).cloned() else {
            return false;
        };
        let (results_tx, results_rx) = mpsc::unbounded_channel();
        self.service
            .view_card_task(piece.id, self.viewer_id, results_tx.clone());
        self.modal = Some(PieceModal {
            mine: piece.user_id == self.viewer_id,
            piece,
            notice: None,
            applauding: false,
            applauded_here: false,
            results_tx,
            results_rx,
        });
        true
    }

    /// Close the modal, handing back the piece it showed.
    pub(crate) fn close(&mut self) -> Option<GalleryPiece> {
        self.modal.take().map(|modal| modal.piece)
    }

    /// Applaud the open piece, or take the applause back. What the row
    /// would refuse is refused here first, as on the gallery page.
    pub(crate) fn applaud(&mut self) {
        let Some(modal) = self.modal.as_mut() else {
            return;
        };
        if let Some(refusal) =
            applause_refusal(&modal.piece, self.viewer_id, current_period_month())
        {
            modal.notice = Some(refusal.to_string());
            return;
        }
        if modal.applauding {
            return;
        }
        modal.applauding = true;
        modal.applauded_here = true;
        self.service
            .applaud_card_task(modal.piece.id, self.viewer_id, modal.results_tx.clone());
    }

    /// Take in a new set of cards and the open piece's answers.
    pub(crate) fn tick(&mut self) -> CardTick {
        let mut tick = CardTick::default();
        if self.cards_rx.has_changed().unwrap_or(false) {
            self.cards = self.cards_rx.borrow_and_update().clone();
            tick.cards_changed = true;
            // Applause from anyone else reaches the open piece too.
            if let Some(modal) = self.modal.as_mut()
                && let Some(card) = self.cards.values().find(|card| card.id == modal.piece.id)
                && modal.piece.applause != card.applause
            {
                modal.piece.applause = card.applause;
                tick.modal_changed = true;
            }
        }
        let Some(modal) = self.modal.as_mut() else {
            return tick;
        };
        while let Ok(result) = modal.results_rx.try_recv() {
            tick.modal_changed = true;
            match result {
                CardResult::Viewed(Some(piece)) => match modal.applauded_here {
                    true => {}
                    false => modal.piece = *piece,
                },
                CardResult::Viewed(None) => tick.taken_down = true,
                CardResult::ViewFailed => {}
                CardResult::Applause(outcome) => {
                    modal.applauding = false;
                    take_applause(&mut modal.piece, outcome);
                    match outcome {
                        ApplauseOutcome::NotFound => tick.taken_down = true,
                        ApplauseOutcome::Applauded(_)
                        | ApplauseOutcome::Withdrawn(_)
                        | ApplauseOutcome::OwnPiece
                        | ApplauseOutcome::Closed => {
                            modal.notice = Some(applause_notice(outcome));
                        }
                    }
                }
                CardResult::ApplauseFailed(error) => {
                    modal.applauding = false;
                    modal.notice = Some(error.to_string());
                }
            }
        }
        if tick.taken_down {
            self.modal = None;
        }
        tick
    }
}
