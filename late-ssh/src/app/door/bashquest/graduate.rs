// Records a verified BashQuest graduation to the database. Constructed once
// at session bootstrap and cloned into each connection, the same fire-and-
// forget shape the roguelike doors' milestone sink uses (see
// `door::ingest::award`). `None` on headless/test paths where there is no
// database.
//
// The caller (`state::State::tick`) only ever passes a `GraduationRecord`
// that came from `proxy::BashquestProcess::take_graduation`, which only ever
// holds something the late-bashquest host sent after independently
// confirming a real certificate file on its own PVC -- never anything
// client-supplied. This module trusts that chain and just persists it.

use anyhow::Result;
use late_core::db::Db;
use late_core::models::bashquest_graduate::BashquestGraduate;
use uuid::Uuid;

#[derive(Clone)]
pub struct BashquestAwards {
    db: Db,
}

impl BashquestAwards {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Record a verified graduation. Fire-and-forget: spawned so a slow or
    /// failing database write can never stall the door session it happened
    /// in. Errors are logged only -- the graduation already happened in the
    /// game; this is a durability side effect of it, not a gate on it.
    pub fn spawn_record(&self, user_id: Uuid, handle: String, certificate: Vec<u8>) {
        let db = self.db.clone();
        tokio::spawn(async move {
            if let Err(e) = record(&db, user_id, &handle, &certificate).await {
                tracing::error!(error = ?e, %handle, "failed to record bashquest graduation");
            }
        });
    }
}

async fn record(db: &Db, user_id: Uuid, handle: &str, certificate: &[u8]) -> Result<()> {
    let client = db.get().await?;
    let certificate_text = String::from_utf8_lossy(certificate);
    let digest = blake3::hash(certificate).to_hex();
    BashquestGraduate::record(&client, user_id, handle, &certificate_text, digest.as_str()).await?;
    Ok(())
}
