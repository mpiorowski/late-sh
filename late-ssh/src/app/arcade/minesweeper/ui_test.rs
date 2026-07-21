use super::*;

#[test]
fn chord_preview_uses_subtle_glyph_without_background() {
    let mut glyph = " \u{00b7} ".to_string();
    let mut style = Style::default()
        .fg(theme::TEXT_BRIGHT())
        .bg(theme::BG_SELECTION());

    apply_chord_preview_style(&mut glyph, &mut style);

    assert_eq!(glyph, CHORD_PREVIEW_GLYPH);
    assert_eq!(style.fg, Some(theme::BORDER_DIM()));
    assert_eq!(style.bg, None);
}

fn board_origin(area: Rect, diff: &state::DifficultyConfig) -> (u16, u16) {
    let br = hit_area(area, diff);
    let content_width = 4 + diff.cols * 4;
    let text_start_x = br.x + (br.width - (content_width as u16)) / 2;
    (text_start_x + 4, br.y + 2)
}

#[test]
fn hit_test_hits_cells() {
    for diff in &state::DIFFICULTIES {
        let area = Rect::new(0, 0, 120, 60);
        let (ox, oy) = board_origin(area, diff);
        assert_eq!(hit_test(area, diff, 0, ox, oy), Some((0, 0)));
        assert_eq!(
            hit_test(area, diff, 0, ox + (diff.cols as u16 - 1) * 4, oy),
            Some((0, diff.cols - 1))
        );
        assert_eq!(
            hit_test(area, diff, 0, ox, oy + (diff.rows as u16 - 1) * 2),
            Some((diff.rows - 1, 0))
        );
        if diff.cols > 1 {
            assert_eq!(hit_test(area, diff, 0, ox + 4, oy), Some((0, 1)));
        }
    }
}

#[test]
fn hit_test_rejects_non_cell_area() {
    let diff = state::DIFFICULTIES[0];
    let area = Rect::new(0, 0, 80, 40);
    let (ox, oy) = board_origin(area, &diff);
    let br = hit_area(area, &diff);

    assert_eq!(
        hit_test(area, &diff, 0, ox + 3, oy),
        None,
        "vertical separator"
    );
    assert_eq!(
        hit_test(area, &diff, 0, ox, oy + 1),
        None,
        "horizontal separator"
    );
    assert_eq!(
        hit_test(area, &diff, 0, br.x + 1, br.y + 2),
        None,
        "row label"
    );
    assert_eq!(hit_test(area, &diff, 0, ox, oy - 1), None, "column header");
    assert_eq!(
        hit_test(area, &diff, 0, ox, oy + (diff.rows as u16 - 1) * 2 + 1),
        None,
        "bottom border"
    );
    assert_eq!(hit_test(area, &diff, 0, 0, 0), None, "top-left corner");
    assert_eq!(
        hit_test(area, &diff, 0, 79, 39),
        None,
        "bottom-right corner"
    );
}
