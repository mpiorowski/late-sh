use chrono::{NaiveDate, TimeZone, Utc};

use super::hung_label;

#[test]
fn the_splash_caption_counts_the_days_since_the_hang() {
    let shown_on = NaiveDate::from_ymd_opt(2026, 9, 22).unwrap();
    let hung = |day: u32| Utc.with_ymd_and_hms(2026, 9, day, 23, 50, 0).unwrap();
    assert_eq!(hung_label(shown_on, hung(21)), "hung yesterday");
    assert_eq!(hung_label(shown_on, hung(19)), "hung 3 days ago");
    // A replica behind the piece's clock still says yesterday.
    assert_eq!(hung_label(shown_on, hung(22)), "hung yesterday");
}

#[test]
fn lounge_card_lines_renders_preview_when_disclaimer_off() {
    use super::{GalleryPiece, lounge_card_lines};
    use dartboard_core::{Canvas, Pos, RgbColor};
    use uuid::Uuid;

    let mut canvas = Canvas::new();
    canvas.put_glyph_colored(Pos { x: 0, y: 0 }, '#', RgbColor { r: 255, g: 0, b: 0 });
    let piece = GalleryPiece {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        username: "artist".to_string(),
        title: "Test Art".to_string(),
        width: 10,
        height: 5,
        canvas,
        credits: vec![],
        period_month: chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        applause: 5,
        applauded_by_viewer: false,
        created: chrono::Utc::now(),
    };

    let lines = lounge_card_lines(&piece, 70, false, None);
    // Row 1: Title
    assert!(lines[0].to_string().contains("🎨 \"Test Art\""));
    // Preview lines: 5 lines for canvas height
    assert_eq!(lines.len(), 1 + 5 + 1 + 1); // title + 5 preview rows + meta + cta
    assert!(lines[1].to_string().contains('#'));
    // Meta line
    assert!(lines[6].to_string().contains("10x5"));
    assert!(lines[6].to_string().contains("all their own work"));
    assert!(lines[6].to_string().contains("5 applause"));
    assert!(lines[6].to_string().contains("Enter preview · o gallery"));
    // CTA line
    assert!(lines[7].to_string().contains("hang a piece on Artboard"));
}

#[test]
fn small_piece_uses_native_cells_and_large_piece_uses_bitmap_preview() {
    use super::{GalleryPiece, lounge_card_lines, piece_fits_native_preview};
    use dartboard_core::{Canvas, Pos, RgbColor};
    use ratatui::text::Line;
    use uuid::Uuid;

    let mut small_canvas = Canvas::new();
    small_canvas.put_glyph_colored(Pos { x: 0, y: 0 }, '#', RgbColor { r: 255, g: 0, b: 0 });
    let small_piece = GalleryPiece {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        username: "artist".to_string(),
        title: "Small Piece".to_string(),
        width: 10,
        height: 5,
        canvas: small_canvas,
        credits: vec![],
        period_month: chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        applause: 0,
        applauded_by_viewer: false,
        created: chrono::Utc::now(),
    };

    assert!(piece_fits_native_preview(&small_piece, 60));
    let mock_bitmap = vec![Line::from("bitmap_row_1"), Line::from("bitmap_row_2")];
    let small_lines = lounge_card_lines(&small_piece, 60, false, Some(&mock_bitmap));
    // Small piece fits, so native rows are rendered instead of mock_bitmap
    assert!(small_lines.iter().any(|l| l.to_string().contains('#')));
    assert!(!small_lines.iter().any(|l| l.to_string().contains("bitmap_row")));

    let mut large_canvas = Canvas::new();
    large_canvas.put_glyph_colored(Pos { x: 0, y: 0 }, '#', RgbColor { r: 255, g: 0, b: 0 });
    let large_piece = GalleryPiece {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        username: "artist".to_string(),
        title: "Large Piece".to_string(),
        width: 100,
        height: 30,
        canvas: large_canvas,
        credits: vec![],
        period_month: chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        applause: 0,
        applauded_by_viewer: false,
        created: chrono::Utc::now(),
    };

    assert!(!piece_fits_native_preview(&large_piece, 60));
    let large_lines = lounge_card_lines(&large_piece, 60, false, Some(&mock_bitmap));
    assert!(large_lines.iter().any(|l| l.to_string().contains("bitmap_row_1")));
    assert!(large_lines.iter().any(|l| l.to_string().contains("bitmap_row_2")));
}

#[test]
fn raster_renders_canvas_to_inline_and_terminal_image() {
    use crate::app::artboard::gallery::raster::{
        render_canvas_inline_preview, render_canvas_terminal_image,
    };
    use crate::app::files::{
        inline_image::InlineImageRenderSettings, terminal_image::TerminalImageProtocol,
    };
    use dartboard_core::{Canvas, Pos, RgbColor};

    let mut canvas = Canvas::new();
    for x in 0..20 {
        for y in 0..10 {
            canvas.put_glyph_colored(Pos { x, y }, 'A', RgbColor { r: 200, g: 150, b: 100 });
        }
    }

    let preview = render_canvas_inline_preview(&canvas, 96, 12, InlineImageRenderSettings::default())
        .expect("inline preview renders");
    assert!(!preview.is_empty());
    assert!(preview.len() <= 12);

    let terminal_image = render_canvas_terminal_image(&canvas, 80, 24, TerminalImageProtocol::Sixel)
        .expect("terminal image renders");
    assert!(terminal_image.supports_protocol(TerminalImageProtocol::Sixel));
    assert!(terminal_image.sixel_bytes.is_some());
    assert!(!terminal_image.png_bytes.is_empty());
    assert!(terminal_image.is_opaque());
}

#[test]
fn lounge_card_lines_shows_disclaimer_notice_when_disclaimer_active() {
    use super::{GalleryPiece, lounge_card_lines};
    use dartboard_core::Canvas;
    use uuid::Uuid;

    let piece = GalleryPiece {
        id: Uuid::new_v4(),
        user_id: Uuid::new_v4(),
        username: "artist".to_string(),
        title: "Spicy Art".to_string(),
        width: 30,
        height: 20,
        canvas: Canvas::new(),
        credits: vec![],
        period_month: chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        applause: 0,
        applauded_by_viewer: false,
        created: chrono::Utc::now(),
    };

    let lines = lounge_card_lines(&piece, 60, true, None);
    assert_eq!(lines.len(), 4); // title + disclaimer note + meta + cta
    assert!(lines[0].to_string().contains("🎨 \"Spicy Art\""));
    assert!(lines[1].to_string().contains("Artboard content disclaimer is on"));
    assert!(lines[2].to_string().contains("30x20"));
    assert!(lines[3].to_string().contains("hang a piece on Artboard"));
}
