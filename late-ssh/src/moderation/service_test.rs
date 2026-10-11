use super::*;
use crate::test_helpers::new_test_db;
use late_core::models::artboard_piece::{HangOutcome, HangParams};
use late_core::test_utils::create_test_user;

fn service(db: Db) -> ModerationService {
    let (events, _) = broadcast::channel(16);
    ModerationService::new(
        db,
        ModerationSessionEffects::default(),
        events,
        ModerationInfra::default(),
    )
}

async fn hang(db: &Db, owner: Uuid) -> Uuid {
    let client = db.get().await.unwrap();
    match ArtboardPiece::hang(
        &client,
        HangParams {
            user_id: owner,
            title: "classified art".into(),
            width: 12,
            height: 4,
            canvas: json!({}),
            provenance: json!({}),
            glyph_count: 40,
            own_share_percent: 100,
            content_hash: format!("staff-classification-{}", Uuid::new_v4()),
        },
    )
    .await
    .unwrap()
    {
        HangOutcome::Hung(piece) => piece.id,
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn artboard_staff_marks_enforce_authority_self_marks_and_targeted_removal() {
    let test_db = new_test_db().await;
    let db = &test_db.db;
    let moderator = create_test_user(db, "rating-moderator").await;
    let admin = create_test_user(db, "rating-administrator").await;
    let admin2 = create_test_user(db, "rating-administrator2").await;
    let regular = create_test_user(db, "rating-regular").await;
    let client = db.get().await.unwrap();
    client
        .execute(
            "UPDATE users SET is_moderator = true WHERE id = $1",
            &[&moderator.id],
        )
        .await
        .unwrap();
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = ANY($1)",
            &[&vec![admin.id, admin2.id]],
        )
        .await
        .unwrap();
    let piece = hang(db, moderator.id).await;
    let prefix = piece.to_string();
    let svc = service(db.clone());
    let mod_permissions = Permissions::new(false, true);
    let admin_permissions = Permissions::new(true, false);
    assert!(
        svc.run_command(
            regular.id,
            admin_permissions,
            &format!("artboard safety sfw {prefix}")
        )
        .await
        .is_err(),
        "stale/spoofed session permissions cannot grant mark authority"
    );
    svc.run_command(
        moderator.id,
        mod_permissions,
        &format!("artboard safety nsfw {prefix} mine"),
    )
    .await
    .unwrap();
    svc.run_command(
        admin.id,
        admin_permissions,
        &format!("artboard safety admin sfw {prefix} reviewed"),
    )
    .await
    .unwrap();
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, regular.id)
            .await
            .unwrap()
            .unwrap()
            .determination()
            .0,
        ArtContentRating::Sfw
    );
    svc.run_command(
        admin2.id,
        admin_permissions,
        &format!("artboard safety admin nsfw {prefix}"),
    )
    .await
    .unwrap();
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, regular.id)
            .await
            .unwrap()
            .unwrap()
            .determination()
            .0,
        ArtContentRating::Nsfw
    );
    svc.run_command(
        admin2.id,
        admin_permissions,
        &format!("artboard safety admin none {prefix}"),
    )
    .await
    .unwrap();
    assert!(
        svc.run_command(
            moderator.id,
            mod_permissions,
            &format!("artboard safety none {prefix} by @{}", moderator.username)
        )
        .await
        .is_err()
    );
    assert!(
        svc.run_command(
            admin.id,
            admin_permissions,
            &format!("artboard safety none {prefix} by @{}", admin2.username)
        )
        .await
        .is_err()
    );
    svc.run_command(
        admin.id,
        admin_permissions,
        &format!(
            "artboard safety none {prefix} by @{} cleanup",
            moderator.username
        ),
    )
    .await
    .unwrap();
    svc.run_command(
        admin.id,
        admin_permissions,
        &format!("artboard safety admin none {prefix}"),
    )
    .await
    .unwrap();
    let inspection = svc
        .run_command(
            moderator.id,
            mod_permissions,
            &format!("artboard safety view {prefix}"),
        )
        .await
        .unwrap();
    assert_eq!(inspection[0], format!("Art id: {piece}"));
    assert_eq!(inspection[1], "SFW (unmarked)");
    let count: i64 = client
        .query_one(
            "SELECT count(*) FROM moderation_audit_log WHERE target_kind = 'artboard_piece'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(count, 6);
    // The target mark's stored authority matters even after a promotion.
    svc.run_command(
        moderator.id,
        mod_permissions,
        &format!("artboard safety nsfw {prefix}"),
    )
    .await
    .unwrap();
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = $1",
            &[&moderator.id],
        )
        .await
        .unwrap();
    svc.run_command(
        admin.id,
        admin_permissions,
        &format!("artboard safety none {prefix} by {}", moderator.id),
    )
    .await
    .unwrap();
    svc.run_command(
        admin.id,
        admin_permissions,
        &format!("artboard safety admin nsfw {prefix}"),
    )
    .await
    .unwrap();
    client
        .execute(
            "UPDATE users SET is_admin = false WHERE id = $1",
            &[&admin.id],
        )
        .await
        .unwrap();
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, regular.id)
            .await
            .unwrap()
            .unwrap()
            .determination()
            .1,
        late_core::models::artboard_piece_rating::RatingSource::Admin
    );
    assert!(
        svc.run_command(
            admin.id,
            admin_permissions,
            &format!("artboard safety admin none {prefix}")
        )
        .await
        .is_err()
    );
    // The demoted admin's mark still decides the verdict; another admin
    // takes it off, and the piece falls back to unmarked.
    svc.run_command(
        admin2.id,
        admin_permissions,
        &format!(
            "artboard safety none {prefix} by @{} demoted",
            admin.username
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        ArtboardPieceRating::read(&client, piece, regular.id)
            .await
            .unwrap()
            .unwrap()
            .determination()
            .1,
        late_core::models::artboard_piece_rating::RatingSource::Default
    );
    let unmark: serde_json::Value = client
        .query_one(
            "SELECT metadata FROM moderation_audit_log
             WHERE action = 'artboard_unmark_by' ORDER BY created DESC LIMIT 1",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(unmark["authority"], "admin");
    assert_eq!(unmark["marked_by"], admin.id.to_string());
}

#[tokio::test]
async fn artboard_safety_admin_mode_is_explicit_and_clearing_is_tier_scoped() {
    let test_db = new_test_db().await;
    let db = &test_db.db;
    let admin = create_test_user(db, "safety-admin").await;
    let moderator = create_test_user(db, "safety-moderator").await;
    let client = db.get().await.unwrap();
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = $1",
            &[&admin.id],
        )
        .await
        .unwrap();
    client
        .execute(
            "UPDATE users SET is_moderator = true WHERE id = $1",
            &[&moderator.id],
        )
        .await
        .unwrap();
    let piece = hang(db, admin.id).await;
    let svc = service(db.clone());
    let admin_permissions = Permissions::new(true, false);
    for permissions in [Permissions::new(false, true), admin_permissions] {
        assert!(
            svc.run_command(
                moderator.id,
                permissions,
                &format!("artboard safety admin sfw {piece}")
            )
            .await
            .is_err(),
            "a moderator cannot use admin mode, even with stale session permissions"
        );
    }
    for (mode, expected) in [
        (
            "nsfw",
            Some((StaffAuthority::Moderator, ArtContentRating::Nsfw)),
        ),
        (
            "admin sfw",
            Some((StaffAuthority::Admin, ArtContentRating::Sfw)),
        ),
        ("none", Some((StaffAuthority::Admin, ArtContentRating::Sfw))),
        ("admin none", None),
        (
            "admin nsfw",
            Some((StaffAuthority::Admin, ArtContentRating::Nsfw)),
        ),
        (
            "sfw",
            Some((StaffAuthority::Moderator, ArtContentRating::Sfw)),
        ),
        (
            "admin none",
            Some((StaffAuthority::Moderator, ArtContentRating::Sfw)),
        ),
        ("none", None),
    ] {
        svc.run_command(
            admin.id,
            admin_permissions,
            &format!("artboard safety {mode} {piece}"),
        )
        .await
        .unwrap();
        let marks = ArtboardPieceRating::staff_marks(&client, piece)
            .await
            .unwrap();
        if let Some((authority, rating)) = expected {
            assert_eq!(marks.len(), 1, "{mode}");
            assert_eq!(marks[0].authority, authority, "{mode}");
            assert_eq!(marks[0].rating, rating, "{mode}");
        } else {
            assert!(marks.is_empty(), "{mode}");
        }
    }
}

