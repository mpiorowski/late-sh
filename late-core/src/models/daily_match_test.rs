use crate::models::daily_match::DailyResult;

#[test]
fn every_result_reads_back_from_its_stored_spelling() {
    for result in DailyResult::ALL {
        assert_eq!(DailyResult::parse(result.as_str()).unwrap(), result);
    }
}

#[test]
fn an_unfinished_rows_empty_result_is_not_a_result() {
    let error = DailyResult::parse("").unwrap_err();
    assert_eq!(error.to_string(), "unknown daily match result: \"\"");
}

/// A write that changes no row tells nobody. Every replica's sweeper runs
/// `forfeit_expired` each minute and it almost always matches nothing; a
/// notify for that would have every replica re-read its lobby for no news.
#[tokio::test]
async fn only_a_write_that_changes_a_row_notifies() {
    use crate::models::daily_match::{DAILY_MATCH_CHANGED_CHANNEL, DailyMatch};
    use crate::test_utils::{create_test_user, test_db};
    use std::future::poll_fn;
    use std::time::Duration;
    use tokio_postgres::{AsyncMessage, NoTls};

    const MARKER: &str = "daily_match_test_marker";

    let test_db = test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-notify-challenger").await;
    let client = test_db.db.get().await.expect("db client");

    let cfg = test_db.db.config();
    let mut listener_config = tokio_postgres::Config::new();
    listener_config
        .host(&cfg.host)
        .port(cfg.port)
        .user(&cfg.user)
        .password(&cfg.password)
        .dbname(&cfg.dbname);
    let (listener, mut connection) = listener_config
        .connect(NoTls)
        .await
        .expect("listener connection");
    let listen_sql = format!("LISTEN {DAILY_MATCH_CHANGED_CHANNEL}; LISTEN {MARKER};");
    let listen = listener.batch_execute(&listen_sql);
    tokio::pin!(listen);
    let mut listen_done = false;
    while !listen_done {
        tokio::select! {
            result = &mut listen, if !listen_done => {
                result.expect("listen");
                listen_done = true;
            }
            message = poll_fn(|cx| connection.poll_message(cx)) => {
                let _ = message.expect("connection open").expect("connection ok");
            }
        }
    }
    // Notifications arrive in commit order, so the channel of the next one
    // says which write sent it.
    let mut next_channel = async || loop {
        let message = tokio::time::timeout(
            Duration::from_secs(5),
            poll_fn(|cx| connection.poll_message(cx)),
        )
        .await
        .expect("a notification")
        .expect("connection open")
        .expect("connection ok");
        if let AsyncMessage::Notification(notification) = message {
            return notification.channel().to_string();
        }
    };

    let forfeited = DailyMatch::forfeit_expired(&client)
        .await
        .expect("sweep an empty table");
    assert!(forfeited.is_empty());
    client
        .batch_execute(&format!("NOTIFY {MARKER};"))
        .await
        .expect("marker");
    assert_eq!(
        next_channel().await,
        MARKER,
        "the sweep changed nothing, so the marker is the first news"
    );

    DailyMatch::create_challenge(&client, DailyMatch::GAME_KIND_CHESS, challenger.id)
        .await
        .expect("post a challenge");
    assert_eq!(next_channel().await, DAILY_MATCH_CHANGED_CHANNEL);
}

/// The lobby lists what just went up at the top: newest challenge first.
#[tokio::test]
async fn open_challenges_list_newest_first() {
    use crate::models::daily_match::DailyMatch;
    use crate::test_utils::{create_test_user, test_db};

    let test_db = test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-open-order").await;
    let client = test_db.db.get().await.expect("db client");

    let mut posted = Vec::new();
    for game in ["chess", "gin", "8ball"] {
        posted.push(
            DailyMatch::create_challenge(&client, game, challenger.id)
                .await
                .expect("create challenge"),
        );
    }
    // Stamp them an hour apart, oldest first, so the order does not hang on
    // statement timing.
    for (hours_ago, challenge) in [(3i32, &posted[0]), (2, &posted[1]), (1, &posted[2])] {
        client
            .execute(
                "UPDATE daily_matches SET created = now() - make_interval(hours => $1) WHERE id = $2",
                &[&hours_ago, &challenge.id],
            )
            .await
            .expect("stamp created");
    }

    let lobby: Vec<_> = DailyMatch::list_lobby(&client)
        .await
        .expect("list lobby")
        .into_iter()
        .map(|challenge| challenge.id)
        .collect();
    assert_eq!(lobby, vec![posted[2].id, posted[1].id, posted[0].id]);
}

