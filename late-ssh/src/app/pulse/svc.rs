//! The pulse service: samples this replica's headcount into
//! `presence_samples` once a bucket and keeps the last day of every
//! replica's samples on a watch the sessions copy on their tick. The
//! orchestration lives here (the loop, its logs and metric); the writes are
//! the model's and the drawing maths `state.rs`.

use std::sync::Arc;
use std::time::Duration;

use late_core::MutexRecover;
use late_core::db::Db;
use late_core::models::presence_sample::{HeadcountPoint, PresenceSample};
use tokio::sync::watch;
use uuid::Uuid;

use crate::metrics;
use crate::state::{ActiveUsers, online_human_count};

/// The last day of headcount buckets, oldest first, by `Arc` so a tick copy
/// is a pointer bump.
pub type History = Arc<Vec<HeadcountPoint>>;

/// How often the loop looks at the clock. A bucket is five minutes wide,
/// so a look a minute never misses one.
const TICK: Duration = Duration::from_secs(60);

/// What one sample did, for `metrics::record_pulse_sample`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PulseSampleOutcome {
    Recorded,
    WriteFailed,
    ReadFailed,
}

#[derive(Clone)]
pub struct PulseService {
    history_rx: watch::Receiver<History>,
}

impl PulseService {
    /// Start this replica's sampler. The replica id is fresh per process:
    /// a restarted pod writes new rows, and the old pod's last bucket
    /// stays as it was.
    pub fn start(db: Db, active_users: ActiveUsers) -> Self {
        let (history_tx, history_rx) = watch::channel(Arc::new(Vec::new()));
        tokio::spawn(sample_loop(db, active_users, Uuid::now_v7(), history_tx));
        Self { history_rx }
    }

    /// A pulse with no sampler behind it, for test apps and headless
    /// paths: the history stays empty.
    pub fn detached() -> Self {
        let (_, history_rx) = watch::channel(Arc::new(Vec::new()));
        Self { history_rx }
    }

    pub fn subscribe(&self) -> watch::Receiver<History> {
        self.history_rx.clone()
    }
}

enum SampleError {
    Write(anyhow::Error),
    Read(anyhow::Error),
}

async fn sample_loop(
    db: Db,
    active_users: ActiveUsers,
    replica_id: Uuid,
    history_tx: watch::Sender<History>,
) {
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_bucket = None;
    loop {
        tick.tick().await;
        let now = chrono::Utc::now();
        let bucket = PresenceSample::bucket_of(now);
        if last_bucket == Some(bucket) {
            continue;
        }
        let online = online_human_count(&active_users.lock_recover()) as i64;
        match record_and_read(&db, replica_id, bucket, now, online).await {
            Ok(points) => {
                metrics::record_pulse_sample(PulseSampleOutcome::Recorded);
                last_bucket = Some(bucket);
                // Nobody listening is fine: the service keeps a receiver.
                let _ = history_tx.send(Arc::new(points));
            }
            Err(SampleError::Write(error)) => {
                metrics::record_pulse_sample(PulseSampleOutcome::WriteFailed);
                tracing::warn!(error = ?error, online, "failed to record the headcount sample");
            }
            Err(SampleError::Read(error)) => {
                metrics::record_pulse_sample(PulseSampleOutcome::ReadFailed);
                tracing::warn!(error = ?error, "failed to read the headcount history");
            }
        }
    }
}

async fn record_and_read(
    db: &Db,
    replica_id: Uuid,
    bucket: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
    online: i64,
) -> Result<Vec<HeadcountPoint>, SampleError> {
    let client = match db.get().await {
        Ok(client) => client,
        Err(error) => return Err(SampleError::Write(error)),
    };
    if let Err(error) = PresenceSample::record(&client, replica_id, bucket, online).await {
        return Err(SampleError::Write(error));
    }
    match PresenceSample::last_day(&client, now).await {
        Ok(points) => Ok(points),
        Err(error) => Err(SampleError::Read(error)),
    }
}
