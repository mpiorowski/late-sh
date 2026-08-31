use crate::{
    models::{
        chips::{INITIAL_CHIP_BALANCE, UserChips},
        media_queue_item::{
            MediaQueueItem, SONG_QUEUE_MAX_PAID_PER_DAY, SONG_QUEUE_REWARD_CHIPS, SongQueueReward,
        },
    },
    test_utils::{create_test_user, test_db},
};
use chrono::Utc;

/// Play a queued row out the way the booth does, so the track leaves the
/// active set and can be brought back. `idx_media_queue_active_track` holds a
/// track once, and only a `playing` row can be marked played.
async fn play_out(client: &tokio_postgres::Client, item_id: uuid::Uuid) {
    MediaQueueItem::mark_playing(client, item_id, Utc::now())
        .await
        .expect("mark playing")
        .expect("queued row promoted to playing");
    MediaQueueItem::mark_played(client, item_id, Utc::now())
        .await
        .expect("mark played");
}

/// Every submission pays, and the track is never the question: the same
/// person putting the same song on twice is paid twice. The day's cap is the
/// only gate, so this is the rule that has to hold when someone re-queues
/// from history or plays a favourite again.
#[tokio::test]
async fn every_submission_pays_including_the_same_track_twice() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let bringer = create_test_user(&test_db.db, "song-bringer").await;
    UserChips::ensure(&client, bringer.id).await.expect("chips");

    let (item, reward) = MediaQueueItem::insert_youtube(
        &mut client,
        bringer.id,
        "trackaaaaaa",
        Some("A Track"),
        Some("Channel"),
        Some(120_000),
        false,
    )
    .await
    .expect("queue the track");
    assert_eq!(reward, SongQueueReward::Paid);
    assert_eq!(
        UserChips::ensure(&client, bringer.id)
            .await
            .expect("chips")
            .balance,
        INITIAL_CHIP_BALANCE + SONG_QUEUE_REWARD_CHIPS
    );

    // The track plays out and leaves the queue. Putting it back on is worth
    // the same as putting anything else on.
    play_out(&client, item.id).await;
    let (_, reward) = MediaQueueItem::insert_youtube(
        &mut client,
        bringer.id,
        "trackaaaaaa",
        Some("A Track"),
        Some("Channel"),
        Some(120_000),
        false,
    )
    .await
    .expect("queue it again");
    assert_eq!(reward, SongQueueReward::Paid);
    assert_eq!(
        UserChips::ensure(&client, bringer.id)
            .await
            .expect("chips")
            .balance,
        INITIAL_CHIP_BALANCE + 2 * SONG_QUEUE_REWARD_CHIPS,
        "a repeat is paid like any other track"
    );
}

/// The daily cap is the whole of the gating, and the queue's own rate limit
/// allows ten submissions every five minutes, so this is the only thing
/// standing between the jukebox and a chip printer. Past it the track still
/// queues: the room keeps the music, the submitter just stops being paid.
#[tokio::test]
async fn past_the_daily_cap_a_track_still_queues_and_mints_nothing() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let bringer = create_test_user(&test_db.db, "song-capped-bringer").await;
    UserChips::ensure(&client, bringer.id).await.expect("chips");

    for index in 0..SONG_QUEUE_MAX_PAID_PER_DAY {
        let (item, reward) = MediaQueueItem::insert_youtube(
            &mut client,
            bringer.id,
            &format!("trackpaid{index}"),
            Some("Paid"),
            None,
            Some(60_000),
            false,
        )
        .await
        .expect("queue a track");
        assert_eq!(
            reward,
            SongQueueReward::Paid,
            "track {index} is under the cap"
        );
        play_out(&client, item.id).await;
    }

    let (item, reward) = MediaQueueItem::insert_youtube(
        &mut client,
        bringer.id,
        "trackcapped",
        Some("Past the cap"),
        None,
        Some(60_000),
        false,
    )
    .await
    .expect("queue past the cap");
    assert_eq!(reward, SongQueueReward::DailyCapReached);
    assert_eq!(
        item.status,
        MediaQueueItem::STATUS_QUEUED,
        "the room still gets the track"
    );
    assert_eq!(
        UserChips::ensure(&client, bringer.id)
            .await
            .expect("chips")
            .balance,
        INITIAL_CHIP_BALANCE + SONG_QUEUE_MAX_PAID_PER_DAY * SONG_QUEUE_REWARD_CHIPS,
        "nothing is minted past the cap"
    );
}
