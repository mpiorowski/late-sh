use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use chrono::{DateTime, Duration, TimeZone, Utc};
use late_core::models::deadchannel_runner::DeadchannelRunner;
use late_core::models::deadchannel_spawn::{
    LoungeDay, LoungeDice, Spawn, SpawnKind, SpawnOutcome, Window,
};
use late_core::models::presence::{ClubhouseStand, PresenceRecord, Spot};
use late_core::test_utils::create_test_user;
use uuid::Uuid;

use super::{HitOutcome, LoungeService};
use crate::app::chat::notifications::svc::NotificationService;
use crate::app::chat::svc::ChatService;
use crate::app::deadchannel::fight::data::RULES;
use crate::app::deadchannel::fight::state::Sheet;
use crate::app::deadchannel::fight::svc::FightService;
use crate::app::deadchannel::lounge::state::lifetime_secs;
use crate::app::deadchannel::runner::state::Look;
use crate::app::deadchannel::runner::svc::{RunnerEntry, fixed_looks_rx};
use crate::app::games::chips::svc::ChipService;
use crate::app::presence::svc::PresenceService;
use crate::test_helpers::new_test_db;

/// A runner standing in #lounge, as presence and the directory see them.
struct Looking {
    user_id: Uuid,
    username: String,
}

fn record(looking: &Looking, lounge: bool) -> PresenceRecord {
    PresenceRecord {
        session_id: Uuid::now_v7(),
        user_id: looking.user_id,
        username: looking.username.clone(),
        clubhouse: ClubhouseStand {
            spot: Spot::Door,
            since_ms: 0,
            emote: None,
            petted_dog_at_ms: None,
        },
        nightcap: None,
        street: None,
        lounge,
    }
}

/// `names` runners with rows, the first `looking` of them looking at
/// #lounge; a lounge over them with no spawner behind it.
async fn lounge_with(
    names: &[&str],
    looking: usize,
) -> (late_core::test_utils::TestDb, Vec<Looking>, LoungeService) {
    let test_db = new_test_db().await;
    let client = test_db.db.get().await.expect("db client");
    let mut runners = Vec::new();
    let mut directory = HashMap::new();
    for name in names {
        let user = create_test_user(&test_db.db, name).await;
        let look = Look::random(1, &mut rand::thread_rng());
        DeadchannelRunner::ensure_for_user(&client, user.id, &look.to_json())
            .await
            .expect("a runner");
        directory.insert(
            user.id,
            RunnerEntry {
                look,
                level: 1,
                peak_level: 1,
                marks: 0,
            },
        );
        runners.push(Looking {
            user_id: user.id,
            username: user.username,
        });
    }
    let records = runners
        .iter()
        .enumerate()
        .map(|(index, runner)| record(runner, index < looking))
        .collect();
    let chat = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let svc = LoungeService::new(
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

/// Noon on a day of its own: the mid window, minute 720.
fn noon() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2101, 3, 9, 12, 0, 0).unwrap()
}

/// Dice that put every flicker at the start of its window and the howler
/// at midnight: everything is due the moment someone looks.
const EARLY_DICE: LoungeDice = LoungeDice {
    flicker_minutes: [0, 480, 960],
    howler_minute: 0,
};

async fn sheet_of(db: &late_core::db::Db, user_id: Uuid) -> Sheet {
    let client = db.get().await.expect("db client");
    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    Sheet::from_row(&row).expect("sheet")
}

