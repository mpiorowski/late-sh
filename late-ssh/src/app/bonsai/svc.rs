use std::time::Duration;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use late_core::db::{Db, DbConfig};
use late_core::models::{
    bonsai::{BONSAI_CHANGED_CHANNEL, Tree, listen_for_bonsai_changes},
    bonsai_decay_protection::BonsaiDecayProtection,
    chips::{ChipMove, UserChips},
};
use tokio::sync::{broadcast, mpsc};
use tracing::{Instrument, info_span};
use uuid::Uuid;

use crate::app::activity::event::ActivityEvent;
use crate::app::bonsai::state::{Applied, BonsaiCommand, BonsaiState};

pub(crate) const WATER_CHIP_BONUS: i64 = 200;

/// What a session's request came back with. Sent to the asking session
/// only; other sessions learn of a stored change from `subscribe_changes`.
#[derive(Debug, Clone)]
pub(crate) enum BonsaiOutcome {
    Acted {
        tree: Tree,
        decay_protection: Option<BonsaiDecayProtection>,
        message: Option<String>,
        selected_branch_id: Option<i32>,
    },
    Reloaded {
        tree: Tree,
        decay_protection: Option<BonsaiDecayProtection>,
    },
    ActionFailed,
}

/// How one action settled, for the metric label.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BonsaiActionResult {
    Stored,
    Refused,
    Failed,
}

/// The work half's answer for one action.
struct Acted {
    tree: Tree,
    decay_protection: Option<BonsaiDecayProtection>,
    message: Option<String>,
    selected_branch_id: Option<i32>,
    applied: Applied,
    died: bool,
    survived_days: i32,
}

/// The one writer of `bonsai_trees`. The row is the truth: every action
/// locks it, loads it, runs one `BonsaiState` rule over it, and stores the
/// result, so any number of sessions on any number of replicas act one
/// after the other on the same tree. Sessions hold a mirror to draw from,
/// fed by their own outcomes and by the `bonsai_changed` notify.
#[derive(Clone)]
pub struct BonsaiService {
    db: Db,
    activity_feed: broadcast::Sender<ActivityEvent>,
    changes_tx: broadcast::Sender<Uuid>,
}

impl BonsaiService {
    pub fn new(db: Db, activity_feed: broadcast::Sender<ActivityEvent>) -> Self {
        let (changes_tx, _) = broadcast::channel(256);
        Self {
            db,
            activity_feed,
            changes_tx,
        }
    }

    /// User ids whose stored tree changed, from any replica.
    pub(crate) fn subscribe_changes(&self) -> broadcast::Receiver<Uuid> {
        self.changes_tx.subscribe()
    }

    /// Load the user's tree at session bootstrap, planting the bare root on
    /// the first login and settling it to today (elapsed days, the repot,
    /// a badge the current ladder scores differently) under the row lock.
    pub async fn ensure_tree(&self, user_id: Uuid) -> Result<Tree> {
        let today = Self::today();
        let mut client = self.db.get().await?;
        let decay_protection = BonsaiDecayProtection::for_user(&client, user_id).await?;
        let tx = client.transaction().await?;
        let (tree, state, settled) =
            Self::lock_settled(&tx, user_id, decay_protection, today).await?;
        let stale = settled.changed || state.badge_glyph() != tree.badge_glyph;
        let tree = match stale {
            true => {
                let stored = Tree::store(&*tx, state.to_write()).await?;
                Tree::notify_changed(&*tx, user_id).await?;
                stored
            }
            false => tree,
        };
        tx.commit().await?;
        if settled.died {
            self.announce_lost(&client, user_id, state.age_days as i32)
                .await;
        }
        Ok(tree)
    }

    /// The live Bonsai Decay Shield window, for the session's mirror.
    /// Separate from `ensure_tree` so a failure here degrades to "no
    /// shield" at bootstrap instead of discarding a tree that loaded.
    pub async fn decay_protection(&self, user_id: Uuid) -> Result<Option<BonsaiDecayProtection>> {
        let client = self.db.get().await?;
        BonsaiDecayProtection::for_user(&client, user_id).await
    }

