use super::calendar::*;
use crate::test_utils::{create_test_user, test_db};
use chrono::{Duration, NaiveDate, TimeZone, Utc};
fn draft() -> EventDraft {
    EventDraft {
        title: "Calendar test".into(),
        description: "Two\nlines".into(),
        timing: EventTiming::AllDay {
            start: "2028-02-29".parse().unwrap(),
            end_exclusive: "2028-03-02".parse().unwrap(),
        },
        notice_lead_seconds: None,
        mod_editable: false,
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
#[tokio::test]
async fn calendar_permissions_sharing_revisions_and_role_changes() {
    let db = test_db().await;
    let c = db.db.get().await.unwrap();
    let user = create_test_user(&db.db, "cal_user").await;
    let moderator = create_test_user(&db.db, "cal_mod").await;
    let admin = create_test_user(&db.db, "cal_admin").await;
    c.execute(
        "UPDATE users SET is_moderator=true WHERE id=$1",
        &[&moderator.id],
    )
    .await
    .unwrap();
    c.execute("UPDATE users SET is_admin=true WHERE id=$1", &[&admin.id])
        .await
        .unwrap();
    let store = CalendarStore::new(db.db.clone());
    assert!(
        store
            .save(user.id, CalendarSource::Server, None, &draft())
            .await
            .is_err()
    );
    let protected = store
        .save(admin.id, CalendarSource::Server, None, &draft())
        .await
        .unwrap();
    assert!(
        store
            .save(
                moderator.id,
                CalendarSource::Server,
                Some((protected.id, 1)),
                &draft()
            )
            .await
            .is_err()
    );
    let mut delegated = draft();
    delegated.mod_editable = true;
    let e = store
        .save(
            admin.id,
            CalendarSource::Server,
            Some((protected.id, 1)),
            &delegated,
        )
        .await
        .unwrap();
    delegated.notice_lead_seconds = Some(86400);
    let e = store
        .save(
            moderator.id,
            CalendarSource::Server,
            Some((e.id, e.revision)),
            &delegated,
        )
        .await
        .unwrap();
    assert!(
        store
            .save(admin.id, CalendarSource::Server, Some((e.id, 1)), &draft())
            .await
            .is_err()
    );
    store.delete(moderator.id, e.id, e.revision).await.unwrap();
    let e = store
        .save(moderator.id, CalendarSource::Server, None, &draft())
        .await
        .unwrap();
    let mut notifications = draft();
    notifications.notice_lead_seconds = Some(0);
    assert!(
        store
            .save(
                moderator.id,
                CalendarSource::Server,
                Some((e.id, 1)),
                &notifications
            )
            .await
            .is_err()
    );
    c.execute(
        "UPDATE users SET is_admin=true WHERE id=$1",
        &[&moderator.id],
    )
    .await
    .unwrap();
    assert_eq!(
        store.event(user.id, e.id).await.unwrap().creation_tier,
        CreationTier::Moderator
    );
    c.execute(
        "UPDATE users SET is_admin=false,is_moderator=false WHERE id=$1",
        &[&moderator.id],
    )
    .await
    .unwrap();
    assert!(store.delete(moderator.id, e.id, 1).await.is_err());
    let personal = store
        .save(
            user.id,
            CalendarSource::Personal(user.id),
            None,
            &notifications,
        )
        .await
        .unwrap();
    assert!(store.event(admin.id, personal.id).await.is_err());
    let p = store
        .save_preferences(
            user.id,
            &CalendarPreferences {
                public: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let shared = store.event(admin.id, personal.id).await.unwrap();
    assert!(shared.notice_lead_seconds.is_none());
    assert!(
        store
            .save(
                admin.id,
                CalendarSource::Personal(user.id),
                Some((shared.id, 1)),
                &draft()
            )
            .await
            .is_err()
    );
    assert!(store.delete(admin.id, personal.id, 1).await.is_err());
    store
        .save_preferences(
            user.id,
            &CalendarPreferences {
                public: false,
                ..p.clone()
            },
        )
        .await
        .unwrap();
    assert!(store.event(admin.id, personal.id).await.is_err());
    assert!(store.save_preferences(user.id, &p).await.is_err());
    let retained_server = store
        .save(admin.id, CalendarSource::Server, None, &draft())
        .await
        .unwrap();
    c.execute("DELETE FROM users WHERE id=$1", &[&admin.id])
        .await
        .unwrap();
    let retained = store.event(user.id, retained_server.id).await.unwrap();
    assert_eq!(retained.creator_id, admin.id);
    assert_eq!(retained.creation_tier, CreationTier::Admin);
}
#[tokio::test]
async fn calendar_notification_windows_and_ranges() {
    let db = test_db().await;
    let c = db.db.get().await.unwrap();
    let user = create_test_user(&db.db, "cal_windows").await;
    c.execute("UPDATE users SET settings=settings || '{\"timezone\":\"America/New_York\"}'::jsonb WHERE id=$1",&[&user.id]).await.unwrap();
    let store = CalendarStore::new(db.db.clone());
    let mut d = draft();
    d.notice_lead_seconds = Some(86400);
    let e = store
        .save(user.id, CalendarSource::Personal(user.id), None, &d)
        .await
        .unwrap();
    assert_eq!(e.creator_timezone, "America/New_York");
    assert!(!e.upcoming(e.notice_start - Duration::seconds(1)));
    assert!(e.upcoming(e.notice_start));
    assert!(!e.upcoming(e.notice_end));
    let a = store
        .visible(
            user.id,
            CalendarSource::Personal(user.id),
            false,
            "2028-03-01".parse().unwrap(),
            "2028-04-01".parse().unwrap(),
            chrono_tz::Asia::Tokyo,
        )
        .await
        .unwrap();
    assert_eq!(a.len(), 1);
    assert!(
        store
            .upcoming_personal(user.id, e.notice_start)
            .await
            .unwrap()
            .iter()
            .any(|x| x.id == e.id)
    );
    let start = Utc.with_ymd_and_hms(2026, 10, 2, 23, 30, 0).unwrap();
    d.timing = EventTiming::Timed { start, end: None };
    let e = store
        .save(user.id, CalendarSource::Personal(user.id), None, &d)
        .await
        .unwrap();
    assert_eq!(e.notice_end, start + Duration::hours(1));
    d.title = "Moved".into();
    d.timing = EventTiming::Timed {
        start: start + Duration::days(3),
        end: None,
    };
    let e = store
        .save(
            user.id,
            CalendarSource::Personal(user.id),
            Some((e.id, e.revision)),
            &d,
        )
        .await
        .unwrap();
    assert!(!e.upcoming(start));
    store.delete(user.id, e.id, e.revision).await.unwrap();
    assert!(store.event(user.id, e.id).await.is_err());
}

#[tokio::test]
async fn calendar_concurrent_writers_preserve_one_revision_and_stale_delete() {
    let db = test_db().await;
    let user = create_test_user(&db.db, "calendar_race").await;
    let store = CalendarStore::new(db.db.clone());
    let source = CalendarSource::Personal(user.id);
    let original = store.save(user.id, source, None, &draft()).await.unwrap();
    let mut a = draft();
    a.title = "First writer".into();
    let mut b = draft();
    b.title = "Second writer".into();
    let revision = Some((original.id, original.revision));
    let (a, b) = tokio::join!(
        store.save(user.id, source, revision, &a),
        store.save(user.id, source, revision, &b)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let latest = store.event(user.id, original.id).await.unwrap();
    assert_eq!(latest.revision, 2);
    assert!(
        store
            .delete(user.id, latest.id, original.revision)
            .await
            .is_err()
    );
    assert_eq!(store.event(user.id, latest.id).await.unwrap(), latest);
    let initial = CalendarPreferences::default();
    let (a, b) = tokio::join!(
        store.save_preferences(user.id, &initial),
        store.save_preferences(user.id, &initial)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    assert_eq!(store.preferences(user.id).await.unwrap().revision, 1);
}

#[tokio::test]
async fn calendar_overlay_scope_and_notice_expiry_are_independent_of_sharing() {
    let db = test_db().await;
    let owner = create_test_user(&db.db, "calendar_owner").await;
    let viewer = create_test_user(&db.db, "calendar_viewer").await;
    db.db
        .get()
        .await
        .unwrap()
        .execute("UPDATE users SET is_admin=true WHERE id=$1", &[&viewer.id])
        .await
        .unwrap();
    let store = CalendarStore::new(db.db.clone());
    let server = store
        .save(viewer.id, CalendarSource::Server, None, &draft())
        .await
        .unwrap();
    let mut d = draft();
    d.notice_lead_seconds = Some(0);
    let personal = store
        .save(owner.id, CalendarSource::Personal(owner.id), None, &d)
        .await
        .unwrap();
    let from = "2028-02-01".parse().unwrap();
    let to = "2028-03-01".parse().unwrap();
    assert!(
        store
            .visible(
                viewer.id,
                CalendarSource::Personal(owner.id),
                true,
                from,
                to,
                chrono_tz::UTC
            )
            .await
            .is_err()
    );
    store
        .save_preferences(
            owner.id,
            &CalendarPreferences {
                public: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let visible = store
        .visible(
            viewer.id,
            CalendarSource::Personal(owner.id),
            true,
            from,
            to,
            chrono_tz::UTC,
        )
        .await
        .unwrap();
    assert_eq!(visible.len(), 2);
    assert!(
        visible
            .iter()
            .any(|e| e.id == server.id && !event_access(e, owner.id, CreationTier::User).edit)
    );
    assert!(
        visible
            .iter()
            .any(|e| e.id == personal.id && e.notice_lead_seconds.is_none())
    );
    assert_eq!(
        store
            .visible(
                viewer.id,
                CalendarSource::Personal(owner.id),
                false,
                from,
                to,
                chrono_tz::UTC
            )
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        store
            .upcoming_personal(viewer.id, personal.notice_start)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store
            .upcoming_personal(owner.id, personal.notice_start)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        store
            .upcoming_personal(owner.id, personal.notice_end)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        store
            .visible(
                owner.id,
                CalendarSource::Personal(owner.id),
                false,
                "2028-03-02".parse().unwrap(),
                "2028-04-01".parse().unwrap(),
                chrono_tz::UTC
            )
            .await
            .unwrap()
            .is_empty()
    );
}
