use crate::app::door::dopewars::proxy::*;

#[test]
fn playname_keeps_valid_late_username_characters() {
    assert_eq!(dopewars_playname("bob"), "bob");
    assert_eq!(dopewars_playname("bob.smith-42_x"), "bob.smith-42_x");
}

#[test]
fn playname_caps_at_32_characters() {
    assert_eq!(dopewars_playname(&"x".repeat(100)), "x".repeat(32));
}

#[test]
fn playname_strips_unsafe_characters_and_falls_back() {
    assert_eq!(dopewars_playname("bob smith!"), "bobsmith");
    assert_eq!(dopewars_playname(""), "drifter");
    assert_eq!(dopewars_playname("$%^"), "drifter");
}

#[test]
fn playname_avoids_the_reserved_stats_username() {
    assert_eq!(dopewars_playname("late_stats"), "late_stats_");
    assert_eq!(dopewars_playname("LATE_STATS"), "late_stats_");
}

#[test]
fn a_playname_reads_back_as_the_username_it_came_from() {
    for username in ["bob", "late_dealer", "late_stats"] {
        assert_eq!(username_of_playname(&dopewars_playname(username)), username);
    }
}
