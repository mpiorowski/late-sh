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

    let listed: Vec<_> = DailyMatch::list_open(&client)
        .await
        .expect("list open")
        .into_iter()
        .map(|challenge| challenge.id)
        .collect();
    assert_eq!(listed, vec![posted[2].id, posted[1].id, posted[0].id]);
}
