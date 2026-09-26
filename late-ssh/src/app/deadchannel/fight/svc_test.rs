use chrono::Utc;
use late_core::models::deadchannel_runner::DeadchannelRunner;
use late_core::test_utils::create_test_user;
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

use super::{FightOutcome, FightService};
use crate::app::chat::notifications::svc::NotificationService;
use crate::app::chat::svc::ChatService;
use crate::app::deadchannel::fight::data::RATIONS_PER_DAY;
use crate::app::deadchannel::fight::state::{Applied, Command, Sheet, Slot};
use crate::app::deadchannel::runner::state::Look;
use crate::test_helpers::new_test_db;

async fn runner_and_service(
    name: &str,
) -> (late_core::test_utils::TestDb, uuid::Uuid, FightService) {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, name).await;
    let client = test_db.db.get().await.expect("db client");
    let look = Look::random(1, &mut rand::thread_rng());
    DeadchannelRunner::ensure_for_user(&client, user.id, &look.to_json())
        .await
        .expect("a runner");
    let chat = ChatService::new(
        test_db.db.clone(),
        NotificationService::new(test_db.db.clone()),
    );
    let svc = FightService::new(test_db.db.clone(), chat);
    (test_db, user.id, svc)
}

async fn answer(rx: &mut mpsc::UnboundedReceiver<FightOutcome>) -> FightOutcome {
    timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("an answer in time")
        .expect("the task answers")
}

#[tokio::test]
async fn stepping_in_spends_a_ration_on_the_row() {
    let (test_db, user_id, svc) = runner_and_service("fight-svc-start").await;
    let (tx, mut rx) = mpsc::unbounded_channel();

    svc.act_task(user_id, "mira".to_string(), Command::Start, tx.clone());
    let FightOutcome::Acted { sheet, outcome } = answer(&mut rx).await else {
        panic!("a start answers with the sheet");
    };
    assert_eq!(outcome.applied, Applied::Started);
    assert_eq!(sheet.rations_left, RATIONS_PER_DAY - 1);
    assert!(sheet.fight.is_some());

    // The row is the truth: the ration and the fight landed.
    let client = test_db.db.get().await.expect("db client");
    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!(row.rations_left, RATIONS_PER_DAY - 1);
    assert!(row.fight.is_some());
    assert_eq!(Sheet::from_row(&row).expect("sheet"), sheet);

    // A second device stepping in finds the same fight and spends nothing.
    svc.act_task(user_id, "mira".to_string(), Command::Start, tx);
    let FightOutcome::Acted { sheet, outcome } = answer(&mut rx).await else {
        panic!("a resume answers with the sheet");
    };
    assert_eq!(outcome.applied, Applied::Resumed);
    assert_eq!(sheet.rations_left, RATIONS_PER_DAY - 1);
}

#[tokio::test]
async fn a_purchase_at_the_armorer_lands_on_the_row() {
    let (test_db, user_id, svc) = runner_and_service("fight-svc-outfit").await;
    let (tx, mut rx) = mpsc::unbounded_channel();

    // A fresh runner's 50 bits buy the tier 1 weapon (48) and nothing more.
    svc.act_task(
        user_id,
        "mira".to_string(),
        Command::Outfit {
            slot: Slot::Weapon,
            tier: 1,
        },
        tx.clone(),
    );
    let FightOutcome::Acted { sheet, outcome } = answer(&mut rx).await else {
        panic!("a purchase answers with the sheet");
    };
    assert_eq!(
        outcome.applied,
        Applied::Outfitted {
            slot: Slot::Weapon,
            tier: 1,
            paid: 48
        }
    );
    assert_eq!((sheet.weapon_tier, sheet.bits), (1, 2));

    svc.act_task(
        user_id,
        "mira".to_string(),
        Command::Outfit {
            slot: Slot::Armor,
            tier: 1,
        },
        tx,
    );
    let FightOutcome::Acted { outcome, .. } = answer(&mut rx).await else {
        panic!("a refusal answers with the sheet");
    };
    assert!(
        matches!(outcome.applied, Applied::Refused(_)),
        "{outcome:?}"
    );

    let client = test_db.db.get().await.expect("db client");
    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!((row.weapon_tier, row.armor_tier, row.bits), (1, 0, 2));
}

#[tokio::test]
async fn a_runner_who_left_has_no_sheet_to_act_on() {
    let (test_db, user_id, svc) = runner_and_service("fight-svc-left").await;
    let client = test_db.db.get().await.expect("db client");
    assert!(
        DeadchannelRunner::mark_left(&client, user_id)
            .await
            .expect("leave")
    );
    let (tx, mut rx) = mpsc::unbounded_channel();

    svc.act_task(user_id, "mira".to_string(), Command::Start, tx.clone());
    assert!(matches!(answer(&mut rx).await, FightOutcome::NoRunner));

    svc.reload_task(user_id, tx);
    assert!(matches!(answer(&mut rx).await, FightOutcome::NoRunner));
}

