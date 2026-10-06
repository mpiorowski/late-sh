use super::*;
use crate::db::Db;
use crate::models::artboard_piece::{ArtboardPiece, HangOutcome, PieceListing};
use crate::models::artboard_piece_test::hang_params;
use crate::test_utils::{create_test_user, test_db};

#[test]
fn community_requires_two_nsfw_votes_and_a_strict_majority() {
    for nsfw_votes in 0..6 {
        for sfw_votes in 0..6 {
            let summary = ContentRatingSummary {
                nsfw_votes,
                sfw_votes,
                ..Default::default()
            };
            assert_eq!(
                summary.determination().0.is_nsfw(),
                nsfw_votes >= 2 && nsfw_votes > sfw_votes
            );
        }
    }
}

#[test]
fn moderator_ties_are_nsfw_and_any_admin_nsfw_wins() {
    for sfw in 0..6 {
        for nsfw in 0..6 {
            if sfw + nsfw == 0 {
                continue;
            }
            let moderator = ContentRatingSummary {
                mod_sfw: sfw,
                mod_nsfw: nsfw,
                ..Default::default()
            };
            assert_eq!(moderator.determination().0.is_nsfw(), nsfw >= sfw);
            let admin = ContentRatingSummary {
                admin_sfw: sfw,
                admin_nsfw: nsfw,
                ..Default::default()
            };
            assert_eq!(admin.determination().0.is_nsfw(), nsfw > 0);
        }
    }
}

#[test]
fn staff_owner_and_community_precedence_includes_explicit_sfw() {
    let mut summary = ContentRatingSummary {
        sfw_votes: 3,
        nsfw_votes: 2,
        ..Default::default()
    };
    assert_eq!(
        summary.determination(),
        (ArtContentRating::Sfw, RatingSource::Community)
    );
    summary.owner_marked_nsfw = true;
    assert_eq!(
        summary.determination(),
        (ArtContentRating::Nsfw, RatingSource::Owner)
    );
    summary.mod_sfw = 2;
    summary.mod_nsfw = 1;
    assert_eq!(
        summary.determination(),
        (ArtContentRating::Sfw, RatingSource::Moderator)
    );
    summary.mod_nsfw = 2;
    assert_eq!(
        summary.determination(),
        (ArtContentRating::Nsfw, RatingSource::Moderator)
    );
    summary.admin_sfw = 10;
    assert_eq!(
        summary.determination(),
        (ArtContentRating::Sfw, RatingSource::Admin)
    );
    summary.admin_nsfw = 1;
    assert_eq!(
        summary.determination(),
        (ArtContentRating::Nsfw, RatingSource::Admin)
    );
    assert_eq!(
        ContentRatingSummary::default().determination(),
        (ArtContentRating::Sfw, RatingSource::Default)
    );
}

async fn hang(db: &Db, owner: Uuid, hash: &str) -> Uuid {
    let client = db.get().await.unwrap();
    match ArtboardPiece::hang(&client, hang_params(owner, "rating test", hash))
        .await
        .unwrap()
    {
        HangOutcome::Hung(piece) => piece.id,
        other => panic!("hang: {other:?}"),
    }
}

async fn try_vote(
    db: &Db,
    piece: Uuid,
    user: Uuid,
    rating: Option<ArtContentRating>,
) -> VoteOutcome {
    let mut client = db.get().await.unwrap();
    let tx = client.transaction().await.unwrap();
    let outcome = ArtboardPieceRating::set_vote(&tx, piece, user, rating)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    outcome
}

/// A vote that must land, and the summary it left.
async fn vote(
    db: &Db,
    piece: Uuid,
    user: Uuid,
    rating: Option<ArtContentRating>,
) -> ContentRatingSummary {
    assert_eq!(try_vote(db, piece, user, rating).await, VoteOutcome::Saved);
    let client = db.get().await.unwrap();
    ArtboardPieceRating::read(&client, piece, user)
        .await
        .unwrap()
        .unwrap()
}

async fn set_owner_flag(db: &Db, piece: Uuid, user: Uuid, nsfw: bool) -> OwnerFlagOutcome {
    let mut client = db.get().await.unwrap();
    let tx = client.transaction().await.unwrap();
    let outcome = ArtboardPieceRating::set_owner_flag(&tx, piece, user, nsfw)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    outcome
}

