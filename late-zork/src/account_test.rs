use super::*;

#[test]
fn accepts_client_label() {
    assert_eq!(
        sanitize("late_112233445566778899aabbcc").as_deref(),
        Some("late_112233445566778899aabbcc")
    );
}

#[test]
fn rejects_paths_and_wrong_shapes() {
    assert_eq!(sanitize("../../late_112233445566778899aabbcc"), None);
    assert_eq!(sanitize("late_short"), None);
    assert_eq!(sanitize("late_112233445566778899aabbcg"), None);
}
