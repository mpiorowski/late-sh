use std::collections::HashMap;

use crate::models::arcade_handle::*;
use crate::models::user::User;
use crate::test_utils::{create_test_user, test_db};

#[test]
fn shape_accepts_plain_handles() {
    assert!(handle_shape_valid("mat"));
    assert!(handle_shape_valid("srcrip"));
    assert!(handle_shape_valid("Gnoll_Fan_99"));
    assert!(handle_shape_valid("a2345678901234567890")); // 20 chars
}

#[test]
fn shape_rejects_bad_lengths() {
    assert!(!handle_shape_valid(""));
    assert!(!handle_shape_valid("ab"));
    assert!(!handle_shape_valid("a23456789012345678901")); // 21 chars
}

#[test]
fn shape_rejects_bad_charset_and_leading_char() {
    assert!(!handle_shape_valid("1mat")); // must start with a letter
    assert!(!handle_shape_valid("_mat"));
    assert!(!handle_shape_valid("mat p")); // crawl allows spaces; we don't
    assert!(!handle_shape_valid("mat-p")); // host sanitizers strip hyphens
    assert!(!handle_shape_valid("mat.p"));
    assert!(!handle_shape_valid("måt")); // ascii only
}

#[test]
fn reserved_blocks_fallback_and_hash_shapes() {
    assert!(handle_reserved("late"));
    assert!(handle_reserved("LATE"));
    assert!(handle_reserved("late_0dd47727a40681b9"));
    assert!(handle_reserved("Late_anything"));
    // Names merely containing or resembling it stay claimable.
    assert!(!handle_reserved("latecomer"));
    assert!(!handle_reserved("chocolate"));
}

/// The live strip and the watch show the late.sh username behind a handle:
/// matched case-insensitively and keyed as the caller spelled it, while a
/// handle whose account is gone, or that nobody claimed, has none.
#[tokio::test]
async fn usernames_by_handles_maps_live_accounts_only() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("client");
    let alive = create_test_user(&test_db.db, "handle-alive").await;
    let gone = create_test_user(&test_db.db, "handle-gone").await;
    for (user_id, handle) in [(alive.id, "Wormsong"), (gone.id, "Ghostly")] {
        let outcome = ArcadeHandle::claim(&client, user_id, handle)
            .await
            .expect("claim handle");
        assert_eq!(outcome, ClaimOutcome::Claimed);
    }
    User::delete_by_id(&client, gone.id)
        .await
        .expect("delete user");

    let usernames = ArcadeHandle::usernames_by_handles(
        &client,
        &[
            "WORMSONG".to_string(),
            "Ghostly".to_string(),
            "Unclaimed".to_string(),
        ],
    )
    .await
    .expect("usernames by handles");

    assert_eq!(
        usernames,
        HashMap::from([("WORMSONG".to_string(), alive.username.clone())])
    );
}
