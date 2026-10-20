use super::col;

#[test]
fn col_pads_short_text_to_width() {
    assert_eq!(col("chess", 8), "chess   ");
}

#[test]
fn col_always_leaves_a_gap_before_the_next_column() {
    // Exactly width chars would swallow the separator; the longest
    // fitting content is width - 1 chars plus one space.
    assert_eq!(col("12345678", 8), "123456… ");
    assert_eq!(col("1234567", 8), "1234567 ");
    assert_eq!(col("challenges @kirii.md", 18), "challenges @kiri… ");
}

#[test]
fn col_counts_chars_not_bytes() {
    assert_eq!(col("héllo", 8), "héllo   ");
}
