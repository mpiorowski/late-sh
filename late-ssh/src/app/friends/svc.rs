//! Fire-and-forget service over `Friendship`. Each user-initiated action
//! spawns a Tokio task; the result is published on `evt_tx` and drained by
//! `FriendsState` on tick. Direct DB calls are async; the UI never awaits.

use anyhow::Result;
use late_core::db::Db;
use late_core::models::friendship::{
    FriendSummary, Friendship, FriendshipStatus, PendingRequest, SendOutcome,
};
use tokio::sync::broadcast;
use tracing::{Instrument, info_span};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub enum FriendsEvent {
    /// Latest snapshot of `user_id`'s friend graph (friends + incoming +
    /// outgoing). Emitted whenever the service refreshes after an action.
    Snapshot {
        user_id: Uuid,
        friends: Vec<FriendSummary>,
        incoming: Vec<PendingRequest>,
        outgoing: Vec<PendingRequest>,
    },
    /// Outcome of a `send_request` initiated by `requester`. `other_username`
    /// is the addressee, included so the UI can surface a banner without
    /// another lookup.
    SendResult {
        requester: Uuid,
        other_username: String,
        outcome: SendOutcome,
    },
    /// A pending incoming request was accepted by `acceptor`.
    Accepted {
        acceptor: Uuid,
        other_username: String,
    },
    /// A pending request was declined or cancelled. `actor` is the user who
    /// performed the action.
    DeclinedOrCancelled { actor: Uuid, other_username: String },
    /// A previously accepted friendship was removed by `actor`.
    Unfriended { actor: Uuid, other_username: String },
    /// Generic error to surface to the actor.
    Error { user_id: Uuid, message: String },
}

#[derive(Clone)]
pub struct FriendsService {
    db: Db,
    evt_tx: broadcast::Sender<FriendsEvent>,
}

impl FriendsService {
    pub fn new(db: Db) -> Self {
        let (evt_tx, _) = broadcast::channel(256);
        Self { db, evt_tx }
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<FriendsEvent> {
        self.evt_tx.subscribe()
    }

    fn publish(&self, event: FriendsEvent) {
        if let Err(e) = self.evt_tx.send(event) {
            tracing::error!(%e, "failed to send friends event");
        }
    }

    /// Synchronously fetch the current status between two users. Used by the
    /// profile modal to render the right relationship label as soon as a
    /// profile loads, without waiting for an async snapshot.
    pub async fn status(&self, user: Uuid, other: Uuid) -> Result<FriendshipStatus> {
        let client = self.db.get().await?;
        Friendship::status(&client, user, other).await
    }

    /// Fire-and-forget: refresh the friend graph snapshot for `user_id`.
    pub fn refresh_task(&self, user_id: Uuid) {
        let svc = self.clone();
        tokio::spawn(
            async move {
                if let Err(e) = svc.do_refresh(user_id).await {
                    tracing::error!(error = ?e, user_id = %user_id, "friends refresh failed");
                }
            }
            .instrument(info_span!("friends.refresh", user_id = %user_id)),
        );
    }

    async fn do_refresh(&self, user_id: Uuid) -> Result<()> {
        let client = self.db.get().await?;
        let friends = Friendship::list_friends(&client, user_id).await?;
        let incoming = Friendship::list_incoming(&client, user_id).await?;
        let outgoing = Friendship::list_outgoing(&client, user_id).await?;
        self.publish(FriendsEvent::Snapshot {
            user_id,
            friends,
            incoming,
            outgoing,
        });
        Ok(())
    }

    pub fn send_request_task(&self, requester: Uuid, addressee: Uuid, addressee_name: String) {
        let svc = self.clone();
        tokio::spawn(
            async move {
                match svc.do_send(requester, addressee).await {
                    Ok(outcome) => {
                        svc.publish(FriendsEvent::SendResult {
                            requester,
                            other_username: addressee_name,
                            outcome,
                        });
                        svc.refresh_task(requester);
                        if outcome == SendOutcome::AutoAccepted {
                            svc.refresh_task(addressee);
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, requester = %requester, addressee = %addressee, "send_request failed");
                        svc.publish(FriendsEvent::Error {
                            user_id: requester,
                            message: "Could not send friend request.".into(),
                        });
                    }
                }
            }
            .instrument(info_span!("friends.send", requester = %requester, addressee = %addressee)),
        );
    }

    async fn do_send(&self, requester: Uuid, addressee: Uuid) -> Result<SendOutcome> {
        let client = self.db.get().await?;
        Friendship::send_request(&client, requester, addressee).await
    }

    pub fn accept_task(&self, acceptor: Uuid, requester: Uuid, requester_name: String) {
        let svc = self.clone();
        tokio::spawn(
            async move {
                match svc.do_accept(acceptor, requester).await {
                    Ok(true) => {
                        svc.publish(FriendsEvent::Accepted {
                            acceptor,
                            other_username: requester_name,
                        });
                        svc.refresh_task(acceptor);
                        svc.refresh_task(requester);
                    }
                    Ok(false) => {
                        // Nothing pending — silent.
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, "accept failed");
                        svc.publish(FriendsEvent::Error {
                            user_id: acceptor,
                            message: "Could not accept friend request.".into(),
                        });
                    }
                }
            }
            .instrument(info_span!("friends.accept", acceptor = %acceptor)),
        );
    }

    async fn do_accept(&self, acceptor: Uuid, requester: Uuid) -> Result<bool> {
        let client = self.db.get().await?;
        Friendship::accept(&client, acceptor, requester).await
    }

    pub fn decline_or_cancel_task(&self, actor: Uuid, other: Uuid, other_name: String) {
        let svc = self.clone();
        tokio::spawn(
            async move {
                match svc.do_decline_or_cancel(actor, other).await {
                    Ok(_) => {
                        svc.publish(FriendsEvent::DeclinedOrCancelled {
                            actor,
                            other_username: other_name,
                        });
                        svc.refresh_task(actor);
                        svc.refresh_task(other);
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, "decline/cancel failed");
                        svc.publish(FriendsEvent::Error {
                            user_id: actor,
                            message: "Could not update friend request.".into(),
                        });
                    }
                }
            }
            .instrument(info_span!("friends.decline_or_cancel", actor = %actor)),
        );
    }

    async fn do_decline_or_cancel(&self, actor: Uuid, other: Uuid) -> Result<u64> {
        let client = self.db.get().await?;
        Friendship::decline_or_cancel(&client, actor, other).await
    }

    pub fn unfriend_task(&self, actor: Uuid, other: Uuid, other_name: String) {
        let svc = self.clone();
        tokio::spawn(
            async move {
                match svc.do_unfriend(actor, other).await {
                    Ok(_) => {
                        svc.publish(FriendsEvent::Unfriended {
                            actor,
                            other_username: other_name,
                        });
                        svc.refresh_task(actor);
                        svc.refresh_task(other);
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, "unfriend failed");
                        svc.publish(FriendsEvent::Error {
                            user_id: actor,
                            message: "Could not remove friend.".into(),
                        });
                    }
                }
            }
            .instrument(info_span!("friends.unfriend", actor = %actor)),
        );
    }

    async fn do_unfriend(&self, actor: Uuid, other: Uuid) -> Result<u64> {
        let client = self.db.get().await?;
        Friendship::unfriend(&client, actor, other).await
    }
}
