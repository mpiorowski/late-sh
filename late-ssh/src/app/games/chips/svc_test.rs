use crate::app::{
    activity::event::{ActivityEvent, ActivityGame},
    games::chips::svc::ChipService,
};
use chrono::NaiveDate;
use late_core::{
    models::{
        chips::{ChipMove, Difficulty, INITIAL_CHIP_BALANCE, UserChips},
        reward::{
            DailyPuzzleRewardGame, REWARD_CLAIM_POLICY_UTC_DAY, RewardTemplate,
            daily_puzzle_reward_key,
        },
    },
    test_utils::create_test_user,
};
use tokio::sync::broadcast;

use crate::test_helpers::new_test_db;

#[tokio::test]
async fn sliding_puzzle_activity_rewards_pay_seeded_tiers_once_per_utc_day() {
    let test_db = new_test_db().await;
    let chips = ChipService::new(test_db.db.clone());
    let payout_date = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
    let (activity_tx, _) = broadcast::channel(16);
    let reward_task = chips.start_activity_reward_task(activity_tx.clone());
    let mut cases = Vec::new();

    for difficulty in [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard] {
        let amount = difficulty.chips();
        let user = create_test_user(
            &test_db.db,
            &format!("sliding-puzzle-{}-payout", difficulty.key()),
        )
        .await;
        chips.ensure_chips(user.id).await.expect("initial chips");

        let reward_key =
            daily_puzzle_reward_key(DailyPuzzleRewardGame::SlidingPuzzle, difficulty.key());
        let client = test_db.db.get().await.expect("db client");
        let template = RewardTemplate::get_active_by_key(&**client, &reward_key)
            .await
            .expect("seeded Sliding Puzzle reward template");
        assert_eq!(template.reward_chips, amount);
        assert_eq!(template.claim_policy, REWARD_CLAIM_POLICY_UTC_DAY);
        assert_eq!(template.game().expect("template game"), "sliding_puzzle");
        assert_eq!(
            template.payout_kind().expect("template payout kind"),
            format!("daily_win_{}", difficulty.key())
        );
        drop(client);

        let event = ActivityEvent::game_won_at(
            user.id,
            "sliding-puzzle-payout-user",
            ActivityGame::SlidingPuzzle,
            Some(difficulty.key().to_string()),
            Some(42),
            ActivityEvent::occurred_on_utc_date(payout_date),
        );
        activity_tx.send(event.clone()).expect("first win event");
        activity_tx
            .send(event)
            .expect("repeated same-day win event");
        cases.push((user.id, reward_key, amount));
    }

    drop(activity_tx);
    reward_task.await.expect("activity reward task exits");

    let client = test_db.db.get().await.expect("db client");
    for (user_id, reward_key, amount) in cases {
        let balance = UserChips::find(&client, user_id)
            .await
            .expect("load chip balance")
            .expect("chip balance exists");
        assert_eq!(balance.balance, INITIAL_CHIP_BALANCE + amount);
        assert!(
            chips
                .has_daily_reward_claim(user_id, &reward_key, payout_date)
                .await
                .expect("load utc_day payout claim")
        );

        let ledger = client
            .query_one(
                "SELECT count(*)::bigint AS rows, COALESCE(sum(delta), 0)::bigint AS delta
                 FROM chip_ledger
                 WHERE user_id = $1
                   AND reason = $2
                   AND source_kind = $3",
                &[
                    &user_id,
                    &ChipMove::DailyPuzzleWin.reason(),
                    &ChipMove::DailyPuzzleWin.source_kind(),
                ],
            )
            .await
            .expect("daily puzzle ledger");
        assert_eq!(ledger.get::<_, i64>("rows"), 1);
        assert_eq!(ledger.get::<_, i64>("delta"), amount);
    }
}

