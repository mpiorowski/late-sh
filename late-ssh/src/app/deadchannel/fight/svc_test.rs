use chrono::Utc;
use late_core::models::chat_message::ChatMessage;
use late_core::models::chat_room::ChatRoom;
use late_core::models::deadchannel_ambush::{Ambush, AmbushOutcome, NewAmbush};
use late_core::models::deadchannel_runner::DeadchannelRunner;
use late_core::models::notification::Notification;
use late_core::models::user::User;
use late_core::test_utils::create_test_user;
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

use super::{FightOutcome, FightService};
use crate::app::chat::notifications::svc::NotificationService;
use crate::app::chat::svc::ChatService;
use crate::app::deadchannel::fight::data::{CROSSING_LEVEL_BAND, RATIONS_PER_DAY};
use crate::app::deadchannel::fight::road::{Mark, Node, Trace};
use crate::app::deadchannel::fight::state::{
    Applied, Call, Command, Pick, Prey, Refusal, Sheet, Slot,
};
use crate::app::deadchannel::runner::state::Look;
use crate::app::games::chips::svc::ChipService;
use crate::test_helpers::{age_payout_claims, new_test_db, wait_until};

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
    let svc = FightService::new(
        test_db.db.clone(),
        chat,
        ChipService::new(test_db.db.clone()),
    );
    (test_db, user.id, svc)
}

/// The first step of every day's road: a glyph in every lane.
const STEP_IN: Command = Command::Step {
    lane: 1,
    call: Call::Fight(Pick::Fair),
};

async fn answer(rx: &mut mpsc::UnboundedReceiver<FightOutcome>) -> FightOutcome {
    timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("an answer in time")
        .expect("the task answers")
}

#[tokio::test]
async fn a_step_spends_a_ration_and_lands_the_fight_and_the_road_on_the_row() {
    let (test_db, user_id, svc) = runner_and_service("fight-svc-start").await;
    let (tx, mut rx) = mpsc::unbounded_channel();

    svc.act_task(user_id, "mira".to_string(), STEP_IN, tx.clone());
    let FightOutcome::Acted { sheet, outcome, .. } = answer(&mut rx).await else {
        panic!("a start answers with the sheet");
    };
    assert_eq!(outcome.applied, Applied::Started { pick: Pick::Fair });
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
    assert!(row.road.is_some());
    assert_eq!(Sheet::from_row(&row).expect("sheet"), sheet);
    assert_eq!(
        sheet.road.path,
        vec![Trace {
            lane: 1,
            mark: Mark::Fighting
        }]
    );

    // A second device stepping in finds the same fight and spends nothing.
    svc.act_task(user_id, "mira".to_string(), STEP_IN, tx);
    let FightOutcome::Acted { sheet, outcome, .. } = answer(&mut rx).await else {
        panic!("a resume answers with the sheet");
    };
    assert_eq!(outcome.applied, Applied::Resumed);
    assert_eq!(sheet.rations_left, RATIONS_PER_DAY - 1);

    // The hand is the row's: the `Auto` key plays a turn of it there, and
    // what the row holds after is what the answer showed.
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.act_task(user_id, "mira".to_string(), Command::Auto, tx);
    let FightOutcome::Acted { sheet, outcome, .. } = answer(&mut rx).await else {
        panic!("a turn answers with the sheet");
    };
    assert!(!outcome.lines.is_empty());
    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!(Sheet::from_row(&row).expect("sheet"), sheet);
}

