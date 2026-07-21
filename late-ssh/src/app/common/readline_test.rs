use crate::app::common::readline::*;
use ratatui_textarea::Key;

#[test]
fn maps_ctrl_a_through_ctrl_z() {
    for (byte, expected) in [(0x01u8, 'a'), (0x05, 'e'), (0x0B, 'k'), (0x1A, 'z')] {
        let input = ctrl_byte_to_input(byte).expect("control byte should map");
        assert_eq!(input.key, Key::Char(expected));
        assert!(input.ctrl);
        assert!(!input.alt);
        assert!(!input.shift);
    }
}

#[test]
fn rejects_non_ctrl_letter_bytes() {
    for byte in [0x00u8, 0x1B, 0x1C, 0x1F, 0x7F, b' ', b'a'] {
        assert!(
            ctrl_byte_to_input(byte).is_none(),
            "byte 0x{byte:02X} should not map"
        );
    }
}
