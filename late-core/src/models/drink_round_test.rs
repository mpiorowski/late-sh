use crate::{
    models::drink_round::{
        DrinkCredit, DrinkRound, MAX_OPEN_CREDITS, ROUND_CREDIT_TTL_HOURS, ROUND_PHRASES,
        ROUND_PRICE_PER_PATRON, contains_round_request, round_phrase_spans,
    },
    test_utils::{create_test_user, test_db},
};

/// The phrase is a spending authorization, so what does and does not count as
/// one is the single most important thing in this module. Every entry on the
/// list has to fire, capitalization and surrounding sentence included, and a
/// word that merely contains one must not: "around" ends in "round".
#[test]
fn only_a_deliberate_phrase_orders_a_round() {
    for phrase in ROUND_PHRASES {
        assert!(
            contains_round_request(&format!("@bartender {phrase} please")),
            "{phrase} should order a round"
        );
    }

    assert!(contains_round_request("@bartender A ROUND FOR EVERYONE!"));
    assert!(contains_round_request(
        "@bartender it's been a good week, round for the house"
    ));

    // "around" ends in "round", and a bar full of people saying "turn around"
    // must never be charged for it.
    assert!(!contains_round_request(
        "@bartender turn around for all of us"
    ));
    assert!(!contains_round_request("@bartender grounds for everyone"));
    // Near misses that are not on the list stay off it.
    assert!(!contains_round_request("@bartender a round for me"));
    assert!(!contains_round_request("@bartender rounds for everyone"));
    assert!(!contains_round_request("@bartender what's on tap"));
}

/// The guide teaches the exact words, so asking what they cost is the next
/// thing a patron types. A question about the phrase is not the phrase: it
/// has to get an answer, not a bill.
#[test]
fn asking_about_a_round_is_not_ordering_one() {
    assert!(!contains_round_request(
        "@bartender how much is a round for everyone?"
    ));
    assert!(!contains_round_request(
        "@bartender is there a round for the house tonight?"
    ));
    assert!(!contains_round_request("@bartender round on me?"));
    // Quoting the words in a code span is talking about them, not saying them.
    assert!(!contains_round_request(
        "@bartender what happens if I say `round for everyone`"
    ));
    // The question has to be the phrase's own sentence. An order followed by
    // a question is still an order, and so is one on its own line.
    assert!(contains_round_request(
        "@bartender round for everyone. what do I owe you?"
    ));
    assert!(contains_round_request(
        "@bartender round for everyone\nwhat do I owe you?"
    ));
    // The slur guard keeps protecting the words either way: a drunk question
    // must not scramble into a drunk order.
    assert_eq!(
        round_phrase_spans("how much is a round for everyone?").len(),
        1
    );
}

/// The spans are what `chat/slur.rs` protects, so they have to land on the
/// phrase itself and nothing around it, in the original text's byte offsets
/// rather than the lowercased copy's.
#[test]
fn spans_cover_the_phrase_and_nothing_else() {
    let text = "well then. ROUND FOR ALL, on me";
    let spans = round_phrase_spans(text);
    assert_eq!(spans.len(), 1);
    let (start, end) = spans[0];
    assert_eq!(&text[start..end], "ROUND FOR ALL");
}

/// What replaced the one-open-credit rule (migration 168): a patron banks
/// every round they were not around to drink, up to `MAX_OPEN_CREDITS`, and
/// the round after that reaches them not at all and so costs its buyer
/// nothing. The cap is the mechanic's whole throttle, so it is asserted from
/// both sides.
#[tokio::test]
async fn credits_stack_to_the_cap_and_no_further() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let buyer = create_test_user(&test_db.db, "round-stacking-buyer").await;
    let patron = create_test_user(&test_db.db, "round-stacking-patron").await;

    // Every round up to the cap reaches them, and every buyer pays for it.
    let tx = client.transaction().await.expect("tx");
    for round in 1..=MAX_OPEN_CREDITS {
        let grant = DrinkRound::open(
            &tx,
            buyer.id,
            ROUND_PRICE_PER_PATRON,
            &[patron.id],
            ROUND_CREDIT_TTL_HOURS,
            MAX_OPEN_CREDITS,
        )
        .await
        .expect("a round");
        assert_eq!(
            grant.patron_ids,
            vec![patron.id],
            "round {round} is one the patron was owed"
        );
        assert_eq!(grant.total_chips(), ROUND_PRICE_PER_PATRON);
    }

    // The one past the cap reaches nobody and bills for nobody.
    let past_the_cap = DrinkRound::open(
        &tx,
        buyer.id,
        ROUND_PRICE_PER_PATRON,
        &[patron.id],
        ROUND_CREDIT_TTL_HOURS,
        MAX_OPEN_CREDITS,
    )
    .await
    .expect("the round past the cap");
    assert!(
        past_the_cap.patron_ids.is_empty(),
        "a patron carrying the cap cannot be bought another"
    );
    assert_eq!(
        past_the_cap.total_chips(),
        0,
        "and nobody is charged for a drink that was not poured"
    );
    tx.commit().await.expect("commit");

    let open: i64 = client
        .query_one(
            "SELECT count(*) AS open FROM drink_credits
             WHERE user_id = $1 AND cashed_at IS NULL",
            &[&patron.id],
        )
        .await
        .expect("count")
        .get("open");
    assert_eq!(open, MAX_OPEN_CREDITS);
}