#[tokio::test]
async fn a_purchase_at_the_armorer_lands_on_the_row() {
    let (test_db, user_id, svc) = runner_and_service("fight-svc-outfit").await;
    let client = test_db.db.get().await.expect("db client");
    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    let mut sheet = Sheet::from_row(&row).expect("sheet");
    sheet.day = FightService::today();
    sheet.bits = 110;
    DeadchannelRunner::store_sheet(&**client, sheet.to_write())
        .await
        .expect("store");
    let (tx, mut rx) = mpsc::unbounded_channel();

    // 110 bits buy the tier 1 weapon (108) and nothing more.
    svc.act_task(
        user_id,
        "mira".to_string(),
        Command::Outfit {
            slot: Slot::Weapon,
            tier: 1,
        },
        tx.clone(),
    );
    let FightOutcome::Acted { sheet, outcome, .. } = answer(&mut rx).await else {
        panic!("a purchase answers with the sheet");
    };
    assert_eq!(
        outcome.applied,
        Applied::Outfitted {
            slot: Slot::Weapon,
            tier: 1,
            paid: 108
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

    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!((row.weapon_tier, row.armor_tier, row.bits), (1, 0, 2));
}

#[tokio::test]
async fn a_loan_and_a_deposit_land_on_the_row() {
    let (test_db, user_id, svc) = runner_and_service("fight-svc-money").await;
    let (tx, mut rx) = mpsc::unbounded_channel();

    // A fresh runner's 50 bits, the level-1 loan of 108 (the wall price
    // of a piece, 11 more on the debt), then all 158 into the locker less
    // its 16.
    svc.act_task(user_id, "mira".to_string(), Command::Borrow, tx.clone());
    let FightOutcome::Acted { outcome, .. } = answer(&mut rx).await else {
        panic!("a loan answers with the sheet");
    };
    assert_eq!(
        outcome.applied,
        Applied::Borrowed {
            amount: 108,
            fee: 11
        }
    );
    svc.act_task(user_id, "mira".to_string(), Command::Deposit, tx);
    let FightOutcome::Acted { sheet, outcome, .. } = answer(&mut rx).await else {
        panic!("a deposit answers with the sheet");
    };
    assert_eq!(
        outcome.applied,
        Applied::Deposited {
            stored: 142,
            fee: 16
        }
    );
    assert_eq!((sheet.bits, sheet.stash, sheet.debt), (0, 142, 119));

    let client = test_db.db.get().await.expect("db client");
    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!((row.bits, row.stash, row.debt), (0, 142, 119));
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

    svc.act_task(user_id, "mira".to_string(), STEP_IN, tx.clone());
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
    let FightOutcome::Reloaded { sheet, .. } = answer(&mut rx).await else {
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

/// Put the runner at the top with the gate open and full kit, step in,
/// knock the Old Signal down to one point, and play the `Auto` key until
/// it answers with something other than a round: the kill's outcome,
/// lines and all.
async fn kill_the_old_signal(
    svc: &FightService,
    client: &tokio_postgres::Client,
    user_id: uuid::Uuid,
) -> crate::app::deadchannel::fight::state::Outcome {
    use crate::app::deadchannel::fight::data::{MAX_LEVEL, RULES, exp_to_seek};
    use crate::app::deadchannel::fight::state::MAX_TIER;

    let row = DeadchannelRunner::find_by_user(client, user_id)
        .await
        .expect("find")
        .expect("row");
    let mut sheet = Sheet::from_row(&row).expect("sheet");
    sheet.day = FightService::today();
    sheet.level = MAX_LEVEL;
    sheet.peak_level = MAX_LEVEL;
    sheet.draft_up();
    sheet.exp = exp_to_seek();
    sheet.weapon_tier = MAX_TIER;
    sheet.armor_tier = MAX_TIER;
    sheet.signal = sheet.max_signal();
    sheet.rations_left = RATIONS_PER_DAY;
    sheet.road = Default::default();
    DeadchannelRunner::store_sheet(client, sheet.to_write())
        .await
        .expect("store");

    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.act_task(user_id, "mira".to_string(), STEP_IN, tx.clone());
    let FightOutcome::Acted { sheet, .. } = answer(&mut rx).await else {
        panic!("a start answers with the sheet");
    };
    let mut sheet = sheet;
    let fight = sheet.fight.as_mut().expect("the Old Signal");
    assert_eq!(fight.foe_max_signal, RULES.old_signal.signal);
    fight.foe_signal = 1;
    DeadchannelRunner::store_sheet(client, sheet.to_write())
        .await
        .expect("store");
    loop {
        svc.act_task(user_id, "mira".to_string(), Command::Auto, tx.clone());
        let FightOutcome::Acted { outcome, .. } = answer(&mut rx).await else {
            panic!("a turn answers with the sheet");
        };
        if outcome.applied != Applied::Round {
            break outcome;
        }
    }
}

async fn balance(client: &tokio_postgres::Client, user_id: uuid::Uuid) -> i64 {
    late_core::models::chips::UserChips::find(client, user_id)
        .await
        .expect("chips")
        .expect("a wallet")
        .balance
}

/// The kill through the service: the reset lands on the row and the peak
/// stays; the first kill grants the `SIG` badge once; the chips pay once
/// per mark and at most once every 30 days, and the kill's last line says
/// which way it went.
#[tokio::test]
async fn putting_the_old_signal_down_resets_the_row_and_pays_once_a_month() {
    use crate::app::deadchannel::fight::data::{MAX_LEVEL, OLD_SIGNAL_PAID_THIS_MONTH_LINE};
    use late_core::models::chips::{ChipLedgerEntry, ChipMove, INITIAL_CHIP_BALANCE, UserChips};
    use late_core::models::profile_award::{
        DEADCHANNEL_OLD_SIGNAL_AWARD_CATEGORY, ProfileAward, list_profile_awards_for_user,
    };
    use late_core::models::reward::{DEADCHANNEL_OLD_SIGNAL_REWARD_KEY, RewardTemplate};

    let (test_db, user_id, svc) = runner_and_service("fight-svc-old-signal").await;
    let client = test_db.db.get().await.expect("db client");
    // The wallet exists before anyone reaches the bottom: the stipend row
    // is written at login, so the payout lands on top of it.
    UserChips::ensure(&client, user_id).await.expect("a wallet");
    let pay = RewardTemplate::get_active_by_key(&**client, DEADCHANNEL_OLD_SIGNAL_REWARD_KEY)
        .await
        .expect("the old signal's reward template")
        .reward_chips;
    assert_eq!(pay, 40_000, "migration 209 seeds the month's payout");
    let paid_rows = |entries: Vec<ChipLedgerEntry>| {
        entries
            .into_iter()
            .filter(|entry| entry.chip_move() == Some(ChipMove::OldSignalSlain))
            .map(|entry| entry.delta)
            .collect::<Vec<_>>()
    };

    // Mark 1: the reset, the badge, and the month's payout.
    let first = kill_the_old_signal(&svc, &client, user_id).await;
    assert_eq!(first.applied, Applied::Slain { marks: 1 });
    assert_eq!(
        first.lines.last().map(String::as_str),
        Some("the house pays 40,000 chips for the broadcast.")
    );
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
    assert_eq!(balance(&client, user_id).await, INITIAL_CHIP_BALANCE + pay);

    // Mark 2 inside the month: the mark lands, the house does not pay, and
    // the line says so.
    let second = kill_the_old_signal(&svc, &client, user_id).await;
    assert_eq!(second.applied, Applied::Slain { marks: 2 });
    assert_eq!(
        second.lines.last().map(String::as_str),
        Some(OLD_SIGNAL_PAID_THIS_MONTH_LINE)
    );
    assert_eq!(balance(&client, user_id).await, INITIAL_CHIP_BALANCE + pay);
    let awards = list_profile_awards_for_user(&client, user_id)
        .await
        .expect("awards");
    assert_eq!(
        badges(awards),
        vec![1],
        "the second kill leaves the badge as the first granted it"
    );

    // Past the month, mark 3 pays again.
    age_payout_claims(&test_db.db, user_id, 31).await;
    let third = kill_the_old_signal(&svc, &client, user_id).await;
    assert_eq!(third.applied, Applied::Slain { marks: 3 });
    assert_eq!(
        third.lines.last().map(String::as_str),
        Some("the house pays 40,000 chips for the broadcast.")
    );
    assert_eq!(
        balance(&client, user_id).await,
        INITIAL_CHIP_BALANCE + 2 * pay
    );
    let ledger = UserChips::recent_ledger(&client, user_id, 20)
        .await
        .expect("ledger");
    assert_eq!(
        paid_rows(ledger),
        vec![pay, pay],
        "one ledger row per paid mark, none for the month's second"
    );
}

/// The nuke (migration 222) puts the marks back to none and keeps the row.
/// A runner paid for mark 1 before it who earns mark 1 again, past the
/// month, is paid again: the payout's key carries the row's reset
/// generation, so the old claim does not answer for the new mark.
#[tokio::test]
async fn a_mark_earned_again_after_the_nuke_pays_again() {
    use late_core::models::chips::{INITIAL_CHIP_BALANCE, UserChips};

    let (test_db, user_id, svc) = runner_and_service("fight-svc-nuke-mark").await;
    let client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, user_id).await.expect("a wallet");

    let first = kill_the_old_signal(&svc, &client, user_id).await;
    assert_eq!(first.applied, Applied::Slain { marks: 1 });
    assert_eq!(
        balance(&client, user_id).await,
        INITIAL_CHIP_BALANCE + 40_000
    );

    client
        .execute("SELECT deadchannel_nuke_runners()", &[])
        .await
        .expect("the nuke");
    age_payout_claims(&test_db.db, user_id, 31).await;

    let again = kill_the_old_signal(&svc, &client, user_id).await;
    assert_eq!(again.applied, Applied::Slain { marks: 1 });
    assert_eq!(
        again.lines.last().map(String::as_str),
        Some("the house pays 40,000 chips for the broadcast.")
    );
    assert_eq!(
        balance(&client, user_id).await,
        INITIAL_CHIP_BALANCE + 2 * 40_000
    );
}

/// The grant erroring after the kill's commit: the mark and the badge land,
/// the scene says the till is jammed, and the row keeps the debt. The next
/// command on the row pays it, once; the one after finds nothing owed.
#[tokio::test]
async fn a_jammed_till_owes_the_mark_until_the_next_command_pays_it() {
    use crate::app::deadchannel::fight::data::OLD_SIGNAL_TILL_JAMMED_LINE;
    use late_core::models::chips::{ChipLedgerEntry, ChipMove, INITIAL_CHIP_BALANCE, UserChips};
    use late_core::models::profile_award::list_profile_awards_for_user;
    use late_core::models::reward::DEADCHANNEL_OLD_SIGNAL_REWARD_KEY;

    let (test_db, user_id, svc) = runner_and_service("fight-svc-jammed-till").await;
    let client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, user_id).await.expect("a wallet");
    // The one way to make the grant error from outside: no active
    // template to read. Set straight from the table, as no code path
    // retires a template.
    let set_template_active = |active: bool| {
        let client = &client;
        async move {
            client
                .execute(
                    "UPDATE reward_templates SET active = $2 WHERE key = $1",
                    &[&DEADCHANNEL_OLD_SIGNAL_REWARD_KEY, &active],
                )
                .await
                .expect("set the template");
        }
    };
    let paid_rows = |entries: Vec<ChipLedgerEntry>| {
        entries
            .into_iter()
            .filter(|entry| entry.chip_move() == Some(ChipMove::OldSignalSlain))
            .count()
    };

    set_template_active(false).await;
    let kill = kill_the_old_signal(&svc, &client, user_id).await;
    assert_eq!(kill.applied, Applied::Slain { marks: 1 });
    assert_eq!(
        kill.lines.last().map(String::as_str),
        Some(OLD_SIGNAL_TILL_JAMMED_LINE)
    );
    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!(
        (row.level, row.marks),
        (1, 1),
        "the reset and the mark land"
    );
    assert_eq!(row.unpaid_mark, Some(1), "the row keeps the debt");
    assert_eq!(balance(&client, user_id).await, INITIAL_CHIP_BALANCE);
    let awards = list_profile_awards_for_user(&client, user_id)
        .await
        .expect("awards");
    assert_eq!(awards.len(), 1, "the badge lands whatever the till did");

    // The till works again: the next command on the row, whatever it is,
    // pays the mark and says so under its own answer.
    set_template_active(true).await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.act_task(user_id, "mira".to_string(), Command::Patch, tx.clone());
    let FightOutcome::Acted { outcome, .. } = answer(&mut rx).await else {
        panic!("a patch answers with the sheet");
    };
    assert!(
        matches!(outcome.applied, Applied::Refused(_)),
        "nothing to patch on a fresh runner: {:?}",
        outcome.applied
    );
    assert_eq!(
        outcome.lines.last().map(String::as_str),
        Some("the house pays 40,000 chips for the broadcast.")
    );
    assert_eq!(
        balance(&client, user_id).await,
        INITIAL_CHIP_BALANCE + 40_000
    );
    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!(row.unpaid_mark, None, "the debt is settled");

    // Nothing owed: the command after answers on its own, and pays nothing.
    svc.act_task(user_id, "mira".to_string(), Command::Patch, tx.clone());
    let FightOutcome::Acted { outcome, .. } = answer(&mut rx).await else {
        panic!("a patch answers with the sheet");
    };
    assert!(
        outcome
            .lines
            .iter()
            .all(|line| !line.contains("the house pays")),
        "{:?}",
        outcome.lines
    );
    assert_eq!(
        balance(&client, user_id).await,
        INITIAL_CHIP_BALANCE + 40_000
    );
    let ledger = UserChips::recent_ledger(&client, user_id, 20)
        .await
        .expect("ledger");
    assert_eq!(paid_rows(ledger), 1, "one ledger row for the healed mark");
}

/// The crystal pass's two columns go through the row like the rest of the
/// sheet: a glass poured is on the row with the crystal it cost gone, and
/// a reload reads both back.
#[tokio::test]
async fn a_glass_and_the_crystals_are_stored_on_the_row() {
    use crate::app::deadchannel::fight::state::Drink;

    let (test_db, user_id, svc) = runner_and_service("fight-svc-glass").await;
    let client = test_db.db.get().await.expect("db client");
    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    let mut sheet = Sheet::from_row(&row).expect("sheet");
    sheet.day = FightService::today();
    sheet.crystals = 3;
    DeadchannelRunner::store_sheet(&**client, sheet.to_write())
        .await
        .expect("store");

    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.act_task(
        user_id,
        "mira".to_string(),
        Command::Drink {
            drink: Drink::DeadAirNeat,
        },
        tx.clone(),
    );
    let FightOutcome::Acted { sheet, outcome, .. } = answer(&mut rx).await else {
        panic!("a glass answers with the sheet");
    };
    assert_eq!(
        outcome.applied,
        Applied::Drank {
            drink: Drink::DeadAirNeat
        }
    );
    assert_eq!((sheet.crystals, sheet.drink), (2, Some(Drink::DeadAirNeat)));

    // The row is the witness, and a reload is what another session sees.
    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!(
        (row.crystals, row.drink.as_deref()),
        (2, Some("dead_air_neat"))
    );
    svc.reload_task(user_id, tx);
    let FightOutcome::Reloaded { sheet, .. } = answer(&mut rx).await else {
        panic!("a reload answers with the sheet");
    };
    assert_eq!((sheet.crystals, sheet.drink), (2, Some(Drink::DeadAirNeat)));
}

/// A second runner on the same database, their row set for today.
async fn another_runner(
    test_db: &late_core::test_utils::TestDb,
    name: &str,
) -> (uuid::Uuid, String) {
    let user = create_test_user(&test_db.db, name).await;
    let client = test_db.db.get().await.expect("db client");
    let look = Look::random(1, &mut rand::thread_rng());
    DeadchannelRunner::ensure_for_user(&client, user.id, &look.to_json())
        .await
        .expect("a runner");
    (user.id, user.username)
}

async fn sheet_of(test_db: &late_core::test_utils::TestDb, user_id: uuid::Uuid) -> Sheet {
    let client = test_db.db.get().await.expect("db client");
    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    Sheet::from_row(&row).expect("sheet")
}

async fn store(test_db: &late_core::test_utils::TestDb, sheet: &Sheet) {
    let client = test_db.db.get().await.expect("db client");
    DeadchannelRunner::store_sheet(&**client, sheet.to_write())
        .await
        .expect("store");
}

/// `user_id`'s row set for today, stood one step short of the crossing
/// down its lane, carrying `bits`. Returns the crossing's lane.
async fn at_the_crossing(
    test_db: &late_core::test_utils::TestDb,
    user_id: uuid::Uuid,
    bits: i64,
) -> u8 {
    let mut sheet = sheet_of(test_db, user_id).await;
    sheet.day = FightService::today();
    sheet.bits = bits;
    let road = sheet.todays_road();
    let (step, lane) = road
        .steps
        .iter()
        .enumerate()
        .find_map(|(step, lanes)| {
            lanes
                .iter()
                .position(|node| *node == Node::Crossing)
                .map(|lane| (step as i32 + 1, lane as u8))
        })
        .expect("every road has a crossing");
    sheet.rations_left = RATIONS_PER_DAY - (step - 1);
    sheet.road.path = (1..step)
        .map(|_| Trace {
            lane,
            mark: Mark::Won,
        })
        .collect();
    store(test_db, &sheet).await;
    lane
}

/// `user_id`'s row set for today and asleep, carrying `bits`.
async fn asleep(test_db: &late_core::test_utils::TestDb, user_id: uuid::Uuid, bits: i64) {
    let mut sheet = sheet_of(test_db, user_id).await;
    sheet.day = FightService::today();
    sheet.bits = bits;
    store(test_db, &sheet).await;
}

/// The fight on `user_id`'s row left at one signal, so the next hit
/// ends it.
async fn one_hit_from_down(test_db: &late_core::test_utils::TestDb, user_id: uuid::Uuid) {
    let mut sheet = sheet_of(test_db, user_id).await;
    sheet.fight.as_mut().expect("a fight on the row").foe_signal = 1;
    store(test_db, &sheet).await;
}

/// The wire's message in #deadchannel that starts with `opening`, once
/// the fire-and-forget post lands.
async fn wire_line(test_db: &late_core::test_utils::TestDb, opening: &str) -> ChatMessage {
    let read = || async {
        let client = test_db.db.get().await.expect("db client");
        let room = ChatRoom::get_or_create_deadchannel_room(&client)
            .await
            .expect("the wire");
        ChatMessage::list_recent(&client, room.id, 20)
            .await
            .expect("list the wire")
            .into_iter()
            .find(|message| message.body.starts_with(opening))
    };
    wait_until(|| async { read().await.is_some() }, opening).await;
    read().await.expect("the line landed")
}

/// Waits for `user_id`'s mention of the wire's `message`.
async fn mentioned(
    test_db: &late_core::test_utils::TestDb,
    user_id: uuid::Uuid,
    message: &ChatMessage,
) {
    wait_until(
        || async {
            let client = test_db.db.get().await.expect("db client");
            Notification::list_for_user(&client, user_id, 20)
                .await
                .expect("list mentions")
                .iter()
                .any(|mention| mention.message_id == message.id)
        },
        "the wire's mention",
    )
    .await;
}

#[tokio::test]
async fn a_crossing_takes_the_sleepers_bits_and_the_sleeper_answers_for_them() {
    let (test_db, mira, svc) = runner_and_service("fight-cross-mira").await;
    let (tom, tom_name) = another_runner(&test_db, "fight-cross-tom").await;
    let lane = at_the_crossing(&test_db, mira, 50).await;
    asleep(&test_db, tom, 340).await;
    let client = test_db.db.get().await.expect("db client");

    // The road shows tom at the crossing before the step.
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.reload_task(mira, tx);
    let FightOutcome::Reloaded { lookout, .. } = answer(&mut rx).await else {
        panic!("a reload answers with the sheet");
    };
    let shown = lookout.crossing.expect("tom is at the crossing");
    assert_eq!(shown.rival.username, tom_name);
    assert_eq!((shown.carrying, shown.stake), (340, 170));
    assert_eq!(shown.rival.level, 1);
    assert!(lookout.token.is_none());
    assert!(lookout.news.is_none());

    // The step: tom on the row, a ration spent, the ambush row claimed.
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.cross_task(mira, "mira".to_string(), lane, tx);
    let FightOutcome::Acted { sheet, outcome, .. } = answer(&mut rx).await else {
        panic!("a crossing answers with the sheet");
    };
    let Applied::Crossed { rival } = outcome.applied else {
        panic!("tom is on the row: {:?}", outcome.applied);
    };
    assert_eq!(rival.username, tom_name);
    assert!(!rival.answer);
    assert_eq!(sheet.fight.as_ref().map(|fight| fight.foe_bits), Some(170));
    let crossings = Ambush::crossings_of_day(&**client, FightService::today())
        .await
        .expect("read");
    let crossing = crossings
        .iter()
        .find(|row| row.id == rival.ambush_id)
        .expect("the ambush row");
    assert_eq!(crossing.defender_user_id, tom);
    assert_eq!(crossing.stake, 170);
    assert!(crossing.outcome.is_none());

    // Tom renames while the fight sits on mira's row: the wire names and
    // mentions him as he is now, the scene keeps the name the fight took.
    let tom_now = User::rename(&client, tom, &format!("{tom_name}now"))
        .await
        .expect("rename")
        .username;
    assert_ne!(tom_now, tom_name);

    // Tom is down in one: half of what he carried moves, the row closes,
    // the news lands.
    one_hit_from_down(&test_db, mira).await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.act_task(mira, "mira".to_string(), Command::Auto, tx);
    let FightOutcome::Acted {
        sheet,
        outcome,
        lookout,
    } = answer(&mut rx).await
    else {
        panic!("a turn answers with the sheet");
    };
    assert!(
        matches!(outcome.applied, Applied::Ambushed { stake: 170, .. }),
        "{:?}",
        outcome.applied
    );
    assert_eq!(
        outcome.lines.last(),
        Some(&format!("you take 170 bits off {tom_name}."))
    );
    assert_eq!(sheet.bits, 220);
    assert_eq!(sheet.exp, 0);
    assert_eq!(sheet.kills, 0);
    assert!(sheet.fight.is_none());
    assert!(lookout.crossing.is_none(), "the step is taken");
    let toms = sheet_of(&test_db, tom).await;
    assert_eq!(toms.bits, 170);
    assert_eq!(toms.signal, 10, "a sleeper loses only what they carry");
    let closed = Ambush::get(&**client, rival.ambush_id)
        .await
        .expect("read")
        .expect("the row");
    assert_eq!(closed.outcome, Some(AmbushOutcome::Won));
    assert_eq!(closed.bits_moved, 170);
    // The wire names who lost what to whom, tom by mention, as he is now.
    let line = wire_line(&test_db, "@mira crossed").await;
    assert_eq!(
        line.body,
        format!(
            "@mira crossed @{tom_now} at step {} and won. @{tom_now} never woke and lost 170 bits to @mira.",
            sheet.road.path.len()
        )
    );
    mentioned(&test_db, tom, &line).await;

    // Mira cannot cross tom again today, nor anyone: the day's caps.
    let lane_again = at_the_crossing(&test_db, mira, 390).await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.cross_task(mira, "mira".to_string(), lane_again, tx);
    let FightOutcome::Acted { sheet, outcome, .. } = answer(&mut rx).await else {
        panic!("a crossing answers");
    };
    assert_eq!(outcome.applied, Applied::Refused(Refusal::NobodyThere));
    assert_eq!(sheet.bits, 390);

    // Tom's row reads the news and is owed an answer, with mira on the wire.
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.reload_task(tom, tx);
    let FightOutcome::Reloaded { lookout, .. } = answer(&mut rx).await else {
        panic!("a reload answers");
    };
    let news = lookout.news.expect("the news");
    assert_eq!(news.id, rival.ambush_id);
    let token = lookout.token.expect("an answer owed");
    assert_eq!(token.ambush.id, rival.ambush_id);
    let crosser = token.prey.expect("mira is on the wire");
    // The name mira goes by now, not the "mira" the crossing row kept: the
    // answer's wire line mentions it, and a mention finds its runner by the
    // current handle.
    let mira_name = User::list_usernames_by_ids(&**client, &[mira])
        .await
        .expect("read")
        .remove(&mira)
        .expect("mira's user row");
    assert_ne!(mira_name, "mira");
    assert_eq!(crosser.rival.username, mira_name);
    assert_eq!((crosser.carrying, crosser.stake), (390, 195));

    // The answer: no ration, mira on tom's row, and half of mira's hand.
    let tom_name = tom_now;
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.answer_task(tom, tom_name.clone(), tx);
    let FightOutcome::Acted { sheet, outcome, .. } = answer(&mut rx).await else {
        panic!("an answer answers");
    };
    let Applied::Crossed { rival: back } = outcome.applied else {
        panic!("mira is on the row: {:?}", outcome.applied);
    };
    assert!(back.answer);
    assert_eq!(back.user_id, mira);
    assert_eq!(sheet.rations_left, RATIONS_PER_DAY);
    assert!(sheet.road.path.is_empty());
    one_hit_from_down(&test_db, tom).await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.act_task(tom, tom_name.clone(), Command::Auto, tx);
    let FightOutcome::Acted {
        sheet,
        outcome,
        lookout,
    } = answer(&mut rx).await
    else {
        panic!("a turn answers");
    };
    assert!(
        matches!(outcome.applied, Applied::Ambushed { stake: 195, .. }),
        "{:?}",
        outcome.applied
    );
    assert_eq!(sheet.bits, 365);
    assert_eq!(sheet_of(&test_db, mira).await.bits, 195);
    assert!(lookout.token.is_none(), "the answer is given");
    let answered = Ambush::get(&**client, back.ambush_id)
        .await
        .expect("read")
        .expect("the answer row");
    assert_eq!(answered.answer_to, Some(rival.ambush_id));
    assert_eq!(answered.outcome, Some(AmbushOutcome::Won));
    assert_eq!(answered.bits_moved, 195);
    let line = wire_line(&test_db, &format!("@{tom_name} answered")).await;
    assert_eq!(
        line.body,
        format!(
            "@{tom_name} answered @{mira_name} and won. @{mira_name} lost 195 bits to @{tom_name}."
        )
    );
    mentioned(&test_db, mira, &line).await;

    // A second answer: nothing owed.
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.answer_task(tom, tom_name, tx);
    let FightOutcome::Acted { outcome, .. } = answer(&mut rx).await else {
        panic!("an answer answers");
    };
    assert_eq!(outcome.applied, Applied::Refused(Refusal::NoAnswerOwed));
}

#[tokio::test]
async fn going_down_at_a_crossing_pays_the_sleeper_and_owes_no_answer() {
    let (test_db, mira, svc) = runner_and_service("fight-fall-mira").await;
    let (tom, tom_name) = another_runner(&test_db, "fight-fall-tom").await;
    let lane = at_the_crossing(&test_db, mira, 120).await;
    asleep(&test_db, tom, 0).await;
    let client = test_db.db.get().await.expect("db client");

    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.cross_task(mira, "mira".to_string(), lane, tx);
    let FightOutcome::Acted { outcome, .. } = answer(&mut rx).await else {
        panic!("a crossing answers");
    };
    let Applied::Crossed { rival } = outcome.applied else {
        panic!("tom is on the row: {:?}", outcome.applied);
    };

    // Mira at one signal against a heavy hitter: the turn ends her day.
    let mut sheet = sheet_of(&test_db, mira).await;
    sheet.signal = 1;
    sheet.exp = 100;
    sheet.fight.as_mut().expect("a fight").foe_attack = 50;
    store(&test_db, &sheet).await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.act_task(mira, "mira".to_string(), Command::EndTurn, tx);
    let FightOutcome::Acted { sheet, outcome, .. } = answer(&mut rx).await else {
        panic!("a turn answers");
    };
    assert!(
        matches!(outcome.applied, Applied::Downed { bits_lost: 120, .. }),
        "{:?}",
        outcome.applied
    );
    assert!(sheet.is_down());
    assert_eq!(sheet.bits, 0);
    assert_eq!(sheet.exp, 65);
    assert_eq!(sheet_of(&test_db, tom).await.bits, 120);
    let closed = Ambush::get(&**client, rival.ambush_id)
        .await
        .expect("read")
        .expect("the row");
    assert_eq!(closed.outcome, Some(AmbushOutcome::Lost));
    assert_eq!(closed.bits_moved, -120);
    let line = wire_line(&test_db, "@mira crossed").await;
    assert_eq!(
        line.body,
        format!(
            "@mira crossed @{tom_name} at step {} and went down. @{tom_name} never woke. @mira lost their signal and 120 bits to @{tom_name}.",
            sheet.road.path.len()
        )
    );
    mentioned(&test_db, tom, &line).await;

    // Tom's row reads the news; nothing to answer.
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.reload_task(tom, tx);
    let FightOutcome::Reloaded { lookout, .. } = answer(&mut rx).await else {
        panic!("a reload answers");
    };
    assert_eq!(lookout.news.map(|news| news.id), Some(rival.ambush_id));
    assert!(lookout.token.is_none());
}

#[tokio::test]
async fn the_crossing_meets_only_a_sleeper_in_the_band_on_the_wire() {
    let (test_db, mira, svc) = runner_and_service("fight-band-mira").await;
    let (tom, _) = another_runner(&test_db, "fight-band-tom").await;
    let lane = at_the_crossing(&test_db, mira, 50).await;

    // Out of the band: nobody there, nothing spent.
    let mut toms = sheet_of(&test_db, tom).await;
    toms.day = FightService::today();
    toms.level = 4;
    store(&test_db, &toms).await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.cross_task(mira, "mira".to_string(), lane, tx);
    let FightOutcome::Acted {
        sheet,
        outcome,
        lookout,
    } = answer(&mut rx).await
    else {
        panic!("a crossing answers");
    };
    assert_eq!(outcome.applied, Applied::Refused(Refusal::NobodyThere));
    assert_eq!(
        sheet.rations_left,
        RATIONS_PER_DAY - sheet.road.path.len() as i32
    );
    assert!(lookout.crossing.is_none());

    // In the band but down: still nobody.
    toms.level = 2;
    toms.signal = 0;
    store(&test_db, &toms).await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.cross_task(mira, "mira".to_string(), lane, tx);
    let FightOutcome::Acted { outcome, .. } = answer(&mut rx).await else {
        panic!("a crossing answers");
    };
    assert_eq!(outcome.applied, Applied::Refused(Refusal::NobodyThere));

    // Up, in the band, on the wire: met.
    toms.signal = toms.max_signal();
    store(&test_db, &toms).await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.cross_task(mira, "mira".to_string(), lane, tx);
    let FightOutcome::Acted { outcome, .. } = answer(&mut rx).await else {
        panic!("a crossing answers");
    };
    assert!(
        matches!(outcome.applied, Applied::Crossed { .. }),
        "{:?}",
        outcome.applied
    );
}

fn blank_portrait() -> [String; 3] {
    [
        "     ".to_string(),
        "  @  ".to_string(),
        "     ".to_string(),
    ]
}

/// A sleeper whose row still carries yesterday's rival fight: the roll
/// under the crosser's lock wipes the fight and closes its ambush row as
/// dropped, the same as the roll on the runner's own touch. Without that
/// the row would read a fight that ended nowhere.
#[tokio::test]
async fn crossing_a_sleeper_rolls_their_day_and_drops_their_hanging_fight() {
    let (test_db, mira, svc) = runner_and_service("fight-roll-mira").await;
    let (tom, tom_name) = another_runner(&test_db, "fight-roll-tom").await;
    let (dax, dax_name) = another_runner(&test_db, "fight-roll-dax").await;
    let lane = at_the_crossing(&test_db, mira, 50).await;
    let client = test_db.db.get().await.expect("db client");

    // Yesterday tom crossed dax and never finished: the fight is on tom's
    // row, dated yesterday, and the ambush row is still open.
    let yesterday = FightService::today() - chrono::Duration::days(1);
    let hanging = Ambush::insert(
        &**client,
        &NewAmbush {
            id: uuid::Uuid::now_v7(),
            day: yesterday,
            attacker_user_id: tom,
            defender_user_id: dax,
            attacker_username: tom_name.clone(),
            defender_username: dax_name.clone(),
            step: Some(3),
            stake: 20,
            answer_to: None,
            started_at: Utc::now() - chrono::Duration::days(1),
        },
    )
    .await
    .expect("insert")
    .expect("the row claimed");
    let mut toms = sheet_of(&test_db, tom).await;
    toms.day = yesterday;
    toms.bits = 120;
    // Dax climbed out of mira's band since, so tom is the one sleeper
    // mira can meet.
    let mut daxs = sheet_of(&test_db, dax).await;
    daxs.level = 1 + CROSSING_LEVEL_BAND + 1;
    store(&test_db, &daxs).await;
    let prey = Prey::of(&daxs, &dax_name, blank_portrait(), hanging.id, false);
    toms.engage_rival(prey, &mut rand::thread_rng());
    assert!(toms.fight.is_some());
    store(&test_db, &toms).await;

    // Mira steps onto the crossing and meets tom, as the preview did.
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.cross_task(mira, "mira".to_string(), lane, tx);
    let FightOutcome::Acted { outcome, .. } = answer(&mut rx).await else {
        panic!("a crossing answers with the sheet");
    };
    let Applied::Crossed { rival } = outcome.applied else {
        panic!("tom is on the row: {:?}", outcome.applied);
    };
    assert_eq!(rival.username, tom_name);

    // Tom's row rolled under the lock: today, no fight, and the hanging
    // ambush closed as dropped with nothing moved.
    let toms = sheet_of(&test_db, tom).await;
    assert_eq!(toms.day, FightService::today());
    assert!(toms.fight.is_none());
    let dropped = Ambush::get(&**client, hanging.id)
        .await
        .expect("read")
        .expect("the row");
    assert_eq!(dropped.outcome, Some(AmbushOutcome::Dropped));
    assert_eq!(dropped.bits_moved, 0);
}