async fn mark(
    db: &Db,
    piece: Uuid,
    actor: Uuid,
    tier: StaffAuthority,
    rating: Option<ArtContentRating>,
) {
    let mut client = db.get().await.unwrap();
    let tx = client.transaction().await.unwrap();
    let outcome = ArtboardPieceRating::set_staff_mark(&tx, piece, actor, tier, rating, "test")
        .await
        .unwrap();
    assert!(matches!(outcome, StaffMarkOutcome::Saved { .. }));
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn votes_are_replaceable_revocable_independent_of_applause_and_never_self_votes() {
    let test_db = test_db().await;
    let db = &test_db.db;
    let owner = create_test_user(db, "rating-owner").await;
    let fan = create_test_user(db, "rating-fan").await;
    let fan2 = create_test_user(db, "rating-fan2").await;
    let piece = hang(db, owner.id, "replaceable-rating").await;
    assert_eq!(
        try_vote(db, piece, owner.id, Some(ArtContentRating::Sfw)).await,
        VoteOutcome::OwnPiece
    );
    assert_eq!(
        try_vote(db, piece, owner.id, Some(ArtContentRating::Nsfw)).await,
        VoteOutcome::OwnPiece
    );
    let first = vote(db, piece, fan.id, Some(ArtContentRating::Nsfw)).await;
    assert_eq!(first.viewer_vote, Some(ArtContentRating::Nsfw));
    assert!(!first.determination().0.is_nsfw());
    let second = vote(db, piece, fan2.id, Some(ArtContentRating::Nsfw)).await;
    assert!(second.determination().0.is_nsfw());
    let replaced = vote(db, piece, fan.id, Some(ArtContentRating::Sfw)).await;
    assert_eq!((replaced.sfw_votes, replaced.nsfw_votes), (1, 1));
    let client = db.get().await.unwrap();
    assert_eq!(
        ArtboardPieceRating::content_votes(&client, piece)
            .await
            .unwrap(),
        vec![
            ContentVote {
                user_id: fan2.id,
                username: fan2.username.clone(),
                rating: ArtContentRating::Nsfw,
            },
            ContentVote {
                user_id: fan.id,
                username: fan.username.clone(),
                rating: ArtContentRating::Sfw,
            },
        ]
    );
    assert!(!replaced.determination().0.is_nsfw());
    vote(db, piece, fan.id, Some(ArtContentRating::Sfw)).await;
    let withdrawn = vote(db, piece, fan.id, None).await;
    assert_eq!(
        (
            withdrawn.sfw_votes,
            withdrawn.nsfw_votes,
            withdrawn.viewer_vote
        ),
        (0, 1, None)
    );
    let client = db.get().await.unwrap();
    let listed = ArtboardPiece::list(&client, fan2.id, PieceListing::Newest)
        .await
        .unwrap();
    assert_eq!(listed[0].applause, 0);
    assert_eq!(
        listed[0].content_rating.viewer_vote,
        Some(ArtContentRating::Nsfw)
    );
    // The database rejects a self-vote even if a caller bypasses the model.
    assert!(client.execute("INSERT INTO artboard_piece_content_votes (piece_id, user_id, author_user_id, nsfw) VALUES ($1, $2, $2, true)",
        &[&piece, &owner.id]).await.is_err());
}

#[tokio::test]
async fn clearing_each_override_reveals_votes_cast_while_overridden_and_past_months_stay_open() {
    let test_db = test_db().await;
    let db = &test_db.db;
    let owner = create_test_user(db, "fallback-owner").await;
    let fan = create_test_user(db, "fallback-fan").await;
    let fan2 = create_test_user(db, "fallback-fan2").await;
    let moderator = create_test_user(db, "fallback-mod").await;
    let admin = create_test_user(db, "fallback-admin").await;
    let piece = hang(db, owner.id, "fallback-rating").await;
    let client = db.get().await.unwrap();
    client.execute("UPDATE artboard_pieces SET period_month = (period_month - INTERVAL '1 month')::date WHERE id = $1", &[&piece]).await.unwrap();
    assert_eq!(
        set_owner_flag(db, piece, owner.id, true).await,
        OwnerFlagOutcome::Saved
    );
    mark(
        db,
        piece,
        moderator.id,
        StaffAuthority::Moderator,
        Some(ArtContentRating::Sfw),
    )
    .await;
    mark(
        db,
        piece,
        admin.id,
        StaffAuthority::Admin,
        Some(ArtContentRating::Sfw),
    )
    .await;
    vote(db, piece, fan.id, Some(ArtContentRating::Nsfw)).await;
    let summary = vote(db, piece, fan2.id, Some(ArtContentRating::Nsfw)).await;
    assert_eq!(
        (summary.nsfw_votes, summary.determination().1),
        (2, RatingSource::Admin)
    );
    mark(db, piece, admin.id, StaffAuthority::Admin, None).await;
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, owner.id)
            .await
            .unwrap()
            .unwrap()
            .determination(),
        (ArtContentRating::Sfw, RatingSource::Moderator)
    );
    mark(db, piece, moderator.id, StaffAuthority::Moderator, None).await;
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, owner.id)
            .await
            .unwrap()
            .unwrap()
            .determination()
            .1,
        RatingSource::Owner
    );
    assert_eq!(
        set_owner_flag(db, piece, owner.id, false).await,
        OwnerFlagOutcome::Saved
    );
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, owner.id)
            .await
            .unwrap()
            .unwrap()
            .determination(),
        (ArtContentRating::Nsfw, RatingSource::Community)
    );
    assert_eq!(
        set_owner_flag(db, piece, fan.id, true).await,
        OwnerFlagOutcome::NotYours
    );
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, owner.id)
            .await
            .unwrap()
            .unwrap()
            .determination(),
        (ArtContentRating::Nsfw, RatingSource::Community),
        "a refused flag changes nothing"
    );
}

