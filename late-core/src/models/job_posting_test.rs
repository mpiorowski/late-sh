use chrono::{Duration, NaiveDate, Utc};
use tokio_postgres::Client;

use crate::models::job_posting::{
    FetchedPosting, JobPosting, JobPressRun, JobRead, JobSource, JobStatus, PressCounts,
    RemoteKind, Settle, Upsert,
};
use crate::test_utils::test_db;

const MAX_ATTEMPTS: i32 = 3;

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, d).unwrap()
}

fn fetched(source: JobSource, external_id: &str, posted_day: u32) -> FetchedPosting {
    FetchedPosting {
        source,
        external_id: external_id.to_string(),
        url: format!("https://example.com/{external_id}"),
        raw: format!("posting {external_id}"),
        posted_at: day(posted_day).and_hms_opt(12, 0, 0).unwrap().and_utc(),
        remote_kind: None,
        regions: Vec::new(),
        dropped: false,
    }
}

fn read(company: &str, tags: &[&str]) -> JobRead {
    JobRead {
        url: "https://example.com/apply".to_string(),
        company: company.to_string(),
        title: "Backend Engineer".to_string(),
        remote_kind: RemoteKind::Regions,
        regions: vec!["EU".to_string()],
        tags: tags.iter().map(|tag| tag.to_string()).collect(),
        pay: String::new(),
        excerpt: "Builds the backend.".to_string(),
    }
}

async fn status_of(client: &Client, source: JobSource, external_id: &str) -> JobStatus {
    JobPosting::find_by_external_id(client, source, external_id)
        .await
        .expect("find")
        .expect("row")
        .status
}

#[tokio::test]
async fn a_repeat_fetch_stamps_the_row_and_leaves_its_status_alone() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let posting = fetched(JobSource::Hn, "upsert-1", 1);

    let first = JobPosting::upsert_fetched(&client, &posting)
        .await
        .expect("insert");
    assert_eq!(first, Upsert::Inserted);
    let row = JobPosting::find_by_external_id(&client, JobSource::Hn, "upsert-1")
        .await
        .expect("find")
        .expect("row");
    JobPosting::settle(&client, row.id, Settle::Dropped)
        .await
        .expect("settle");

    let again = JobPosting::upsert_fetched(&client, &posting)
        .await
        .expect("upsert");
    assert_eq!(again, Upsert::Seen);
    let row = JobPosting::find_by_external_id(&client, JobSource::Hn, "upsert-1")
        .await
        .expect("find")
        .expect("row");
    assert_eq!(row.status, JobStatus::Dropped);
    assert_eq!(row.raw, "", "the source text goes with the settle");
    assert!(row.last_seen >= row.first_seen);

    // A source-side tombstone is born dropped and keeps no text.
    let tombstone = FetchedPosting {
        dropped: true,
        ..fetched(JobSource::Jobicy, "trust-not-rust", 1)
    };
    JobPosting::upsert_fetched(&client, &tombstone)
        .await
        .expect("insert tombstone");
    let row = JobPosting::find_by_external_id(&client, JobSource::Jobicy, "trust-not-rust")
        .await
        .expect("find")
        .expect("row");
    assert_eq!(row.status, JobStatus::Dropped);
    assert_eq!(row.raw, "");
    assert!(
        JobPosting::list_pending(&client, MAX_ATTEMPTS, 100)
            .await
            .expect("pending")
            .iter()
            .all(|row| row.external_id != "trust-not-rust")
    );
}

