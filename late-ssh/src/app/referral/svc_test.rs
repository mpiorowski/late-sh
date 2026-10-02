//! Service integration tests for invites against a real ephemeral DB: who
//! may attach, the exact monthly cap, both credits landing once, and expiry.

use chrono::{Duration, Utc};
use late_core::models::{
    chips::{ChipMove, INITIAL_CHIP_BALANCE, UserChips},
    referral::{Referral, ReferralSource, ReferralStatus},
};
use late_core::test_utils::create_test_user;
use uuid::Uuid;

use crate::app::referral::state::{
    AttachRefusal, INVITEE_BONUS_CHIPS, INVITER_REWARD_CHIPS, MONTHLY_PAID_CAP,
};
use crate::app::referral::svc::{JudgeTally, PayOutcome, ReferralService};
use crate::test_helpers::new_test_db;

async fn balance(client: &tokio_postgres::Client, user_id: Uuid) -> i64 {
    UserChips::find(client, user_id)
        .await
        .expect("chips")
        .expect("chips row")
        .balance
}

async fn status_of(
    client: &tokio_postgres::Client,
    inviter_id: Uuid,
    invitee_username: &str,
) -> ReferralStatus {
    Referral::list_for_inviter(client, inviter_id)
        .await
        .expect("list")
        .into_iter()
        .find(|invited| invited.username == invitee_username)
        .expect("invitee listed")
        .status
}

#[tokio::test]
async fn an_invite_attaches_once_and_refuses_the_rest() {
    let test_db = new_test_db().await;
    let service = ReferralService::new(test_db.db.clone());
    let inviter = create_test_user(&test_db.db, "attach-inviter").await;
    let invitee = create_test_user(&test_db.db, "attach-invitee").await;
    let code = service.overview(inviter.id).await.expect("overview").code;

    let first = service
        .attach(invitee.id, &code, ReferralSource::Ssh)
        .await
        .expect("attach");
    let second = service
        .attach(invitee.id, &code, ReferralSource::Settings)
        .await
        .expect("attach again");
    let unknown = service
        .attach(invitee.id, "zzzzzzzz", ReferralSource::Settings)
        .await
        .expect("unknown code");
    let own = service
        .attach(inviter.id, &code, ReferralSource::Settings)
        .await
        .expect("own code");
    // The invitee is newer than the inviter, so the inviter can never take
    // the invitee's code: no account invites someone who was here first.
    let invitee_code = service.overview(invitee.id).await.expect("overview").code;
    let backwards = service
        .attach(inviter.id, &invitee_code, ReferralSource::Settings)
        .await
        .expect("backwards");

    assert_eq!(
        [first, second, unknown, own, backwards],
        [
            Ok(inviter.username.clone()),
            Err(AttachRefusal::AlreadyInvited),
            Err(AttachRefusal::UnknownCode),
            Err(AttachRefusal::OwnCode),
            Err(AttachRefusal::InviterNewer),
        ]
    );

    let invitee_view = service.overview(invitee.id).await.expect("invitee view");
    assert_eq!(invitee_view.inviter, Some(inviter.username.clone()));
    assert!(!invitee_view.can_attach);
    let inviter_view = service.overview(inviter.id).await.expect("inviter view");
    assert_eq!(inviter_view.code, code);
    assert_eq!(inviter_view.inviter, None);
    assert_eq!(inviter_view.invited.len(), 1);
    assert_eq!(inviter_view.invited[0].username, invitee.username);
    assert_eq!(inviter_view.invited[0].status, ReferralStatus::Pending);
}