    /// Run one care action for a session and answer on `reply`.
    pub(crate) fn act_task(
        &self,
        user_id: Uuid,
        command: BonsaiCommand,
        reply: mpsc::UnboundedSender<BonsaiOutcome>,
    ) {
        let svc = self.clone();
        let span = info_span!("bonsai.act_task", user_id = %user_id, action = ?command.action);
        tokio::spawn(
            async move {
                let outcome = match svc.act(user_id, command).await {
                    Ok(acted) => {
                        let result = match acted.applied {
                            Applied::Unchanged => BonsaiActionResult::Refused,
                            Applied::Changed | Applied::Watered => BonsaiActionResult::Stored,
                        };
                        crate::metrics::record_bonsai_action(command.action, result);
                        svc.announce(&acted, user_id).await;
                        BonsaiOutcome::Acted {
                            tree: acted.tree,
                            decay_protection: acted.decay_protection,
                            message: acted.message,
                            selected_branch_id: acted.selected_branch_id,
                        }
                    }
                    Err(e) => {
                        crate::metrics::record_bonsai_action(
                            command.action,
                            BonsaiActionResult::Failed,
                        );
                        tracing::error!(error = ?e, "failed to apply bonsai action");
                        BonsaiOutcome::ActionFailed
                    }
                };
                // A closed channel is a session that left; nothing to tell.
                let _ = reply.send(outcome);
            }
            .instrument(span),
        );
    }

    /// Lock, settle, apply, store. The watering chips ride the same
    /// transaction as the `last_watered` stamp they are paid for, so the
    /// day can never be spent without the credit landing, and the locked
    /// row is the only witness of "already watered today".
    async fn act(&self, user_id: Uuid, command: BonsaiCommand) -> Result<Acted> {
        let today = Self::today();
        let mut client = self.db.get().await?;
        let decay_protection = BonsaiDecayProtection::for_user(&client, user_id).await?;
        let tx = client.transaction().await?;
        let (tree, mut state, settled) =
            Self::lock_settled(&tx, user_id, decay_protection, today).await?;
        let survived_days = state.age_days as i32;
        // A tree that died in the catch-up takes no action on top: the
        // session sees it dead first, and the next `w` replants.
        let applied = match settled.died {
            true => Applied::Unchanged,
            false => state.apply(command, today),
        };
        let mut message = state.message.clone();
        match applied {
            Applied::Watered => {
                UserChips::apply(
                    &*tx,
                    user_id,
                    ChipMove::BonsaiWatered,
                    WATER_CHIP_BONUS,
                    &today.to_string(),
                )
                .await
                .context("crediting the watering chips")?;
                message = Some(format!("Watered (+{WATER_CHIP_BONUS} chips)"));
            }
            Applied::Changed | Applied::Unchanged => {}
        }
        let stale = settled.changed
            || applied != Applied::Unchanged
            || state.badge_glyph() != tree.badge_glyph;
        let tree = match stale {
            true => {
                let stored = Tree::store(&*tx, state.to_write()).await?;
                Tree::notify_changed(&*tx, user_id).await?;
                stored
            }
            false => tree,
        };
        tx.commit().await?;
        Ok(Acted {
            tree,
            decay_protection,
            message,
            selected_branch_id: state.selected_branch_id,
            applied,
            died: settled.died,
            survived_days,
        })
    }

    /// The row under its lock, as a state settled to `today`. Plants first,
    /// so an account whose bootstrap load failed still gets its tree.
    async fn lock_settled(
        tx: &tokio_postgres::Transaction<'_>,
        user_id: Uuid,
        decay_protection: Option<BonsaiDecayProtection>,
        today: NaiveDate,
    ) -> Result<(Tree, BonsaiState, crate::app::bonsai::state::Settled)> {
        let seed = user_id.as_u128() as i64;
        let graph = crate::app::bonsai::state::seeded_graph_value(seed);
        let badge = crate::app::bonsai::state::seeded_badge_glyph(seed);
        Tree::ensure(tx, user_id, seed, today, graph, &badge)
            .await
            .context("planting bonsai tree")?;
        let tree = Tree::lock(tx, user_id).await?;
        let mut state = BonsaiState::from_tree(tree.clone(), decay_protection);
        let settled = state.settle(today);
        Ok((tree, state, settled))
    }

