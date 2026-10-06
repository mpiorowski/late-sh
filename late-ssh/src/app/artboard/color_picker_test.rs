use super::*;

#[test]
fn opens_on_the_paint_color_with_the_preset_cursor_on_it() {
    let picker = ColorPicker::open(PAINT_PALETTE[5]);
    assert_eq!(
        picker,
        ColorPicker {
            color: PAINT_PALETTE[5],
            row: PickerRow::Channel(Channel::Red),
            hex: String::new(),
            preset: 5,
        }
    );
    assert_eq!(picker.hex_text(), "48DCAA");
}

#[test]
fn arrows_walk_the_rows_and_nudge_the_focused_channel() {
    let mut picker = ColorPicker::open(RgbColor::new(10, 250, 128));
    picker.adjust(-COARSE_STEP);
    picker.move_row(1);
    picker.adjust(COARSE_STEP);
    picker.move_row(1);
    picker.jump(Edge::Max);
    picker.move_row(-5);
    picker.adjust(1);
    assert_eq!(
        picker,
        ColorPicker {
            color: RgbColor::new(1, 255, 255),
            row: PickerRow::Channel(Channel::Red),
            hex: String::new(),
            preset: 0,
        }
    );
}

#[test]
fn six_hex_digits_become_the_working_color() {
    let mut picker = ColorPicker::open(PAINT_PALETTE[0]);
    for ch in "ff6e4".chars() {
        assert!(picker.type_hex(ch));
    }
    assert_eq!(picker.row, PickerRow::Hex);
    assert_eq!(picker.hex_text(), "FF6E4");
    assert_eq!(picker.color, PAINT_PALETTE[0]);
    assert!(!picker.type_hex('g'));

    assert!(picker.type_hex('1'));
    assert_eq!(picker.color, RgbColor::new(255, 110, 65));

    // Backspace opens the mirrored digits so the last one can be redone.
    picker.hex_backspace();
    picker.hex_backspace();
    assert_eq!(picker.hex_text(), "FF6E");
    picker.type_hex('0');
    picker.type_hex('0');
    assert_eq!(picker.color, RgbColor::new(255, 110, 0));
    // A seventh digit starts a fresh field.
    picker.type_hex('a');
    assert_eq!(picker.hex_text(), "A");
    assert_eq!(picker.color, RgbColor::new(255, 110, 0));
}

#[test]
fn presets_wrap_and_track_the_working_color() {
    let mut picker = ColorPicker::open(PAINT_PALETTE[0]);
    picker.select_preset(0);
    picker.adjust(-1);
    assert_eq!(picker.preset, PAINT_PALETTE.len() - 1);
    assert_eq!(picker.color, PAINT_PALETTE[PAINT_PALETTE.len() - 1]);

    picker.move_row(-1);
    assert_eq!(picker.row, PickerRow::Hex);
    for ch in hex_of(PAINT_PALETTE[3]).chars() {
        picker.type_hex(ch);
    }
    assert_eq!(picker.preset, 3);
}
