use super::*;

#[test]
fn enter_takes_esc_keeps_for_later_and_every_other_key_holds() {
    assert_eq!(answer_for(&ParsedInput::Byte(b'\r')), Answer::Take);
    assert_eq!(answer_for(&ParsedInput::Byte(b'\n')), Answer::Take);
    assert_eq!(answer_for(&ParsedInput::Byte(0x1B)), Answer::Later);
    assert_eq!(answer_for(&ParsedInput::Byte(b'q')), Answer::Hold);
    assert_eq!(answer_for(&ParsedInput::Char('?')), Answer::Hold);
    assert_eq!(answer_for(&ParsedInput::Arrow(b'B')), Answer::Hold);
}
