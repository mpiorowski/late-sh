use super::calendar::*;
use super::calendar_ban::CalendarBan;
use crate::db::Db;
use crate::test_utils::{create_test_user, test_db};
use chrono::{DateTime, Duration, NaiveDate, Utc};

fn draft() -> EventDraft {
    EventDraft {
        title: "Calendar test".into(),
        description: "Two\nlines".into(),
        timing: EventTiming::AllDay {
            start: "2028-02-29".parse().unwrap(),
            end_exclusive: "2028-03-02".parse().unwrap(),
        },
    }
}

fn timed_at(now: DateTime<Utc>, offset: Duration) -> EventDraft {
    EventDraft {
        title: "Movie night".into(),
        description: String::new(),
        timing: EventTiming::Timed {
            start: now + offset,
            end: Some(now + offset + Duration::hours(2)),
        },
    }
}

/// An all-day board post on the supplied UTC date.
fn all_day_on(day: NaiveDate) -> EventDraft {
    EventDraft {
        title: "Hackathon".into(),
        description: String::new(),
        timing: EventTiming::AllDay {
            start: day,
            end_exclusive: day.succ_opt().unwrap(),
        },
    }
}

fn refusal(error: CalendarError) -> CalendarRefusal {
    match error {
        CalendarError::Refused(refusal) => refusal,
        CalendarError::Failed(error) => panic!("expected a refusal, got {error:?}"),
    }
}

#[test]
fn calendar_dst_and_inclusive_dates() {
    let tz = chrono_tz::America::New_York;
    let local = "2026-11-01T01:30:00".parse().unwrap();
    assert!(local_instant(local, tz, None).is_err());
    assert_eq!(
        local_instant(local, tz, Some(Occurrence::Later)).unwrap()
            - local_instant(local, tz, Some(Occurrence::Earlier)).unwrap(),
        Duration::hours(1)
    );
    assert!(local_instant("2026-03-08T02:30:00".parse().unwrap(), tz, None).is_err());
    let day: NaiveDate = "2026-03-08".parse().unwrap();
    assert_eq!(
        day_boundary(day.succ_opt().unwrap(), tz).unwrap() - day_boundary(day, tz).unwrap(),
        Duration::hours(23)
    );
    let mut d = draft();
    d.timing = EventTiming::AllDay {
        start: day,
        end_exclusive: day,
    };
    assert!(d.validate().is_err());
}

