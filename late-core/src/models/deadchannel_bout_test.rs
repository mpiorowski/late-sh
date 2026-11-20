use chrono::{DateTime, Duration, NaiveDate, SubsecRound, Utc};
use serde_json::json;
use tokio_postgres::{Client, GenericClient};
use uuid::Uuid;

use super::{AcceptResult, Bout, BoutBet, BoutOutcome, CallResult, NewCall, Side, StakeResult};
use crate::models::user::User;
use crate::test_utils::{create_test_user, test_db};

/// Postgres keeps microseconds; trim so stored stamps compare equal.
fn now() -> DateTime<Utc> {
    Utc::now().trunc_subsecs(6)
}

/// A call from `caller` to `called` on `day`, made at `at` and lapsing two
/// minutes later.
fn call(caller: &User, called: &User, day: NaiveDate, at: DateTime<Utc>) -> NewCall {
    NewCall {
        day,
        caller_user_id: caller.id,
        called_user_id: called.id,
        caller_username: caller.username.clone(),
        called_username: called.username.clone(),
        called_at: at,
        expires_at: at + Duration::minutes(2),
    }
}

async fn called(client: &Client, new: &NewCall) -> Bout {
    match Bout::call(client, new).await.expect("call") {
        CallResult::Called(bout) => *bout,
        other => panic!("the call lands, got {other:?}"),
    }
}

/// Accept `id` at `at` with the same decks and duel every test uses: the
/// bell thirty seconds later, round 1 due a minute after that.
async fn accept(client: &mut impl GenericClient, id: Uuid, at: DateTime<Utc>) -> AcceptResult {
    Bout::accept(
        client,
        id,
        &json!(["caller deck"]),
        &json!(["called deck"]),
        &json!({"duel": 1}),
        at + Duration::seconds(30),
        at + Duration::seconds(90),
        at,
    )
    .await
    .expect("accept")
}

async fn accepted(client: &mut impl GenericClient, id: Uuid, at: DateTime<Utc>) -> Bout {
    match accept(client, id, at).await {
        AcceptResult::Accepted(bout) => *bout,
        other => panic!("the accept lands, got {other:?}"),
    }
}

/// `bettor` puts `bits` on `side` of `bout_id`, three presses at most.
async fn stake(
    client: &Client,
    bout_id: Uuid,
    bettor: &User,
    side: Side,
    bits: i64,
    at: DateTime<Utc>,
) -> StakeResult {
    BoutBet::stake(
        client,
        bout_id,
        bettor.id,
        &bettor.username,
        side,
        bits,
        3,
        at,
    )
    .await
    .expect("stake")
}

async fn is_unsettled(client: &Client, id: Uuid) -> bool {
    Bout::unsettled(client)
        .await
        .expect("unsettled")
        .iter()
        .any(|unsettled| unsettled.id == id)
}

async fn held(client: &Client, user: &User) -> bool {
    Bout::holds(client, user.id).await.expect("holds")
}

#[tokio::test]
async fn a_second_accept_while_a_bout_is_live_is_ring_taken_and_leaves_the_first_alone() {
    let test_db = test_db().await;
    let mut pooled = test_db.db.get().await.expect("db client");
    let client: &mut Client = &mut pooled;
    let tom = create_test_user(&test_db.db, "ring-tom").await;
    let mira = create_test_user(&test_db.db, "ring-mira").await;
    let dax = create_test_user(&test_db.db, "ring-dax").await;
    let ana = create_test_user(&test_db.db, "ring-ana").await;
    let at = now();
    let day = at.date_naive();

    let first = called(client, &call(&tom, &mira, day, at)).await;
    let second = called(client, &call(&dax, &ana, day, at)).await;
    assert!(!first.is_accepted());

    let live = accepted(&mut *client, first.id, at).await;
    assert!(live.is_accepted());
    assert_eq!(live.bell_at, Some(at + Duration::seconds(30)));
    assert_eq!(live.deadline_at, Some(at + Duration::seconds(90)));
    assert_eq!(live.caller_deck, Some(json!(["caller deck"])));
    assert_eq!(live.side_of(mira.id), Some(Side::Called));
    assert_eq!(live.side_of(dax.id), None);

    // The second accept runs inside a transaction holding the call's row
    // lock, the way a service runs it. The refusal costs that transaction
    // nothing: it reads on and commits.
    let mut tx = client.transaction().await.expect("tx");
    let locked = Bout::lock(&tx, second.id)
        .await
        .expect("lock")
        .expect("the open call");
    assert_eq!(
        accept(&mut tx, locked.id, at).await,
        AcceptResult::RingTaken
    );
    assert_eq!(
        Bout::get(&tx, live.id)
            .await
            .expect("read in the transaction"),
        Some(live.clone())
    );
    tx.commit().await.expect("commit");

    assert_eq!(
        Bout::get(&*client, second.id).await.expect("read"),
        Some(second),
        "the refused call is still open, untouched"
    );
    // Nobody new calls while the ring is taken.
    assert_eq!(
        Bout::call(&*client, &call(&ana, &tom, day, at))
            .await
            .expect("call"),
        CallResult::RingTaken
    );
}

