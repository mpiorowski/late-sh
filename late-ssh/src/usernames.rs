use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Result;
use late_core::{MutexRecover, db::Db, models::user::User, shutdown::CancellationToken};
use tokio::task::JoinHandle;
use uuid::Uuid;

pub const USERNAME_DIRECTORY_REFRESH_INTERVAL: Duration = Duration::from_secs(30 * 60);

pub type UsernameDirectory = Arc<Mutex<HashMap<Uuid, String>>>;

pub async fn load(db: &Db) -> Result<UsernameDirectory> {
    let client = db.get().await?;
    let usernames = User::list_all_username_map(&client).await?;
    Ok(Arc::new(Mutex::new(usernames)))
}

pub fn snapshot(directory: &UsernameDirectory) -> HashMap<Uuid, String> {
    directory.lock_recover().clone()
}

pub fn get(directory: &UsernameDirectory, user_id: Uuid) -> Option<String> {
    directory.lock_recover().get(&user_id).cloned()
}

pub fn upsert(directory: &UsernameDirectory, user_id: Uuid, username: impl Into<String>) {
    let username = username.into();
    if username.trim().is_empty() {
        directory.lock_recover().remove(&user_id);
    } else {
        directory.lock_recover().insert(user_id, username);
    }
}

pub fn remove(directory: &UsernameDirectory, user_id: Uuid) {
    directory.lock_recover().remove(&user_id);
}

pub fn start_refresh_task(
    db: Db,
    directory: UsernameDirectory,
    shutdown: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(USERNAME_DIRECTORY_REFRESH_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await;

        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = interval.tick() => {
                    match refresh(&db, &directory).await {
                        Ok(count) => tracing::debug!(count, "username directory refreshed"),
                        Err(error) => {
                            tracing::warn!(error = ?error, "failed to refresh username directory");
                        }
                    }
                }
            }
        }
    })
}

async fn refresh(db: &Db, directory: &UsernameDirectory) -> Result<usize> {
    let client = db.get().await?;
    let usernames = User::list_all_username_map(&client).await?;
    let count = usernames.len();
    *directory.lock_recover() = usernames;
    Ok(count)
}
