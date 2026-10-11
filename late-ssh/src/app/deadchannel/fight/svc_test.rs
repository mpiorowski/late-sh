use chrono::Utc;
use late_core::models::deadchannel_runner::DeadchannelRunner;
use late_core::test_utils::create_test_user;
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

use super::{FightOutcome, FightService};
use crate::app::chat::notifications::svc::NotificationService;
use crate::app::chat::svc::ChatService;
use crate::app::deadchannel::fight::data::RATIONS_PER_DAY;
use crate::app::deadchannel::fight::road::{Mark, Trace};
use crate::app::deadchannel::fight::state::{Applied, Call, Command, Pick, Sheet, Slot};
use crate::app::deadchannel::runner::state::Look;
use crate::app::games::chips::svc::ChipService;
use crate::test_helpers::{age_payout_claims, new_test_db};

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
    let FightOutcome::Acted { sheet, outcome } = answer(&mut rx).await else {
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
    let FightOutcome::Acted { sheet, outcome } = answer(&mut rx).await else {
        panic!("a resume answers with the sheet");
    };
    assert_eq!(outcome.applied, Applied::Resumed);
    assert_eq!(sheet.rations_left, RATIONS_PER_DAY - 1);

    // The hand is the row's: the `Auto` key plays a turn of it there, and
    // what the row holds after is what the answer showed.
    let (tx, mut rx) = mpsc::unbounded_channel();
    svc.act_task(user_id, "mira".to_string(), Command::Auto, tx);
    let FightOutcome::Acted { sheet, outcome } = answer(&mut rx).await else {
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
    let FightOutcome::Acted { sheet, outcome } = answer(&mut rx).await else {
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

    // A fresh runner's 50 bits, the level-1 loan of 50 (5 more on the
    // debt), then all 100 into the locker less its 10.
    svc.act_task(user_id, "mira".to_string(), Command::Borrow, tx.clone());
    let FightOutcome::Acted { outcome, .. } = answer(&mut rx).await else {
        panic!("a loan answers with the sheet");
    };
    assert_eq!(outcome.applied, Applied::Borrowed { amount: 50, fee: 5 });
    svc.act_task(user_id, "mira".to_string(), Command::Deposit, tx);
    let FightOutcome::Acted { sheet, outcome } = answer(&mut rx).await else {
        panic!("a deposit answers with the sheet");
    };
    assert_eq!(
        outcome.applied,
        Applied::Deposited {
            stored: 90,
            fee: 10
        }
    );
    assert_eq!((sheet.bits, sheet.stash, sheet.debt), (0, 90, 55));

    let client = test_db.db.get().await.expect("db client");
    let row = DeadchannelRunner::find_by_user(&client, user_id)
        .await
        .expect("find")
        .expect("row");
    assert_eq!((row.bits, row.stash, row.debt), (0, 90, 55));
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
    sheet.exp = exp_to_seek(sheet.marks);
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
    let FightOutcome::Acted { sheet, outcome } = answer(&mut rx).await else {
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
    let FightOutcome::Reloaded { sheet } = answer(&mut rx).await else {
        panic!("a reload answers with the sheet");
    };
    assert_eq!((sheet.crystals, sheet.drink), (2, Some(Drink::DeadAirNeat)));
}