#[tokio::test]
async fn a_pair_fights_once_a_day_whoever_calls() {
    let test_db = test_db().await;
    let mut pooled = test_db.db.get().await.expect("db client");
    let client: &mut Client = &mut pooled;
    let tom = create_test_user(&test_db.db, "pair-tom").await;
    let mira = create_test_user(&test_db.db, "pair-mira").await;
    let at = now();
    let day = at.date_naive();

    let first = called(client, &call(&tom, &mira, day, at)).await;
    accepted(&mut *client, first.id, at).await;
    let ended = Bout::end(
        &*client,
        first.id,
        BoutOutcome::Caller,
        &json!({"over": true}),
        at + Duration::minutes(5),
    )
    .await
    .expect("end")
    .expect("the bout ends");
    assert_eq!(ended.outcome, Some(BoutOutcome::Caller));
    assert_eq!(ended.state, Some(json!({"over": true})));

    // Mira calls tom back the same day: the call lands, the fight does not.
    let later = at + Duration::minutes(6);
    let rematch = called(client, &call(&mira, &tom, day, later)).await;
    assert_eq!(
        accept(&mut *client, rematch.id, later).await,
        AcceptResult::FoughtToday
    );
    assert!(
        !Bout::get(&*client, rematch.id)
            .await
            .expect("read")
            .expect("the call")
            .is_accepted()
    );

    // Tomorrow they fight again, to a draw.
    let tomorrow = called(client, &call(&tom, &mira, day + Duration::days(1), later)).await;
    accepted(&mut *client, tomorrow.id, later).await;
    Bout::end(
        &*client,
        tomorrow.id,
        BoutOutcome::Draw,
        &json!({}),
        later + Duration::minutes(5),
    )
    .await
    .expect("end")
    .expect("the bout ends");

    // The win counts once each way; the draw counts for neither.
    let records = Bout::records(&*client).await.expect("records");
    assert_eq!(records.get(&tom.id), Some(&(1, 0)));
    assert_eq!(records.get(&mira.id), Some(&(0, 1)));
}

#[tokio::test]
async fn expire_calls_ends_only_the_lapsed_unanswered_ones() {
    let test_db = test_db().await;
    let mut pooled = test_db.db.get().await.expect("db client");
    let client: &mut Client = &mut pooled;
    let tom = create_test_user(&test_db.db, "expire-tom").await;
    let mira = create_test_user(&test_db.db, "expire-mira").await;
    let dax = create_test_user(&test_db.db, "expire-dax").await;
    let at = now();
    let day = at.date_naive();

    let lapsing = called(client, &call(&tom, &mira, day, at)).await;
    let waiting = called(
        client,
        &NewCall {
            expires_at: at + Duration::minutes(10),
            ..call(&dax, &mira, day, at + Duration::seconds(1))
        },
    )
    .await;
    // Past its time too, but accepted: a bout, not a call to expire.
    let fought = called(client, &call(&mira, &tom, day, at)).await;
    // One open call per caller.
    assert_eq!(
        Bout::call(&*client, &call(&tom, &dax, day, at))
            .await
            .expect("call"),
        CallResult::AlreadyCalling
    );
    accepted(&mut *client, fought.id, at).await;

    let lapsed_at = at + Duration::minutes(2);
    let expired = Bout::expire_calls(&*client, lapsed_at)
        .await
        .expect("expire");
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].id, lapsing.id);
    assert_eq!(expired[0].outcome, Some(BoutOutcome::Expired));
    assert_eq!(expired[0].ended_at, Some(lapsed_at));
    assert!(
        Bout::expire_calls(&*client, lapsed_at)
            .await
            .expect("expire again")
            .is_empty()
    );
    // Ended once: a late decline finds nothing.
    assert!(
        Bout::end_call(&*client, lapsing.id, BoutOutcome::Declined, lapsed_at)
            .await
            .expect("decline")
            .is_none()
    );

    // The board keeps the bout and the waiting call, oldest call first,
    // and drops the lapsed one.
    let board: Vec<Uuid> = Bout::board(&*client, at)
        .await
        .expect("board")
        .iter()
        .map(|bout| bout.id)
        .collect();
    assert_eq!(board, vec![fought.id, waiting.id]);
}