/// The three rules of the board: the poster edits and deletes their own
/// post, staff delete any post, a ban stops new posts and nothing else.
#[tokio::test]
async fn board_posts_follow_the_three_rules() {
    let db = test_db().await;
    let c = db.db.get().await.unwrap();
    let user = create_test_user(&db.db, "cal_user").await;
    let other = create_test_user(&db.db, "cal_other").await;
    let moderator = create_test_user(&db.db, "cal_mod").await;
    c.execute(
        "UPDATE users SET is_moderator=true WHERE id=$1",
        &[&moderator.id],
    )
    .await
    .unwrap();
    let store = CalendarStore::new(db.db.clone());

    let saved = store
        .save(user.id, CalendarSource::Board, None, &draft())
        .await
        .unwrap();
    assert!(saved.created);
    let post = saved.event;
    assert_eq!(post.creator_name, "cal_user");
    assert!(post.is_board());

    // Another user reads it, may not edit it, may not delete it.
    let seen = store.event(other.id, post.id).await.unwrap();
    assert_eq!(seen.id, post.id);
    let access = event_access(&seen, other.id, false);
    assert_eq!(
        access,
        EventAccess {
            edit: false,
            delete: false,
            rsvp: true
        }
    );
    let mut edit = draft();
    edit.title = "Hijacked".into();
    assert_eq!(
        refusal(
            store
                .save(
                    other.id,
                    CalendarSource::Board,
                    Some((post.id, post.revision)),
                    &edit
                )
                .await
                .unwrap_err()
        ),
        CalendarRefusal::ReadOnly
    );
    assert_eq!(
        refusal(
            store
                .delete(other.id, post.id, post.revision)
                .await
                .unwrap_err()
        ),
        CalendarRefusal::ReadOnly
    );

    // The poster edits it; a stale revision is refused afterwards.
    let edited = store
        .save(
            user.id,
            CalendarSource::Board,
            Some((post.id, post.revision)),
            &edit,
        )
        .await
        .unwrap();
    assert!(!edited.created);
    assert_eq!(edited.event.revision, post.revision + 1);
    assert_eq!(
        refusal(
            store
                .save(
                    user.id,
                    CalendarSource::Board,
                    Some((post.id, post.revision)),
                    &edit
                )
                .await
                .unwrap_err()
        ),
        CalendarRefusal::Revision
    );

    // A moderator may not edit it, but deletes it, and the delete says so.
    assert!(
        !event_access(&edited.event, moderator.id, true).edit,
        "staff edit nobody's words"
    );
    let deleted = store
        .delete(moderator.id, post.id, edited.event.revision)
        .await
        .unwrap();
    assert!(deleted.by_staff);
    assert_eq!(
        refusal(store.event(user.id, post.id).await.unwrap_err()),
        CalendarRefusal::Gone
    );

    // A ban stops board posts and leaves personal events alone.
    CalendarBan::activate(&c, user.id, moderator.id, "spam", None)
        .await
        .unwrap();
    assert_eq!(
        refusal(
            store
                .save(user.id, CalendarSource::Board, None, &draft())
                .await
                .unwrap_err()
        ),
        CalendarRefusal::Banned
    );
    let own = store
        .save(user.id, CalendarSource::Personal, None, &draft())
        .await
        .unwrap();
    assert_eq!(own.event.owner_id, Some(user.id));
    CalendarBan::delete_for_user(&c, user.id).await.unwrap();
    assert!(
        store
            .save(user.id, CalendarSource::Board, None, &draft())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn personal_events_are_private_even_from_staff() {
    let db = test_db().await;
    let c = db.db.get().await.unwrap();
    let owner = create_test_user(&db.db, "cal_owner").await;
    let admin = create_test_user(&db.db, "cal_admin").await;
    c.execute("UPDATE users SET is_admin=true WHERE id=$1", &[&admin.id])
        .await
        .unwrap();
    let store = CalendarStore::new(db.db.clone());
    let own = store
        .save(owner.id, CalendarSource::Personal, None, &draft())
        .await
        .unwrap()
        .event;
    assert_eq!(
        refusal(store.event(admin.id, own.id).await.unwrap_err()),
        CalendarRefusal::Gone
    );
    let from: NaiveDate = "2028-02-01".parse().unwrap();
    let to: NaiveDate = "2028-04-01".parse().unwrap();
    assert!(
        store
            .visible(admin.id, true, from, to, chrono_tz::UTC)
            .await
            .unwrap()
            .is_empty()
    );
    // Every write answers exactly as a missing event does, so no answer
    // tells a stranger the event exists.
    for source in [CalendarSource::Personal, CalendarSource::Board] {
        assert_eq!(
            refusal(
                store
                    .save(admin.id, source, Some((own.id, own.revision)), &draft())
                    .await
                    .unwrap_err()
            ),
            CalendarRefusal::Gone
        );
    }
    assert_eq!(
        refusal(
            store
                .delete(admin.id, own.id, own.revision)
                .await
                .unwrap_err()
        ),
        CalendarRefusal::Gone
    );
    assert_eq!(
        refusal(store.set_rsvp(admin.id, own.id, true).await.unwrap_err()),
        CalendarRefusal::Gone
    );
    // The owner's own personal event still takes no "I'm in".
    assert_eq!(
        refusal(store.set_rsvp(owner.id, own.id, true).await.unwrap_err()),
        CalendarRefusal::NoRsvp
    );
    // The owner sees it with the board hidden and with it shown.
    for board in [false, true] {
        let mine = store
            .visible(owner.id, board, from, to, chrono_tz::UTC)
            .await
            .unwrap();
        assert_eq!(mine.iter().map(|e| e.id).collect::<Vec<_>>(), vec![own.id]);
    }
}

#[tokio::test]
async fn the_daily_cap_counts_board_posts_only() {
    let db = test_db().await;
    let user = create_test_user(&db.db, "cal_poster").await;
    let store = CalendarStore::new(db.db.clone());
    for _ in 0..BOARD_POSTS_PER_DAY {
        store
            .save(user.id, CalendarSource::Board, None, &draft())
            .await
            .unwrap();
    }
    assert_eq!(
        refusal(
            store
                .save(user.id, CalendarSource::Board, None, &draft())
                .await
                .unwrap_err()
        ),
        CalendarRefusal::DailyCap
    );
    // Personal events are not posts, and an edit is not a post.
    assert!(
        store
            .save(user.id, CalendarSource::Personal, None, &draft())
            .await
            .is_ok()
    );
    let from: NaiveDate = "2028-02-01".parse().unwrap();
    let to: NaiveDate = "2028-04-01".parse().unwrap();
    let first = store
        .visible(user.id, true, from, to, chrono_tz::UTC)
        .await
        .unwrap()
        .into_iter()
        .find(|e| e.is_board())
        .unwrap();
    assert!(
        store
            .save(
                user.id,
                CalendarSource::Board,
                Some((first.id, first.revision)),
                &draft()
            )
            .await
            .is_ok()
    );
}

/// Deleting a post does not give its slot back: post, delete, post again
/// would otherwise put a line in #lounge every round.
#[tokio::test]
async fn a_deleted_post_still_counts_against_the_daily_cap() {
    let db = test_db().await;
    let user = create_test_user(&db.db, "cal_reposter").await;
    let store = CalendarStore::new(db.db.clone());
    let mut posts = Vec::new();
    for _ in 0..BOARD_POSTS_PER_DAY {
        posts.push(
            store
                .save(user.id, CalendarSource::Board, None, &draft())
                .await
                .unwrap()
                .event,
        );
    }
    let gone = posts.pop().unwrap();
    store.delete(user.id, gone.id, gone.revision).await.unwrap();
    assert_eq!(
        refusal(
            store
                .save(user.id, CalendarSource::Board, None, &draft())
                .await
                .unwrap_err()
        ),
        CalendarRefusal::DailyCap
    );
}

/// The cap's day is the UTC day whatever zone the database session runs
/// in. Posts are moved halfway between UTC midnight and the session zone's
/// midnight, so they sit on one day in UTC and on the other in the zone.
#[tokio::test]
async fn the_daily_cap_resets_at_utc_midnight_in_any_session_zone() {
    let db = test_db().await;
    let user = create_test_user(&db.db, "cal_zoned").await;
    let c = db.db.get().await.unwrap();
    let name: String = c
        .query_one("SELECT current_database()", &[])
        .await
        .unwrap()
        .get(0);
    // Every new session on this test database runs an hour behind UTC.
    c.execute(
        &format!("ALTER DATABASE \"{name}\" SET TimeZone = 'Etc/GMT+1'"),
        &[],
    )
    .await
    .unwrap();
    let zone = chrono_tz::Etc::GMTPlus1;
    let store = CalendarStore::new(Db::new(db.db.config()).unwrap());
    for _ in 0..BOARD_POSTS_PER_DAY {
        store
            .save(user.id, CalendarSource::Board, None, &draft())
            .await
            .unwrap();
    }
    let now = Utc::now();
    let utc_midnight = now.date_naive().and_hms_opt(0, 0, 0).unwrap().and_utc();
    let zone_midnight = day_boundary(now.with_timezone(&zone).date_naive(), zone).unwrap();
    let between = utc_midnight + (zone_midnight - utc_midnight) / 2;
    c.execute(
        "UPDATE calendar_posts SET created=$1 WHERE user_id=$2",
        &[&between, &user.id],
    )
    .await
    .unwrap();
    let next = store
        .save(user.id, CalendarSource::Board, None, &draft())
        .await;
    if between >= utc_midnight {
        // Today in UTC: the five still count.
        assert_eq!(refusal(next.unwrap_err()), CalendarRefusal::DailyCap);
    } else {
        // Yesterday in UTC: a new day, five new posts.
        assert!(next.is_ok(), "{next:?}");
    }
}

#[tokio::test]
async fn rsvps_count_and_the_start_is_claimed_once() {
    // Five minutes ago is yesterday just after UTC midnight: a timed event
    // can sort before today's all-day event. Cover both orders independently
    // of the wall clock, including the boundary that failed in CI.
    for (timestamp, all_day_first) in [
        ("2026-10-13T00:01:00Z", false),
        ("2026-10-13T12:00:00Z", true),
    ] {
        rsvps_and_start_claim_at(timestamp.parse().unwrap(), all_day_first).await;
    }
}

async fn rsvps_and_start_claim_at(now: DateTime<Utc>, all_day_first: bool) {
    let db = test_db().await;
    let poster = create_test_user(&db.db, "cal_host").await;
    let guest = create_test_user(&db.db, "cal_guest").await;
    let store = CalendarStore::new(db.db.clone());
    let soon = store
        .save(
            poster.id,
            CalendarSource::Board,
            None,
            &timed_at(now, Duration::minutes(30)),
        )
        .await
        .unwrap()
        .event;
    let on = store
        .save(
            poster.id,
            CalendarSource::Board,
            None,
            &timed_at(now, -Duration::minutes(5)),
        )
        .await
        .unwrap()
        .event;
    let over = store
        .save(
            poster.id,
            CalendarSource::Board,
            None,
            &timed_at(now, -Duration::hours(5)),
        )
        .await
        .unwrap()
        .event;
    let far = store
        .save(
            poster.id,
            CalendarSource::Board,
            None,
            &timed_at(now, Duration::days(3)),
        )
        .await
        .unwrap()
        .event;

    let day = store
        .save(
            poster.id,
            CalendarSource::Board,
            None,
            &all_day_on(now.date_naive()),
        )
        .await
        .unwrap()
        .event;

    let counted = store.set_rsvp(guest.id, soon.id, true).await.unwrap();
    assert_eq!(counted.going, 1);
    // Saying it twice is one person.
    assert_eq!(
        store.set_rsvp(guest.id, soon.id, true).await.unwrap().going,
        1
    );
    assert_eq!(store.rsvps(guest.id).await.unwrap(), vec![soon.id]);
    assert_eq!(
        store
            .set_rsvp(guest.id, soon.id, false)
            .await
            .unwrap()
            .going,
        0
    );
    assert!(store.rsvps(guest.id).await.unwrap().is_empty());
    store.set_rsvp(poster.id, soon.id, true).await.unwrap();

    // Live is the window: the one three days out is not in it, the one
    // five hours over is not either, the one on now and the one an hour
    // out are, and the all-day one is for its whole day.
    let live: Vec<_> = store
        .live_board(now)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.id)
        .collect();
    let expected = if all_day_first {
        vec![day.id, on.id, soon.id]
    } else {
        vec![on.id, day.id, soon.id]
    };
    assert_eq!(live, expected);
    assert!(soon.live(now) && on.live(now) && day.live(now));
    assert!(!far.live(now) && !over.live(now));
    assert!(on.started(now) && !soon.started(now));

    // The sweeper claims what started and is not over, once. The all-day
    // post is on and is never claimed.
    let claimed: Vec<_> = store
        .claim_started(now)
        .await
        .unwrap()
        .into_iter()
        .map(|e| (e.id, e.going))
        .collect();
    assert_eq!(claimed, vec![(on.id, 0)]);
    assert!(store.claim_started(now).await.unwrap().is_empty());
    // Half an hour on, the next one starts and is claimed with its count.
    let later = now + Duration::minutes(31);
    let claimed: Vec<_> = store
        .claim_started(later)
        .await
        .unwrap()
        .into_iter()
        .map(|e| (e.id, e.going))
        .collect();
    assert_eq!(claimed, vec![(soon.id, 1)]);
}