#[tokio::test]
async fn a_flicker_lands_once_a_runner_looks_and_the_day_row_spends_it() {
    let (test_db, _runners, svc) = lounge_with(&["lounge-flicker"], 1).await;
    let client = test_db.db.get().await.expect("db client");
    let now = noon();
    LoungeDay::ensure(&**client, now.date_naive(), EARLY_DICE)
        .await
        .expect("the day");

    svc.sweep(now).await;
    let spawn = Spawn::live(&**client)
        .await
        .expect("live")
        .expect("a spawn landed");
    assert_eq!(spawn.kind, SpawnKind::Flicker);
    assert_eq!(spawn.hits_needed, 1);
    assert_eq!(spawn.spawned_at_ms, now.timestamp_millis());
    assert_eq!(
        spawn.expires_at_ms,
        now.timestamp_millis() + lifetime_secs(SpawnKind::Flicker) * 1_000
    );
    let day = LoungeDay::ensure(&**client, now.date_naive(), EARLY_DICE)
        .await
        .expect("the day");
    assert_eq!(day.flickers_spent, [false, true, false]);
    assert!(!day.howler_spent);

    // The same tick again: one live spawn at a time, nothing else lands.
    svc.sweep(now).await;
    assert_eq!(
        Spawn::live(&**client)
            .await
            .expect("live")
            .map(|live| live.id),
        Some(spawn.id)
    );

    // Past its time: it slips, and the sweeper ends it.
    let later = now + Duration::seconds(lifetime_secs(SpawnKind::Flicker));
    svc.sweep(later).await;
    let ended = Spawn::get(&**client, spawn.id)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(ended.outcome, Some(SpawnOutcome::Slipped));
    assert!(ended.hits.is_empty());
}

#[tokio::test]
async fn nothing_lands_with_nobody_looking_and_a_howler_waits_for_two() {
    let (test_db, _runners, svc) = lounge_with(&["lounge-alone"], 0).await;
    let client = test_db.db.get().await.expect("db client");
    let now = noon();
    LoungeDay::ensure(&**client, now.date_naive(), EARLY_DICE)
        .await
        .expect("the day");
    svc.sweep(now).await;
    assert_eq!(Spawn::live(&**client).await.expect("live"), None);

    // One runner looking, every flicker of the day spent: the howler
    // still waits for a second runner.
    let (test_db, _runners, svc) = lounge_with(&["lounge-one"], 1).await;
    let client = test_db.db.get().await.expect("db client");
    LoungeDay::ensure(&**client, now.date_naive(), EARLY_DICE)
        .await
        .expect("the day");
    let gap_closed_at = now;
    for window in Window::ALL {
        assert!(
            LoungeDay::claim_flicker(&**client, now.date_naive(), window, now, gap_closed_at)
                .await
                .expect("spend")
        );
    }
    svc.sweep(now + Duration::hours(1)).await;
    assert_eq!(Spawn::live(&**client).await.expect("live"), None);
}

#[tokio::test]
async fn one_hit_puts_a_flicker_down_and_pays_a_cache_through_the_sheet() {
    let (test_db, runners, svc) = lounge_with(&["lounge-hitter"], 1).await;
    let client = test_db.db.get().await.expect("db client");
    let now = noon();
    LoungeDay::ensure(&**client, now.date_naive(), EARLY_DICE)
        .await
        .expect("the day");
    svc.sweep(now).await;
    let spawn = Spawn::live(&**client)
        .await
        .expect("live")
        .expect("a spawn");
    let before = sheet_of(&test_db.db, runners[0].user_id).await;

    let outcome = svc
        .hit(
            runners[0].user_id,
            &runners[0].username,
            now + Duration::seconds(5),
        )
        .await
        .expect("hit");
    assert_eq!(
        outcome,
        HitOutcome::Landed {
            kind: SpawnKind::Flicker,
            left: 0,
            weapon: "bare hands",
        }
    );
    let down = Spawn::get(&**client, spawn.id)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(down.outcome, Some(SpawnOutcome::Slain));
    assert_eq!(down.hits.len(), 1);
    assert_eq!(down.hits[0].username, runners[0].username);

    // Paid before the hit answered: a cache's worth at level 1, on hand.
    let after = sheet_of(&test_db.db, runners[0].user_id).await;
    assert_eq!(after.bits, before.bits + RULES.cache(1));
    assert_eq!(after.crystals, before.crystals);

    // Nothing left to hit.
    assert_eq!(
        svc.hit(
            runners[0].user_id,
            &runners[0].username,
            now + Duration::seconds(6)
        )
        .await
        .expect("hit again"),
        HitOutcome::Nothing
    );
}

