use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use chrono::{DateTime, Duration, Utc};
use late_core::models::deadchannel_bout::{Bout, BoutOutcome, Side};
use late_core::models::deadchannel_runner::DeadchannelRunner;
use late_core::models::presence::{ClubhouseStand, PresenceRecord, Spot};
use late_core::test_utils::{TestDb, create_test_user};
use uuid::Uuid;

use super::{Act, AnswerRefusal, BetRefusal, CallRefusal, LiveBout, Reply, RingService};
use crate::app::chat::notifications::svc::NotificationService;
use crate::app::chat::svc::ChatService;
use crate::app::deadchannel::fight::data::RULES;
use crate::app::deadchannel::fight::state::Sheet;
use crate::app::deadchannel::fight::svc::FightService;
use crate::app::deadchannel::ring::data::{
    BELL_SECS, CALL_SECS, PURSE_PERCENT, RAKE_PERCENT, ROUND_SECS,
};
use crate::app::deadchannel::ring::state::Move;
use crate::app::deadchannel::runner::state::Look;
use crate::app::deadchannel::runner::svc::{RunnerEntry, fixed_looks_rx};
use crate::app::games::chips::svc::ChipService;
use crate::app::presence::svc::PresenceService;
use crate::test_helpers::new_test_db;

/// The level every runner here stands at: a draft level, so a marked one
/// owes the ring a bout.
const LEVEL: i32 = 3;
/// The bits every runner here starts with.
const BITS: i64 = 10_000;

struct Runner {
    user_id: Uuid,
    username: String,
}

/// Runners at [`LEVEL`] with [`BITS`], all on the wire: `marked` of them
/// hold a mark, `others` do not. A ring over them with no sweeper and no
/// listener behind it: a test re-reads the board itself.
async fn ring_with(marked: &[&str], others: &[&str]) -> (TestDb, Vec<Runner>, RingService) {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let mut runners = Vec::new();
    let mut directory = HashMap::new();
    let mut records = Vec::new();
    for (name, marks) in marked
        .iter()
        .map(|name| (name, 1))
        .chain(others.iter().map(|name| (name, 0)))
    {
        let user = create_test_user(&test_db.db, name).await;
        let look = Look::random(1, &mut rand::thread_rng());
        let (row, _) = DeadchannelRunner::ensure_for_user(&client, user.id, &look.to_json())
            .await
            .expect("a runner");
        let mut sheet = Sheet::from_row(&row).expect("sheet");
        sheet.day = FightService::today();
        sheet.level = LEVEL;
        sheet.peak_level = LEVEL;
        sheet.draft_up();
        sheet.marks = marks;
        sheet.bits = BITS;
        DeadchannelRunner::store_sheet(&**client, sheet.to_write())
            .await
            .expect("store");
        directory.insert(
            user.id,
            RunnerEntry {
                look,
                level: LEVEL,
                peak_level: LEVEL,
                marks,
            },
        );
        records.push(PresenceRecord {
            session_id: Uuid::now_v7(),
            user_id: user.id,
            username: user.username.clone(),
            clubhouse: ClubhouseStand {
                spot: Spot::Door,
                since_ms: 0,
                emote: None,
                petted_dog_at_ms: None,
            },
            nightcap: None,
            street: None,
            lounge: false,
        });
        runners.push(Runner {
            user_id: user.id,
            username: user.username,
        });
    }
    let chat = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let svc = RingService::new(
        test_db.db.clone(),
        FightService::new(
            test_db.db.clone(),
            chat.clone(),
            ChipService::new(test_db.db.clone()),
        ),
        chat,
        PresenceService::detached(records),
        fixed_looks_rx(directory),
        Arc::new(AtomicBool::new(false)),
    );
    (test_db, runners, svc)
}

async fn sheet_of(test_db: &TestDb, runner: &Runner) -> Sheet {
    let client = test_db.db.get().await.expect("db client");
    let row = DeadchannelRunner::find_by_user(&client, runner.user_id)
        .await
        .expect("find")
        .expect("row");
    Sheet::from_row(&row).expect("sheet")
}

async fn bits_of(test_db: &TestDb, runners: [&Runner; 4]) -> [i64; 4] {
    let mut bits = [0; 4];
    for (at, runner) in runners.into_iter().enumerate() {
        bits[at] = sheet_of(test_db, runner).await.bits;
    }
    bits
}

/// The board as the rows have it right now.
async fn board(svc: &RingService) -> super::RingBoard {
    svc.refresh().await.expect("refresh");
    svc.subscribe().borrow().clone()
}

async fn live(svc: &RingService) -> LiveBout {
    board(svc).await.live.clone().expect("a bout in the ring")
}

