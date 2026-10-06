use super::*;

#[test]
fn player_dirs_are_distinct_per_playname() {
    let a = player_dir("/data", "alice");
    let b = player_dir("/data", "bob");
    assert_ne!(a, b);
    assert_eq!(a, "/data/players/alice");
    // A trailing slash on the configured data dir must not double up.
    assert_eq!(player_dir("/data/", "bob"), "/data/players/bob");
}
