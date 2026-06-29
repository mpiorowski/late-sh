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

#[cfg(test)]
mod tests {
    use super::*;

    fn handle() -> (
        mpsc::UnboundedSender<IrcControl>,
        mpsc::UnboundedReceiver<IrcControl>,
    ) {
        mpsc::unbounded_channel()
    }

    #[test]
    fn per_user_cap_is_enforced() {
        let registry = IrcRegistry::new();
        let user = Uuid::new_v4();
        let (tx, _rx) = handle();
        assert!(registry.try_register(user, 1, tx.clone(), 2));
        assert!(registry.try_register(user, 2, tx.clone(), 2));
        assert!(!registry.try_register(user, 3, tx, 2));
    }

    #[test]
    fn disconnect_signals_all_user_connections() {
        let registry = IrcRegistry::new();
        let user = Uuid::new_v4();
        let (tx1, mut rx1) = handle();
        let (tx2, mut rx2) = handle();
        registry.try_register(user, 1, tx1, 3);
        registry.try_register(user, 2, tx2, 3);
        assert_eq!(registry.disconnect_user(user, "revoked"), 2);
        assert!(matches!(rx1.try_recv(), Ok(IrcControl::Disconnect { .. })));
        assert!(matches!(rx2.try_recv(), Ok(IrcControl::Disconnect { .. })));
    }

    #[test]
    fn username_change_signals_all_connections() {
        let registry = IrcRegistry::new();
        let user = Uuid::new_v4();
        let other = Uuid::new_v4();
        let (tx1, mut rx1) = handle();
        let (tx2, mut rx2) = handle();
        registry.try_register(user, 1, tx1, 3);
        registry.try_register(other, 2, tx2, 3);

        assert_eq!(
            registry.project_username_change(user, "old.name", "new.name"),
            2
        );
        assert!(matches!(
            rx1.try_recv(),
            Ok(IrcControl::UserRenamed {
                user_id,
                old_username,
                new_username,
            }) if user_id == user && old_username == "old.name" && new_username == "new.name"
        ));
        assert!(matches!(rx2.try_recv(), Ok(IrcControl::UserRenamed { .. })));
    }

    #[test]
    fn unregister_removes_user_when_last_conn_drops() {
        let registry = IrcRegistry::new();
        let user = Uuid::new_v4();
        let (tx, _rx) = handle();
        registry.try_register(user, 7, tx, 3);
        assert!(registry.is_online(user));
        registry.unregister(user, 7);
        assert!(!registry.is_online(user));
        assert_eq!(registry.connection_count(), 0);
    }
}
