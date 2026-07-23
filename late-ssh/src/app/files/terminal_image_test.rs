use super::*;

fn sixel_placement_key(
    msg_id: Uuid,
    x: u16,
    y: u16,
    cols: u16,
    rows: u16,
) -> TerminalImagePlacementKey {
    TerminalImagePlacementKey {
        message_id: msg_id,
        x,
        y,
        cols,
        rows,
        cache_key: 0,
    }
}

// Tag the seeded "previous frame" screen so tests that should NOT fire a
// wipe can hold the screen constant; the screen-change test flips it.
const SEED_SCREEN_TAG: u16 = 0;

fn seed_sixel_state(state: &mut TerminalImageRenderState, msg_id: Uuid) {
    state.protocol = Some(TerminalImageProtocol::Sixel);
    state.placements = vec![sixel_placement_key(msg_id, 2, 3, 5, 2)];
    state.last_intent = SixelIntent {
        image_modal_msg_id: Some(msg_id),
        overlay_blocks_sixel: false,
        screen_tag: SEED_SCREEN_TAG,
    };
}

#[test]
fn pre_frame_wipe_skips_when_image_modal_unchanged() {
    let mut state = TerminalImageRenderState::default();
    let msg = Uuid::new_v4();
    seed_sixel_state(&mut state, msg);
    // Same modal, no overlay, same screen → no wipe, no churn.
    let out = state.pre_frame_sixel_wipe_bytes(Some(msg), false, SEED_SCREEN_TAG);
    assert!(out.is_empty());
    assert_eq!(state.placements.len(), 1);
}

#[test]
fn pre_frame_wipe_fires_when_image_modal_closes() {
    let mut state = TerminalImageRenderState::default();
    let msg = Uuid::new_v4();
    seed_sixel_state(&mut state, msg);
    let out = state.pre_frame_sixel_wipe_bytes(None, false, SEED_SCREEN_TAG);
    assert!(!out.is_empty(), "expected wipe bytes when modal closed");
    // Each wiped row writes a cursor sequence; 2 rows for a 5x2 rect.
    let wipe = String::from_utf8_lossy(&out);
    assert!(wipe.contains("\x1b[0m"));
    assert!(
        wipe.contains("\x1b[4;3H"),
        "row 1 cursor: 1-indexed (y=3+1, x=2+1)"
    );
    assert!(wipe.contains("\x1b[5;3H"), "row 2 cursor");
    // Placements cleared so build_commands re-emits cleanly.
    assert!(state.placements.is_empty());
}

#[test]
fn pre_frame_wipe_fires_when_image_swapped() {
    let mut state = TerminalImageRenderState::default();
    let old_msg = Uuid::new_v4();
    let new_msg = Uuid::new_v4();
    seed_sixel_state(&mut state, old_msg);
    let out = state.pre_frame_sixel_wipe_bytes(Some(new_msg), false, SEED_SCREEN_TAG);
    assert!(!out.is_empty());
    assert!(state.placements.is_empty());
}

#[test]
fn pre_frame_wipe_fires_when_overlay_opens() {
    let mut state = TerminalImageRenderState::default();
    let msg = Uuid::new_v4();
    seed_sixel_state(&mut state, msg);
    // Same modal still open, but a foreground overlay (icon picker) opened.
    let out = state.pre_frame_sixel_wipe_bytes(Some(msg), true, SEED_SCREEN_TAG);
    assert!(
        !out.is_empty(),
        "expected wipe when overlay opens on top of Sixel"
    );
}

#[test]
fn pre_frame_wipe_fires_when_screen_changes() {
    let mut state = TerminalImageRenderState::default();
    let msg = Uuid::new_v4();
    seed_sixel_state(&mut state, msg);
    // No modal/overlay change, but the screen changed out from under a
    // non-modal Sixel placement (e.g. leaving the Lateania landing banner).
    // The leftover pixels must be wiped or they leak onto the next screen.
    let out = state.pre_frame_sixel_wipe_bytes(Some(msg), false, SEED_SCREEN_TAG + 1);
    assert!(
        !out.is_empty(),
        "expected wipe when the screen changes while Sixel was visible"
    );
    assert!(state.placements.is_empty());
}

#[test]
fn pre_frame_wipe_noop_when_no_prior_sixel() {
    let mut state = TerminalImageRenderState::default();
    let out = state.pre_frame_sixel_wipe_bytes(None, false, SEED_SCREEN_TAG);
    assert!(out.is_empty());
}

#[test]
fn build_commands_suppresses_sixel_emission_under_overlay() {
    let mut state = TerminalImageRenderState::default();
    // Simulate a frame that would normally emit a Sixel placement, but
    // an overlay is blocking — expect no Sixel data to be emitted.
    let placement = TerminalImagePlacement {
        message_id: Uuid::new_v4(),
        area: Rect::new(2, 3, 5, 2),
        data: TerminalImageData::new(vec![0; 4], Some(b"\x1bPq~\x1b\\".to_vec()), 5, 2),
    };
    let mut frame = TerminalImageFrame::default();
    frame.push(placement);
    let cmds = state.build_commands(
        Some(TerminalImageProtocol::Sixel),
        &frame,
        /* suppress_sixel */ true,
    );
    let any_sixel = cmds.iter().any(|c| c.starts_with(b"\x1bP"));
    assert!(
        !any_sixel,
        "Sixel should be suppressed; got commands: {cmds:?}"
    );
}

