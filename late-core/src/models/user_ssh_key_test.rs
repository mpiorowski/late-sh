use crate::models::user::{RightSidebarMode, RoomListMode};
use crate::models::user_ssh_key::{
    KeyAudio, KeyLayout, UserSshKey, extract_key_audio, extract_key_layout,
};
use crate::test_utils::{create_test_user, test_db};
use serde_json::json;

/// The stored rail layout, read the way bootstrap reads it.
async fn stored_layout(
    client: &tokio_postgres::Client,
    user_id: uuid::Uuid,
    fingerprint: &str,
) -> anyhow::Result<Option<KeyLayout>> {
    let key = UserSshKey::find_by_fingerprint(client, user_id, fingerprint).await?;
    Ok(key.and_then(|key| extract_key_layout(&key.settings)))
}

fn phone_layout() -> KeyLayout {
    KeyLayout {
        room_list_mode: RoomListMode::Off,
        right_sidebar_mode: RightSidebarMode::Auto,
    }
}

fn muted_at_sixty() -> KeyAudio {
    KeyAudio {
        muted: true,
        volume_percent: 60,
    }
}

#[test]
fn stored_audio_round_trips_and_partial_blobs_fall_back() {
    let audio = muted_at_sixty();
    assert_eq!(extract_key_audio(&audio.to_value()), Some(audio));

    // Nothing stored, or only half a pair, means "use the caller's default".
    assert_eq!(extract_key_audio(&json!({})), None);
    assert_eq!(extract_key_audio(&json!({"audio_muted": true})), None);
    assert_eq!(
        extract_key_audio(&json!({"audio_volume_percent": 60})),
        None
    );
    // A volume outside the 0-100 range is a mangled blob, not a clamp target.
    assert_eq!(
        extract_key_audio(&json!({"audio_muted": true, "audio_volume_percent": 140})),
        None
    );
}

#[tokio::test]
async fn stored_audio_is_per_device_and_leaves_the_rail_layout_alone() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = create_test_user(&test_db.db, "audioowner").await;

    UserSshKey::ensure(&client, owner.id, "SHA256:laptop")
        .await
        .expect("laptop key");
    UserSshKey::ensure(&client, owner.id, "SHA256:desktop")
        .await
        .expect("desktop key");

    // A device that has never reported audio has none stored, so the caller
    // falls back rather than assuming "unmuted".
    assert_eq!(
        UserSshKey::audio_for(&client, owner.id, "SHA256:laptop")
            .await
            .expect("laptop audio"),
        None
    );

    UserSshKey::set_layout(&client, owner.id, "SHA256:laptop", phone_layout())
        .await
        .expect("store laptop layout");
    UserSshKey::set_audio(&client, owner.id, "SHA256:laptop", muted_at_sixty())
        .await
        .expect("store laptop audio");

    assert_eq!(
        UserSshKey::audio_for(&client, owner.id, "SHA256:laptop")
            .await
            .expect("laptop audio"),
        Some(muted_at_sixty())
    );
    // Muting the laptop must not silence the desktop.
    assert_eq!(
        UserSshKey::audio_for(&client, owner.id, "SHA256:desktop")
            .await
            .expect("desktop audio"),
        None
    );
    // Audio and layout share one settings blob, so writing either must merge
    // rather than replace.
    assert_eq!(
        stored_layout(&client, owner.id, "SHA256:laptop")
            .await
            .expect("laptop layout"),
        Some(phone_layout())
    );

    // Another account cannot write onto this key even holding its fingerprint.
    let stranger = create_test_user(&test_db.db, "audiostranger").await;
    assert!(
        UserSshKey::set_audio(
            &client,
            stranger.id,
            "SHA256:laptop",
            KeyAudio {
                muted: false,
                volume_percent: 5,
            }
        )
        .await
        .is_err()
    );
    assert_eq!(
        UserSshKey::audio_for(&client, owner.id, "SHA256:laptop")
            .await
            .expect("laptop audio"),
        Some(muted_at_sixty())
    );
}

#[test]
fn stored_layout_round_trips_and_partial_blobs_inherit() {
    let layout = phone_layout();
    assert_eq!(extract_key_layout(&layout.to_value()), Some(layout));

    // Nothing stored, or only half a pair, means "follow the account default".
    assert_eq!(extract_key_layout(&json!({})), None);
    assert_eq!(extract_key_layout(&json!({"room_list_mode": "off"})), None);
    assert_eq!(
        extract_key_layout(&json!({"room_list_mode": "off", "right_sidebar_mode": "sideways"})),
        None
    );
}

#[tokio::test]
async fn ensure_creates_the_key_then_repoints_it_on_reconnect() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let desktop = create_test_user(&test_db.db, "keyowner").await;
    let other = create_test_user(&test_db.db, "keyother").await;

    UserSshKey::ensure(&client, desktop.id, "SHA256:phone")
        .await
        .expect("first sight");
    let key = UserSshKey::find_by_fingerprint(&client, desktop.id, "SHA256:phone")
        .await
        .expect("load key")
        .expect("key exists");
    assert_eq!(key.user_id, desktop.id);
    assert!(key.label.is_none(), "unlabelled until the user names it");

    // Re-ensuring under another account moves the key rather than duplicating it.
    UserSshKey::ensure(&client, other.id, "SHA256:phone")
        .await
        .expect("repoint");
    assert!(
        UserSshKey::find_by_fingerprint(&client, desktop.id, "SHA256:phone")
            .await
            .expect("load key")
            .is_none(),
        "old owner no longer sees the key"
    );
    assert_eq!(
        UserSshKey::list_by_user_id(&client, other.id)
            .await
            .expect("list keys")
            .len(),
        1,
        "one row, not two"
    );
}

