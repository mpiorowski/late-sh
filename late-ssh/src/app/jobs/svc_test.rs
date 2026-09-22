use std::time::Duration;

use chrono::{NaiveDate, TimeZone, Utc};
use late_core::models::job_posting::{
    FetchedPosting, JobPosting, JobRead, JobSource, JobStatus, NewPosting, RemoteKind, Settle,
};
use uuid::Uuid;

use super::svc::{
    JOBS_DRIP_DAYS, JOBS_POSTS_PER_USER, JobsEvent, PostOutcome, PressJob, PressOutcome,
    RetractOutcome, drip_slice, press_due_day, tidy_excerpt,
};
use crate::moderation::policy::Permissions;
use crate::test_helpers::{chat_compose_app, wait_for_render_contains};

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, d).unwrap()
}

#[test]
fn the_press_is_due_at_half_past_eleven_utc_and_catches_up_after_it() {
    let before = Utc.with_ymd_and_hms(2026, 9, 22, 23, 29, 59).unwrap();
    assert_eq!(press_due_day(before), day(21), "still yesterday's run");
    let due = Utc.with_ymd_and_hms(2026, 9, 22, 23, 30, 0).unwrap();
    assert_eq!(press_due_day(due), day(22));
    // A replica back at 00:10 owes the 22nd its run, not the 23rd.
    let late = Utc.with_ymd_and_hms(2026, 9, 23, 0, 10, 0).unwrap();
    assert_eq!(press_due_day(late), day(22));
}

#[test]
fn the_drip_spreads_the_queue_over_the_window_and_flushes_past_it() {
    let thread_day = day(1);
    // Day one of a 130-post thread: ceil(130 / 14).
    assert_eq!(drip_slice(130, thread_day, day(1)), 10);
    // Day eight, 60 left over the 7 days that remain.
    assert_eq!(drip_slice(60, thread_day, day(8)), 9);
    // The last day of the window takes everything left.
    assert_eq!(
        drip_slice(
            4,
            thread_day,
            thread_day + chrono::Duration::days(JOBS_DRIP_DAYS - 1)
        ),
        4
    );
    // Past the window, or with late posts queued after it closed: all at once.
    assert_eq!(drip_slice(7, thread_day, day(20)), 7);
    assert_eq!(drip_slice(0, thread_day, day(3)), 0);
}

#[test]
fn an_excerpt_is_folded_and_cut_on_a_character_boundary() {
    assert_eq!(
        tidy_excerpt("  Acme builds\n\n  storage.  "),
        "Acme builds storage."
    );
    let long = "é".repeat(500);
    let cut = tidy_excerpt(&long);
    assert_eq!(cut.chars().count(), 400);
    assert!(cut.ends_with('…'));
}

async fn seed_queued(db: &late_core::db::Db, company: &str, tags: &[&str], posted_day: u32) {
    let client = db.get().await.expect("db client");
    let fetched = FetchedPosting {
        source: JobSource::Hn,
        external_id: format!("hn-{}", company.to_ascii_lowercase()),
        url: format!("https://news.ycombinator.com/item?id={company}"),
        raw: format!("{company} | Engineer | REMOTE"),
        posted_at: day(posted_day).and_hms_opt(15, 0, 0).unwrap().and_utc(),
        remote_kind: None,
        regions: Vec::new(),
        dropped: false,
    };
    JobPosting::upsert_fetched(&client, &fetched)
        .await
        .expect("insert");
    let row = JobPosting::find_by_external_id(&client, JobSource::Hn, &fetched.external_id)
        .await
        .expect("find")
        .expect("row");
    JobPosting::settle(
        &client,
        row.id,
        Settle::Queued(JobRead {
            url: format!("https://{}.example/careers", company.to_ascii_lowercase()),
            company: company.to_string(),
            title: "Systems Engineer".to_string(),
            remote_kind: RemoteKind::Worldwide,
            regions: Vec::new(),
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
            pay: "$150k".to_string(),
            excerpt: format!("{company} builds things."),
        }),
    )
    .await
    .expect("settle queued");
}

