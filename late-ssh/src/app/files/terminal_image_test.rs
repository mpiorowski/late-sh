use super::*;
use image::Rgba;

fn persistent_placement_key(
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
        opaque: false,
    }
}

// Tag the seeded "previous frame" screen so tests that should NOT fire a
// wipe can hold the screen constant; the screen-change test flips it.
const SEED_SCREEN_TAG: u16 = 0;

fn seed_persistent_raster_state(state: &mut TerminalImageRenderState, msg_id: Uuid) {
    state.protocol = Some(TerminalImageProtocol::Sixel);
    state.placements = vec![persistent_placement_key(msg_id, 2, 3, 5, 2)];
    state.last_intent = PersistentRasterIntent {
        image_modal_msg_id: Some(msg_id),
        overlay_blocks_raster: false,
        screen_tag: SEED_SCREEN_TAG,
        non_modal_image_tag: None,
    };
}

#[test]
fn pre_frame_wipe_skips_when_image_modal_unchanged() {
    let mut state = TerminalImageRenderState::default();
    let msg = Uuid::new_v4();
    seed_persistent_raster_state(&mut state, msg);
    // Same modal, no overlay, same screen → no wipe, no churn.
    let out = state.pre_frame_persistent_raster_wipe_bytes(
        Some(msg),
        false,
        SEED_SCREEN_TAG,
        None,
        Some(TerminalImageProtocol::Sixel),
    );
    assert!(out.is_empty());
    assert_eq!(state.placements.len(), 1);
}

#[test]
fn pre_frame_wipe_fires_when_image_modal_closes() {
    let mut state = TerminalImageRenderState::default();
    let msg = Uuid::new_v4();
    seed_persistent_raster_state(&mut state, msg);
    let out = state.pre_frame_persistent_raster_wipe_bytes(
        None,
        false,
        SEED_SCREEN_TAG,
        None,
        Some(TerminalImageProtocol::Sixel),
    );
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
    seed_persistent_raster_state(&mut state, old_msg);
    let out = state.pre_frame_persistent_raster_wipe_bytes(
        Some(new_msg),
        false,
        SEED_SCREEN_TAG,
        None,
        Some(TerminalImageProtocol::Sixel),
    );
    assert!(!out.is_empty());
    assert!(state.placements.is_empty());
}

#[test]
fn pre_frame_wipe_fires_when_overlay_opens() {
    let mut state = TerminalImageRenderState::default();
    let msg = Uuid::new_v4();
    seed_persistent_raster_state(&mut state, msg);
    // Same modal still open, but a foreground overlay (icon picker) opened.
    let out = state.pre_frame_persistent_raster_wipe_bytes(
        Some(msg),
        true,
        SEED_SCREEN_TAG,
        None,
        Some(TerminalImageProtocol::Sixel),
    );
    assert!(
        !out.is_empty(),
        "expected wipe when overlay opens on top of Sixel"
    );
}

#[test]
fn pre_frame_wipe_fires_when_screen_changes() {
    let mut state = TerminalImageRenderState::default();
    let msg = Uuid::new_v4();
    seed_persistent_raster_state(&mut state, msg);
    // No modal/overlay change, but the screen changed out from under a
    // non-modal Sixel placement (e.g. leaving the Lateania landing banner).
    // The leftover pixels must be wiped or they leak onto the next screen.
    let out = state.pre_frame_persistent_raster_wipe_bytes(
        Some(msg),
        false,
        SEED_SCREEN_TAG + 1,
        None,
        Some(TerminalImageProtocol::Sixel),
    );
    assert!(
        !out.is_empty(),
        "expected wipe when the screen changes while Sixel was visible"
    );
    assert!(state.placements.is_empty());
}

#[test]
fn pre_frame_wipe_fires_when_non_modal_image_generation_changes() {
    let mut state = TerminalImageRenderState::default();
    let msg = Uuid::new_v4();
    seed_persistent_raster_state(&mut state, msg);
    state.last_intent.non_modal_image_tag = Some(11);

    // Hold the modal id, overlay, screen and protocol at their seeded values
    // so the tag is the only term that differs. Passing `None` for the modal
    // id here would satisfy `needs_wipe` through `modal_closed_or_swapped`
    // and the assertion would hold with the tag check deleted entirely.
    let out = state.pre_frame_persistent_raster_wipe_bytes(
        Some(msg),
        false,
        SEED_SCREEN_TAG,
        Some(12),
        Some(TerminalImageProtocol::Sixel),
    );

    assert!(!out.is_empty(), "expected wipe when board raster changes");
    assert!(state.placements.is_empty());
}