#[tokio::test]
async fn a_read_settles_the_row_and_failures_count_up_to_the_cap() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    for id in ["read-a", "read-b", "read-c"] {
        JobPosting::upsert_fetched(&client, &fetched(JobSource::Wwr, id, 2))
            .await
            .expect("insert");
    }
    let pending = JobPosting::list_pending(&client, MAX_ATTEMPTS, 100)
        .await
        .expect("pending");
    let ids: Vec<&str> = pending.iter().map(|row| row.external_id.as_str()).collect();
    assert!(ids.contains(&"read-a") && ids.contains(&"read-b") && ids.contains(&"read-c"));
    let a = pending.iter().find(|row| row.external_id == "read-a").unwrap();
    let b = pending.iter().find(|row| row.external_id == "read-b").unwrap();
    let c = pending.iter().find(|row| row.external_id == "read-c").unwrap();

    JobPosting::settle(&client, a.id, Settle::Active(read("Acme", &["rust"]), day(22)))
        .await
        .expect("settle active");
    JobPosting::settle(&client, b.id, Settle::Dead(read("Gone", &["go"])))
        .await
        .expect("settle dead");
    for _ in 0..MAX_ATTEMPTS {
        JobPosting::note_read_failure(&client, c.id)
            .await
            .expect("note failure");
    }

    let active = JobPosting::list_active(&client, 100).await.expect("active");
    let a = active
        .iter()
        .find(|row| row.external_id == "read-a")
        .expect("read-a on the shelf");
    assert_eq!(a.company, "Acme");
    assert_eq!(a.tags, vec!["rust"]);
    assert_eq!(a.remote_kind, Some(RemoteKind::Regions));
    assert_eq!(a.released_on, Some(day(22)));
    assert_eq!(a.raw, "");
    assert_eq!(status_of(&client, JobSource::Wwr, "read-b").await, JobStatus::Dead);

    // At the cap the row stops being listed, and the sweep tombstones it.
    assert!(
        JobPosting::list_pending(&client, MAX_ATTEMPTS, 100)
            .await
            .expect("pending")
            .iter()
            .all(|row| row.external_id != "read-c")
    );
    let dropped = JobPosting::drop_exhausted(&client, MAX_ATTEMPTS)
        .await
        .expect("drop exhausted");
    assert_eq!(dropped, 1);
    assert_eq!(
        status_of(&client, JobSource::Wwr, "read-c").await,
        JobStatus::Dropped
    );
}

#[tokio::test]
async fn the_drip_releases_the_oldest_slice_and_expiry_clears_the_shelf() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    // Every test gets its own database, so the counts here are exact.
    for id in ["drip-3", "drip-1", "drip-2"] {
        let posted_day = match id {
            "drip-1" => 1,
            "drip-2" => 2,
            _ => 3,
        };
        JobPosting::upsert_fetched(&client, &fetched(JobSource::Hn, id, posted_day))
            .await
            .expect("insert");
        let row = JobPosting::find_by_external_id(&client, JobSource::Hn, id)
            .await
            .expect("find")
            .expect("row");
        JobPosting::settle(&client, row.id, Settle::Queued(read(id, &["elixir"])))
            .await
            .expect("settle queued");
    }
    let (queued, earliest) = JobPosting::queued(&client, JobSource::Hn)
        .await
        .expect("queued");
    assert_eq!((queued, earliest), (3, Some(day(1))));

    // The two oldest posts go first, whatever order they were fetched in.
    let released = JobPosting::release_slice(&client, JobSource::Hn, 2, day(10))
        .await
        .expect("release");
    assert_eq!(released.len(), 2);
    let on_day = JobPosting::list_released_on(&client, day(10))
        .await
        .expect("released on");
    let mut companies: Vec<&str> = on_day.iter().map(|row| row.company.as_str()).collect();
    companies.sort();
    assert_eq!(companies, vec!["drip-1", "drip-2"]);
    assert!(on_day.iter().all(|row| row.status == JobStatus::Active));
    assert_eq!(
        JobPosting::queued(&client, JobSource::Hn)
            .await
            .expect("queued"),
        (1, Some(day(3)))
    );
    // The shelf lists the newest release first; nothing queued shows.
    let shelf = JobPosting::list_active(&client, 100).await.expect("active");
    assert_eq!(shelf.len(), 2);

    // Thirty days on, the slice leaves the shelf; the tombstone stays.
    let expired = JobPosting::expire_released_before(&client, day(11))
        .await
        .expect("expire");
    assert_eq!(expired, 2);
    assert!(
        JobPosting::list_released_on(&client, day(10))
            .await
            .expect("released on")
            .is_empty()
    );

    // A feed row the feed stopped listing expires by `last_seen`.
    JobPosting::upsert_fetched(&client, &fetched(JobSource::Wwr, "unseen-1", 2))
        .await
        .expect("insert");
    let row = JobPosting::find_by_external_id(&client, JobSource::Wwr, "unseen-1")
        .await
        .expect("find")
        .expect("row");
    JobPosting::settle(&client, row.id, Settle::Active(read("Old", &["go"]), day(22)))
        .await
        .expect("settle");
    let none =
        JobPosting::expire_unseen_since(&client, JobSource::Wwr, Utc::now() - Duration::days(7))
            .await
            .expect("expire unseen");
    assert_eq!(none, 0, "seen just now");
    let some = JobPosting::expire_unseen_since(
        &client,
        JobSource::Wwr,
        Utc::now() + Duration::seconds(1),
    )
    .await
    .expect("expire unseen");
    assert_eq!(some, 1);
    assert_eq!(
        status_of(&client, JobSource::Wwr, "unseen-1").await,
        JobStatus::Expired
    );
}