/// `/jobs release` from an admin releases a slice, the replica's snapshot
/// picks it up, the session copies it, and the shelf shows the card.
#[tokio::test]
async fn a_release_lands_on_the_shelf_through_the_snapshot() {
    let (test_db, mut app) = chat_compose_app("jobs-release").await;
    app.set_permissions(Permissions::new(true, true));
    // Two posts from a thread that started a fortnight ago: the window is
    // over, so the slice is both of them.
    seed_queued(&test_db.db, "Quobyte", &["rust", "distributed"], 1).await;
    seed_queued(&test_db.db, "Fastly", &["go"], 2).await;

    let mut events = app.jobs.service.subscribe_events();
    app.jobs
        .service
        .request_press(app.user_id, PressJob::Release);
    let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("jobs event in time")
        .expect("jobs event");
    match event {
        JobsEvent::Press { outcome, .. } => match outcome {
            PressOutcome::Released { count, .. } => assert_eq!(count, 2),
            other => panic!("expected a release, got {other:?}"),
        },
        other @ (JobsEvent::Posted { .. } | JobsEvent::Retracted { .. }) => {
            panic!("expected a press event, got {other:?}")
        }
    }
    {
        let client = test_db.db.get().await.expect("db client");
        let shelf = JobPosting::list_active(&client, 10).await.expect("active");
        assert_eq!(shelf.len(), 2);
        assert!(shelf.iter().all(|row| row.status == JobStatus::Active));
        assert!(shelf.iter().all(|row| row.released_on.is_some()));
    }

    // The composer's `/jobs` opens the shelf; the tick has copied the
    // snapshot by then and the cards print.
    app.handle_input(b"/jobs\r");
    wait_for_render_contains(&mut app, "Quobyte").await;
    wait_for_render_contains(&mut app, "Fastly").await;
    wait_for_render_contains(&mut app, "remote worldwide").await;
}

#[tokio::test]
async fn a_non_admin_cannot_run_the_press_and_an_admin_gets_its_banner() {
    let (_test_db, mut app) = chat_compose_app("jobs-cmd").await;
    app.handle_input(b"/jobs pull\r");
    wait_for_render_contains(&mut app, "Only admins can run the job press").await;

    app.set_permissions(Permissions::new(true, true));
    // The test app has no model configured: the press says so instead of
    // pretending to run.
    app.handle_input(b"/jobs pull\r");
    wait_for_render_contains(&mut app, "AI is not configured here").await;
}

fn new_posting(by: Uuid, company: &str) -> NewPosting {
    NewPosting {
        posted_by: by,
        url: format!("https://{}.example/jobs", company.to_ascii_lowercase()),
        company: company.to_string(),
        title: "Rust Engineer".to_string(),
        remote_kind: RemoteKind::Worldwide,
        regions: Vec::new(),
        tags: vec!["rust".to_string()],
        pay: String::new(),
        excerpt: format!("{company} builds things."),
    }
}

async fn next_event(events: &mut tokio::sync::broadcast::Receiver<JobsEvent>) -> JobsEvent {
    tokio::time::timeout(Duration::from_secs(5), events.recv())
        .await
        .expect("jobs event in time")
        .expect("jobs event")
}

/// A posting written on the shelf goes live through the same snapshot the
/// press feeds, the fourth is refused at the cap, and `d` on your own
/// brings it down; a stranger's `d` changes nothing.
#[tokio::test]
async fn a_posting_written_here_lands_on_the_shelf_caps_per_person_and_comes_down() {
    let (test_db, mut app) = chat_compose_app("jobs-post").await;
    let mut events = app.jobs.service.subscribe_events();
    let me = app.user_id;
    for n in 0..JOBS_POSTS_PER_USER {
        app.jobs
            .service
            .request_post(new_posting(me, &format!("Acme{n}")));
        match next_event(&mut events).await {
            JobsEvent::Posted { outcome, .. } => assert_eq!(
                outcome,
                PostOutcome::Posted {
                    company: format!("Acme{n}"),
                    title: "Rust Engineer".to_string()
                }
            ),
            other => panic!("expected a post, got {other:?}"),
        }
    }
    app.jobs.service.request_post(new_posting(me, "Fourth"));
    match next_event(&mut events).await {
        JobsEvent::Posted { outcome, .. } => assert_eq!(outcome, PostOutcome::AtCap),
        other => panic!("expected the cap, got {other:?}"),
    }

    app.handle_input(b"/jobs\r");
    wait_for_render_contains(&mut app, "Acme0").await;
    wait_for_render_contains(&mut app, "via late.sh").await;

    let (first_id, _) = {
        let client = test_db.db.get().await.expect("db client");
        let shelf = JobPosting::list_active(&client, 10).await.expect("shelf");
        assert_eq!(shelf.len(), JOBS_POSTS_PER_USER as usize);
        assert!(shelf.iter().all(|row| row.posted_by == Some(me)));
        (shelf[0].id, ())
    };
    let stranger = Uuid::now_v7();
    app.jobs.service.request_retract(stranger, first_id, false);
    match next_event(&mut events).await {
        JobsEvent::Retracted { outcome, .. } => assert_eq!(outcome, RetractOutcome::NotYours),
        other => panic!("expected a refusal, got {other:?}"),
    }
    app.jobs.service.request_retract(me, first_id, false);
    match next_event(&mut events).await {
        JobsEvent::Retracted { outcome, .. } => assert_eq!(outcome, RetractOutcome::Gone),
        other => panic!("expected a take-down, got {other:?}"),
    }
    let client = test_db.db.get().await.expect("db client");
    let shelf = JobPosting::list_active(&client, 10).await.expect("shelf");
    assert_eq!(shelf.len(), JOBS_POSTS_PER_USER as usize - 1);
    assert!(shelf.iter().all(|row| row.id != first_id));
}
