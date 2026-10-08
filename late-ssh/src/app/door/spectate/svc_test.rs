use super::*;

fn key(playname: &str) -> LiveGameKey {
    LiveGameKey::new(SpectateGame::Dcss, playname).expect("a handle")
}

/// The count is of people: one user in two sessions is one watcher, and
/// stays one until the last of their sessions lets go.
#[test]
fn a_user_counts_once_however_many_sessions_hold_the_watch() {
    let watches = OpenWatches::new();
    let alice = Uuid::now_v7();

    let first = watches.open(key("mat"), alice);
    let second = watches.open(key("mat"), alice);
    assert_eq!(watches.watchers_of(key("mat")), 1);

    drop(first);
    assert_eq!(watches.watchers_of(key("mat")), 1);

    drop(second);
    assert_eq!(watches.watchers_of(key("mat")), 0);
}

/// Watches are per game, and a watcher does not count themselves among the
/// others on the game they are watching.
#[test]
fn watchers_are_counted_per_game_and_the_others_leave_you_out() {
    let watches = OpenWatches::new();
    let alice = Uuid::now_v7();
    let bob = Uuid::now_v7();

    let _alice_on_mat = watches.open(key("mat"), alice);
    let _bob_on_mat = watches.open(key("mat"), bob);
    let _bob_on_eggy = watches.open(key("eggy"), bob);

    assert_eq!(watches.watchers_of(key("mat")), 2);
    assert_eq!(watches.watchers_of(key("eggy")), 1);
    assert_eq!(watches.watchers_of(key("nobody")), 0);
    assert_eq!(watches.others_watching(key("mat"), alice), 1);
    assert_eq!(watches.others_watching(key("mat"), bob), 1);
    assert_eq!(watches.others_watching(key("eggy"), alice), 1);
    assert_eq!(watches.others_watching(key("eggy"), bob), 0);
}
