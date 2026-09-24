//! The one Postgres `LISTEN` connection this process holds.
//!
//! Cross-replica fan-out rides `pg_notify` (root CONTEXT.md, multi-replica
//! rule). Every domain that reacts to a notify subscribes here for its own
//! channels and gets its own queue; one task per process LISTENs on all of
//! them and routes each notification by channel. The router never awaits
//! domain code, so a slow domain cannot hold up another one's notifications.
//!
//! On every (re)connect, once the LISTEN is live, each subscriber gets a
//! [`Signal::Resync`] before any notification that follows: a domain
//! re-reads there, so a write committed while the connection was down, or
//! between connect and LISTEN, is caught by the read instead of lost.
//!
//! Queues are unbounded on purpose. A notify is a channel name plus a short
//! payload, the router must never drop one, and a domain that falls behind
//! only delays itself.

use std::time::Duration;

use anyhow::Result;
use late_core::db::DbConfig;
use late_core::models::{
    app_flag::APP_FLAG_CHANGED_CHANNEL,
    article::ARTICLES_CHANGED_CHANNEL,
    bonsai::BONSAI_CHANGED_CHANNEL,
    chat_message_gild::CHAT_MESSAGE_GILDED_CHANNEL,
    chips::CHIP_USER_CHANGED_CHANNEL,
    crown::CROWN_CHANGED_CHANNEL,
    deadchannel_name_hit::DEADCHANNEL_NAME_HIT_CHANNEL,
    deadchannel_runner::DEADCHANNEL_RUNNER_CHANGED_CHANNEL,
    marketplace::{SHOP_CATALOG_CHANGED_CHANNEL, SHOP_USER_CHANGED_CHANNEL},
    pot::POT_CHANGED_CHANNEL,
    quest::{QUEST_ASSIGNMENTS_CHANGED_CHANNEL, QUEST_USER_CHANGED_CHANNEL},
};
use tokio::sync::mpsc;

use crate::metrics::{self, RefreshOutcome};

const RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// How long a domain waits before retrying a failed re-read.
const RETRY_DELAY: Duration = Duration::from_secs(5);

/// Every domain re-read that rides a notify. Closed: it is the metric
/// label of `late_ssh_pg_refresh_seconds`, so a new worker names itself
/// here before it can retry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refresh {
    AppFlags,
    RunnerLooks,
    CrownHolder,
    Pot,
    Articles,
    ActiveQuestBoards,
    ShopFlairDirectory,
}

impl Refresh {
    /// The log line's name for the read.
    fn what(self) -> &'static str {
        match self {
            Refresh::AppFlags => "app flags",
            Refresh::RunnerLooks => "runner looks",
            Refresh::CrownHolder => "crown holder",
            Refresh::Pot => "pot",
            Refresh::Articles => "articles",
            Refresh::ActiveQuestBoards => "active quest boards",
            Refresh::ShopFlairDirectory => "shop flair directory",
        }
    }
}

/// Run a domain's re-read until it succeeds. The read after a resync is
/// the only thing that seeds a domain's shared state after a (re)connect,
/// and nothing else fires until the next write, so a failure here retries
/// every [`RETRY_DELAY`] instead of leaving the replica stale (or, at
/// startup, empty). Signals that arrive meanwhile stay queued; the caller
/// drains or applies them once this returns. Every attempt lands on the
/// dashboard by domain and outcome: a replica stuck retrying is a stale
/// replica, and that must be visible without reading its log.
pub async fn read_until_ok<F, Fut>(what: Refresh, mut read: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<()>>,
{
    loop {
        let started = std::time::Instant::now();
        let result = read().await;
        let took = started.elapsed().as_secs_f64();
        match result {
            Ok(()) => {
                metrics::record_pg_refresh(what, RefreshOutcome::Ok, took);
                return;
            }
            Err(error) => {
                metrics::record_pg_refresh(what, RefreshOutcome::Failed, took);
                tracing::warn!(error = ?error, what = what.what(), "re-read failed, retrying");
                tokio::time::sleep(RETRY_DELAY).await;
            }
        }
    }
}

/// Every notify channel the process listens on. Closed: a new channel
/// breaks the build here until it has a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    AppFlagChanged,
    ArticlesChanged,
    BonsaiChanged,
    ChatMessageGilded,
    ChipUserChanged,
    CrownChanged,
    DeadchannelNameHit,
    DeadchannelRunnerChanged,
    PotChanged,
    QuestAssignmentsChanged,
    QuestUserChanged,
    ShopCatalogChanged,
    ShopUserChanged,
}

