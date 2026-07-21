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
