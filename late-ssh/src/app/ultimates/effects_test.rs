use super::*;

#[test]
fn wonderland_postprocess_maps_glyph_positions_to_varied_colors() {
    let mut buffer = Buffer::with_lines([
        "################",
        "################",
        "################",
        "################",
    ]);
    apply_ultimate_postprocess(
        &mut buffer,
        UltimateThemeEffect {
            kind: UltimateEffectKind::Wonderland,
            seed: 42,
            elapsed_ms: 1_250,
        },
    );

    let mut colors = Vec::new();
    for cell in buffer.content() {
        if !colors.contains(&cell.fg) {
            colors.push(cell.fg);
        }
    }
    assert!(
        colors.len() > 3,
        "expected varied glyph colors, got {colors:?}"
    );
}

#[test]
fn wonderland_postprocess_skips_blank_cells() {
    let mut buffer = Buffer::with_lines(["# #"]);
    apply_ultimate_postprocess(
        &mut buffer,
        UltimateThemeEffect {
            kind: UltimateEffectKind::Wonderland,
            seed: 7,
            elapsed_ms: 500,
        },
    );

    assert_ne!(buffer.cell((0, 0)).expect("left glyph").fg, Color::Reset);
    assert_eq!(buffer.cell((1, 0)).expect("blank cell").fg, Color::Reset);
    assert_ne!(buffer.cell((2, 0)).expect("right glyph").fg, Color::Reset);
}

#[test]
fn thematrix_postprocess_uses_background_colors_only() {
    let mut buffer = Buffer::with_lines(["################", "################"]);
    for x in 0..16u16 {
        buffer
            .cell_mut((x, 0))
            .expect("top cell")
            .set_fg(Color::Red);
    }

    apply_ultimate_postprocess(
        &mut buffer,
        UltimateThemeEffect {
            kind: UltimateEffectKind::Thematrix,
            seed: 42,
            elapsed_ms: 1_750,
        },
    );

    assert_eq!(buffer.cell((0, 0)).expect("top left").fg, Color::Red);
    assert!(
        buffer
            .content()
            .iter()
            .all(|cell| cell.fg == Color::Red || cell.fg == Color::Reset)
    );
    assert!(buffer.content().iter().all(|cell| cell.bg != Color::Reset));
}

#[test]
fn thematrix_postprocess_creates_varied_background_brightness() {
    let mut buffer = Buffer::with_lines([
        "########################",
        "########################",
        "########################",
        "########################",
        "########################",
        "########################",
        "########################",
        "########################",
    ]);

    apply_ultimate_postprocess(
        &mut buffer,
        UltimateThemeEffect {
            kind: UltimateEffectKind::Thematrix,
            seed: 99,
            elapsed_ms: 2_250,
        },
    );

    let mut backgrounds = Vec::new();
    for cell in buffer.content() {
        if !backgrounds.contains(&cell.bg) {
            backgrounds.push(cell.bg);
        }
    }
    assert!(
        backgrounds.len() > 3,
        "expected varied The Matrix backgrounds, got {backgrounds:?}"
    );
}

#[test]
fn thematrix_z_range_and_top_width_are_bounded() {
    for col in 0..64 {
        for cycle in -4..12 {
            let z = thematrix_z(123, col, cycle, THEMATRIX_Z_MAX);
            assert!(z <= THEMATRIX_Z_MAX);
            assert_eq!(
                thematrix_line_width(z, THEMATRIX_Z_MAX),
                if z == THEMATRIX_Z_MAX { 2 } else { 1 }
            );
        }
    }
}

#[test]
fn thematrix_z_extremes_have_distinct_green_intensity() {
    let Color::Rgb(_, low_green, _) = thematrix_green(1.0, 0.0) else {
        panic!("expected rgb color");
    };
    let Color::Rgb(_, high_green, _) = thematrix_green(1.0, 1.0) else {
        panic!("expected rgb color");
    };

    assert!(
        high_green.saturating_sub(low_green) >= 160,
        "expected z max to be much brighter than z min: low={low_green}, high={high_green}"
    );
}

#[test]
fn thematrix_fadeout_keeps_cutoff_lines_without_spawning_new_cycles() {
    let cutoff_secs = THEMATRIX_MAIN_PHASE_MS as f32 / 1000.0;
    let (col, cutoff_line) = (0..80)
        .find_map(|col| {
            thematrix_line_for_col(12, 99, col, cutoff_secs).map(|line| (col, line))
        })
        .expect("expected at least one active cutoff line");

    let fadeout_line =
        thematrix_line_for_col(12, 99, col, cutoff_secs + 0.5).expect("fadeout line");

    assert_eq!(fadeout_line.cycle, cutoff_line.cycle);
    assert!(fadeout_line.head_y > cutoff_line.head_y);
}

#[test]
fn thematrix_fadeout_reaches_zero_at_total_duration() {
    assert_eq!(thematrix_fade_factor(THEMATRIX_MAIN_PHASE_MS), 1.0);
    assert_eq!(thematrix_fade_factor(THEMATRIX_TOTAL_MS), 0.0);
}
