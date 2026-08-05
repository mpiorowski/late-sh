use super::*;

#[test]
fn macro_dirs_are_distinct_per_playname() {
    let a = macro_dir("/data", "alice");
    let b = macro_dir("/data", "bob");
    assert_ne!(a, b);
    assert_eq!(a, "/data/.crawl/macros/alice");
    // A trailing slash on the configured data dir must not double up.
    assert_eq!(macro_dir("/data/", "bob"), "/data/.crawl/macros/bob");
}

#[test]
fn crawl_args_reforce_macro_dir_after_the_player_rc() {
    // The macro_dir opt-last guard must come AFTER the -rc pair: crawl applies
    // -extra-opt-last lines after the whole rc, and later lines win, so this
    // ordering is what stops a pushed rc's `macro_dir =` from redirecting
    // another player's macro dir. See crawl_args' doc comment.
    let args = crawl_args(
        "alice",
        "/data/.crawl/macros/alice",
        Some("/data/rc/alice.rc"),
    );
    let rc = args.iter().position(|a| a == "-rc").expect("-rc present");
    assert_eq!(args[rc + 1], "/data/rc/alice.rc");
    let guard = args
        .iter()
        .position(|a| a == "macro_dir=/data/.crawl/macros/alice")
        .expect("macro_dir guard present");
    assert!(guard > rc, "guard must follow the rc so it wins");
    assert_eq!(args[guard - 1], "-extra-opt-last");

    // No rc pushed: the guard still stands (the shared init.txt path), and no
    // -rc pair sneaks in.
    let args = crawl_args("bob", "/data/.crawl/macros/bob", None);
    assert!(!args.contains(&"-rc".to_string()));
    assert!(args.contains(&"macro_dir=/data/.crawl/macros/bob".to_string()));
}

#[test]
fn rc_paths_are_distinct_per_playname() {
    assert_eq!(rc_path("/data", "alice"), "/data/rc/alice.rc");
    assert_eq!(rc_path("/data/", "bob"), "/data/rc/bob.rc");
}

fn scratch_data_dir(test: &str) -> String {
    let dir = std::env::temp_dir().join(format!("late-dcss-{}-{test}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.to_str().expect("utf-8 temp dir").to_string()
}

#[test]
fn materialize_writes_then_clears_the_rc_file() {
    let data_dir = scratch_data_dir("materialize");

    let path = materialize_rc(&data_dir, "alice", Some("autopickup = $?!+\n"))
        .expect("push should land a file");
    assert_eq!(
        std::fs::read_to_string(&path).expect("rc readable"),
        "autopickup = $?!+\n"
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
    let path = materialize_rc(&data_dir, "bob", Some("show_more = false\n")).expect("file lands");
    assert_eq!(materialize_rc(&data_dir, "bob", None), Some(path));
}