#[tokio::test]
async fn transfer_chips_records_atomic_gift_ledgers() {
    let test_db = new_test_db().await;
    let sender = create_test_user(&test_db.db, "gift-sender").await;
    let recipient = create_test_user(&test_db.db, "gift-recipient").await;
    let client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, sender.id)
        .await
        .expect("sender chips");
    UserChips::ensure(&client, recipient.id)
        .await
        .expect("recipient chips");
    drop(client);

    let chips = ChipService::new(test_db.db.clone());
    let (sender_balance, recipient_balance) = chips
        .transfer_chips(sender.id, recipient.id, 500)
        .await
        .expect("gift succeeds");

    assert_eq!(sender_balance, 500);
    assert_eq!(recipient_balance, 1_500);

    let client = test_db.db.get().await.expect("db client");
    let rows = client
        .query(
            "SELECT user_id, delta, reason
             FROM chip_ledger
             WHERE user_id IN ($1, $2)
               AND reason IN ($3, $4)
             ORDER BY delta ASC",
            &[
                &sender.id,
                &recipient.id,
                &ChipMove::GiftSent.reason(),
                &ChipMove::GiftReceived.reason(),
            ],
        )
        .await
        .expect("ledger rows");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<_, i64>("delta"), -500);
    assert_eq!(
        rows[0].get::<_, &str>("reason"),
        ChipMove::GiftSent.reason()
    );
    assert_eq!(rows[1].get::<_, i64>("delta"), 500);
    assert_eq!(
        rows[1].get::<_, &str>("reason"),
        ChipMove::GiftReceived.reason()
    );
}

#[tokio::test]
async fn transfer_chips_initializes_recipient_without_existing_chips_row() {
    let test_db = new_test_db().await;
    let sender = create_test_user(&test_db.db, "gift-init-sender").await;
    let recipient = create_test_user(&test_db.db, "gift-init-recipient").await;
    // Only the sender starts with a chips row; the recipient has never had one.
    let client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, sender.id)
        .await
        .expect("sender chips");
    drop(client);

    let chips = ChipService::new(test_db.db.clone());
    let (sender_balance, recipient_balance) = chips
        .transfer_chips(sender.id, recipient.id, 500)
        .await
        .expect("gift to fresh recipient succeeds");

    assert_eq!(sender_balance, 500);
    assert_eq!(recipient_balance, 1_500);
}

#[tokio::test]
async fn transfer_chips_insufficient_funds_leaves_balances_and_ledger_untouched() {
    let test_db = new_test_db().await;
    let sender = create_test_user(&test_db.db, "gift-poor-sender").await;
    let recipient = create_test_user(&test_db.db, "gift-poor-recipient").await;
    let client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, sender.id)
        .await
        .expect("sender chips");
    UserChips::ensure(&client, recipient.id)
        .await
        .expect("recipient chips");
    drop(client);

    let chips = ChipService::new(test_db.db.clone());
    let error = chips
        .transfer_chips(sender.id, recipient.id, 1_000)
        .await
        .expect_err("gift fails at floor");
    assert!(error.to_string().contains("insufficient chips"));

    let client = test_db.db.get().await.expect("db client");
    let sender_balance = client
        .query_one(
            "SELECT balance FROM user_chips WHERE user_id = $1",
            &[&sender.id],
        )
        .await
        .expect("sender balance")
        .get::<_, i64>("balance");
    let recipient_balance = client
        .query_one(
            "SELECT balance FROM user_chips WHERE user_id = $1",
            &[&recipient.id],
        )
        .await
        .expect("recipient balance")
        .get::<_, i64>("balance");
    assert_eq!(sender_balance, 1_000);
    assert_eq!(recipient_balance, 1_000);

    let ledger_count = client
        .query_one(
            "SELECT count(*)::int AS count
             FROM chip_ledger
             WHERE user_id IN ($1, $2)
               AND reason IN ($3, $4)",
            &[
                &sender.id,
                &recipient.id,
                &ChipMove::GiftSent.reason(),
                &ChipMove::GiftReceived.reason(),
            ],
        )
        .await
        .expect("ledger count")
        .get::<_, i32>("count");
    assert_eq!(ledger_count, 0);
}

