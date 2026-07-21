//! Live IRC connection registry: user → connection control handles.
//!
//! Owned by shared `State` so moderation paths (server ban/kick, token
//! revocation) can force-disconnect a user's IRC connections immediately.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use late_core::MutexRecover;
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub enum IrcControl {
    /// Close the connection: send `ERROR :<reason>` then drop the socket.
    Disconnect { reason: String },
    /// Project a late.sh username change to live IRC clients as a nick change.
    UserRenamed {
        user_id: Uuid,
        old_username: String,
        new_username: String,
    },
}

struct ConnHandle {
    conn_id: u64,
    control: mpsc::UnboundedSender<IrcControl>,
}

#[derive(Clone, Default)]
pub struct IrcRegistry {
    inner: Arc<Mutex<HashMap<Uuid, Vec<ConnHandle>>>>,
    next_conn_id: Arc<AtomicU64>,
}

impl IrcRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_conn_id(&self) -> u64 {
        self.next_conn_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Register a connection unless the user is already at `max_per_user`.
    pub fn try_register(
        &self,
        user_id: Uuid,
        conn_id: u64,
        control: mpsc::UnboundedSender<IrcControl>,
        max_per_user: usize,
    ) -> bool {
        let mut inner = self.inner.lock_recover();
        let conns = inner.entry(user_id).or_default();
        if conns.len() >= max_per_user {
            return false;
        }
        conns.push(ConnHandle { conn_id, control });
        true
    }

    pub fn unregister(&self, user_id: Uuid, conn_id: u64) {
        let mut inner = self.inner.lock_recover();
        if let Some(conns) = inner.get_mut(&user_id) {
            conns.retain(|c| c.conn_id != conn_id);
            if conns.is_empty() {
                inner.remove(&user_id);
            }
        }
    }

    /// Ask every connection of `user_id` to disconnect. Returns how many
    /// connections were signaled.
    pub fn disconnect_user(&self, user_id: Uuid, reason: &str) -> usize {
        let inner = self.inner.lock_recover();
        let Some(conns) = inner.get(&user_id) else {
            return 0;
        };
        let mut signaled = 0;
        for conn in conns {
            if conn
                .control
                .send(IrcControl::Disconnect {
                    reason: reason.to_string(),
                })
                .is_ok()
            {
                signaled += 1;
            }
        }
        signaled
    }

    /// Ask every connection to disconnect (process shutdown). Returns how
    /// many connections were signaled.
    pub fn disconnect_all(&self, reason: &str) -> usize {
        let inner = self.inner.lock_recover();
        inner
            .values()
            .flatten()
            .filter(|conn| {
                conn.control
                    .send(IrcControl::Disconnect {
                        reason: reason.to_string(),
                    })
                    .is_ok()
            })
            .count()
    }

    /// Ask every live IRC connection to project a late.sh username change. The
    /// receiving session decides whether the nick is visible in its joined rooms.
    pub fn project_username_change(
        &self,
        user_id: Uuid,
        old_username: &str,
        new_username: &str,
    ) -> usize {
        let inner = self.inner.lock_recover();
        inner
            .values()
            .flatten()
            .filter(|conn| {
                conn.control
                    .send(IrcControl::UserRenamed {
                        user_id,
                        old_username: old_username.to_string(),
                        new_username: new_username.to_string(),
                    })
                    .is_ok()
            })
            .count()
    }

    pub fn is_online(&self, user_id: Uuid) -> bool {
        self.inner.lock_recover().contains_key(&user_id)
    }

    pub fn connection_count(&self) -> usize {
        self.inner.lock_recover().values().map(Vec::len).sum()
    }

    pub fn online_user_ids(&self) -> Vec<Uuid> {
        self.inner.lock_recover().keys().copied().collect()
    }
}
