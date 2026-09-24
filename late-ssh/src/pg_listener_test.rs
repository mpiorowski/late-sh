use std::cell::Cell;
use std::collections::HashSet;

use tokio::time::{Duration, timeout};

use crate::pg_listener::{Channel, PgListener, Signal, read_until_ok};
use crate::test_helpers::new_test_db;

/// The read after a resync is the only thing that seeds a domain, so a
/// failure retries (paused time skips the backoff) instead of leaving the
/// replica empty until the next write.
#[tokio::test(start_paused = true)]
async fn a_failed_re_read_is_retried_until_it_succeeds() {
    let attempts = Cell::new(0);

    read_until_ok("test", || {
        attempts.set(attempts.get() + 1);
        let attempt = attempts.get();
        async move {
            if attempt < 3 {
                anyhow::bail!("not yet");
            }
            Ok(())
        }
    })
    .await;

    assert_eq!(attempts.get(), 3);
}

#[test]
fn every_channel_round_trips_through_its_unique_postgres_name() {
    let names: HashSet<&str> = Channel::ALL.into_iter().map(Channel::name).collect();
    assert_eq!(names.len(), Channel::ALL.len(), "two channels share a name");

    for channel in Channel::ALL {
        assert_eq!(Channel::parse(channel.name()), Some(channel));
    }
    assert_eq!(Channel::parse("not_a_channel"), None);
}

async fn next_signal(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Signal>) -> Signal {
    timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("signal timeout")
        .expect("listener queue open")
}

/// One connection serves every domain: each subscriber hears a resync once
/// the LISTEN is live, then only the channels it asked for. The pot notify
/// is sent before the crown one on the same connection, so if the crown
/// subscriber were handed pot traffic it would arrive first.
#[tokio::test]
async fn each_subscriber_resyncs_then_hears_only_its_own_channels() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");

    let mut listener = PgListener::new();
    let mut crown_rx = listener.subscribe(&[Channel::CrownChanged]);
    let mut pot_rx = listener.subscribe(&[Channel::PotChanged]);
    let _task = listener.start(test_db.db.config().clone());

    assert_eq!(next_signal(&mut crown_rx).await, Signal::Resync);
    assert_eq!(next_signal(&mut pot_rx).await, Signal::Resync);

    client
        .execute(
            "SELECT pg_notify($1, 'drawn'), pg_notify($2, 'taken')",
            &[&Channel::PotChanged.name(), &Channel::CrownChanged.name()],
        )
        .await
        .expect("notify");

    assert_eq!(
        next_signal(&mut crown_rx).await,
        Signal::Notify {
            channel: Channel::CrownChanged,
            payload: "taken".to_string(),
        }
    );
    assert_eq!(
        next_signal(&mut pot_rx).await,
        Signal::Notify {
            channel: Channel::PotChanged,
            payload: "drawn".to_string(),
        }
    );
}
