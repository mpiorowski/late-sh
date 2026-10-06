use super::{snapshot_label, snapshot_title};

#[test]
fn snapshot_labels_are_human_readable() {
    assert_eq!(snapshot_label("main"), "Live");
    assert_eq!(snapshot_label("curated:2026-05-25"), "2026-05-25");
    assert_eq!(snapshot_label("daily:2026-04-24"), "2026-04-24");
    assert_eq!(snapshot_label("monthly:2026-04"), "2026-04");
}

#[test]
fn snapshot_titles_include_kind() {
    assert_eq!(snapshot_title("main"), "Live / latest saved");
    assert_eq!(snapshot_title("curated:2026-05-25"), "Curated 2026-05-25");
    assert_eq!(snapshot_title("daily:2026-04-24"), "Daily 2026-04-24");
    assert_eq!(snapshot_title("monthly:2026-04"), "Monthly 2026-04");
}
