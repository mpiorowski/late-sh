use super::*;

#[test]
fn symbol_mode_from_identity_routes_requested_terminals() {
    assert_eq!(
        InlineImageSymbolMode::from_identity("xterm-kitty"),
        InlineImageSymbolMode::Octant
    );
    assert_eq!(
        InlineImageSymbolMode::from_identity("WezTerm 20240203"),
        InlineImageSymbolMode::Octant
    );
    assert_eq!(
        InlineImageSymbolMode::from_identity("ghostty"),
        InlineImageSymbolMode::Octant
    );
    assert_eq!(
        InlineImageSymbolMode::from_identity("mtermux"),
        InlineImageSymbolMode::Octant
    );
    assert_eq!(
        InlineImageSymbolMode::from_identity("iTerm2"),
        InlineImageSymbolMode::Sextant
    );
    assert_eq!(
        InlineImageSymbolMode::from_identity("alacritty"),
        InlineImageSymbolMode::Sextant
    );
    assert_eq!(
        InlineImageSymbolMode::from_identity("xterm-256color"),
        InlineImageSymbolMode::Default
    );
}

#[test]
fn symbol_modes_extend_chafa_default() {
    let mut octant = InlineImageSymbolMode::Octant.symbol_map();
    octant.prepare();
    assert!(octant.has_symbol('\u{1cd00}'));

    let mut sextant = InlineImageSymbolMode::Sextant.symbol_map();
    sextant.prepare();
    assert!(sextant.has_symbol('\u{1fb00}'));
}

#[test]
fn transparent_chafa_cell_renders_as_space() {
    let span = cell_span(&CellOut {
        c: '┈' as u32,
        fg: 0x01ff_ffff,
        bg: 0x0000_0000,
    })
    .expect("cell converts");

    assert_eq!(span.content.as_ref(), " ");
    assert_eq!(span.style.fg, None);
    assert_eq!(span.style.bg, None);
}

#[test]
fn alpha_at_chafa_threshold_edge_renders_as_transparent() {
    let span = cell_span(&CellOut {
        c: '┈' as u32,
        fg: 0x7fff_ffff,
        bg: 0x0000_0000,
    })
    .expect("cell converts");

    assert_eq!(span.content.as_ref(), " ");
    assert_eq!(span.style.fg, None);
    assert_eq!(span.style.bg, None);
}

#[test]
fn transparent_foreground_with_background_uses_reversed_video() {
    let span = cell_span(&CellOut {
        c: '┈' as u32,
        fg: 0x0000_0000,
        bg: 0xff12_3456,
    })
    .expect("cell converts");

    assert_eq!(span.content.as_ref(), "┈");
    assert_eq!(span.style.fg, Some(Color::Rgb(0x12, 0x34, 0x56)));
    assert!(span.style.add_modifier.contains(Modifier::REVERSED));
}