/// `caller` calls `called` at `at` and `called` takes it: the bout's id.
async fn set(svc: &RingService, caller: &Runner, called: &Runner, at: DateTime<Utc>) -> Uuid {
    assert_eq!(
        svc.call(caller.user_id, &caller.username, called.user_id, at)
            .await
            .expect("call"),
        Reply::Called
    );
    let call = board(svc).await.calls[0].id;
    assert_eq!(
        svc.accept(called.user_id, call, at).await.expect("accept"),
        Reply::Accepted
    );
    call
}

/// Let the clock play the bout set at `at` to its end: the bout as it
/// ended.
async fn fight_out(svc: &RingService, at: DateTime<Utc>) -> LiveBout {
    let mut now = at + Duration::seconds(BELL_SECS);
    loop {
        now += Duration::seconds(ROUND_SECS * 2);
        let _ = board(svc).await;
        svc.sweep(now, false).await;
        let bout = live(svc).await;
        if bout.bout.ended_at.is_some() {
            return bout;
        }
    }
}

#[tokio::test]
async fn one_bout_holds_the_ring_and_a_pair_fights_once_a_day() {
    let (_db, runners, svc) = ring_with(&["mira", "tom", "ash"], &["wren"]).await;
    let [mira, tom, ash, wren] = &runners[..] else {
        panic!("four runners");
    };
    let at = Utc::now();

    // The ring is for the marked, from both ends of a call.
    let call = |from: &Runner, to: &Runner| {
        let svc = svc.clone();
        let (from_id, from_name, to_id) = (from.user_id, from.username.clone(), to.user_id);
        async move {
            svc.call(from_id, &from_name, to_id, at)
                .await
                .expect("call")
        }
    };
    assert_eq!(
        call(wren, mira).await,
        Reply::CallRefused(CallRefusal::NotMarked)
    );
    assert_eq!(
        call(mira, wren).await,
        Reply::CallRefused(CallRefusal::TheyAreNotMarked)
    );

    // An open call from ash, then mira and tom take the ring.
    assert_eq!(call(ash, tom).await, Reply::Called);
    assert_eq!(
        call(ash, mira).await,
        Reply::CallRefused(CallRefusal::AlreadyCalling)
    );
    assert_eq!(call(mira, tom).await, Reply::Called);
    let open = board(&svc).await.calls.clone();
    let from = |runner: &Runner| {
        open.iter()
            .find(|call| call.caller_user_id == runner.user_id)
            .expect("the open call")
            .id
    };
    assert_eq!(
        svc.accept(ash.user_id, from(mira), at)
            .await
            .expect("accept"),
        Reply::AnswerRefused(AnswerRefusal::NotYours)
    );
    assert_eq!(
        svc.accept(tom.user_id, from(mira), at)
            .await
            .expect("accept"),
        Reply::Accepted
    );
    // The ring is taken: ash's call cannot be accepted, and nobody calls.
    assert_eq!(
        svc.accept(tom.user_id, from(ash), at)
            .await
            .expect("accept"),
        Reply::AnswerRefused(AnswerRefusal::RingTaken)
    );
    assert_eq!(
        svc.decline(ash.user_id, from(ash), at)
            .await
            .expect("take back"),
        Reply::Declined
    );
    assert_eq!(
        call(ash, tom).await,
        Reply::CallRefused(CallRefusal::RingTaken)
    );

    // The bout ends and the ring is free, but not for the same pair
    // today: the call is ended on the spot instead of left to lapse.
    fight_out(&svc, at).await;
    assert_eq!(call(tom, mira).await, Reply::Called);
    let rematch = board(&svc).await.calls[0].id;
    assert_eq!(
        svc.accept(mira.user_id, rematch, at).await.expect("accept"),
        Reply::AnswerRefused(AnswerRefusal::FoughtToday)
    );
    assert_eq!(board(&svc).await.calls, vec![]);
    assert_eq!(call(ash, tom).await, Reply::Called);
}

#[tokio::test]
async fn a_call_nobody_answers_lapses() {
    let (_db, runners, svc) = ring_with(&["mira", "tom"], &[]).await;
    let [mira, tom] = &runners[..] else {
        panic!("two runners");
    };
    let at = Utc::now();
    assert_eq!(
        svc.call(mira.user_id, &mira.username, tom.user_id, at)
            .await
            .expect("call"),
        Reply::Called
    );
    let call = board(&svc).await.calls[0].id;

    // A second short of its time, the call stands.
    svc.sweep(at + Duration::seconds(CALL_SECS - 1), false)
        .await;
    assert_eq!(board(&svc).await.calls.len(), 1);

    svc.sweep(at + Duration::seconds(CALL_SECS), false).await;
    assert_eq!(board(&svc).await.calls, vec![]);
    assert_eq!(
        svc.accept(tom.user_id, call, at + Duration::seconds(CALL_SECS))
            .await
            .expect("accept"),
        Reply::AnswerRefused(AnswerRefusal::Gone)
    );
}

