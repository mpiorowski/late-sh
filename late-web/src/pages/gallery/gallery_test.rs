use super::{snapshot_label, snapshot_title};

#[test]
fn snapshot_labels_are_human_readable() {
    assert_eq!(snapshot_label("main"), "Live");
    assert_eq!(snapshot_label("curated:2026-05-25"), "2026-05-25");
    assert_eq!(snapshot_label("daily:2026-04-24"), "2026-04-24");
    assert_eq!(snapshot_label("monthly:2026-04"), "2026-04");
}

#[test]
fn snapshot_titles_include_kind() {
    assert_eq!(snapshot_title("main"), "Live / latest saved");
    assert_eq!(snapshot_title("curated:2026-05-25"), "Curated 2026-05-25");
    assert_eq!(snapshot_title("daily:2026-04-24"), "Daily 2026-04-24");
    assert_eq!(snapshot_title("monthly:2026-04"), "Monthly 2026-04");
}

#[test]
fn canvas_json_keeps_cells_colours_and_cannot_close_its_script_element() {
    use dartboard_core::{Canvas, Pos, RgbColor};

    let mut canvas = Canvas::with_size(32, 4);
    for (x, ch) in "</script><script>".chars().enumerate() {
        canvas.put_glyph_colored(Pos { x, y: 0 }, ch, RgbColor::new(255, 0, 80));
    }
    canvas.put_glyph(Pos { x: 0, y: 2 }, '界');
    let json = super::canvas_json(&canvas).unwrap();
    assert!(!json.contains("</"));
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["width"], 32);
    assert_eq!(value["height"], 4);
    assert_eq!(
        value["cells"][0],
        serde_json::json!([0, 0, "<", 1, "#FF0050", null])
    );
    assert!(
        value["cells"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!([0, 2, "界", 2, null, null]))
    );
}
