use crate::{
    models::drink_round::{
        Bar, BarOrder, DrinkCredit, DrinkRound, MAX_OPEN_CREDITS, ROUND_CREDIT_TTL_HOURS,
        ROUND_DRINK_POINTS, ROUND_PHRASES, ROUND_PRICE_PER_PATRON, bar_order,
        contains_round_request, gift_drink_target, round_phrase_spans, spending_phrase_spans,
    },
    test_utils::{create_test_user, test_db},
};

/// A gift moves chips for somebody else, so it reads like the round: any
/// phrase on the list, opening a clause anywhere in the message, as a
/// statement, naming one person.
#[test]
fn a_gift_phrase_opening_its_clause_names_its_recipient() {
    for message in [
        "@bartender buy @alice a drink",
        "  @BARTENDER BUY @alice A DRINK  ",
        "@bartender drink for @alice",
        "@bartender a drink for @alice.",
        "@bartender drink for @alice, she earned it",
        "@bartender get @alice a drink please",
        "@bartender pour @alice one!",
        "@bartender ok then, please get @alice a drink",
        "@bartender can you buy a drink for @alice",
        "@bartender I'll buy @alice a beer",
        "@bartender let me get @alice a shot",
        "@bartender I'd like to send @alice a round",
        "@bartender one for @alice please",
        "@bartender put a drink for @alice on my tab",
        "@bartender @alice's next one is on me",
        "hey @bartender, @alice\u{2019}s drink is on me.",
        "@bartender buy @alice a drink. what do I owe you?",
        "@bartender buy @alice a drink `what?` now",
    ] {
        assert_eq!(
            gift_drink_target(message, "bartender"),
            Some("alice"),
            "{message}"
        );
    }
    assert_eq!(
        gift_drink_target("@bartender drink for @Alice_2.", "bartender"),
        Some("Alice_2")
    );
}

/// What stays a conversation: questions, quotes, near misses, and anything
/// that leaves who the drink is for in doubt.
#[test]
fn a_gift_is_refused_when_it_is_not_plainly_one_order() {
    for message in [
        "@bartender can you buy @alice a drink?",
        "@bartender drink for @alice?",
        "@bartender buy @alice a drink with `chips`?",
        "@bartender what if I say `buy @alice a drink`",
        "@bartender buy @alice! a drink",
        "@bartender buy @alice two drinks",
        "@bartender buy @alice a drinks",
        "@bartender drinks for @alice",
        "@bartender I'll get @alice one of those",
        "@bartender buy @ a drink",
        "@bartender buy @alice a drink for @bob",
        "@bartender drink for @alice and @bob",
        "@bartender buy @alice a drink\n@bob pay for it",
        "@bartender buy @alice a drink, then buy @bob a drink",
    ] {
        assert_eq!(gift_drink_target(message, "bartender"), None, "{message}");
    }
}

/// An order opens its clause: after the bartender's name and a few lead-in
/// words at most. The same phrase further into a sentence is somebody talking
/// about a drink (reporting one, refusing one, thanking for one) and spends
/// nothing, for the gift and the round alike. A clause break before the phrase
/// starts the count again.
#[test]
fn a_phrase_inside_a_longer_sentence_spends_nothing() {
    for message in [
        "@bartender don't buy @alice a drink",
        "@bartender DONT buy @alice a drink",
        "@bartender do not get @alice a drink, she is wasted",
        "@bartender I never said pour @alice one",
        "@bartender no drink for @alice tonight",
        "@bartender I already got a drink for @alice earlier",
        "@bartender I told him to get @alice a drink",
        "@bartender thanks, that drink for @alice made her night",
        "@bartender I would buy @alice a drink if I could",
        "@bartender @alice's drink was on me",
        "@bartender `a` then buy @alice a drink",
    ] {
        assert_eq!(gift_drink_target(message, "bartender"), None, "{message}");
    }
    for message in [
        "@bartender no round for everyone tonight",
        "@bartender I am not saying round on me",
        "@bartender don\u{2019}t make it a round for the house",
        "@bartender he said round on me",
        "@bartender we had drinks for everyone last night",
        "@bartender @alice drinks on me",
        "@bartender I'll buy a round",
    ] {
        assert!(!contains_round_request(message, "bartender"), "{message}");
    }

    assert_eq!(
        gift_drink_target("@bartender don't worry, buy @alice a drink", "bartender"),
        Some("alice")
    );
    assert_eq!(
        gift_drink_target("@bartender no. drink for @alice", "bartender"),
        Some("alice")
    );
    assert!(contains_round_request(
        "@bartender why not, round on me",
        "bartender"
    ));
    assert!(contains_round_request(
        "@bartender and another round for the bar",
        "bartender"
    ));
}