#[tokio::test]
async fn the_daily_run_is_claimed_once_and_reclaimed_only_when_stale_or_failed() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let run_on = NaiveDate::from_ymd_opt(2031, 1, 15).unwrap();
    let stale_before = Utc::now() - Duration::minutes(90);

    assert!(
        JobPressRun::claim(&client, run_on, stale_before, MAX_ATTEMPTS)
            .await
            .expect("claim")
    );
    assert!(
        !JobPressRun::claim(&client, run_on, stale_before, MAX_ATTEMPTS)
            .await
            .expect("second claim"),
        "a fresh running row belongs to the first replica"
    );
    assert!(
        !JobPressRun::is_finished(&client, run_on, MAX_ATTEMPTS)
            .await
            .expect("finished")
    );

    // A failed run is claimed again until the cap, then it is over.
    JobPressRun::mark_failed(&client, run_on)
        .await
        .expect("mark failed");
    assert!(
        JobPressRun::claim(&client, run_on, stale_before, MAX_ATTEMPTS)
            .await
            .expect("reclaim")
    );
    JobPressRun::mark_failed(&client, run_on)
        .await
        .expect("mark failed");
    assert!(
        JobPressRun::claim(&client, run_on, stale_before, MAX_ATTEMPTS)
            .await
            .expect("reclaim")
    );
    JobPressRun::mark_failed(&client, run_on)
        .await
        .expect("mark failed");
    assert!(
        !JobPressRun::claim(&client, run_on, stale_before, MAX_ATTEMPTS)
            .await
            .expect("claim at cap")
    );
    assert!(
        JobPressRun::is_finished(&client, run_on, MAX_ATTEMPTS)
            .await
            .expect("finished at cap")
    );

    // A stale running claim is taken over.
    let other = NaiveDate::from_ymd_opt(2031, 1, 16).unwrap();
    assert!(
        JobPressRun::claim(&client, other, stale_before, MAX_ATTEMPTS)
            .await
            .expect("claim")
    );
    assert!(
        JobPressRun::claim(&client, other, Utc::now() + Duration::seconds(1), MAX_ATTEMPTS)
            .await
            .expect("take over stale")
    );
    JobPressRun::finish(
        &client,
        other,
        PressCounts {
            fetched: 5,
            read: 4,
            released: 2,
            expired: 0,
        },
    )
    .await
    .expect("finish");
    assert!(
        JobPressRun::is_finished(&client, other, MAX_ATTEMPTS)
            .await
            .expect("finished")
    );
    assert!(
        !JobPressRun::claim(&client, other, Utc::now() + Duration::seconds(1), MAX_ATTEMPTS)
            .await
            .expect("done stays done")
    );
}