#[test]
fn pre_frame_wipe_cleans_iterm_when_non_modal_image_disappears() {
    let mut state = TerminalImageRenderState::default();
    let msg = Uuid::new_v4();
    state.protocol = Some(TerminalImageProtocol::Iterm2);
    state.placements = vec![persistent_placement_key(msg, 2, 3, 5, 2)];
    state.last_intent = PersistentRasterIntent {
        image_modal_msg_id: None,
        overlay_blocks_raster: false,
        screen_tag: SEED_SCREEN_TAG,
        non_modal_image_tag: Some(11),
    };

    let out = state.pre_frame_persistent_raster_wipe_bytes(
        None,
        false,
        SEED_SCREEN_TAG,
        None,
        Some(TerminalImageProtocol::Iterm2),
    );

    assert!(!out.is_empty(), "expected iTerm2 cell wipe");
    assert!(state.placements.is_empty());
}

#[test]
fn pre_frame_wipe_fires_before_switching_away_from_a_persistent_protocol() {
    let mut state = TerminalImageRenderState::default();
    let msg = Uuid::new_v4();
    seed_persistent_raster_state(&mut state, msg);
    state.last_intent.non_modal_image_tag = Some(11);

    // Only the protocol differs from the seeded frame; see the note above.
    let out = state.pre_frame_persistent_raster_wipe_bytes(
        Some(msg),
        false,
        SEED_SCREEN_TAG,
        Some(11),
        Some(TerminalImageProtocol::Kitty),
    );

    assert!(!out.is_empty(), "expected wipe before protocol change");
    assert!(state.placements.is_empty());
}

/// The `was_persistent_raster` gate is what keeps Kitty — which deletes by
/// id — out of the wipe path entirely. Without it every Kitty frame that
/// changed a modal would emit a pointless rect of spaces.
#[test]
fn pre_frame_wipe_skips_a_previous_kitty_frame() {
    let mut state = TerminalImageRenderState::default();
    let msg = Uuid::new_v4();
    state.protocol = Some(TerminalImageProtocol::Kitty);
    state.placements = vec![persistent_placement_key(msg, 2, 3, 5, 2)];
    state.last_intent = PersistentRasterIntent {
        image_modal_msg_id: Some(msg),
        overlay_blocks_raster: false,
        screen_tag: SEED_SCREEN_TAG,
        non_modal_image_tag: None,
    };

    let out = state.pre_frame_persistent_raster_wipe_bytes(
        None,
        true,
        SEED_SCREEN_TAG + 1,
        Some(12),
        Some(TerminalImageProtocol::Kitty),
    );

    assert!(out.is_empty(), "Kitty deletes by id and must not be wiped");
}

#[test]
fn pre_frame_wipe_noop_when_no_prior_sixel() {
    let mut state = TerminalImageRenderState::default();
    let out = state.pre_frame_persistent_raster_wipe_bytes(
        None,
        false,
        SEED_SCREEN_TAG,
        None,
        Some(TerminalImageProtocol::Sixel),
    );
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
        /* suppress_raster */ true,
    );
    let any_sixel = cmds.iter().any(|c| c.starts_with(b"\x1bP"));
    assert!(
        !any_sixel,
        "Sixel should be suppressed; got commands: {cmds:?}"
    );
}

#[test]
fn persistent_raster_tag_tracks_both_placement_and_content() {
    let rect = Rect::new(36, 1, 48, 24);
    let moved = Rect::new(24, 1, 48, 24);

    assert_ne!(
        persistent_raster_tag(rect, 17),
        persistent_raster_tag(moved, 17),
        "a raster that moves must count as changed"
    );
    assert_ne!(
        persistent_raster_tag(rect, 17),
        persistent_raster_tag(rect, 18),
        "a raster whose content changes must count as changed"
    );
    assert_eq!(
        persistent_raster_tag(rect, 17),
        persistent_raster_tag(rect, 17)
    );
}

