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

pub type UsernameDirectory = Arc<Mutex<Arc<HashMap<Uuid, String>>>>;

pub struct UsernameLookup<'a> {
    chat: &'a HashMap<Uuid, String>,
    directory: Option<&'a HashMap<Uuid, String>>,
}

pub trait UsernameResolver {
    fn username(&self, user_id: &Uuid) -> Option<&String>;
}

impl<'a> UsernameLookup<'a> {
    pub fn new(
        chat: &'a HashMap<Uuid, String>,
        directory: Option<&'a HashMap<Uuid, String>>,
    ) -> Self {
        Self { chat, directory }
    }

    pub fn get(&self, user_id: &Uuid) -> Option<&'a String> {
        match self.directory {
            Some(directory) => directory.get(user_id),
            None => self.chat.get(user_id),
        }
    }
}

impl UsernameResolver for HashMap<Uuid, String> {
    fn username(&self, user_id: &Uuid) -> Option<&String> {
        self.get(user_id)
    }
}

impl UsernameResolver for UsernameLookup<'_> {
    fn username(&self, user_id: &Uuid) -> Option<&String> {
        self.get(user_id)
    }
}

pub async fn load(db: &Db) -> Result<UsernameDirectory> {
    let client = db.get().await?;
    let usernames = User::list_all_username_map(&client).await?;
    Ok(Arc::new(Mutex::new(Arc::new(usernames))))
}

pub fn snapshot(directory: &UsernameDirectory) -> Arc<HashMap<Uuid, String>> {
    Arc::clone(&directory.lock_recover())
}

pub fn get(directory: &UsernameDirectory, user_id: Uuid) -> Option<String> {
    directory.lock_recover().get(&user_id).cloned()
}

pub fn upsert(directory: &UsernameDirectory, user_id: Uuid, username: impl Into<String>) {
    let username = username.into();
    let mut guard = directory.lock_recover();
    let usernames = Arc::make_mut(&mut *guard);
    if username.trim().is_empty() {
        usernames.remove(&user_id);
    } else {
        usernames.insert(user_id, username);
    }
}

pub fn remove(directory: &UsernameDirectory, user_id: Uuid) {
    let mut guard = directory.lock_recover();
    Arc::make_mut(&mut *guard).remove(&user_id);
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
    *directory.lock_recover() = Arc::new(usernames);
    Ok(count)
}