#[tokio::test]
async fn a_howler_takes_three_different_runners_and_pays_a_crystal_each() {
    let (test_db, runners, svc) =
        lounge_with(&["lounge-tom", "lounge-dax", "lounge-mira"], 2).await;
    let client = test_db.db.get().await.expect("db client");
    let now = noon();
    // Every flicker spent: the howler is what is due.
    let day = LoungeDay::ensure(&**client, now.date_naive(), EARLY_DICE)
        .await
        .expect("the day");
    for window in Window::ALL {
        LoungeDay::claim_flicker(&**client, day.day, window, now - Duration::hours(2), now)
            .await
            .expect("spend");
    }
    svc.sweep(now).await;
    let spawn = Spawn::live(&**client)
        .await
        .expect("live")
        .expect("a howler");
    assert_eq!(spawn.kind, SpawnKind::Howler);

    let at = now + Duration::seconds(10);
    let tom = &runners[0];
    assert_eq!(
        svc.hit(tom.user_id, &tom.username, at).await.expect("tom"),
        HitOutcome::Landed {
            kind: SpawnKind::Howler,
            left: 2,
            weapon: "bare hands",
        }
    );
    assert_eq!(
        svc.hit(tom.user_id, &tom.username, at)
            .await
            .expect("tom again"),
        HitOutcome::AlreadyHit
    );
    let dax = &runners[1];
    assert_eq!(
        svc.hit(dax.user_id, &dax.username, at).await.expect("dax"),
        HitOutcome::Landed {
            kind: SpawnKind::Howler,
            left: 1,
            weapon: "bare hands",
        }
    );
    // Two hits are not three: still live, nobody paid.
    assert!(
        Spawn::live(&**client)
            .await
            .expect("live")
            .is_some_and(|live| live.id == spawn.id)
    );
    assert_eq!(sheet_of(&test_db.db, tom.user_id).await.crystals, 0);

    let mira = &runners[2];
    assert_eq!(
        svc.hit(mira.user_id, &mira.username, at)
            .await
            .expect("mira"),
        HitOutcome::Landed {
            kind: SpawnKind::Howler,
            left: 0,
            weapon: "bare hands",
        }
    );
    for runner in &runners {
        let sheet = sheet_of(&test_db.db, runner.user_id).await;
        assert_eq!(sheet.crystals, 1, "{}", runner.username);
        assert_eq!(sheet.bits, 50, "{}", runner.username);
    }
    let down = Spawn::get(&**client, spawn.id)
        .await
        .expect("get")
        .expect("row");
    assert_eq!(down.outcome, Some(SpawnOutcome::Slain));
}

#[tokio::test]
async fn a_runner_with_the_signal_down_or_no_row_cannot_hit() {
    let (test_db, runners, svc) = lounge_with(&["lounge-down"], 1).await;
    let client = test_db.db.get().await.expect("db client");
    let now = noon();
    LoungeDay::ensure(&**client, now.date_naive(), EARLY_DICE)
        .await
        .expect("the day");
    svc.sweep(now).await;
    assert!(Spawn::live(&**client).await.expect("live").is_some());

    // The signal down on the row, today, so the roll does not lift it.
    let mut sheet = sheet_of(&test_db.db, runners[0].user_id).await;
    sheet.signal = 0;
    sheet.day = now.date_naive();
    DeadchannelRunner::store_sheet(&**client, sheet.to_write())
        .await
        .expect("store");
    // The hit at `now` is the sheet's day; the service settles under the
    // wall clock, so a day stamped in the future never rolls.
    assert_eq!(
        svc.hit(runners[0].user_id, &runners[0].username, now)
            .await
            .expect("hit"),
        HitOutcome::Down
    );
    assert_eq!(
        svc.hit(Uuid::now_v7(), "nobody", now).await.expect("hit"),
        HitOutcome::NoRunner
    );
    // Nobody touched it.
    let live = Spawn::live(&**client)
        .await
        .expect("live")
        .expect("still live");
    assert!(live.hits.is_empty());
}