    /// Activity events for what an action did. Private feed lines, sent
    /// after the commit.
    async fn announce(&self, acted: &Acted, user_id: Uuid) {
        if !acted.died && acted.applied != Applied::Watered {
            return;
        }
        let client = match self.db.get().await {
            Ok(client) => client,
            Err(e) => {
                tracing::error!(error = ?e, "failed to announce a bonsai event");
                return;
            }
        };
        if acted.died {
            self.announce_lost(&client, user_id, acted.survived_days)
                .await;
        }
        if acted.applied == Applied::Watered {
            let username = late_core::models::profile::fetch_username(&client, user_id).await;
            let _ = self
                .activity_feed
                .send(ActivityEvent::bonsai_watered(user_id, username));
        }
    }

    async fn announce_lost(
        &self,
        client: &tokio_postgres::Client,
        user_id: Uuid,
        survived_days: i32,
    ) {
        let username = late_core::models::profile::fetch_username(client, user_id).await;
        let _ =
            self.activity_feed
                .send(ActivityEvent::bonsai_lost(user_id, username, survived_days));
    }

    /// Re-read the stored tree for a session whose mirror went stale.
    pub(crate) fn reload_task(&self, user_id: Uuid, reply: mpsc::UnboundedSender<BonsaiOutcome>) {
        let svc = self.clone();
        let span = info_span!("bonsai.reload_task", user_id = %user_id);
        tokio::spawn(
            async move {
                match svc.reload(user_id).await {
                    Ok(Some((tree, decay_protection))) => {
                        let _ = reply.send(BonsaiOutcome::Reloaded {
                            tree,
                            decay_protection,
                        });
                    }
                    Ok(None) => {
                        tracing::warn!("bonsai change notice for a user with no tree");
                    }
                    Err(e) => {
                        tracing::error!(error = ?e, "failed to reload bonsai tree");
                    }
                }
            }
            .instrument(span),
        );
    }

    async fn reload(&self, user_id: Uuid) -> Result<Option<(Tree, Option<BonsaiDecayProtection>)>> {
        let client = self.db.get().await?;
        let tree = Tree::find_by_user_id(&client, user_id).await?;
        let decay_protection = BonsaiDecayProtection::for_user(&client, user_id).await?;
        Ok(tree.map(|tree| (tree, decay_protection)))
    }

    /// Fan `bonsai_changed` out to this replica's sessions. One long-lived
    /// Postgres connection LISTENs; a dropped connection reconnects after
    /// five seconds. A change committed during the gap is not replayed: the
    /// mirror of an idle second session lags until its owner's next action
    /// or change, which both carry the stored tree. Same shape as
    /// `CrownService::start_listener_task`.
    pub fn start_listener_task(&self, db_config: DbConfig) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            loop {
                if let Err(error) = service.listen_once(&db_config).await {
                    tracing::warn!(error = ?error, "bonsai postgres listener stopped");
                }
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        })
    }

    async fn listen_once(&self, db_config: &DbConfig) -> Result<()> {
        let mut config = tokio_postgres::Config::new();
        config.host(&db_config.host);
        config.port(db_config.port);
        config.user(&db_config.user);
        config.password(&db_config.password);
        config.dbname(&db_config.dbname);

        let (client, mut connection) = config.connect(tokio_postgres::NoTls).await?;
        let listen = listen_for_bonsai_changes(&client);
        tokio::pin!(listen);
        loop {
            tokio::select! {
                result = &mut listen => {
                    result?;
                    break;
                }
                message = std::future::poll_fn(|cx| connection.poll_message(cx)) => {
                    let Some(message) = message else {
                        return Ok(());
                    };
                    self.handle_notification(message?);
                }
            }
        }

        loop {
            let Some(message) = std::future::poll_fn(|cx| connection.poll_message(cx)).await else {
                return Ok(());
            };
            self.handle_notification(message?);
        }
    }

    fn handle_notification(&self, message: tokio_postgres::AsyncMessage) {
        let tokio_postgres::AsyncMessage::Notification(notification) = message else {
            return;
        };
        if notification.channel() != BONSAI_CHANGED_CHANNEL {
            return;
        }
        self.publish_change(notification.payload());
    }

    pub(super) fn publish_change(&self, payload: &str) {
        match Uuid::parse_str(payload) {
            // No receiver is a replica with no sessions; nothing to tell.
            Ok(user_id) => {
                let _ = self.changes_tx.send(user_id);
            }
            Err(error) => {
                tracing::warn!(error = ?error, payload, "unreadable bonsai_changed payload");
            }
        }
    }

    pub fn today() -> NaiveDate {
        chrono::Utc::now().date_naive()
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