#[tokio::test]
async fn staff_authority_survives_role_changes_and_deletion_and_removal_by_actor_clears_either_tier()
 {
    let test_db = test_db().await;
    let db = &test_db.db;
    let owner = create_test_user(db, "authority-owner").await;
    let moderator = create_test_user(db, "authority-mod").await;
    let admin = create_test_user(db, "authority-admin").await;
    let piece = hang(db, owner.id, "authority-rating").await;
    // Staff can mark their own art; the community self-vote rule is separate.
    mark(
        db,
        piece,
        owner.id,
        StaffAuthority::Moderator,
        Some(ArtContentRating::Nsfw),
    )
    .await;
    mark(
        db,
        piece,
        moderator.id,
        StaffAuthority::Moderator,
        Some(ArtContentRating::Sfw),
    )
    .await;
    let mut client = db.get().await.unwrap();
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, owner.id)
            .await
            .unwrap()
            .unwrap()
            .determination()
            .0,
        ArtContentRating::Nsfw
    );
    mark(
        db,
        piece,
        admin.id,
        StaffAuthority::Admin,
        Some(ArtContentRating::Sfw),
    )
    .await;
    client
        .execute(
            "UPDATE users SET is_admin = false, is_moderator = false WHERE id = $1",
            &[&admin.id],
        )
        .await
        .unwrap();
    client
        .execute("DELETE FROM users WHERE id = $1", &[&admin.id])
        .await
        .unwrap();
    let marks = ArtboardPieceRating::staff_marks(&client, piece)
        .await
        .unwrap();
    assert!(marks.iter().any(|mark| mark.actor_user_id == admin.id
        && mark.authority == StaffAuthority::Admin
        && mark.username.is_none()));
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, owner.id)
            .await
            .unwrap()
            .unwrap()
            .determination(),
        (ArtContentRating::Sfw, RatingSource::Admin)
    );
    // The deleted admin's mark is still removable, by its actor id.
    let tx = client.transaction().await.unwrap();
    assert_eq!(
        ArtboardPieceRating::remove_staff_mark(&tx, piece, admin.id)
            .await
            .unwrap(),
        RemoveMarkOutcome::Removed {
            owner: owner.id,
            authority: StaffAuthority::Admin
        }
    );
    tx.commit().await.unwrap();
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, owner.id)
            .await
            .unwrap()
            .unwrap()
            .determination(),
        (ArtContentRating::Nsfw, RatingSource::Moderator),
        "the moderator tie decides again once the admin mark is gone"
    );
    let tx = client.transaction().await.unwrap();
    assert_eq!(
        ArtboardPieceRating::remove_staff_mark(&tx, piece, moderator.id)
            .await
            .unwrap(),
        RemoveMarkOutcome::Removed {
            owner: owner.id,
            authority: StaffAuthority::Moderator
        }
    );
    assert_eq!(
        ArtboardPieceRating::remove_staff_mark(&tx, piece, moderator.id)
            .await
            .unwrap(),
        RemoveMarkOutcome::NoMark
    );
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn concurrent_votes_are_counted_once_and_removed_pieces_refuse_writes() {
    let test_db = test_db().await;
    let db = &test_db.db;
    let owner = create_test_user(db, "concurrent-rating-owner").await;
    let fan = create_test_user(db, "concurrent-rating-fan").await;
    let fan2 = create_test_user(db, "concurrent-rating-fan2").await;
    let piece = hang(db, owner.id, "concurrent-rating").await;
    let outcomes = tokio::join!(
        try_vote(db, piece, fan.id, Some(ArtContentRating::Nsfw)),
        try_vote(db, piece, fan.id, Some(ArtContentRating::Nsfw)),
        try_vote(db, piece, fan2.id, Some(ArtContentRating::Nsfw))
    );
    assert_eq!(
        outcomes,
        (VoteOutcome::Saved, VoteOutcome::Saved, VoteOutcome::Saved)
    );
    let client = db.get().await.unwrap();
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, fan.id)
            .await
            .unwrap()
            .unwrap()
            .nsfw_votes,
        2
    );
    ArtboardPiece::remove(&client, piece).await.unwrap();
    assert_eq!(
        try_vote(db, piece, fan.id, None).await,
        VoteOutcome::NotFound
    );
    assert!(
        ArtboardPieceRating::read(&client, piece, fan.id)
            .await
            .unwrap()
            .is_none()
    );
}