#[tokio::test]
async fn transfer_chips_leaves_unrelated_users_untouched() {
    let test_db = new_test_db().await;
    let sender = create_test_user(&test_db.db, "gift-scope-sender").await;
    let recipient = create_test_user(&test_db.db, "gift-scope-recipient").await;
    let bystander = create_test_user(&test_db.db, "gift-scope-bystander").await;
    let client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, sender.id)
        .await
        .expect("sender chips");
    UserChips::ensure(&client, recipient.id)
        .await
        .expect("recipient chips");
    UserChips::ensure(&client, bystander.id)
        .await
        .expect("bystander chips");

    let chips = ChipService::new(test_db.db.clone());
    chips
        .transfer_chips(sender.id, recipient.id, 500)
        .await
        .expect("gift succeeds");

    let balance: i64 = client
        .query_one(
            "SELECT balance FROM user_chips WHERE user_id = $1",
            &[&bystander.id],
        )
        .await
        .expect("bystander balance")
        .get(0);
    assert_eq!(balance, 1_000, "a transfer must not touch a third user");

    let ledger_rows: i64 = client
        .query_one(
            "SELECT COUNT(*) FROM chip_ledger WHERE user_id = $1",
            &[&bystander.id],
        )
        .await
        .expect("bystander ledger")
        .get(0);
    assert_eq!(ledger_rows, 0);
}

#[tokio::test]
async fn welcome_pour_comps_only_the_first_drink_ever() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "welcome-pour").await;
    let chips = ChipService::new(test_db.db.clone());

    let first = chips
        .grant_free_drink(user.id, late_core::models::drinks::WELCOME_DRINK_POINTS)
        .await
        .expect("first comp succeeds")
        .expect("first comp pours");
    assert_eq!(
        first.drunk_points,
        late_core::models::drinks::WELCOME_DRINK_POINTS
    );
    assert_eq!(first.lifetime_spent, 0);

    // A tour rerun after a mid-tour disconnect: the welcome is spent.
    let second = chips
        .grant_free_drink(user.id, late_core::models::drinks::WELCOME_DRINK_POINTS)
        .await
        .expect("second comp succeeds");
    assert!(second.is_none());

    // The comp stayed off the tab: one drink, no lifetime spend.
    let client = test_db.db.get().await.expect("db client");
    let row = client
        .query_one(
            "SELECT drunk_points, lifetime_spent, drink_count
             FROM user_drinks WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("drinks row");
    assert_eq!(
        row.get::<_, i64>("drunk_points"),
        late_core::models::drinks::WELCOME_DRINK_POINTS
    );
    assert_eq!(row.get::<_, i64>("lifetime_spent"), 0);
    assert_eq!(row.get::<_, i64>("drink_count"), 1);
}

#[tokio::test]
async fn welcome_pour_never_comps_a_prior_drinker() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "welcome-pour-veteran").await;
    let client = test_db.db.get().await.expect("db client");
    late_core::models::drinks::UserDrinks::record_purchase(&client, user.id, 200)
        .await
        .expect("paid drink");
    drop(client);

    let chips = ChipService::new(test_db.db.clone());
    let comp = chips
        .grant_free_drink(user.id, late_core::models::drinks::WELCOME_DRINK_POINTS)
        .await
        .expect("comp call succeeds");
    assert!(
        comp.is_none(),
        "a prior drinker never gets the welcome pour"
    );
}

#[tokio::test]
async fn apply_move_settles_a_seat_and_refuses_what_the_balance_cannot_cover() {
    // The Super Snake arena banks a seat through this one call, so both
    // directions matter: a winning visit credits and hands back the new
    // balance, and a losing one is declined outright rather than pushing a
    // player negative.
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "ssnake-settler").await;
    let chips = ChipService::new(test_db.db.clone());
    // Every session ensures its chips row at bootstrap, so a player who can
    // reach the table always has the 1000 stipend behind them.
    chips.ensure_chips(user.id).await.expect("chips row");

    let balance = chips
        .apply_move(user.id, ChipMove::SsnakeArenaEarned, 250)
        .await
        .expect("credit succeeds");
    assert_eq!(balance, Some(1_250), "starting 1000 plus the seat's take");

    let declined = chips
        .apply_move(user.id, ChipMove::SsnakeArenaLost, 5_000)
        .await
        .expect("the call itself succeeds");
    assert_eq!(declined, None, "a debit past zero is refused, not applied");

    let client = test_db.db.get().await.expect("db client");
    let row = client
        .query_one(
            "SELECT balance FROM user_chips WHERE user_id = $1",
            &[&user.id],
        )
        .await
        .expect("chips row");
    assert_eq!(
        row.get::<_, i64>("balance"),
        1_250,
        "the declined debit left the balance alone"
    );
}
