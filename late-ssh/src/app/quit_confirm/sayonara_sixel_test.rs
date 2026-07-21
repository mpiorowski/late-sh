use super::*;

#[test]
fn sayonara_image_caches() {
    let a = sayonara_terminal_image(TerminalImageProtocol::Kitty).unwrap();
    let b = sayonara_terminal_image(TerminalImageProtocol::Kitty).unwrap();
    assert!(Arc::ptr_eq(&a, &b));
}

#[test]
fn sayonara_image_has_expected_display_size() {
    let data = sayonara_terminal_image(TerminalImageProtocol::Kitty).unwrap();
    assert_eq!(data.display_cols, SAYONARA_DISPLAY_COLS);
    assert_eq!(data.display_rows, SAYONARA_DISPLAY_ROWS);
}

#[test]
fn sayonara_image_sixel_only_for_sixel_protocol() {
    let kitty = sayonara_terminal_image(TerminalImageProtocol::Kitty).unwrap();
    assert!(kitty.sixel_bytes.is_none());

    let sixel = sayonara_terminal_image(TerminalImageProtocol::Sixel).unwrap();
    assert!(sixel.sixel_bytes.is_some());
}

#[test]
fn drawn_scene_has_meaningful_pixel_coverage() {
    let img = draw_sayonara_rgba();
    let non_transparent = img.pixels().filter(|p| p.0[3] > 0).count();
    let total = (CANVAS_W * CANVAS_H) as usize;
    // Sky + water bands cover the full canvas, so this should be
    // ~the entire image rather than a sparse sprite.
    assert!(
        non_transparent > total / 2,
        "scene should fill most of the canvas, got {non_transparent} of {total}"
    );
}