#[tokio::test]
async fn lobby_preserves_active_and_result_order_and_excludes_seen_old_and_cancelled() {
    use crate::models::daily_match::{DailyMatch, DailyResult};
    use crate::test_utils::{create_test_user, test_db};
    use chrono::{Duration, Utc};

    let test_db = test_db().await;
    let challenger = create_test_user(&test_db.db, "daily-lobby-challenger").await;
    let opponent = create_test_user(&test_db.db, "daily-lobby-opponent").await;
    let client = test_db.db.get().await.expect("db client");
    let state = serde_json::json!({"revision": 0});

    // Active matches by deadline: claimed with the far deadline first so the
    // nearest deadline sorting cannot hang on insertion order.
    let mut active = Vec::new();
    for hours in [2, 1] {
        let challenge = DailyMatch::create_challenge(&client, "chess", challenger.id)
            .await
            .expect("create challenge");
        let claimed = DailyMatch::claim(
            &client,
            challenge.id,
            opponent.id,
            challenger.id,
            Utc::now() + Duration::hours(hours),
            &state,
        )
        .await
        .expect("claim")
        .expect("challenge was open");
        active.push(claimed.id);
    }

    // Finished matches: two unseen, one seen by both players, one seen by
    // the challenger only, and one unseen but older than the 30-day window.
    let mut finished = Vec::new();
    for (days_ago, seen_by) in [
        (2, Vec::new()),
        (1, Vec::new()),
        (0, vec![challenger.id, opponent.id]),
        (0, vec![challenger.id]),
        (31, Vec::new()),
    ] {
        let challenge = DailyMatch::create_challenge(&client, "chess", challenger.id)
            .await
            .expect("create challenge");
        DailyMatch::claim(
            &client,
            challenge.id,
            opponent.id,
            challenger.id,
            Utc::now() + Duration::hours(1),
            &state,
        )
        .await
        .expect("claim")
        .expect("challenge was open");
        assert_eq!(
            DailyMatch::finish(
                &client,
                challenge.id,
                Some(challenger.id),
                DailyResult::Resign,
                &state,
                0,
            )
            .await
            .expect("finish"),
            1
        );
        for user_id in seen_by {
            assert_eq!(
                DailyMatch::mark_result_seen(&client, challenge.id, user_id)
                    .await
                    .expect("mark seen"),
                1
            );
        }
        // The finish time is the sort key and the window cutoff; stamp it.
        client
            .execute(
                "UPDATE daily_matches SET updated = now() - make_interval(days => $1) WHERE id = $2",
                &[&days_ago, &challenge.id],
            )
            .await
            .expect("stamp finished");
        finished.push(challenge.id);
    }

    let cancelled = DailyMatch::create_challenge(&client, "chess", challenger.id)
        .await
        .expect("create challenge");
    assert_eq!(
        DailyMatch::cancel_challenge(&client, cancelled.id, challenger.id)
            .await
            .expect("cancel"),
        1
    );

    let lobby = DailyMatch::list_lobby(&client).await.expect("list lobby");
    let listed_active: Vec<_> = lobby
        .iter()
        .filter(|row| row.status == DailyMatch::STATUS_ACTIVE)
        .map(|row| row.id)
        .collect();
    let listed_finished: Vec<_> = lobby
        .iter()
        .filter(|row| row.status == DailyMatch::STATUS_FINISHED)
        .map(|row| row.id)
        .collect();
    assert_eq!(listed_active, vec![active[1], active[0]]);
    // Newest finish first. A result only one player has seen stays listed;
    // one both players saw, and one older than 30 days, do not.
    assert_eq!(listed_finished, vec![finished[3], finished[1], finished[0]]);
    assert_eq!(
        lobby.len(),
        listed_active.len() + listed_finished.len(),
        "cancelled challenges never reach the lobby"
    );
}