#[test]
fn artboard_safety_review_table_aligns_unicode_cells() {
    let admin_piece = ArtSafetyPiece {
        id: Uuid::parse_str("4e161f73-5a6c-4000-8000-000000000001").unwrap(),
        title: "09 X Cross".into(),
        username: "绘师绘师绘师".into(),
        splash_on: None,
        summary: ContentRatingSummary {
            admin_sfw: 1,
            admin_nsfw: 1,
            ..Default::default()
        },
    };
    let community_piece = ArtSafetyPiece {
        id: Uuid::parse_str("0e77a39a-eadc-4000-8000-000000000002").unwrap(),
        title: "03 Rings ◯".into(),
        username: "art_artist3".into(),
        splash_on: None,
        summary: ContentRatingSummary {
            nsfw_votes: 2,
            ..Default::default()
        },
    };
    let lines = artboard_safety_review_table(&[
        (&admin_piece, SafetyReviewReason::StaffDisagree),
        (
            &community_piece,
            SafetyReviewReason::CommunityNsfwUnreviewed,
        ),
    ]);
    let separator_columns = |line: &str| {
        line.match_indices('|')
            .map(|(index, _)| Line::from(&line[..index]).width())
            .collect::<Vec<_>>()
    };
    let columns = separator_columns(&lines[0]);
    assert_eq!(columns.len(), 5);
    for line in &lines[1..] {
        assert_eq!(separator_columns(line), columns, "{line}");
    }
    assert_eq!(
        lines[2].split('|').map(str::trim).collect::<Vec<_>>(),
        [
            "4e161f73-5a6c",
            "NSFW",
            "admin",
            "staff disagree",
            "绘师绘师绘师",
            "09 X Cross"
        ]
    );
    assert_eq!(
        lines[3].split('|').map(str::trim).collect::<Vec<_>>(),
        [
            "0e77a39a-eadc",
            "NSFW",
            "commu.",
            "community NS; no staff",
            "art_artist3",
            "03 Rings ◯"
        ]
    );
}