#[test]
fn build_commands_suppresses_iterm_emission_under_overlay() {
    let mut state = TerminalImageRenderState::default();
    let placement = TerminalImagePlacement {
        message_id: Uuid::new_v4(),
        area: Rect::new(2, 3, 5, 2),
        data: TerminalImageData::new(vec![1, 2, 3], None, 5, 2),
    };
    let mut frame = TerminalImageFrame::default();
    frame.push(placement);

    let commands = state.build_commands(Some(TerminalImageProtocol::Iterm2), &frame, true);

    assert!(commands.is_empty(), "iTerm2 must not emit under overlay");
}

fn opaque_test_data(color: Rgba<u8>, protocol: TerminalImageProtocol) -> TerminalImageData {
    let rgba = RgbaImage::from_pixel(
        TERMINAL_IMAGE_CELL_PIXEL_WIDTH,
        TERMINAL_IMAGE_CELL_PIXEL_HEIGHT,
        color,
    );
    terminal_image_from_rgba(&rgba, 1, 1, protocol).expect("encode opaque test image")
}

fn placement(message_id: Uuid, x: u16, data: TerminalImageData) -> TerminalImagePlacement {
    TerminalImagePlacement {
        message_id,
        area: Rect::new(x, 3, 1, 1),
        data,
    }
}

#[test]
fn kitty_opaque_cell_replacement_is_installed_before_targeted_cleanup() {
    let old_id = Uuid::from_u128(1);
    let new_id = Uuid::from_u128(2);
    let old = placement(
        old_id,
        2,
        opaque_test_data(Rgba([255, 0, 0, 255]), TerminalImageProtocol::Kitty),
    );
    let new = placement(
        new_id,
        2,
        opaque_test_data(Rgba([0, 255, 0, 255]), TerminalImageProtocol::Kitty),
    );
    let mut state = TerminalImageRenderState::default();
    let mut frame = TerminalImageFrame::default();
    frame.push(old);
    state.build_commands(Some(TerminalImageProtocol::Kitty), &frame, false);

    frame.clear();
    frame.push(new);
    let commands = state.build_commands(Some(TerminalImageProtocol::Kitty), &frame, false);
    let stream = commands.concat();
    let text = String::from_utf8_lossy(&stream);
    let transmit = text.find("a=T").expect("new image transmission");
    let targeted_delete = format!("a=d,d=I,i={}", kitty_image_id(old_id));
    let delete = text
        .find(&targeted_delete)
        .expect("old image targeted delete");

    assert!(
        transmit < delete,
        "new image must be placed before old cleanup"
    );
    assert!(
        !text.contains("a=d,d=Z"),
        "must not clear every Kitty placement"
    );
    assert!(
        !text.contains("a=d,d=R"),
        "must not clear the shared image-id range"
    );
}

#[test]
fn iterm_opaque_cell_replacement_emits_only_changed_cells() {
    let unchanged = placement(
        Uuid::from_u128(1),
        2,
        opaque_test_data(Rgba([255, 0, 0, 255]), TerminalImageProtocol::Iterm2),
    );
    let old = placement(
        Uuid::from_u128(2),
        3,
        opaque_test_data(Rgba([0, 255, 0, 255]), TerminalImageProtocol::Iterm2),
    );
    let new = placement(
        Uuid::from_u128(3),
        3,
        opaque_test_data(Rgba([0, 0, 255, 255]), TerminalImageProtocol::Iterm2),
    );
    let mut state = TerminalImageRenderState::default();
    let mut frame = TerminalImageFrame::default();
    frame.push(unchanged.clone());
    frame.push(old);
    state.build_commands(Some(TerminalImageProtocol::Iterm2), &frame, false);

    frame.clear();
    frame.push(unchanged);
    frame.push(new);
    let commands = state.build_commands(Some(TerminalImageProtocol::Iterm2), &frame, false);
    let image_commands = commands
        .iter()
        .filter(|command| command.starts_with(b"\x1b]1337;"))
        .count();

    assert_eq!(
        image_commands, 1,
        "unchanged cells must not be retransmitted"
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