#[tokio::test]
async fn a_round_waits_for_the_bell_and_the_clock_plays_a_corner_that_laid_nothing() {
    let (_db, runners, svc) = ring_with(&["mira", "tom"], &[]).await;
    let [mira, tom] = &runners[..] else {
        panic!("two runners");
    };
    let at = Utc::now();
    let bout = set(&svc, mira, tom, at).await;
    let bell = at + Duration::seconds(BELL_SECS);

    // Both corners lock round 1 during the wait, holding: nothing comes
    // out before the bell.
    for runner in [mira, tom] {
        assert_eq!(
            svc.act(runner.user_id, bout, Act::Lock, at)
                .await
                .expect("lock"),
            Reply::Laid
        );
    }
    let _ = board(&svc).await;
    svc.sweep(bell - Duration::seconds(1), false).await;
    assert_eq!(live(&svc).await.duel.tape.len(), 0);

    // At the bell the locked round resolves, without waiting out its
    // clock: two corners that held.
    svc.sweep(bell, false).await;
    let after_one = live(&svc).await;
    assert_eq!((after_one.duel.round, after_one.duel.tape.len()), (2, 1));
    assert!(
        after_one.duel.tape[0]
            .iter()
            .flatten()
            .all(|half| half.mv == Move::Empty)
    );

    // Round 2: nobody touches it. Short of its clock nothing moves; past
    // it the clock lays a round for both corners and plays it.
    let deadline = after_one.bout.deadline_at.expect("round 2 is on the clock");
    svc.sweep(deadline - Duration::seconds(1), false).await;
    assert_eq!(live(&svc).await.duel.tape.len(), 1);
    svc.sweep(deadline, false).await;
    let after_two = live(&svc).await;
    assert_eq!(after_two.duel.tape.len(), 2);
    for seat in 0..2 {
        assert!(
            after_two.duel.tape[1]
                .iter()
                .any(|beat| matches!(beat[seat].mv, Move::Played(_))),
            "the clock played a card for corner {seat}"
        );
    }
}

#[tokio::test]
async fn a_bet_takes_bits_until_the_bell_one_corner_and_three_presses() {
    let (test_db, runners, svc) = ring_with(&["mira", "tom"], &["wren", "ash"]).await;
    let [mira, tom, wren, ash] = &runners[..] else {
        panic!("four runners");
    };
    let at = Utc::now();
    let bout = set(&svc, mira, tom, at).await;
    let stake = RULES.cache(LEVEL);
    let bet = |runner: &Runner, side: Side, when: DateTime<Utc>| {
        let svc = svc.clone();
        let (user_id, username) = (runner.user_id, runner.username.clone());
        async move {
            svc.bet(user_id, &username, bout, side, when)
                .await
                .expect("bet")
        }
    };

    // Three presses on one corner, and no fourth; never the other corner.
    for _ in 0..3 {
        assert_eq!(
            bet(wren, Side::Caller, at).await,
            Reply::Staked { bits: stake }
        );
    }
    assert_eq!(
        bet(wren, Side::Caller, at).await,
        Reply::BetRefused(BetRefusal::Capped)
    );
    assert_eq!(
        bet(wren, Side::Called, at).await,
        Reply::BetRefused(BetRefusal::OtherSide)
    );
    // A fighter backs their own corner or nobody.
    assert_eq!(
        bet(tom, Side::Caller, at).await,
        Reply::BetRefused(BetRefusal::OwnCornerOnly)
    );
    assert_eq!(
        bet(tom, Side::Called, at).await,
        Reply::Staked { bits: stake }
    );
    // At the bell the book is shut.
    assert_eq!(
        bet(ash, Side::Called, at + Duration::seconds(BELL_SECS)).await,
        Reply::BetRefused(BetRefusal::Closed)
    );

    // Every refused press left the bits and the pool alone.
    let on_row = live(&svc).await.bout;
    assert_eq!((on_row.pool_caller, on_row.pool_called), (stake * 3, stake));
    assert_eq!(
        (
            sheet_of(&test_db, wren).await.bits,
            sheet_of(&test_db, tom).await.bits,
            sheet_of(&test_db, ash).await.bits,
            sheet_of(&test_db, mira).await.bits,
        ),
        (BITS - stake * 3, BITS - stake, BITS, BITS)
    );
}

