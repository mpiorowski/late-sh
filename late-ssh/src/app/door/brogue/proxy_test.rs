use super::HvpNormalizer;

fn parse(bytes: &[u8]) -> vt100::Parser {
    let mut parser = vt100::Parser::new(10, 20, 0);
    parser.process(bytes);
    parser
}

fn cell_char(parser: &vt100::Parser, row: u16, col: u16) -> String {
    parser
        .screen()
        .cell(row, col)
        .map(|c| c.contents().to_string())
        .unwrap_or_default()
}

/// The root cause this normalizer exists for: the vt100 crate ignores the HVP
/// final byte (`f`), dropping the move entirely. If a vt100 upgrade makes this
/// test fail, HVP is now supported upstream and `HvpNormalizer` can be deleted.
#[test]
fn vt100_drops_hvp_without_normalizer() {
    let parser = parse(b"\x1b[3;5fX");
    assert_eq!(cell_char(&parser, 2, 4), "", "vt100 grew HVP support");
    assert_eq!(cell_char(&parser, 0, 0), "X", "X prints at the unmoved cursor");
}

#[test]
fn hvp_positions_like_cup() {
    let mut norm = HvpNormalizer::new();
    let bytes = norm.feed(b"\x1b[3;5fX");
    let parser = parse(&bytes);
    assert_eq!(cell_char(&parser, 2, 4), "X");
}

#[test]
fn hvp_split_across_chunks_positions_like_cup() {
    let mut norm = HvpNormalizer::new();
    let mut bytes = norm.feed(b"\x1b[3;");
    bytes.extend(norm.feed(b"5fX"));
    let parser = parse(&bytes);
    assert_eq!(cell_char(&parser, 2, 4), "X");
}

#[test]
fn esc_split_right_after_escape_byte() {
    let mut norm = HvpNormalizer::new();
    let mut bytes = norm.feed(b"a\x1b");
    bytes.extend(norm.feed(b"[2;2fY"));
    let parser = parse(&bytes);
    assert_eq!(cell_char(&parser, 1, 1), "Y");
    assert_eq!(cell_char(&parser, 0, 0), "a");
}

#[test]
fn other_sequences_pass_through_verbatim() {
    let mut norm = HvpNormalizer::new();
    let input: &[u8] = b"\x1b[38;2;10;20;30mZ\x1b[2;3H\x1b[0m plain";
    assert_eq!(norm.feed(input), input);
}

#[test]
fn overlong_numeric_csi_flushes_verbatim() {
    let mut norm = HvpNormalizer::new();
    // Longer than any real HVP: not rewritten, not swallowed.
    let input: &[u8] = b"\x1b[123456789;123456789;12f";
    assert_eq!(norm.feed(input), input);
}