#[tokio::test]
async fn artboard_safety_view_summarizes_filters_and_inspects_without_writing() {
    let test_db = new_test_db().await;
    let db = &test_db.db;
    let admin = create_test_user(db, "safety-reader").await;
    let artist = create_test_user(db, "safety-artist").await;
    let other = create_test_user(db, "safety-other-artist").await;
    let voter = create_test_user(db, "safety-voter").await;
    let client = db.get().await.unwrap();
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = $1",
            &[&admin.id],
        )
        .await
        .unwrap();
    client
        .execute(
            "UPDATE users SET is_moderator = true WHERE id = $1",
            &[&artist.id],
        )
        .await
        .unwrap();
    let svc = service(db.clone());
    let permissions = Permissions::new(true, false);
    let empty = svc
        .run_command(admin.id, permissions, "artboard safety view")
        .await
        .unwrap()
        .join("\n");
    assert!(empty.contains("0 hanging pieces; SFW 0, NSFW 0"));
    assert!(empty.contains("Review candidates: 0"));
    let reported = hang(db, artist.id).await;
    let clean = hang(db, artist.id).await;
    let removed = hang(db, artist.id).await;
    let disputed = hang(db, other.id).await;
    let unrelated = hang(db, other.id).await;
    client
        .execute(
            "UPDATE artboard_pieces SET removed_at = current_timestamp WHERE id = $1",
            &[&removed],
        )
        .await
        .unwrap();
    client
        .execute(
            "UPDATE artboard_pieces SET splash_on = $2 WHERE id = $1",
            &[&reported, &Utc::now().date_naive()],
        )
        .await
        .unwrap();
    ArtboardPieceRating::set_owner_flag(&client, reported, artist.id, true)
        .await
        .unwrap();
    for user_id in [other.id, voter.id] {
        ArtboardPieceRating::set_vote(&client, reported, user_id, Some(ArtContentRating::Nsfw))
            .await
            .unwrap();
    }
    svc.run_command(
        artist.id,
        Permissions::new(false, true),
        &format!("artboard safety nsfw {disputed}"),
    )
    .await
    .unwrap();
    svc.run_command(
        admin.id,
        permissions,
        &format!("artboard safety sfw {disputed} reviewed"),
    )
    .await
    .unwrap();
    let summary = svc
        .run_command(admin.id, permissions, "artboard safety view")
        .await
        .unwrap()
        .join("\n");
    assert!(summary.contains("4 hanging pieces; SFW 2, NSFW 2"));
    assert!(summary.contains("Today's splash:"));
    assert!(summary.contains("Review candidates: 2"));
    assert!(summary.contains("staff disagree"));
    assert!(summary.contains("owner NS; no staff"));
    assert!(summary.contains("art id") && summary.contains("art title"));
    let disputed_row = summary
        .lines()
        .find(|line| line.starts_with(&disputed.to_string()[..13]))
        .unwrap();
    let disputed_cells: Vec<_> = disputed_row.split('|').map(str::trim).collect();
    assert_eq!(
        disputed_cells,
        [
            &disputed.to_string()[..13],
            "NSFW",
            "mod",
            "staff disagree",
            &other.username,
            "classified art"
        ]
    );
    assert!(!summary.contains(&removed.to_string()[..13]));
    let owned = svc
        .run_command(
            admin.id,
            permissions,
            &format!("artboard safety view @{}", artist.username),
        )
        .await
        .unwrap()
        .join("\n");
    assert!(owned.contains("2 hanging pieces"));
    assert!(owned.contains(&reported.to_string()[..13]));
    assert!(owned.contains(&clean.to_string()[..13]));
    assert!(owned.contains("votes SFW 0 / NSFW 2"));
    assert!(!owned.contains(&disputed.to_string()[..13]));
    assert!(!owned.contains(&unrelated.to_string()[..13]));
    assert!(!owned.contains(&removed.to_string()[..13]));
    for id in [disputed.to_string(), disputed.to_string()[..13].to_owned()] {
        let detailed = svc
            .run_command(admin.id, permissions, &format!("artboard safety view {id}"))
            .await
            .unwrap()
            .join("\n");
        assert!(detailed.contains("NSFW (moderator marks)"));
        assert!(
            detailed
                .lines()
                .any(|line| line == format!("Art id: {disputed}"))
        );
        assert!(detailed.contains("Moderators SFW 1 / NSFW 1"));
        assert!(detailed.contains("reviewed"));
    }
    // The per-piece record names the community voters, for staff only.
    let reported_record = svc
        .run_command(
            admin.id,
            permissions,
            &format!("artboard safety view {reported}"),
        )
        .await
        .unwrap();
    let mut voters = [
        format!("@{}", other.username),
        format!("@{}", voter.username),
    ];
    voters.sort_by_key(|name| name.to_lowercase());
    assert!(reported_record.contains(&format!("Voted NSFW: {}", voters.join(", "))));
    assert!(
        !reported_record
            .iter()
            .any(|line| line.starts_with("Voted SFW"))
    );
    assert!(
        svc.run_command(
            voter.id,
            Permissions::new(false, false),
            "artboard safety view"
        )
        .await
        .is_err()
    );
    assert!(
        svc.run_command(
            admin.id,
            permissions,
            &format!("artboard safety view {removed}")
        )
        .await
        .is_err()
    );
    assert!(
        svc.run_command(
            admin.id,
            permissions,
            "artboard safety view @missing-artist"
        )
        .await
        .is_err()
    );
    let audit_count: i64 = client
        .query_one("SELECT count(*) FROM moderation_audit_log", &[])
        .await
        .unwrap()
        .get(0);
    assert_eq!(
        audit_count, 2,
        "view commands must not create audit mutations"
    );
}