#[tokio::test]
async fn the_descent_rolls_a_stale_day() {
    let (test_db, user_id, svc) = runner_and_service("fight-svc-roll").await;
    let client = test_db.db.get().await.expect("db client");
    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    let mut stale = Sheet::from_row(&row).expect("sheet");
    stale.day = Utc::now().date_naive().pred_opt().expect("yesterday");
    stale.rations_left = 0;
    stale.signal = 0;
    DeadchannelRunner::store_sheet(&**client, stale.to_write())
        .await
        .expect("store a spent yesterday");

    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.reload_task(user_id, tx);
    let FightOutcome::Reloaded { sheet } = answer(&mut rx).await else {
        panic!("a reload answers with the sheet");
    };
    assert_eq!(sheet.day, Utc::now().date_naive());
    assert_eq!(sheet.rations_left, RATIONS_PER_DAY);
    assert_eq!(sheet.signal, sheet.max_signal());

    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!(
        row.rations_left, RATIONS_PER_DAY,
        "the roll is stored, not only shown"
    );
}

/// The kill through the service: the reset lands on the row, the peak
/// stays, and the first kill grants the `SIG` badge once; a second kill
/// leaves a second mark and no second badge.
#[tokio::test]
async fn putting_the_old_signal_down_resets_the_row_and_grants_the_badge() {
    use crate::app::deadchannel::fight::data::{MAX_LEVEL, OLD_SIGNAL_TIER, exp_to_seek};
    use crate::app::deadchannel::fight::state::MAX_TIER;
    use late_core::models::profile_award::{
        DEADCHANNEL_OLD_SIGNAL_AWARD_CATEGORY, ProfileAward, list_profile_awards_for_user,
    };

    let (test_db, user_id, svc) = runner_and_service("fight-svc-old-signal").await;
    let client = test_db.db.get().await.expect("db client");
    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    let mut sheet = Sheet::from_row(&row).expect("sheet");
    sheet.day = FightService::today();
    sheet.level = MAX_LEVEL;
    sheet.peak_level = MAX_LEVEL;
    sheet.exp = exp_to_seek(0);
    sheet.weapon_tier = MAX_TIER;
    sheet.armor_tier = MAX_TIER;
    sheet.signal = sheet.max_signal();
    DeadchannelRunner::store_sheet(&**client, sheet.to_write())
        .await
        .expect("store");

    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.act_task(user_id, "mira".to_string(), Command::Start, tx.clone());
    let FightOutcome::Acted { sheet, .. } = answer(&mut rx).await else {
        panic!("a start answers with the sheet");
    };
    let mut sheet = sheet;
    let fight = sheet.fight.as_mut().expect("the Old Signal");
    assert_eq!(fight.foe_max_signal, OLD_SIGNAL_TIER.signal);
    fight.foe_signal = 1;
    DeadchannelRunner::store_sheet(&**client, sheet.to_write())
        .await
        .expect("store");

    let slain = loop {
        svc.act_task(user_id, "mira".to_string(), Command::Attack, tx.clone());
        let FightOutcome::Acted { outcome, .. } = answer(&mut rx).await else {
            panic!("an attack answers with the sheet");
        };
        if outcome.applied != Applied::Round {
            break outcome.applied;
        }
    };
    assert_eq!(slain, Applied::Slain { marks: 1 });

    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!(
        (
            row.level,
            row.peak_level,
            row.marks,
            row.weapon_tier,
            row.armor_tier
        ),
        (1, MAX_LEVEL, 1, 0, 0)
    );
    let badges = |awards: Vec<ProfileAward>| {
        awards
            .into_iter()
            .filter(|award| award.category == DEADCHANNEL_OLD_SIGNAL_AWARD_CATEGORY)
            .map(|award| award.score_value)
            .collect::<Vec<_>>()
    };
    let awards = list_profile_awards_for_user(&client, user_id)
        .await
        .expect("awards");
    assert_eq!(badges(awards), vec![1], "one badge, granted on mark 1");

    // The climb again, to the top with the mark's own threshold.
    let mut sheet = Sheet::from_row(&row).expect("sheet");
    sheet.level = MAX_LEVEL;
    sheet.peak_level = MAX_LEVEL;
    sheet.exp = exp_to_seek(1);
    sheet.weapon_tier = MAX_TIER;
    sheet.armor_tier = MAX_TIER;
    sheet.signal = sheet.max_signal();
    DeadchannelRunner::store_sheet(&**client, sheet.to_write())
        .await
        .expect("store");
    svc.act_task(user_id, "mira".to_string(), Command::Start, tx.clone());
    let FightOutcome::Acted { sheet, .. } = answer(&mut rx).await else {
        panic!("a start answers with the sheet");
    };
    let mut sheet = sheet;
    sheet
        .fight
        .as_mut()
        .expect("the Old Signal again")
        .foe_signal = 1;
    DeadchannelRunner::store_sheet(&**client, sheet.to_write())
        .await
        .expect("store");
    let slain = loop {
        svc.act_task(user_id, "mira".to_string(), Command::Attack, tx.clone());
        let FightOutcome::Acted { outcome, .. } = answer(&mut rx).await else {
            panic!("an attack answers with the sheet");
        };
        if outcome.applied != Applied::Round {
            break outcome.applied;
        }
    };
    assert_eq!(slain, Applied::Slain { marks: 2 });

    let awards = list_profile_awards_for_user(&client, user_id)
        .await
        .expect("awards");
    assert_eq!(
        badges(awards),
        vec![1],
        "the second kill leaves the badge as the first granted it"
    );
}
