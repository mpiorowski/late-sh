use std::time::Instant;

use tokio::sync::oneshot;

use super::{DailyLoad, Polled};

fn never() -> oneshot::Receiver<Option<u8>> {
    panic!("no load may start here")
}

/// A board with its puzzle starts nothing; one without starts a single load,
/// waits out a failure, and starts the next only once the wait has passed.
#[tokio::test]
async fn a_board_without_a_puzzle_loads_once_and_retries_after_the_wait() {
    let mut load = DailyLoad::after_connect(true);
    assert!(matches!(load.poll(false, never), Polled::Waiting));

    let (tx, rx) = oneshot::channel();
    let mut first = Some(rx);
    assert!(matches!(
        load.poll(true, || first.take().expect("one start")),
        Polled::Waiting
    ));
    assert!(matches!(load.poll(true, never), Polled::Waiting));

    drop(tx);
    assert!(matches!(load.poll(true, never), Polled::Failed));
    assert!(matches!(load.poll(true, never), Polled::Waiting));

    load = DailyLoad::Failed {
        retry_after: Instant::now(),
    };
    let (tx, rx) = oneshot::channel();
    let mut retry = Some(rx);
    load.poll(true, || retry.take().expect("one retry"));
    tx.send(Some(7)).expect("deliver the load");
    assert!(matches!(load.poll(true, never), Polled::Loaded(7)));
    assert!(matches!(load.poll(false, never), Polled::Waiting));
}

/// A connect-time load that failed waits before its first retry, as any
/// failed load does.
#[tokio::test]
async fn a_failed_connect_time_load_waits_before_retrying() {
    let mut load = DailyLoad::<u8>::after_connect(false);
    assert!(matches!(load.poll(true, never), Polled::Waiting));
}