#[tokio::test]
async fn artboard_mark_and_audit_are_atomic() {
    let test_db = new_test_db().await;
    let db = &test_db.db;
    let actor = create_test_user(db, "atomic-rating-admin").await;
    let client = db.get().await.unwrap();
    client
        .execute(
            "UPDATE users SET is_admin = true WHERE id = $1",
            &[&actor.id],
        )
        .await
        .unwrap();
    let piece = hang(db, actor.id).await;
    client
        .batch_execute(
            "CREATE FUNCTION refuse_art_audit() RETURNS trigger LANGUAGE plpgsql AS $$
        BEGIN RAISE EXCEPTION 'audit unavailable'; END $$;
        CREATE TRIGGER refuse_art_audit BEFORE INSERT ON moderation_audit_log
        FOR EACH ROW EXECUTE FUNCTION refuse_art_audit();",
        )
        .await
        .unwrap();
    let svc = service(db.clone());
    assert!(
        svc.run_command(
            actor.id,
            Permissions::new(true, false),
            &format!("artboard safety admin nsfw {piece}")
        )
        .await
        .is_err()
    );
    assert!(
        ArtboardPieceRating::staff_marks(&client, piece)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        !ArtboardPieceRating::read(&client, piece, actor.id)
            .await
            .unwrap()
            .unwrap()
            .determination()
            .0
            .is_nsfw()
    );
}
