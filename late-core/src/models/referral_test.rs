use chrono::{Duration, TimeZone, Utc};

use crate::{
    models::referral::{
        ActivityDay, INVITE_CODE_LEN, InviteCode, NewcomerActivity, Referral, ReferralSource,
        ReferralStatus, normalize_code,
    },
    test_utils::{create_test_user, test_db},
};

#[test]
fn codes_normalize_case_and_reject_anything_but_a_whole_code() {
    assert_eq!(normalize_code(" AbCd2345 "), Some("abcd2345".to_string()));
    // Confusables are outside the alphabet, so a code containing one cannot
    // be anybody's.
    assert_eq!(normalize_code("abcd0o1i"), None);
    assert_eq!(normalize_code("abcd234"), None);
    assert_eq!(normalize_code("abcd23456"), None);
}

#[tokio::test]
async fn an_account_keeps_one_code() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let owner = create_test_user(&test_db.db, "invite-code-owner").await;

    let first = InviteCode::ensure(&**client, owner.id).await.expect("mint");
    let second = InviteCode::ensure(&**client, owner.id).await.expect("read");

    assert_eq!(first, second);
    assert_eq!(first.len(), INVITE_CODE_LEN);
    assert_eq!(normalize_code(&first), Some(first.clone()));
    let found = InviteCode::find_owner(&**client, &first)
        .await
        .expect("find")
        .expect("owner");
    assert_eq!(found.user_id, owner.id);
}

/// The once-only rule is the primary key: whatever the source, the first
/// inviter is the only inviter.
#[tokio::test]
async fn an_invitee_attaches_once() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let inviter = create_test_user(&test_db.db, "referral-inviter").await;
    let other = create_test_user(&test_db.db, "referral-other").await;
    let invitee = create_test_user(&test_db.db, "referral-invitee").await;
    let until = Utc::now() + Duration::days(60);

    assert!(
        Referral::attach(&**client, invitee.id, inviter.id, ReferralSource::Ssh, until)
            .await
            .expect("attach")
    );
    assert!(
        !Referral::attach(
            &**client,
            invitee.id,
            other.id,
            ReferralSource::Settings,
            until
        )
        .await
        .expect("second attach")
    );

    assert_eq!(
        Referral::inviter_username(&**client, invitee.id)
            .await
            .expect("inviter"),
        Some(inviter.username.clone())
    );
    let listed = Referral::list_for_inviter(&**client, inviter.id)
        .await
        .expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].username, invitee.username);
    assert_eq!(listed[0].status, ReferralStatus::Pending);
    assert!(
        Referral::list_for_inviter(&**client, other.id)
            .await
            .expect("list other")
            .is_empty()
    );
}

/// Two sessions of one account report the same wall minute: it counts once.
/// A late report of an earlier minute counts nothing either.
#[tokio::test]
async fn a_minute_counts_once_across_sessions() {
    let test_db = test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let user = create_test_user(&test_db.db, "newcomer-minutes").await;
    let minute = Utc.with_ymd_and_hms(2026, 10, 2, 12, 30, 5).unwrap();

    let first = NewcomerActivity::record_minute(&**client, user.id, minute).await;
    let same = NewcomerActivity::record_minute(&**client, user.id, minute + Duration::seconds(40))
        .await;
    let next = NewcomerActivity::record_minute(&**client, user.id, minute + Duration::minutes(1))
        .await;
    let late = NewcomerActivity::record_minute(&**client, user.id, minute).await;
    let next_day =
        NewcomerActivity::record_minute(&**client, user.id, minute + Duration::days(1)).await;

    assert_eq!(
        [first, same, next, late, next_day].map(|result| result.expect("record")),
        [true, false, true, false, true]
    );
    let days = NewcomerActivity::days(
        &**client,
        user.id,
        minute - Duration::days(1),
        minute + Duration::days(2),
    )
    .await
    .expect("days");
    assert_eq!(
        days,
        vec![
            ActivityDay {
                day: minute.date_naive(),
                active_minutes: 2,
            },
            ActivityDay {
                day: (minute + Duration::days(1)).date_naive(),
                active_minutes: 1,
            },
        ]
    );
}
