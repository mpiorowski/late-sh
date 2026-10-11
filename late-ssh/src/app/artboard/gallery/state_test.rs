use chrono::{NaiveDate, Utc};

use super::*;
use crate::app::artboard::gallery::svc::GalleryService;

fn state() -> GalleryState {
    GalleryState::new(GalleryService::disabled(), Uuid::nil())
}

#[test]
fn the_rail_shrinks_to_board_and_archives_while_the_gallery_is_off() {
    // A disabled service has no database, so the gallery is off.
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
            RailRow::Gallery(GallerySection::Newest),
            RailRow::Gallery(GallerySection::ThisMonth),
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
fn the_ranking_hint_names_each_place_badge_and_prize() {
    assert_eq!(
        GallerySection::ThisMonth.hint(),
        "this month's pieces by applause; at month end 1st ART1 + 40,000 chips, 2nd ART2 + 15,000 chips, 3rd ART3 + 10,000 chips"
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
        content_rating: Default::default(),
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

/// Throw away what the disabled service answered at once, so the test
/// hands the state every result itself.
fn drain(state: &mut GalleryState) {
    while state.results_rx.try_recv().is_ok() {}
}

fn rating_dialog(piece_id: Uuid) -> RatingDialog {
    RatingDialog {
        piece_id,
        selected: 0,
        pending: true,
        error: None,
        action_areas: Cell::new(Vec::new()),
    }
}

#[test]
fn rating_dialog_offers_owner_flag_or_votes_and_keeps_mouse_targets() {
    let mut gallery = state();
    let piece = listed_piece(2, Utc::now().date_naive());
    gallery.sections[0].pieces.push(piece.clone());
    gallery.rating_dialog = Some(rating_dialog(piece.id));
    assert!(gallery.captures_typing());
    assert!(gallery.claims_escape());
    assert!(gallery.claims_q());
    assert_eq!(gallery.rating_actions().len(), 3);
    gallery.rating_move(-1);
    assert_eq!(gallery.rating_dialog.as_ref().unwrap().selected, 2);
    gallery
        .rating_dialog
        .as_ref()
        .unwrap()
        .action_areas
        .set(vec![Rect::new(10, 12, 40, 1)]);
    assert_eq!(gallery.rating_action_at(15, 12), Some(0));
    assert_eq!(gallery.rating_action_at(15, 12), Some(0));
    gallery.viewer_id = piece.user_id;
    assert_eq!(
        gallery.rating_actions(),
        vec![("Mark my piece NSFW", ContentRatingAction::OwnerFlag(true))]
    );
    gallery.sections[0].pieces[0]
        .content_rating
        .owner_marked_nsfw = true;
    assert_eq!(
        gallery.rating_actions(),
        vec![("Remove my NSFW flag", ContentRatingAction::OwnerFlag(false))]
    );
    gallery.close_rating_dialog();
    assert!(!gallery.captures_typing());
}

#[test]
fn rating_result_updates_all_copies_after_close_and_invalidates_older_listing() {
    use late_core::models::artboard_piece_rating::ContentRatingSummary;
    let mut gallery = state();
    drain(&mut gallery);
    let piece = listed_piece(2, Utc::now().date_naive());
    for section in &mut gallery.sections {
        section.pieces.push(piece.clone());
    }
    gallery.reload(GallerySection::Newest);
    drain(&mut gallery);
    gallery.rating_generation = 2;
    gallery.pending_rating = true;
    gallery.rating_dialog = Some(rating_dialog(piece.id));
    gallery.close_rating_dialog();
    let summary = ContentRatingSummary {
        nsfw_votes: 2,
        viewer_vote: Some(ArtContentRating::Nsfw),
        ..Default::default()
    };
    gallery
        .results_tx
        .send(GalleryResult::ContentRating {
            piece_id: piece.id,
            generation: 1,
            outcome: ContentRatingOutcome::Rated(summary),
        })
        .unwrap();
    gallery.tick();
    assert!(
        gallery.pending_rating,
        "a stale answer must not finish a newer write"
    );
    assert_eq!(
        gallery.sections[0].pieces[0].content_rating,
        Default::default()
    );
    gallery
        .results_tx
        .send(GalleryResult::ContentRating {
            piece_id: piece.id,
            generation: 2,
            outcome: ContentRatingOutcome::Rated(summary),
        })
        .unwrap();
    gallery.tick();
    drain(&mut gallery);
    assert!(!gallery.pending_rating);
    // The disabled test service answers its replacement request immediately;
    // restore that section as if the real replacement request were still pending.
    gallery.sections[0].pieces = vec![piece.clone()];
    gallery.sections[0].pieces[0].content_rating = summary;
    gallery
        .results_tx
        .send(GalleryResult::Listed {
            listing: PieceListing::Newest,
            generation: 1,
            pieces: vec![piece],
        })
        .unwrap();
    gallery.tick();
    for section in &gallery.sections {
        assert_eq!(section.pieces[0].content_rating, summary);
    }
}

#[test]
fn refused_and_failed_rating_results_leave_the_verdict_and_vote_unchanged() {
    let mut gallery = state();
    drain(&mut gallery);
    let piece = listed_piece(2, Utc::now().date_naive());
    gallery.sections[0].pieces.push(piece.clone());
    gallery.rating_dialog = Some(rating_dialog(piece.id));
    gallery.pending_rating = true;
    gallery.rating_generation = 1;
    gallery
        .results_tx
        .send(GalleryResult::ContentRating {
            piece_id: piece.id,
            generation: 1,
            outcome: ContentRatingOutcome::Closed,
        })
        .unwrap();
    gallery.tick();
    assert_eq!(gallery.sections[0].pieces[0], piece);
    assert!(!gallery.pending_rating);
    assert_eq!(
        gallery.rating_dialog.as_ref().unwrap().error.as_deref(),
        Some("The gallery is closed.")
    );
    assert_eq!(gallery.notice(), Some("The gallery is closed."));

    gallery.pending_rating = true;
    gallery
        .results_tx
        .send(GalleryResult::ContentRatingFailed {
            piece_id: piece.id,
            generation: 1,
            error: "The content rating did not go through. Try again.".to_string(),
        })
        .unwrap();
    gallery.tick();
    assert_eq!(gallery.sections[0].pieces[0], piece);
    assert!(!gallery.pending_rating);
    assert_eq!(
        gallery.rating_dialog.as_ref().unwrap().error.as_deref(),
        Some("The content rating did not go through. Try again.")
    );
}

#[test]
fn a_piece_disappearing_closes_its_rating_dialog_and_releases_keys() {
    let mut gallery = state();
    drain(&mut gallery);
    gallery.rating_dialog = Some(rating_dialog(Uuid::now_v7()));
    assert!(gallery.tick());
    assert!(gallery.rating_dialog.is_none());
    assert!(!gallery.captures_typing());
    assert_eq!(
        gallery.notice(),
        Some("The piece is no longer in this listing.")
    );
}

#[test]
fn a_listing_asked_for_before_a_take_down_cannot_put_the_piece_back() {
    let viewer = Uuid::from_u128(1);
    let this_month = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
    let mut state = GalleryState::new(GalleryService::disabled(), viewer);
    let piece = listed_piece(1, this_month);
    let mine = GallerySection::Mine;

    // The first request (generation 1) lands with the piece.
    state.ensure_loaded(mine);
    drain(&mut state);
    state
        .results_tx
        .send(GalleryResult::Listed {
            listing: PieceListing::Mine,
            generation: 1,
            pieces: vec![piece.clone()],
        })
        .unwrap();
    assert!(state.tick());
    assert_eq!(state.section_pieces(mine), std::slice::from_ref(&piece));

    // A second request (generation 2) is in flight when the take-down
    // lands: the hang handler reloads Mine after every hang.
    state.reload(mine);
    drain(&mut state);
    state
        .results_tx
        .send(GalleryResult::TakeDown {
            piece_id: piece.id,
            outcome: TakeDownOutcome::TakenDown,
        })
        .unwrap();
    assert!(state.tick());
    assert_eq!(state.section_pieces(mine), &[]);
    assert_eq!(state.notice(), Some("Taken down. The wall forgets it."));
    drain(&mut state);

    // The second request's answer ran before the take-down, so it still
    // carries the piece. It is stale and must not land.
    state
        .results_tx
        .send(GalleryResult::Listed {
            listing: PieceListing::Mine,
            generation: 2,
            pieces: vec![piece.clone()],
        })
        .unwrap();
    state.tick();
    assert_eq!(state.section_pieces(mine), &[]);
    assert_eq!(state.section_count(mine), Some(0));
}