/// A message places one order at most. A gift and a round together are two,
/// so the bar rings up neither and the bartender gets the message as talk; a
/// phrase the other list refused (a question, one inside a longer sentence)
/// does not count.
#[test]
fn a_message_places_one_order_or_none() {
    let orders: Vec<_> = [
        "@bartender pour @alice one",
        "@bartender round for everyone",
        "@bartender round for everyone, and pour @alice one",
        "@bartender buy @alice a drink. round on me",
        "@bartender round on me, don't buy @alice a drink",
        "@bartender buy @alice a drink. is there a round for the house?",
        "@bartender what's on tap",
    ]
    .into_iter()
    .map(|message| bar_order(message, "bartender"))
    .collect();
    assert_eq!(
        orders,
        vec![
            BarOrder::Gift("alice"),
            BarOrder::Round,
            BarOrder::Talk,
            BarOrder::Talk,
            BarOrder::Round,
            BarOrder::Gift("alice"),
            BarOrder::Talk,
        ]
    );
}

/// The slur guard's view covers the gift phrase from its first word to its
/// last, in the original text's byte offsets, questions included, and the
/// lead-in words an order rides in on: a scrambled "and a" would stop the
/// phrase opening its clause. A phrase that opens nothing is covered alone.
#[test]
fn spending_spans_cover_gift_phrases_too() {
    let text = "ok. Drink For @alice, and a ROUND FOR ALL? he said round on me";
    let covered: Vec<&str> = spending_phrase_spans(text)
        .into_iter()
        .map(|(start, end)| &text[start..end])
        .collect();
    assert_eq!(
        covered,
        vec!["Drink For @alice", "and a ROUND FOR ALL", "round on me"]
    );
}

/// The phrase is a spending authorization, so what does and does not count as
/// one is the single most important thing in this module. Every entry on the
/// list has to fire, capitalization and surrounding sentence included, and a
/// word that merely contains one must not: "around" ends in "round".
#[test]
fn only_a_deliberate_phrase_orders_a_round() {
    for phrase in ROUND_PHRASES {
        assert!(
            contains_round_request(&format!("@bartender {phrase} please"), "bartender"),
            "{phrase} should order a round"
        );
    }

    assert!(contains_round_request(
        "@bartender A ROUND FOR EVERYONE!",
        "bartender"
    ));
    assert!(contains_round_request(
        "@bartender it's been a good week, round for the house",
        "bartender"
    ));

    // The way people order at a bar: a verb or a few words of intent in front
    // of the phrase are a lead-in, not a longer sentence.
    for message in [
        "@bartender I'll buy a round for everyone",
        "@bartender let's do a round for the house",
        "@bartender buy everyone a drink",
        "@bartender the next round's on me",
        "@bartender can I get drinks for the whole bar",
        "hey @bartender, drinks on me tonight",
    ] {
        assert!(contains_round_request(message, "bartender"), "{message}");
    }

    // "around" ends in "round", and a bar full of people saying "turn around"
    // must never be charged for it.
    assert!(!contains_round_request(
        "@bartender turn around for all of us",
        "bartender"
    ));
    assert!(!contains_round_request(
        "@bartender grounds for everyone",
        "bartender"
    ));
    // Near misses that are not on the list stay off it.
    assert!(!contains_round_request(
        "@bartender a round for me",
        "bartender"
    ));
    assert!(!contains_round_request(
        "@bartender rounds for everyone",
        "bartender"
    ));
    assert!(!contains_round_request(
        "@bartender what's on tap",
        "bartender"
    ));
}

