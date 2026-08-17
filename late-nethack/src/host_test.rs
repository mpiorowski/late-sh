use super::*;

#[test]
fn rc_paths_are_distinct_per_playname() {
    assert_eq!(rc_path("/data", "alice"), "/data/rc/alice.nethackrc");
    assert_eq!(rc_path("/data/", "bob"), "/data/rc/bob.nethackrc");
}

fn scratch_data_dir(test: &str) -> String {
    let dir = std::env::temp_dir().join(format!("late-nethack-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.to_str().expect("utf-8 temp dir").to_string()
}

#[test]
fn materialize_writes_then_clears_the_rc_file() {
    let data_dir = scratch_data_dir("materialize");

    let path = materialize_rc(&data_dir, "alice", Some("OPTIONS=autopickup\n"))
        .expect("push should land a file");
    assert_eq!(
        std::fs::read_to_string(&path).expect("rc readable"),
        "OPTIONS=autopickup\n"
    );

    // An empty push deletes the file and reports no rc to use.
    assert_eq!(materialize_rc(&data_dir, "alice", Some("")), None);
    assert!(std::fs::metadata(&path).is_err());
    // Clearing again (nothing on disk) stays quiet.
    assert_eq!(materialize_rc(&data_dir, "alice", Some("")), None);
}

#[test]
fn materialize_none_keeps_whatever_is_on_disk() {
    let data_dir = scratch_data_dir("none");

    // No push, no file: launch with defaults.
    assert_eq!(materialize_rc(&data_dir, "bob", None), None);

    // No push, existing file (written by a newer client earlier): keep using it.
    let path = materialize_rc(&data_dir, "bob", Some("OPTIONS=color\n")).expect("file lands");
    assert_eq!(materialize_rc(&data_dir, "bob", None), Some(path));
}
