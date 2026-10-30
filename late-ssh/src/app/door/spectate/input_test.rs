use super::is_f4;

#[test]
fn f4_matches_both_encodings() {
    assert!(is_f4(b"\x1bOS"));
    assert!(is_f4(b"\x1b[14~"));
    assert!(!is_f4(b"\x1bOR"), "F3 stays Zork's suggestion key");
    assert!(!is_f4(b"\x1bOP"), "F1 stays the roguelikes' help key");
    assert!(!is_f4(b"S"));
}