#[tokio::test]
async fn layout_is_per_key_and_scoped_to_its_owner() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "keylayout").await;
    let stranger = create_test_user(&test_db.db, "keystranger").await;

    UserSshKey::ensure(&client, user.id, "SHA256:phone")
        .await
        .expect("phone key");
    UserSshKey::ensure(&client, user.id, "SHA256:desktop")
        .await
        .expect("desktop key");

    // A key with nothing stored inherits the account default.
    assert_eq!(
        stored_layout(&client, user.id, "SHA256:desktop")
            .await
            .expect("desktop layout"),
        None
    );

    UserSshKey::set_layout(&client, user.id, "SHA256:phone", phone_layout())
        .await
        .expect("store phone layout");
    assert_eq!(
        stored_layout(&client, user.id, "SHA256:phone")
            .await
            .expect("phone layout"),
        Some(phone_layout()),
        "the phone keeps its own layout"
    );
    assert_eq!(
        stored_layout(&client, user.id, "SHA256:desktop")
            .await
            .expect("desktop layout"),
        None,
        "and does not touch the desktop's"
    );

    // Another account cannot write onto this key even knowing its fingerprint.
    let denied = UserSshKey::set_layout(
        &client,
        stranger.id,
        "SHA256:phone",
        KeyLayout {
            room_list_mode: RoomListMode::On,
            right_sidebar_mode: RightSidebarMode::On,
        },
    )
    .await;
    assert!(denied.is_err(), "cross-account write is refused");
    assert_eq!(
        stored_layout(&client, user.id, "SHA256:phone")
            .await
            .expect("phone layout"),
        Some(phone_layout()),
        "the owner's layout survives"
    );
}

#[tokio::test]
async fn linking_accounts_moves_keys_and_keeps_their_layouts() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let kept = create_test_user(&test_db.db, "keykept").await;
    let abandoned = create_test_user(&test_db.db, "keymerged").await;

    UserSshKey::ensure(&client, kept.id, "SHA256:desktop")
        .await
        .expect("desktop key");
    UserSshKey::ensure(&client, abandoned.id, "SHA256:phone")
        .await
        .expect("phone key");
    UserSshKey::set_layout(&client, abandoned.id, "SHA256:phone", phone_layout())
        .await
        .expect("store phone layout");

    // Linking runs the move inside its own transaction, like `account_link`,
    // which hands the model a `tokio_postgres` transaction rather than a
    // pooled one.
    let inner: &mut tokio_postgres::Client = &mut client;
    let tx = inner.transaction().await.expect("transaction");
    UserSshKey::move_to_user(&tx, abandoned.id, kept.id)
        .await
        .expect("move keys");
    tx.commit().await.expect("commit");

    assert_eq!(
        UserSshKey::list_by_user_id(&client, kept.id)
            .await
            .expect("list keys")
            .len(),
        2,
        "both devices now belong to the kept account"
    );
    assert_eq!(
        stored_layout(&client, kept.id, "SHA256:phone")
            .await
            .expect("phone layout"),
        Some(phone_layout()),
        "the phone's layout survives the link"
    );
}

#[tokio::test]
async fn left_at_is_per_device_served_once_and_scoped_to_its_owner() {
    use chrono::{Duration, SubsecRound, Utc};

    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = create_test_user(&test_db.db, "leftowner").await;
    let stranger = create_test_user(&test_db.db, "leftstranger").await;

    UserSshKey::ensure(&client, owner.id, "SHA256:laptop")
        .await
        .expect("laptop key");
    UserSshKey::ensure(&client, owner.id, "SHA256:desktop")
        .await
        .expect("desktop key");

    // A key that has never ended a session has no mark: the next session
    // starts with no line rather than one at some made-up instant.
    assert_eq!(
        UserSshKey::take_left_at(&client, owner.id, "SHA256:laptop")
            .await
            .expect("laptop left_at"),
        None
    );

    // Postgres keeps microseconds; the stamp is minted here, so truncate it.
    let last_night = (Utc::now() - Duration::hours(9)).trunc_subsecs(6);
    UserSshKey::set_left_at(&client, owner.id, "SHA256:laptop", last_night)
        .await
        .expect("stamp laptop");

    // The desktop did not leave because the laptop did.
    assert_eq!(
        UserSshKey::take_left_at(&client, owner.id, "SHA256:desktop")
            .await
            .expect("desktop left_at"),
        None
    );
    assert_eq!(
        UserSshKey::take_left_at(&client, owner.id, "SHA256:laptop")
            .await
            .expect("laptop left_at"),
        Some(last_night)
    );
    // Taking it spends it: the session that follows a lost write must not
    // inherit this leave as if it were the latest one.
    assert_eq!(
        UserSshKey::take_left_at(&client, owner.id, "SHA256:laptop")
            .await
            .expect("laptop left_at again"),
        None
    );

    // A stale fingerprint under another account can neither read nor write
    // this device's mark.
    assert!(
        UserSshKey::set_left_at(&client, stranger.id, "SHA256:laptop", Utc::now())
            .await
            .is_err()
    );
    UserSshKey::set_left_at(&client, owner.id, "SHA256:laptop", last_night)
        .await
        .expect("stamp laptop again");
    assert_eq!(
        UserSshKey::take_left_at(&client, stranger.id, "SHA256:laptop")
            .await
            .expect("stranger read"),
        None
    );
    assert_eq!(
        UserSshKey::take_left_at(&client, owner.id, "SHA256:laptop")
            .await
            .expect("owner still holds the mark"),
        Some(last_night),
        "a stranger's read must not spend the owner's mark"
    );
}