/// The tab is drunk one drink at a time, oldest first, and the patron is told
/// what is left behind each one. `remaining` is what @bartender says out loud,
/// so it has to be the count after the pour, never including the drink just
/// handed over.
#[tokio::test]
async fn a_banked_tab_is_drunk_one_at_a_time() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let buyer = create_test_user(&test_db.db, "round-tab-buyer").await;
    let patron = create_test_user(&test_db.db, "round-tab-patron").await;

    let mut round_ids = Vec::new();
    for _ in 0..MAX_OPEN_CREDITS {
        let tx = client.transaction().await.expect("tx");
        let grant = DrinkRound::open(
            &tx,
            buyer.id,
            ROUND_PRICE_PER_PATRON,
            &[patron.id],
            ROUND_CREDIT_TTL_HOURS,
            MAX_OPEN_CREDITS,
        )
        .await
        .expect("a round");
        tx.commit().await.expect("commit");
        round_ids.push(grant.round.id);
    }

    for (drunk, round_id) in round_ids.iter().enumerate() {
        let expected_left = MAX_OPEN_CREDITS - drunk as i64 - 1;
        let next = DrinkCredit::find_open(&client, patron.id)
            .await
            .expect("read")
            .expect("a credit is still open");
        assert_eq!(
            next.round_id, *round_id,
            "the oldest credit is the next one poured"
        );

        let cashed = DrinkCredit::cash(&client, patron.id)
            .await
            .expect("cash")
            .expect("a drink");
        assert_eq!(cashed.round_id, *round_id);
        assert_eq!(cashed.buyer_user_id, Some(buyer.id));
        assert_eq!(
            cashed.remaining, expected_left,
            "the drink just poured is never counted as still waiting"
        );
    }

    assert!(
        DrinkCredit::cash(&client, patron.id)
            .await
            .expect("cash")
            .is_none(),
        "the tab is empty once every banked drink is drunk"
    );
}

/// Cashing is the moment a free drink stops existing. Two orders landing
/// together must not both drink it, and an expired credit is not a drink at
/// all.
#[tokio::test]
async fn a_credit_is_drunk_once_and_expires_on_its_own() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let buyer = create_test_user(&test_db.db, "round-cash-buyer").await;
    let patron = create_test_user(&test_db.db, "round-cash-patron").await;
    let latecomer = create_test_user(&test_db.db, "round-cash-latecomer").await;

    let tx = client.transaction().await.expect("tx");
    let grant = DrinkRound::open(
        &tx,
        buyer.id,
        ROUND_PRICE_PER_PATRON,
        &[patron.id, latecomer.id],
        ROUND_CREDIT_TTL_HOURS,
        MAX_OPEN_CREDITS,
    )
    .await
    .expect("round");
    assert_eq!(grant.patron_count(), 2);
    tx.commit().await.expect("commit");

    let open = DrinkCredit::find_open(&client, patron.id)
        .await
        .expect("read")
        .expect("an open credit");
    assert_eq!(open.buyer_user_id, Some(buyer.id));

    let cashed = DrinkCredit::cash(&client, patron.id)
        .await
        .expect("cash")
        .expect("a drink");
    assert_eq!(cashed.round_id, grant.round.id);
    assert!(
        DrinkCredit::cash(&client, patron.id)
            .await
            .expect("second cash")
            .is_none(),
        "a credit is drunk exactly once"
    );

    // The latecomer never walked up. Age their credit past its expiry: the
    // bar owes them nothing, and the slot is free for the next round.
    client
        .execute(
            "UPDATE drink_credits SET expires_at = current_timestamp - interval '1 minute'
             WHERE user_id = $1",
            &[&latecomer.id],
        )
        .await
        .expect("age the credit");
    assert!(
        DrinkCredit::find_open(&client, latecomer.id)
            .await
            .expect("read")
            .is_none()
    );
    assert!(
        DrinkCredit::cash(&client, latecomer.id)
            .await
            .expect("cash")
            .is_none(),
        "an expired credit cannot be drunk"
    );
}

/// An expired credit is still an uncashed row, and under the old
/// one-per-patron index it went on occupying the patron's only slot. The cap
/// counts what a patron can actually drink, so a round the patron slept
/// through does not spend their allowance forever.
#[tokio::test]
async fn an_expired_credit_does_not_block_the_next_round() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let buyer = create_test_user(&test_db.db, "round-stale-buyer").await;
    let patron = create_test_user(&test_db.db, "round-stale-patron").await;

    let tx = client.transaction().await.expect("tx");
    DrinkRound::open(
        &tx,
        buyer.id,
        ROUND_PRICE_PER_PATRON,
        &[patron.id],
        ROUND_CREDIT_TTL_HOURS,
        MAX_OPEN_CREDITS,
    )
    .await
    .expect("first round");
    tx.commit().await.expect("commit");

    client
        .execute(
            "UPDATE drink_credits SET expires_at = current_timestamp - interval '1 minute'
             WHERE user_id = $1",
            &[&patron.id],
        )
        .await
        .expect("age the credit");

    let tx = client.transaction().await.expect("tx");
    let second = DrinkRound::open(
        &tx,
        buyer.id,
        ROUND_PRICE_PER_PATRON,
        &[patron.id],
        ROUND_CREDIT_TTL_HOURS,
        MAX_OPEN_CREDITS,
    )
    .await
    .expect("second round");
    tx.commit().await.expect("commit");

    assert_eq!(
        second.patron_ids,
        vec![patron.id],
        "a stale credit is not a drink in hand"
    );
    let open = DrinkCredit::find_open(&client, patron.id)
        .await
        .expect("read")
        .expect("a fresh credit");
    assert_eq!(open.round_id, second.round.id);
}
