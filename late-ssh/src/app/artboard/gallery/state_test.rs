use chrono::{NaiveDate, Utc};

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

/// A piece as a listing holds it, for the refusal rules.
fn listed_piece(user: u128, period_month: NaiveDate) -> GalleryPiece {
    GalleryPiece {
        id: Uuid::now_v7(),
        user_id: Uuid::from_u128(user),
        username: format!("user-{user}"),
        title: "a piece".to_string(),
        width: 4,
        height: 2,
        canvas: dartboard_core::Canvas::with_size(4, 2),
        credits: Vec::new(),
        applause: 0,
        applauded_by_viewer: false,
        created: Utc::now(),
        period_month,
    }
}

#[test]
fn applause_and_take_down_refuse_before_the_round_trip() {
    let viewer = Uuid::from_u128(1);
    let this_month = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
    let last_month = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
    let mine = listed_piece(1, this_month);
    let theirs = listed_piece(2, this_month);
    let mine_settled = listed_piece(1, last_month);
    let theirs_settled = listed_piece(2, last_month);

    assert_eq!(applause_refusal(&theirs, viewer, this_month), None);
    assert_eq!(
        applause_refusal(&mine, viewer, this_month),
        Some(OWN_PIECE_APPLAUSE)
    );
    assert_eq!(
        applause_refusal(&theirs_settled, viewer, this_month),
        Some(CLOSED_MONTH_APPLAUSE)
    );

    assert_eq!(take_down_refusal(&mine, viewer, this_month), None);
    assert_eq!(
        take_down_refusal(&theirs, viewer, this_month),
        Some(NOT_YOURS_TAKE_DOWN)
    );
    assert_eq!(
        take_down_refusal(&mine_settled, viewer, this_month),
        Some(CLOSED_MONTH_TAKE_DOWN)
    );
}
