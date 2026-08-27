use crate::app::games::chips::svc::ChipService;
use late_core::{
    models::chips::{ChipMove, UserChips},
    models::reward::{
        DARKROOM_ESCAPE_REWARD_KEY, GREENDRAGON_DRAGON_REWARD_KEY, LATEANIA_ARCHDEMON_REWARD_KEY,
    },
    test_utils::create_test_user,
};
use uuid::Uuid;

use crate::test_helpers::new_test_db;

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

// ---- the repeatable door payouts (SHOP.md Phase 6) -----------------------

/// Walk every claim this account holds past a lockout window without sleeping.
async fn age_claims(db: &late_core::db::Db, user_id: Uuid, days: i32) {
    let client = db.get().await.expect("db client");
    client
        .execute(
            "UPDATE game_payout_claims
             SET created = created - make_interval(days => $2)
             WHERE user_id = $1",
            &[&user_id, &days],
        )
        .await
        .expect("age claims");
}

/// The dragon sends the character back to level 1, so the climb is the gate
/// and every kill pays. The payout is keyed on the character row plus the kill
/// number, so a retry of the same kill pays once and a recreated character
/// starts its own count.
#[tokio::test]
async fn a_dragon_kill_pays_every_kill_and_a_new_character_starts_over() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "gd-kills").await;
    let client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, user.id)
        .await
        .expect("chips row");
    drop(client);
    let chips = ChipService::new(test_db.db.clone());
    let (first_character, second_character) = (Uuid::now_v7(), Uuid::now_v7());

    let kill = |character: Uuid, number: u32| {
        let chips = chips.clone();
        async move {
            chips
                .credit_per_event_reward_template(
                    user.id,
                    GREENDRAGON_DRAGON_REWARD_KEY,
                    &format!("{character}:{number}"),
                    ChipMove::GreendragonDragonSlain,
                )
                .await
                .expect("dragon kill payout")
        }
    };

    let first = kill(first_character, 1).await;
    assert!(first.credited);
    assert_eq!(first.amount, 10_000);
    assert_eq!(first.balance, 11_000);

    // The same kill, seen twice (a retried fire-and-forget task).
    assert!(!kill(first_character, 1).await.credited);

    // The next kill on the same character, and the first kill of a character
    // rolled after this one was deleted: both are their own event.
    assert!(kill(first_character, 2).await.credited);
    let fresh_character = kill(second_character, 1).await;
    assert!(fresh_character.credited);
    assert_eq!(fresh_character.balance, 31_000);
}

/// A Dark Room wipes the save on the way out, so a repeat is the whole arc
/// again and every run that gets out pays. The run id is the whole gate.
#[tokio::test]
async fn a_darkroom_escape_pays_every_run() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "adr-runs").await;
    let client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, user.id)
        .await
        .expect("chips row");
    drop(client);
    let chips = ChipService::new(test_db.db.clone());

    let escape = |run: Uuid| {
        let chips = chips.clone();
        async move {
            chips
                .credit_per_event_reward_template(
                    user.id,
                    DARKROOM_ESCAPE_REWARD_KEY,
                    &run.to_string(),
                    ChipMove::DarkroomEscape,
                )
                .await
                .expect("escape payout")
        }
    };

    let run = Uuid::now_v7();
    let first = escape(run).await;
    assert!(first.credited);
    assert_eq!(first.amount, 15_000);
    assert!(!escape(run).await.credited, "one run, one payout");

    let second = escape(Uuid::now_v7()).await;
    assert!(second.credited);
    assert_eq!(second.balance, 31_000);
}

/// A Lateania crown is behind two gates at once: the character persists, so a
/// maxed one would take the easy crowns nightly without the weekly lockout,
/// and `d` deletes the character, so the lockout has to key on the account.
#[tokio::test]
async fn a_lateania_crown_pays_once_per_character_and_once_a_week() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "mud-crowns").await;
    let client = test_db.db.get().await.expect("db client");
    UserChips::ensure(&client, user.id)
        .await
        .expect("chips row");
    drop(client);
    let chips = ChipService::new(test_db.db.clone());
    let (first_character, second_character) = (Uuid::now_v7(), Uuid::now_v7());

    let crown = |character: Uuid| {
        let chips = chips.clone();
        async move {
            chips
                .credit_run_cooldown_reward_template(
                    user.id,
                    LATEANIA_ARCHDEMON_REWARD_KEY,
                    &character.to_string(),
                    ChipMove::LateaniaArchdemonDefeat,
                )
                .await
                .expect("crown payout")
        }
    };

    let first = crown(first_character).await;
    assert!(first.credited);
    assert_eq!(first.amount, 10_000);
    assert_eq!(first.balance, 11_000);

    // The same character taking the same crown again: never.
    assert!(!crown(first_character).await.credited);
    // A rerolled character inside the week: the lockout answers.
    assert!(!crown(second_character).await.credited);
    assert_eq!(balance(&test_db.db, user.id).await, 11_000);

    // Past the week, the second character is paid, and the first still is not.
    age_claims(&test_db.db, user.id, 8).await;
    let later = crown(second_character).await;
    assert!(later.credited);
    assert_eq!(later.balance, 21_000);
    assert!(!crown(first_character).await.credited);
    assert_eq!(balance(&test_db.db, user.id).await, 21_000);
}

async fn balance(db: &late_core::db::Db, user_id: Uuid) -> i64 {
    let client = db.get().await.expect("db client");
    UserChips::ensure(&client, user_id)
        .await
        .expect("chips row")
        .balance
}