/// The guide teaches the exact words, so asking what they cost is the next
/// thing a patron types. A question about the phrase is not the phrase: it
/// has to get an answer, not a bill.
#[test]
fn asking_about_a_round_is_not_ordering_one() {
    assert!(!contains_round_request(
        "@bartender how much is a round for everyone?",
        "bartender"
    ));
    assert!(!contains_round_request(
        "@bartender round for everyone in `#lounge`?",
        "bartender"
    ));
    assert!(!contains_round_request(
        "@bartender is there a round for the house tonight?",
        "bartender"
    ));
    assert!(!contains_round_request(
        "@bartender round on me?",
        "bartender"
    ));
    // Quoting the words in a code span is talking about them, not saying them.
    assert!(!contains_round_request(
        "@bartender what happens if I say `round for everyone`",
        "bartender"
    ));
    // The question has to be the phrase's own sentence. An order followed by
    // a question is still an order, and so is one on its own line.
    assert!(contains_round_request(
        "@bartender round for everyone. what do I owe you?",
        "bartender"
    ));
    assert!(contains_round_request(
        "@bartender round for everyone\nwhat do I owe you?",
        "bartender"
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
            Bar::Tavern,
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
        Bar::Tavern,
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
            Bar::Tavern,
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
        Bar::Tavern,
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

/// What a free drink is worth is set by the bar that bought the round, not
/// by where it is drunk: a credit is good at either bar. A Nightcap round
/// pours exactly what its buyer paid a head, because it was bought for the
/// patrons on the stools who will drink it; the tavern's pours the premium
/// that covers the room it was bought for and never walks up.
#[tokio::test]
async fn a_credit_pours_what_the_bar_that_bought_it_pours() {
    let test_db = test_db().await;
    let mut client = test_db.db.get().await.expect("db client");
    let buyer = create_test_user(&test_db.db, "round-bar-buyer").await;
    let patron = create_test_user(&test_db.db, "round-bar-patron").await;

    let tx = client.transaction().await.expect("tx");
    DrinkRound::open(
        &tx,
        buyer.id,
        ROUND_PRICE_PER_PATRON,
        Bar::Nightcap,
        &[patron.id],
        ROUND_CREDIT_TTL_HOURS,
        MAX_OPEN_CREDITS,
    )
    .await
    .expect("a nightcap round");
    tx.commit().await.expect("commit");

    assert_eq!(
        DrinkCredit::count_open(&client, patron.id)
            .await
            .expect("count"),
        1,
        "the menu counts the drink they are holding"
    );

    let cashed = DrinkCredit::cash(&client, patron.id)
        .await
        .expect("cash")
        .expect("a drink");
    assert_eq!(cashed.bar, Bar::Nightcap);
    assert_eq!(
        cashed.bar.drink_points(),
        ROUND_PRICE_PER_PATRON,
        "a nightcap round pours chips to points 1:1"
    );
    assert_eq!(
        Bar::Tavern.drink_points(),
        ROUND_DRINK_POINTS,
        "the tavern's round still pours its premium"
    );
    assert_eq!(
        DrinkCredit::count_open(&client, patron.id)
            .await
            .expect("count"),
        0,
        "the drink they drank is not one they are holding"
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
        Bar::Tavern,
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
        Bar::Tavern,
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

#[tokio::test]
async fn rounds_resolve_by_id() {
    let test_db = test_db().await;
    let buyer = create_test_user(&test_db.db, "round-buyer").await;
    let patron = create_test_user(&test_db.db, "round-patron").await;
    let mut client = test_db.db.get().await.expect("db client");
    let tx = client.transaction().await.expect("tx");
    let grant = DrinkRound::open(
        &tx,
        buyer.id,
        ROUND_PRICE_PER_PATRON,
        Bar::Tavern,
        &[patron.id],
        ROUND_CREDIT_TTL_HOURS,
        MAX_OPEN_CREDITS,
    )
    .await
    .expect("round");
    tx.commit().await.expect("commit");

    let rounds = DrinkRound::find_by_ids(&client, &[grant.round.id, uuid::Uuid::now_v7()])
        .await
        .expect("rounds");
    assert_eq!(rounds.len(), 1);
    assert_eq!(
        rounds
            .get(&grant.round.id)
            .map(|round| round.price_per_patron),
        Some(ROUND_PRICE_PER_PATRON)
    );
}
