use tokio::sync::mpsc;
use uuid::Uuid;
use crate::ircd::registry::*;

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
