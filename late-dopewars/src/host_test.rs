use std::fs;

use super::fresh_home;

/// A host killed mid-session leaves its scratch home behind, and the next
/// process hands the same name out again. The new session's home starts
/// empty, so the old session's high-score table is not logged a second time.
#[test]
fn a_fresh_home_drops_what_an_earlier_host_left_in_it() {
    let home = std::env::temp_dir().join(format!(
        "late-dopewars-host-test-leftover-{}",
        std::process::id()
    ));
    fs::create_dir_all(&home).unwrap();
    fs::write(home.join("dopewars.sco"), b"an earlier session's table").unwrap();

    fresh_home(&home).unwrap();

    assert_eq!(fs::read_dir(&home).unwrap().count(), 0);
    fs::remove_dir_all(&home).unwrap();
}
