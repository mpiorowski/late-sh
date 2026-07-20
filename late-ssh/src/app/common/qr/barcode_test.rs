use rstest::rstest;

use super::*;

#[rstest]
#[case::empty(0b00, ' ')]
#[case::top(0b01, '\u{2580}')]
#[case::bot(0b10, '\u{2584}')]
#[case::full(0b11, '\u{2588}')]
fn half_block_glyph(#[case] modules: u32, #[case] expected: char) {
    assert_eq!(HalfBlock::glyph(modules), expected);
}

#[rstest]
#[case::off(0, ' ')]
#[case::on(1, '\u{2588}')]
fn full_block_glyph(#[case] modules: u32, #[case] expected: char) {
    assert_eq!(FullBlock::glyph(modules), expected);
}

#[rstest]
#[case::blank(0b00000000, '\u{2800}')]
#[case::all(0b11111111, '\u{28FF}')]
#[case::top_left(0b00000001, '\u{2801}')]
#[case::top_right(0b00000010, '\u{2808}')]
#[case::bottom_left(0b01000000, '\u{2840}')]
#[case::bottom_right(0b10000000, '\u{2880}')]
fn braille_glyph(#[case] modules: u32, #[case] expected: char) {
    assert_eq!(Braille::glyph(modules), expected);
}