#[tokio::test]
async fn a_finished_bout_opens_both_gates_and_pays_every_bet_once() {
    let (test_db, runners, svc) = ring_with(&["mira", "tom"], &["wren", "ash"]).await;
    let [mira, tom, wren, ash] = &runners[..] else {
        panic!("four runners");
    };
    let at = Utc::now();
    assert_eq!(sheet_of(&test_db, mira).await.ring_gate(), Some(LEVEL));
    let bout = set(&svc, mira, tom, at).await;
    let stake = RULES.cache(LEVEL);
    for (runner, side) in [(wren, Side::Caller), (ash, Side::Called)] {
        assert_eq!(
            svc.bet(runner.user_id, &runner.username, bout, side, at)
                .await
                .expect("bet"),
            Reply::Staked { bits: stake }
        );
    }

    let ended = fight_out(&svc, at).await;

    // Win, lose or draw, the bout was fought: both roads are open.
    assert_eq!(
        (
            sheet_of(&test_db, mira).await.ring_gate(),
            sheet_of(&test_db, tom).await.ring_gate()
        ),
        (None, None)
    );
    // The pool is two stakes: a tenth burned, a tenth to the winning
    // fighter, the rest to the one bet on them. A draw hands both back.
    let pool = stake * 2;
    let purse = pool * PURSE_PERCENT / 100;
    let split = pool - purse - pool * RAKE_PERCENT / 100;
    let bits = bits_of(&test_db, [mira, tom, wren, ash]).await;
    let [mira_bits, tom_bits, wren_bits, ash_bits] = bits;
    let expected = match ended.bout.outcome.expect("an ended bout has an outcome") {
        BoutOutcome::Caller => (BITS + purse, BITS, BITS - stake + split, BITS - stake),
        BoutOutcome::Called => (BITS, BITS + purse, BITS - stake, BITS - stake + split),
        BoutOutcome::Draw => (BITS, BITS, BITS, BITS),
        BoutOutcome::Declined | BoutOutcome::Expired => panic!("a fought bout ends fought"),
    };
    assert_eq!((mira_bits, tom_bits, wren_bits, ash_bits), expected);

    // A second settle of the same bout (another replica, a late sweeper)
    // finds every claim taken and moves nothing.
    let ending = ended.duel.ending.expect("the duel ended");
    svc.settle(&ended.bout, ending, at + Duration::hours(1))
        .await;
    svc.sweep(at + Duration::hours(1), true).await;
    assert_eq!(bits_of(&test_db, [mira, tom, wren, ash]).await, bits);
    let client = test_db.db.get().await.expect("db client");
    assert_eq!(Bout::unsettled(&**client).await.expect("unsettled"), vec![]);
}

#[tokio::test]
async fn the_nuke_leaves_the_ring_nothing_to_pay() {
    let (test_db, runners, svc) = ring_with(&["mira", "tom", "ash"], &["wren"]).await;
    let [mira, tom, ash, wren] = &runners[..] else {
        panic!("four runners");
    };
    let at = Utc::now();
    // An open call, a bout set with a bet on it, and a gate already
    // cleared on this climb.
    assert_eq!(
        svc.call(ash.user_id, &ash.username, tom.user_id, at)
            .await
            .expect("call"),
        Reply::Called
    );
    assert_eq!(
        svc.call(mira.user_id, &mira.username, tom.user_id, at)
            .await
            .expect("call"),
        Reply::Called
    );
    let from_mira = board(&svc)
        .await
        .calls
        .iter()
        .find(|call| call.caller_user_id == mira.user_id)
        .expect("mira's call")
        .id;
    assert_eq!(
        svc.accept(tom.user_id, from_mira, at)
            .await
            .expect("accept"),
        Reply::Accepted
    );
    assert_eq!(
        svc.bet(wren.user_id, &wren.username, from_mira, Side::Caller, at)
            .await
            .expect("bet"),
        Reply::Staked {
            bits: RULES.cache(LEVEL)
        }
    );
    let client = test_db.db.get().await.expect("db client");
    let mut cleared = sheet_of(&test_db, mira).await;
    cleared.ring_cleared = LEVEL;
    DeadchannelRunner::store_sheet(&**client, cleared.to_write())
        .await
        .expect("store");

    client
        .execute("SELECT deadchannel_nuke_runners()", &[])
        .await
        .expect("the nuke");

    // No call is left open, the bout is over and settled, and the gates
    // went back with the sheets.
    let after = board(&svc).await;
    assert_eq!(after.calls, vec![]);
    let ended = after.live.as_ref().expect("the bout, in its aftermath");
    assert_eq!(ended.bout.outcome, Some(BoutOutcome::Draw));
    assert_eq!(Bout::unsettled(&**client).await.expect("unsettled"), vec![]);
    assert_eq!(sheet_of(&test_db, mira).await.ring_cleared, 0);
    // A sweeper on any replica afterwards hands nothing out: every sheet
    // holds the fresh fifty and nothing more.
    let fresh = bits_of(&test_db, [mira, tom, ash, wren]).await;
    svc.sweep(at + Duration::hours(1), true).await;
    assert_eq!(bits_of(&test_db, [mira, tom, ash, wren]).await, fresh);
    assert_eq!(fresh, [50; 4]);
}
