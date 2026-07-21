use super::skip_threshold;

#[test]
fn skip_threshold_floors_at_two_and_uses_thirty_percent_ceil() {
    // Small rooms collapse to the floor: at least two YouTube-pref users
    // must agree before a skip fires.
    assert_eq!(skip_threshold(0), 2);
    assert_eq!(skip_threshold(1), 2);
    assert_eq!(skip_threshold(5), 2);
    assert_eq!(skip_threshold(6), 2);
    // 30% ceil kicks in above 6 paired clients.
    assert_eq!(skip_threshold(7), 3);
    assert_eq!(skip_threshold(10), 3);
    assert_eq!(skip_threshold(11), 4);
    assert_eq!(skip_threshold(20), 6);
    assert_eq!(skip_threshold(21), 7);
    assert_eq!(skip_threshold(100), 30);
    assert_eq!(skip_threshold(101), 31);
}
