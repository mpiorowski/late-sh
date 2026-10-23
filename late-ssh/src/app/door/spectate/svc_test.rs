use super::*;

fn key(playname: &str) -> LiveGameKey {
    LiveGameKey::new(SpectateGame::Dcss, playname).expect("a handle")
}

fn listed(playname: &str) -> LiveGame {
    LiveGame {
        playname: playname.to_string(),
        started_unix: 1_790_000_000,
        watchers: 0,
        status: String::new(),
    }
}

/// A roster block becomes rows: a name outside the handle shape is no row,
/// and each handle carries the username looked up for it, or none when the
/// lookup had nothing.
#[test]
fn roster_rows_drop_non_handles_and_carry_usernames() {
    let usernames = HashMap::from([("Wormsong".to_string(), "mat".to_string())]);

    let rows = live_rows_of(
        SpectateGame::Dcss,
        vec![listed("Wormsong"), listed("no spaces"), listed("eggy")],
        &usernames,
    );

    assert_eq!(
        rows,
        vec![
            LiveRow {
                key: key("Wormsong"),
                entry: listed("Wormsong"),
                username: Some("mat".to_string()),
            },
            LiveRow {
                key: key("eggy"),
                entry: listed("eggy"),
                username: None,
            },
        ]
    );
    assert_eq!(rows[0].player(), "mat");
    assert_eq!(rows[1].player(), "eggy");
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

/// Watches are counted per game.
#[test]
fn watchers_are_counted_per_game() {
    let watches = OpenWatches::new();
    let alice = Uuid::now_v7();
    let bob = Uuid::now_v7();

    let _alice_on_mat = watches.open(key("mat"), alice);
    let _bob_on_mat = watches.open(key("mat"), bob);
    let _bob_on_eggy = watches.open(key("eggy"), bob);

    assert_eq!(watches.watchers_of(key("mat")), 2);
    assert_eq!(watches.watchers_of(key("eggy")), 1);
    assert_eq!(watches.watchers_of(key("nobody")), 0);
}