#[test]
fn kitty_family_identities_use_kitty_protocol() {
    for value in [
        "kitty",
        "xterm-kitty",
        "ghostty",
        "xterm-ghostty",
        "rio",
        "WarpTerminal",
        "konsole",
    ] {
        assert_eq!(
            protocol_from_identity(value),
            Some(TerminalImageProtocol::Kitty)
        );
    }
}

#[test]
fn iterm_family_identities_use_iterm2_protocol() {
    for value in ["iTerm.app", "iTerm2", "mintty", "hterm", "WezTerm 20240203"] {
        assert_eq!(
            protocol_from_identity(value),
            Some(TerminalImageProtocol::Iterm2)
        );
    }
}

#[test]
fn sixel_family_identities_use_sixel_protocol() {
    for value in [
        "Windows Terminal 1.23.0",
        "foot",
        "foot-extra",
        "contour",
        "mlterm",
        "xterm-sixel",
    ] {
        assert_eq!(
            protocol_from_identity(value),
            Some(TerminalImageProtocol::Sixel)
        );
    }
}

#[test]
fn terminal_env_hints_enable_image_protocols() {
    assert_eq!(
        protocol_from_env_hint("LC_TERMINAL", "iTerm2"),
        Some(TerminalImageProtocol::Iterm2)
    );
    assert_eq!(
        protocol_from_env_hint("WEZTERM_PANE", "3"),
        Some(TerminalImageProtocol::Iterm2)
    );
    assert_eq!(
        protocol_from_env_hint("WT_SESSION", "abc"),
        Some(TerminalImageProtocol::Sixel)
    );
    assert_eq!(protocol_from_env_hint("WEZTERM_PANE", ""), None);
    assert_eq!(protocol_from_env_hint("WT_SESSION", ""), None);
}

#[test]
fn device_attribute_4_enables_sixel_protocol() {
    // xterm-style reply: CSI ? 62 ; 4 ; 22 c
    assert_eq!(
        protocol_from_device_attributes(&[62, 4, 22]),
        Some(TerminalImageProtocol::Sixel)
    );
    // VT220 without sixel: CSI ? 62 ; 22 c
    assert_eq!(protocol_from_device_attributes(&[62, 22]), None);
    assert_eq!(protocol_from_device_attributes(&[]), None);
}

#[test]
fn terminal_features_enable_iterm2_file_protocol() {
    assert_eq!(
        protocol_from_terminal_features("T1CwMUBSxF"),
        Some(TerminalImageProtocol::Iterm2)
    );
    assert_eq!(protocol_from_terminal_features("T1CwMUBSx"), None);
}

#[test]
fn tmux_term_disables_terminal_images() {
    assert!(term_disables_terminal_images("tmux-256color"));
    assert!(term_disables_terminal_images("screen-256color"));
    assert!(term_disables_terminal_images("screen.xterm-256color"));
    assert!(!term_disables_terminal_images("xterm-kitty"));
}

#[test]
fn sixel_encoder_emits_dcs_raster_palette_and_pixels() {
    let rgba = RgbaImage::from_pixel(4, 1, image::Rgba([255, 0, 0, 255]));
    let encoded = encode_sixel_with_levels(&rgba, 4, 1, 6);
    let text = String::from_utf8_lossy(&encoded);

    assert!(encoded.starts_with(b"\x1bPq"));
    assert!(encoded.ends_with(terminal_string_terminator()));
    assert!(text.contains("\"1;1;4;1"));
    assert!(text.contains("#180;2;100;0;0"));
    assert!(text.contains("#180!4@"));
}

#[test]
fn sixel_encoder_leaves_transparent_pixels_unpainted() {
    let rgba = RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 0]));
    let encoded = encode_sixel_with_levels(&rgba, 1, 1, 6);
    let text = String::from_utf8_lossy(&encoded);

    assert!(!text.contains("#180"));
    assert!(text.contains("\"1;1;1;1"));
}

#[test]
fn sixel_command_does_not_reencode_when_placement_is_smaller_than_cache() {
    let rgba = RgbaImage::from_pixel(16, 16, image::Rgba([0, 255, 0, 255]));
    let sixel = encode_sixel_image(&rgba, 16, 16).expect("sixel encodes");
    let data = TerminalImageData::new(vec![], Some(sixel), 2, 1);
    let placement = TerminalImagePlacement {
        message_id: Uuid::nil(),
        area: Rect::new(0, 0, 1, 1),
        data,
    };

    assert_eq!(
        sixel_image_commands(&placement),
        vec![cursor_to(placement.area)]
    );
}

#[test]
fn non_sixel_terminal_image_data_skips_sixel_encoding() {
    let mut png = Vec::new();
    {
        let rgba = RgbaImage::from_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        let encoder = PngEncoder::new(Cursor::new(&mut png));
        encoder
            .write_image(rgba.as_raw(), 1, 1, ExtendedColorType::Rgba8)
            .unwrap();
    }

    let data = terminal_image_from_bytes(&png, 1, 1, TerminalImageProtocol::Kitty).unwrap();
    assert!(data.sixel_bytes.is_none());
    assert!(data.supports_protocol(TerminalImageProtocol::Kitty));
    assert!(!data.supports_protocol(TerminalImageProtocol::Sixel));
}