#[tokio::test]
async fn a_stake_presses_its_side_up_to_the_cap_and_never_the_other() {
    let test_db = test_db().await;
    let mut pooled = test_db.db.get().await.expect("db client");
    let client: &mut Client = &mut pooled;
    let tom = create_test_user(&test_db.db, "stake-tom").await;
    let mira = create_test_user(&test_db.db, "stake-mira").await;
    let dax = create_test_user(&test_db.db, "stake-dax").await;
    let at = now();
    let bout = called(client, &call(&tom, &mira, at.date_naive(), at)).await;
    accepted(&mut *client, bout.id, at).await;

    let StakeResult::Staked(first) = stake(client, bout.id, &dax, Side::Caller, 10, at).await
    else {
        panic!("the first stake lands");
    };
    assert_eq!(
        (first.side, first.bits, first.presses),
        (Side::Caller, 10, 1)
    );
    let StakeResult::Staked(second) = stake(client, bout.id, &dax, Side::Caller, 15, at).await
    else {
        panic!("the second press lands");
    };
    assert_eq!((second.bits, second.presses), (25, 2));
    assert_eq!(
        stake(client, bout.id, &dax, Side::Called, 5, at).await,
        StakeResult::OtherSide
    );
    let StakeResult::Staked(third) = stake(client, bout.id, &dax, Side::Caller, 5, at).await else {
        panic!("the third press lands");
    };
    assert_eq!((third.bits, third.presses), (30, 3));
    assert_eq!(
        stake(client, bout.id, &dax, Side::Caller, 5, at).await,
        StakeResult::Capped
    );

    assert_eq!(
        BoutBet::list(&*client, bout.id).await.expect("list"),
        vec![third]
    );
}

#[tokio::test]
async fn the_pool_takes_bits_only_until_the_bell() {
    let test_db = test_db().await;
    let mut pooled = test_db.db.get().await.expect("db client");
    let client: &mut Client = &mut pooled;
    let tom = create_test_user(&test_db.db, "pool-tom").await;
    let mira = create_test_user(&test_db.db, "pool-mira").await;
    let at = now();
    let bout = called(client, &call(&tom, &mira, at.date_naive(), at)).await;

    // An open call has no pool yet.
    assert!(
        !Bout::add_to_pool(&*client, bout.id, Side::Caller, 40, at)
            .await
            .expect("pool")
    );
    accepted(&mut *client, bout.id, at).await;
    assert!(
        Bout::add_to_pool(&*client, bout.id, Side::Caller, 40, at)
            .await
            .expect("pool")
    );
    assert!(
        Bout::add_to_pool(
            &*client,
            bout.id,
            Side::Called,
            15,
            at + Duration::seconds(29)
        )
        .await
        .expect("pool")
    );
    // The bell rings thirty seconds after the accept.
    assert!(
        !Bout::add_to_pool(
            &*client,
            bout.id,
            Side::Caller,
            1,
            at + Duration::seconds(30)
        )
        .await
        .expect("pool")
    );

    let read = Bout::get(&*client, bout.id)
        .await
        .expect("read")
        .expect("the bout");
    assert_eq!((read.pool_caller, read.pool_called), (40, 15));
}

