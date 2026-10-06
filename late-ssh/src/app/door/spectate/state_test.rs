use super::*;

fn roster(names: &[&str]) -> Vec<LiveGame> {
    names
        .iter()
        .map(|name| LiveGame {
            playname: name.to_string(),
            started_unix: 0,
            watchers: 0,
        })
        .collect()
}

#[test]
fn step_target_wraps_both_ways() {
    let games = roster(&["alice", "bob", "cleo"]);
    assert_eq!(step_target(&games, "alice", true), Some("bob"));
    assert_eq!(step_target(&games, "cleo", true), Some("alice"));
    assert_eq!(step_target(&games, "alice", false), Some("cleo"));
    assert_eq!(step_target(&games, "bob", false), Some("alice"));
}

#[test]
fn step_target_has_nowhere_to_go_alone() {
    assert_eq!(step_target(&roster(&["alice"]), "alice", true), None);
    assert_eq!(step_target(&roster(&[]), "alice", true), None);
}

#[test]
fn step_target_from_a_game_that_ended_starts_at_an_end() {
    let games = roster(&["bob", "cleo"]);
    assert_eq!(step_target(&games, "alice", true), Some("bob"));
    assert_eq!(step_target(&games, "alice", false), Some("cleo"));
}

#[test]
fn a_watch_ends_off_the_hub_or_with_its_stream() {
    assert_eq!(
        end_reason(false, WatchStatus::Watching, "alice"),
        Some(WatchEnd::LeftHub)
    );
    assert_eq!(
        end_reason(true, WatchStatus::Ended, "alice"),
        Some(WatchEnd::GameEnded("alice".to_string()))
    );
    assert_eq!(end_reason(true, WatchStatus::Connecting, "alice"), None);
    assert_eq!(end_reason(true, WatchStatus::Watching, "alice"), None);
}
