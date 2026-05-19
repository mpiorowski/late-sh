//! Sync UI state over the friends event channel. Holds the current user's
//! friends list and pending request queues; drained on tick. Translates
//! [`FriendsEvent`] action results into banners.

use late_core::models::friendship::{FriendSummary, PendingRequest};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::svc::{FriendsEvent, FriendsService};
use crate::app::common::primitives::Banner;

pub struct FriendsState {
    service: FriendsService,
    user_id: Uuid,
    event_rx: broadcast::Receiver<FriendsEvent>,
    pub friends: Vec<FriendSummary>,
    pub incoming: Vec<PendingRequest>,
    pub outgoing: Vec<PendingRequest>,
}

impl FriendsState {
    pub fn new(service: FriendsService, user_id: Uuid) -> Self {
        let event_rx = service.subscribe_events();
        service.refresh_task(user_id);
        Self {
            service,
            user_id,
            event_rx,
            friends: Vec::new(),
            incoming: Vec::new(),
            outgoing: Vec::new(),
        }
    }

    pub fn service(&self) -> &FriendsService {
        &self.service
    }

    pub fn is_friend(&self, other: Uuid) -> bool {
        self.friends.iter().any(|f| f.user_id == other)
    }

    /// Local view of the relationship with `other`. Computed from the cached
    /// lists, so it's instant and stale-but-correct on next tick — good enough
    /// for the profile-modal status badge and the key-handler guards.
    pub fn local_status(&self, other: Uuid) -> late_core::models::friendship::FriendshipStatus {
        use late_core::models::friendship::FriendshipStatus;
        if other == self.user_id {
            return FriendshipStatus::None;
        }
        if self.friends.iter().any(|f| f.user_id == other) {
            return FriendshipStatus::Friends;
        }
        if self.incoming.iter().any(|r| r.other_user_id == other) {
            return FriendshipStatus::IncomingPending;
        }
        if self.outgoing.iter().any(|r| r.other_user_id == other) {
            return FriendshipStatus::OutgoingPending;
        }
        FriendshipStatus::None
    }

    pub fn incoming_count(&self) -> usize {
        self.incoming.len()
    }

    pub fn tick(&mut self) -> Option<Banner> {
        let mut banner: Option<Banner> = None;
        loop {
            match self.event_rx.try_recv() {
                Ok(event) => {
                    if let Some(b) = self.apply(event) {
                        banner = Some(b);
                    }
                }
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    tracing::warn!(lagged = n, "friends event channel lagged");
                    self.service.refresh_task(self.user_id);
                }
                Err(broadcast::error::TryRecvError::Closed) => break,
            }
        }
        banner
    }

    fn apply(&mut self, event: FriendsEvent) -> Option<Banner> {
        use super::svc::FriendsEvent::*;
        use late_core::models::friendship::SendOutcome;
        match event {
            Snapshot {
                user_id,
                friends,
                incoming,
                outgoing,
            } if user_id == self.user_id => {
                self.friends = friends;
                self.incoming = incoming;
                self.outgoing = outgoing;
                None
            }
            SendResult {
                requester,
                other_username,
                outcome,
            } if requester == self.user_id => match outcome {
                SendOutcome::Sent => Some(Banner::success(&format!(
                    "Friend request sent to {other_username}."
                ))),
                SendOutcome::AutoAccepted => Some(Banner::success(&format!(
                    "You and {other_username} are now friends."
                ))),
                SendOutcome::AlreadyExists => Some(Banner::success(&format!(
                    "Already connected with {other_username}."
                ))),
                SendOutcome::SelfRequest => None,
            },
            Accepted {
                acceptor,
                other_username,
            } if acceptor == self.user_id => Some(Banner::success(&format!(
                "You and {other_username} are now friends."
            ))),
            DeclinedOrCancelled {
                actor,
                other_username,
            } if actor == self.user_id => Some(Banner::success(&format!(
                "Request with {other_username} cleared."
            ))),
            Unfriended {
                actor,
                other_username,
            } if actor == self.user_id => Some(Banner::success(&format!(
                "Removed {other_username} from your friends."
            ))),
            Error { user_id, message } if user_id == self.user_id => Some(Banner::error(&message)),
            _ => None,
        }
    }
}