#[tokio::test]
async fn settle_and_pay_claims_are_each_taken_once_and_the_ring_holds_a_runner_until_theirs_is() {
    let test_db = test_db().await;
    let mut pooled = test_db.db.get().await.expect("db client");
    let client: &mut Client = &mut pooled;
    let tom = create_test_user(&test_db.db, "settle-tom").await;
    let mira = create_test_user(&test_db.db, "settle-mira").await;
    let dax = create_test_user(&test_db.db, "settle-dax").await;
    let at = now();
    let bout = called(client, &call(&tom, &mira, at.date_naive(), at)).await;
    // An open call holds nobody.
    assert!(!held(client, &tom).await);
    accepted(&mut *client, bout.id, at).await;
    stake(client, bout.id, &dax, Side::Caller, 20, at).await;
    // The ring holds both fighters and the bettor from here on.
    assert!(held(client, &tom).await && held(client, &mira).await && held(client, &dax).await);

    // Nothing to settle while the bout is being fought.
    assert!(
        !Bout::claim_fighter_settle(&*client, bout.id, Side::Caller, at)
            .await
            .expect("settle")
    );
    assert!(!is_unsettled(client, bout.id).await);

    let ended_at = at + Duration::minutes(5);
    Bout::end(&*client, bout.id, BoutOutcome::Caller, &json!({}), ended_at)
        .await
        .expect("end")
        .expect("the bout ends");
    assert!(
        Bout::end(&*client, bout.id, BoutOutcome::Called, &json!({}), ended_at)
            .await
            .expect("end again")
            .is_none(),
        "a bout ends once"
    );
    assert!(is_unsettled(client, bout.id).await);

    for side in [Side::Caller, Side::Called] {
        assert!(
            Bout::claim_fighter_settle(&*client, bout.id, side, ended_at)
                .await
                .expect("settle")
        );
        assert!(
            !Bout::claim_fighter_settle(&*client, bout.id, side, ended_at)
                .await
                .expect("settle again")
        );
    }
    // Both fighters settled, dax still unpaid.
    assert!(is_unsettled(client, bout.id).await);
    assert!(!held(client, &tom).await && !held(client, &mira).await && held(client, &dax).await);

    let paid = BoutBet::claim_pay(&*client, bout.id, dax.id, 40, ended_at)
        .await
        .expect("pay")
        .expect("the first claim pays");
    assert_eq!((paid.paid_at, paid.paid_bits), (Some(ended_at), Some(40)));
    assert!(
        BoutBet::claim_pay(&*client, bout.id, dax.id, 40, ended_at)
            .await
            .expect("pay again")
            .is_none()
    );
    assert!(
        BoutBet::unpaid(&*client, bout.id)
            .await
            .expect("unpaid")
            .is_empty()
    );
    assert!(!is_unsettled(client, bout.id).await);
    assert!(!held(client, &dax).await);
}

#[tokio::test]
async fn a_call_taken_back_cannot_ring_the_same_runner_again_until_its_time_is_up() {
    let test_db = test_db().await;
    let mut pooled = test_db.db.get().await.expect("db client");
    let client: &mut Client = &mut pooled;
    let tom = create_test_user(&test_db.db, "again-tom").await;
    let mira = create_test_user(&test_db.db, "again-mira").await;
    let dax = create_test_user(&test_db.db, "again-dax").await;
    let at = now();
    let day = at.date_naive();

    let first = called(client, &call(&tom, &mira, day, at)).await;
    Bout::end_call(&*client, first.id, BoutOutcome::Declined, at)
        .await
        .expect("take it back")
        .expect("the call ends");

    // The call would have waited two minutes: until then mira is not
    // rung again, though anybody else can be.
    let soon = at + Duration::seconds(5);
    assert_eq!(
        Bout::call(&*client, &call(&tom, &mira, day, soon))
            .await
            .expect("call again"),
        CallResult::TooSoon
    );
    let other = called(client, &call(&tom, &dax, day, soon)).await;
    Bout::end_call(&*client, other.id, BoutOutcome::Declined, soon)
        .await
        .expect("take it back")
        .expect("the call ends");

    let later = at + Duration::minutes(2);
    assert!(
        !called(client, &call(&tom, &mira, day, later))
            .await
            .is_accepted()
    );
}