/// The money path: three paid in a month, the fourth waits as qualified,
/// both sides are credited exactly once, and a second pay of a settled row
/// does nothing.
#[tokio::test]
async fn payouts_credit_both_sides_once_under_the_monthly_cap() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = ReferralService::new(test_db.db.clone());
    let inviter = create_test_user(&test_db.db, "pay-inviter").await;
    UserChips::ensure(&client, inviter.id).await.expect("chips");
    let mut invitees = Vec::new();
    for index in 0..=MONTHLY_PAID_CAP {
        let invitee = create_test_user(&test_db.db, &format!("pay-invitee-{index}")).await;
        UserChips::ensure(&client, invitee.id).await.expect("chips");
        assert!(
            Referral::attach(
                &**client,
                invitee.id,
                inviter.id,
                ReferralSource::Ssh,
                Utc::now() + Duration::days(60),
            )
            .await
            .expect("attach")
        );
        assert!(
            Referral::mark_qualified(&**client, invitee.id)
                .await
                .expect("qualify")
        );
        invitees.push(invitee);
    }

    let mut outcomes = Vec::new();
    for invitee in &invitees {
        outcomes.push(service.pay(invitee.id).await.expect("pay"));
    }
    outcomes.push(service.pay(invitees[0].id).await.expect("pay again"));

    let paid = PayOutcome::Paid {
        inviter_id: inviter.id,
    };
    assert_eq!(
        outcomes,
        vec![
            paid,
            paid,
            paid,
            PayOutcome::Deferred,
            PayOutcome::AlreadySettled
        ]
    );
    assert_eq!(
        balance(&client, inviter.id).await,
        INITIAL_CHIP_BALANCE + MONTHLY_PAID_CAP * INVITER_REWARD_CHIPS
    );
    let mut invitee_balances = Vec::new();
    let mut statuses = Vec::new();
    for invitee in &invitees {
        invitee_balances.push(balance(&client, invitee.id).await);
        statuses.push(status_of(&client, inviter.id, &invitee.username).await);
    }
    let welcomed = INITIAL_CHIP_BALANCE + INVITEE_BONUS_CHIPS;
    assert_eq!(
        invitee_balances,
        vec![welcomed, welcomed, welcomed, INITIAL_CHIP_BALANCE]
    );
    assert_eq!(
        statuses,
        vec![
            ReferralStatus::Paid,
            ReferralStatus::Paid,
            ReferralStatus::Paid,
            ReferralStatus::Qualified,
        ]
    );
    let reward_refs: Vec<String> = client
        .query(
            "SELECT source_ref FROM chip_ledger WHERE user_id = $1 AND reason = $2
             ORDER BY source_ref",
            &[&inviter.id, &ChipMove::ReferralReward.reason()],
        )
        .await
        .expect("ledger")
        .into_iter()
        .map(|row| row.get("source_ref"))
        .collect();
    let mut expected_refs: Vec<String> = invitees[..3]
        .iter()
        .map(|invitee| invitee.id.to_string())
        .collect();
    expected_refs.sort();
    assert_eq!(reward_refs, expected_refs);
}

#[tokio::test]
async fn judging_expires_a_closed_window_and_leaves_an_open_one() {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let service = ReferralService::new(test_db.db.clone());
    let inviter = create_test_user(&test_db.db, "judge-inviter").await;
    let lapsed = create_test_user(&test_db.db, "judge-lapsed").await;
    let fresh = create_test_user(&test_db.db, "judge-fresh").await;
    let now = Utc::now();
    for (invitee, until) in [
        (&lapsed, now - Duration::seconds(1)),
        (&fresh, now + Duration::days(60)),
    ] {
        Referral::attach(
            &**client,
            invitee.id,
            inviter.id,
            ReferralSource::Settings,
            until,
        )
        .await
        .expect("attach");
    }

    let first = service.judge_pending(now).await.expect("judge");
    let second = service.judge_pending(now).await.expect("judge again");

    assert_eq!(
        (first, second),
        (
            JudgeTally {
                qualified: 0,
                expired: 1,
            },
            JudgeTally::default(),
        )
    );
    assert_eq!(
        status_of(&client, inviter.id, &lapsed.username).await,
        ReferralStatus::Expired
    );
    assert_eq!(
        status_of(&client, inviter.id, &fresh.username).await,
        ReferralStatus::Pending
    );
}
