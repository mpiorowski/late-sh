use super::*;
use crate::app::artboard::gallery::svc::GalleryService;

fn state() -> GalleryState {
    GalleryState::new(GalleryService::disabled(), Uuid::nil())
}

#[test]
fn the_rail_shrinks_to_board_and_archives_while_the_gallery_is_off() {
    // A disabled service reads as the switch being off.
    let gallery = state();
    assert_eq!(
        gallery.rows(),
        vec![
            RailRow::Board,
            RailRow::Archive(ArtboardSnapshotKind::Daily),
            RailRow::Archive(ArtboardSnapshotKind::Monthly),
            RailRow::Archive(ArtboardSnapshotKind::Curated),
        ]
    );
    assert_eq!(gallery.selected_row(), RailRow::Board);
    assert_eq!(
        RailRow::rows(true),
        vec![
            RailRow::Board,
            RailRow::Gallery(GallerySection::ThisMonth),
            RailRow::Gallery(GallerySection::Newest),
            RailRow::Gallery(GallerySection::HallOfFame),
            RailRow::Gallery(GallerySection::Mine),
            RailRow::Hang,
            RailRow::Archive(ArtboardSnapshotKind::Daily),
            RailRow::Archive(ArtboardSnapshotKind::Monthly),
            RailRow::Archive(ArtboardSnapshotKind::Curated),
        ]
    );
}

#[test]
fn the_page_lands_on_the_rail_and_activation_answers_the_page() {
    let mut gallery = state();
    assert_eq!(gallery.focus(), Focus::Rail);
    assert!(!gallery.claims_escape());
    gallery.rail_move(5);
    assert_eq!(
        gallery.selected_row(),
        RailRow::Archive(ArtboardSnapshotKind::Curated)
    );
    assert_eq!(
        gallery.rail_activate(),
        RailActivation::OpenArchive(ArtboardSnapshotKind::Curated)
    );
    gallery.focus_archive();
    assert_eq!(gallery.focus(), Focus::Archive);
    assert_eq!(
        gallery.rail_row_at(0, 0),
        None,
        "an archive list in the rail's place has no rows to click"
    );
    gallery.rail_move(-5);
    assert_eq!(gallery.selected_row(), RailRow::Board);
    assert_eq!(gallery.rail_activate(), RailActivation::FocusCanvas);
    assert_eq!(gallery.focus(), Focus::Canvas);
    assert!(
        gallery.claims_escape(),
        "Esc on the board goes back to the rail"
    );
    gallery.focus_rail();
    assert!(
        !gallery.claims_escape(),
        "Esc on the rail is not the page's"
    );
}

#[test]
fn the_hang_flow_needs_a_title_and_typing_captures_keys() {
    let mut gallery = state();
    assert!(!gallery.captures_typing());
    gallery.begin_framing();
    assert!(gallery.is_framing());
    assert!(gallery.captures_typing());
    assert!(gallery.claims_escape());

    let framed = crate::app::artboard::gallery::frame::FramedPiece {
        width: 2,
        height: 1,
        canvas: dartboard_core::Canvas::with_size(2, 1),
        provenance: Default::default(),
        glyph_count: 40,
        own_share_percent: 100,
        credits: Vec::new(),
        content_hash: "h".to_string(),
    };
    gallery.set_confirm(framed);
    assert!(gallery.is_confirming());
    gallery.submit_hang();
    assert!(
        gallery.is_confirming(),
        "an empty title must not be sent: {:?}",
        gallery.notice()
    );
    assert_eq!(gallery.notice(), Some("Give it a title first."));
    for ch in "sunset".chars() {
        gallery.title_push(ch);
    }
    gallery.title_push('\u{7}');
    match gallery.hang() {
        HangFlow::Confirm { title, .. } => assert_eq!(title, "sunset"),
        other => panic!("expected the confirm step, got {other:?}"),
    }
    gallery.cancel_hang();
    assert_eq!(gallery.hang(), &HangFlow::Idle);
    assert!(!gallery.claims_q());
}