impl Channel {
    pub const ALL: [Channel; 13] = [
        Channel::AppFlagChanged,
        Channel::ArticlesChanged,
        Channel::BonsaiChanged,
        Channel::ChatMessageGilded,
        Channel::ChipUserChanged,
        Channel::CrownChanged,
        Channel::DeadchannelNameHit,
        Channel::DeadchannelRunnerChanged,
        Channel::PotChanged,
        Channel::QuestAssignmentsChanged,
        Channel::QuestUserChanged,
        Channel::ShopCatalogChanged,
        Channel::ShopUserChanged,
    ];

    /// The Postgres channel name, owned by the model that sends it.
    pub fn name(self) -> &'static str {
        match self {
            Channel::AppFlagChanged => APP_FLAG_CHANGED_CHANNEL,
            Channel::ArticlesChanged => ARTICLES_CHANGED_CHANNEL,
            Channel::BonsaiChanged => BONSAI_CHANGED_CHANNEL,
            Channel::ChatMessageGilded => CHAT_MESSAGE_GILDED_CHANNEL,
            Channel::ChipUserChanged => CHIP_USER_CHANGED_CHANNEL,
            Channel::CrownChanged => CROWN_CHANGED_CHANNEL,
            Channel::DeadchannelNameHit => DEADCHANNEL_NAME_HIT_CHANNEL,
            Channel::DeadchannelRunnerChanged => DEADCHANNEL_RUNNER_CHANGED_CHANNEL,
            Channel::PotChanged => POT_CHANGED_CHANNEL,
            Channel::QuestAssignmentsChanged => QUEST_ASSIGNMENTS_CHANGED_CHANNEL,
            Channel::QuestUserChanged => QUEST_USER_CHANGED_CHANNEL,
            Channel::ShopCatalogChanged => SHOP_CATALOG_CHANGED_CHANNEL,
            Channel::ShopUserChanged => SHOP_USER_CHANGED_CHANNEL,
        }
    }

    pub fn parse(name: &str) -> Option<Channel> {
        Channel::ALL
            .into_iter()
            .find(|channel| channel.name() == name)
    }
}

/// What a subscriber's queue carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signal {
    /// The LISTEN is live (again). Re-read whatever this domain mirrors.
    Resync,
    /// One notification on a channel this subscriber asked for.
    Notify { channel: Channel, payload: String },
}

struct Subscriber {
    channels: Vec<Channel>,
    tx: mpsc::UnboundedSender<Signal>,
}

/// Collects subscriptions at startup, then runs as one task per process.
pub struct PgListener {
    subscribers: Vec<Subscriber>,
}

impl PgListener {
    pub fn new() -> Self {
        Self {
            subscribers: Vec::new(),
        }
    }

    /// One queue for one domain, carrying only the channels it names. A
    /// domain names them in a `CHANNELS` const next to its worker, so the
    /// subscription and the worker's match cannot drift apart.
    pub fn subscribe(&mut self, channels: &[Channel]) -> mpsc::UnboundedReceiver<Signal> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.subscribers.push(Subscriber {
            channels: channels.to_vec(),
            tx,
        });
        rx
    }

    /// Hold the connection for the life of the process: a dropped connection
    /// reconnects after five seconds and every subscriber resyncs.
    pub fn start(self, db_config: DbConfig) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if let Err(error) = self.listen_once(&db_config).await {
                    tracing::warn!(error = ?error, "postgres listener stopped");
                }
                tokio::time::sleep(RECONNECT_DELAY).await;
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
        let statement = self.listen_statement();
        let listen = client.batch_execute(&statement);
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
                    self.route(message?);
                }
            }
        }

        for subscriber in &self.subscribers {
            // A closed queue is a domain whose worker is gone; nothing
            // listens for its resync, and its notifies drop the same way.
            let _ = subscriber.tx.send(Signal::Resync);
        }

        loop {
            let Some(message) = std::future::poll_fn(|cx| connection.poll_message(cx)).await else {
                return Ok(());
            };
            self.route(message?);
        }
    }

    /// `LISTEN` for every channel some subscriber asked for, once each.
    fn listen_statement(&self) -> String {
        Channel::ALL
            .into_iter()
            .filter(|channel| {
                self.subscribers
                    .iter()
                    .any(|subscriber| subscriber.channels.contains(channel))
            })
            .map(|channel| format!("LISTEN {};", channel.name()))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn route(&self, message: tokio_postgres::AsyncMessage) {
        let tokio_postgres::AsyncMessage::Notification(notification) = message else {
            return;
        };
        let Some(channel) = Channel::parse(notification.channel()) else {
            // Only channels built from `Channel::name` are LISTENed on.
            tracing::error!(
                channel = notification.channel(),
                "notification on a channel this process never listened on"
            );
            return;
        };
        for subscriber in &self.subscribers {
            if subscriber.channels.contains(&channel) {
                let _ = subscriber.tx.send(Signal::Notify {
                    channel,
                    payload: notification.payload().to_string(),
                });
            }
        }
    }
}

impl Default for PgListener {
    fn default() -> Self {
        Self::new()
    }
}
