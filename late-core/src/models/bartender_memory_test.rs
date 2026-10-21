use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use crate::{
    models::{
        bartender_memory::{
            BartenderMemory, MEMORY_CAP, MemoryKind, MemorySlot, REQUESTED_KEEP, Recorded,
            evictions, is_forget_request, memory_phrase_spans, remember_request,
        },
        chat_message::{ChatMessage, ChatMessageParams},
        chat_room::ChatRoom,
    },
    test_utils::{create_test_user, test_db},
};

fn slots(requested: &[bool]) -> Vec<MemorySlot> {
    requested
        .iter()
        .map(|&requested| MemorySlot {
            id: Uuid::now_v7(),
            requested,
        })
        .collect()
}

#[test]
fn nothing_goes_while_the_patron_is_under_the_cap() {
    assert!(evictions(&slots(&[false; MEMORY_CAP])).is_empty());
    assert!(evictions(&slots(&[true; MEMORY_CAP])).is_empty());
    assert!(evictions(&[]).is_empty());
}

#[test]
fn the_oldest_exchange_makes_room() {
    let all = slots(&[false; MEMORY_CAP + 1]);
    assert_eq!(evictions(&all), vec![all[MEMORY_CAP].id]);
}

#[test]
fn requested_memories_outlast_newer_exchanges() {
    // Newest first: nine nights of exchanges on top of eight old requested memories.
    let mut flags = vec![false; 9];
    flags.extend([true; REQUESTED_KEEP]);
    let all = slots(&flags);
    assert_eq!(
        evictions(&all),
        vec![all[8].id],
        "the oldest exchange goes, not an older requested memory"
    );
}

#[test]
fn only_the_newest_requested_memories_are_protected() {
    // Sixteen requested memories and a new exchange: the new one still gets
    // in, and the oldest requested memory is the one that goes.
    let mut flags = vec![false];
    flags.extend([true; MEMORY_CAP]);
    let all = slots(&flags);
    assert_eq!(evictions(&all), vec![all[MEMORY_CAP].id]);
}

#[test]
fn unprotected_requested_memories_compete_on_recency() {
    // Ten requested memories on top of seven exchanges: two are past the
    // protected eight and are simply the newest of the rest.
    let mut flags = vec![true; 10];
    flags.extend([false; 7]);
    let all = slots(&flags);
    assert_eq!(evictions(&all), vec![all[16].id]);
}

#[test]
fn forget_me_must_be_said_exactly() {
    for message in [
        "@bartender forget me",
        "@bartender forget  me",
        "  @BARTENDER   Forget ME  ",
        "@bartender\tforget me",
        "@bartender\nforget me",
        "@bartender, forget me",
        "@bartender: forget me",
        "forget me @bartender",
        "forget me\t@bartender",
        "@bartender *hic* forget me",
        "@bartender forget me *hic*",
        "forget me",
    ] {
        assert!(is_forget_request(message, "bartender"), "{message:?}");
    }
    for message in [
        "@bartender please forget me",
        "@bartender forget me?",
        "@bartender forget me.",
        "@bartender don't forget me",
        "@bartender forget me not",
        "@bartender forget",
        "@bartender @bartender forget me",
        "forget @bartender me",
        "@bot forget me",
        "@bartender",
    ] {
        assert!(!is_forget_request(message, "bartender"), "{message:?}");
    }
}

#[test]
fn forget_me_requires_ascii_spaces_between_the_words() {
    for separator in ["\t", "\n", "\r\n", " \n ", "\u{a0}", "\u{2003}", " *hic* "] {
        let message = format!("@bartender forget{separator}me");
        assert!(!is_forget_request(&message, "bartender"), "{message:?}");
        assert!(memory_phrase_spans(&message).is_empty(), "{message:?}");
    }
}

#[test]
fn a_remember_request_opens_with_the_handle_and_remember() {
    for (message, words) in [
        ("@bartender remember I take it neat", "I take it neat"),
        ("@BARTENDER REMEMBER: stouts", "stouts"),
        ("@bartender, remember, no ice", "no ice"),
        (
            "@bartender:  remember\n  two lines\nof it  ",
            "two lines\nof it",
        ),
        ("  @bartender\tremember it", "it"),
        ("@bartender *hic* remember stouts", "stouts"),
        ("@bartender remember stouts *hic*", "stouts"),
        ("@bartender remember", ""),
        ("@bartender remember *hic*", ""),
        ("@bartender remember:", ""),
        (
            "@bartender remember buy @alice a drink",
            "buy @alice a drink",
        ),
    ] {
        assert_eq!(
            remember_request(message, "bartender"),
            Some(words),
            "{message:?}"
        );
    }
    for message in [
        "@bartender please remember I take it neat",
        "@bartender do you remember me?",
        "remember stouts @bartender",
        "remember stouts",
        "@bartender remembered my wallet",
        "@bartender remember.",
        "@bartender misremember",
        "@bot remember stouts",
        "@bartender @bartender remember stouts",
        "@bartender",
        "",
    ] {
        assert_eq!(remember_request(message, "bartender"), None, "{message:?}");
    }
}

#[test]
fn drunk_text_spares_both_triggers_and_the_whole_request() {
    let text = "@bartender Remember this, then forget  me";
    let spans = memory_phrase_spans(text);
    let covered: Vec<&str> = spans.iter().map(|&(a, b)| &text[a..b]).collect();
    assert_eq!(
        covered,
        vec!["Remember this, then forget  me", "forget  me"]
    );

    let quoted = "> @alice: earlier\n@bartender remember stouts";
    let spans = memory_phrase_spans(quoted);
    let covered: Vec<&str> = spans.iter().map(|&(a, b)| &quoted[a..b]).collect();
    assert_eq!(covered, vec!["remember stouts"]);

    assert!(memory_phrase_spans("forget meow, remembered").is_empty());
    assert!(memory_phrase_spans("I remember stouts").is_empty());
    assert!(memory_phrase_spans("@bartender do you remember me").is_empty());
}

async fn say(
    client: &deadpool_postgres::Client,
    room_id: Uuid,
    user_id: Uuid,
    body: &str,
    at: DateTime<Utc>,
) -> ChatMessage {
    let message = ChatMessage::create_with_reply_to(
        client,
        ChatMessageParams {
            room_id,
            user_id,
            body: body.to_string(),
        },
        None,
    )
    .await
    .expect("message");
    // Use the persisted timestamp, which PostgreSQL truncates to microseconds.
    let row = client
        .query_one(
            "UPDATE chat_messages SET created = $2 WHERE id = $1 RETURNING created",
            &[&message.id, &at],
        )
        .await
        .expect("backdate message");
    ChatMessage {
        created: row.get("created"),
        ..message
    }
}

fn secs(base: DateTime<Utc>, offset: i64) -> DateTime<Utc> {
    base + Duration::seconds(offset)
}

#[tokio::test]
async fn an_exchange_keeps_what_the_patron_said_until_the_tail_closes() {
    let test_db = test_db().await;
    let patron = create_test_user(&test_db.db, "memory-patron").await;
    let other = create_test_user(&test_db.db, "memory-other").await;
    let bartender = create_test_user(&test_db.db, "memory-keep").await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_public_room(&client, "memory-room")
        .await
        .expect("room");
    let elsewhere = ChatRoom::get_or_create_public_room(&client, "memory-elsewhere")
        .await
        .expect("room");
    let t0 = DateTime::from_timestamp(1_700_000_000, 123_456_789).expect("fixture timestamp");

    let order = say(&client, room.id, patron.id, "@memory-keep a stout", t0).await;
    say(&client, room.id, patron.id, "while I wait", secs(t0, 1)).await;
    say(&client, room.id, other.id, "not theirs", secs(t0, 2)).await;
    say(&client, elsewhere.id, patron.id, "other room", secs(t0, 2)).await;
    let reply = say(&client, room.id, bartender.id, "one stout", secs(t0, 3)).await;
    say(&client, room.id, patron.id, "cheers", secs(t0, 8)).await;
    say(&client, room.id, patron.id, "too late", secs(t0, 9)).await;

    let recorded = BartenderMemory::record_exchange(&mut client, &order, &reply)
        .await
        .expect("record");
    assert_eq!(recorded, Recorded::Stored { evicted: 0 });

    let memories = BartenderMemory::list_for_user(&client, patron.id)
        .await
        .expect("list");
    assert_eq!(memories.len(), 1);
    let memory = &memories[0];
    assert_eq!(memory.ordered_at, order.created);
    assert_eq!(memory.order_body, "@memory-keep a stout");
    assert_eq!(
        memory.kind,
        MemoryKind::Exchange {
            replied_at: reply.created,
            reply_body: "one stout".to_string(),
            trailing_bodies: vec!["while I wait".to_string(), "cheers".to_string()],
        }
    );
}

#[tokio::test]
async fn the_tail_keeps_the_first_five_lines() {
    let test_db = test_db().await;
    let patron = create_test_user(&test_db.db, "tail-patron").await;
    let bartender = create_test_user(&test_db.db, "tail-keep").await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_public_room(&client, "tail-room")
        .await
        .expect("room");
    let t0 = Utc::now() - Duration::hours(1);

    let order = say(&client, room.id, patron.id, "@tail-keep a beer", t0).await;
    let reply = say(&client, room.id, bartender.id, "a beer", secs(t0, 1)).await;
    for line in 1..=7 {
        say(
            &client,
            room.id,
            patron.id,
            &format!("line {line}"),
            secs(t0, 1 + line / 2),
        )
        .await;
    }

    BartenderMemory::record_exchange(&mut client, &order, &reply)
        .await
        .expect("record");
    let memories = BartenderMemory::list_for_user(&client, patron.id)
        .await
        .expect("list");
    let MemoryKind::Exchange {
        trailing_bodies, ..
    } = &memories[0].kind
    else {
        panic!("an exchange: {memories:?}");
    };
    assert_eq!(
        trailing_bodies,
        &["line 1", "line 2", "line 3", "line 4", "line 5"]
    );
}

#[tokio::test]
async fn the_cap_keeps_an_old_requested_memory_over_newer_nights() {
    let test_db = test_db().await;
    let patron = create_test_user(&test_db.db, "cap-patron").await;
    let bartender = create_test_user(&test_db.db, "cap-keep").await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_public_room(&client, "cap-room")
        .await
        .expect("room");
    let t0 = Utc::now() - Duration::hours(2);

    let request = say(
        &client,
        room.id,
        patron.id,
        "@cap-keep remember order 0",
        t0,
    )
    .await;
    BartenderMemory::record_requested(&mut client, &request, "order 0")
        .await
        .expect("record requested");
    for night in 1..=MEMORY_CAP as i64 + 1 {
        let at = secs(t0, night * 60);
        let order = say(&client, room.id, patron.id, &format!("order {night}"), at).await;
        let reply = say(&client, room.id, bartender.id, "poured", secs(at, 1)).await;
        BartenderMemory::record_exchange(&mut client, &order, &reply)
            .await
            .expect("record");
    }

    let memories = BartenderMemory::list_for_user(&client, patron.id)
        .await
        .expect("list");
    assert_eq!(memories.len(), MEMORY_CAP);
    let bodies: Vec<&str> = memories.iter().map(|m| m.order_body.as_str()).collect();
    assert_eq!(bodies[0], "order 0", "the requested memory stays");
    assert_eq!(memories[0].kind, MemoryKind::Requested);
    assert!(
        !bodies.contains(&"order 1") && !bodies.contains(&"order 2"),
        "the two oldest exchanges made room: {bodies:?}"
    );
    assert_eq!(bodies[MEMORY_CAP - 1], "order 17");
}

#[tokio::test]
async fn forget_me_wipes_and_turns_away_what_was_still_in_flight() {
    let test_db = test_db().await;
    let patron = create_test_user(&test_db.db, "forget-patron").await;
    let bartender = create_test_user(&test_db.db, "forget-keep").await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_public_room(&client, "forget-room")
        .await
        .expect("room");
    let t0 = Utc::now() - Duration::hours(1);

    let first = say(&client, room.id, patron.id, "@forget-keep one", t0).await;
    let first_reply = say(&client, room.id, bartender.id, "one", secs(t0, 1)).await;
    BartenderMemory::record_exchange(&mut client, &first, &first_reply)
        .await
        .expect("record");
    // Ordered before the wipe, answered after it.
    let in_flight = say(
        &client,
        room.id,
        patron.id,
        "@forget-keep two",
        secs(t0, 10),
    )
    .await;
    let wipe = say(
        &client,
        room.id,
        patron.id,
        "@forget-keep forget me",
        secs(t0, 11),
    )
    .await;

    let forgotten = BartenderMemory::forget(&mut client, patron.id, wipe.created)
        .await
        .expect("forget");
    assert_eq!(forgotten, 1);

    let late_reply = say(&client, room.id, bartender.id, "two", secs(t0, 12)).await;
    let recorded = BartenderMemory::record_exchange(&mut client, &in_flight, &late_reply)
        .await
        .expect("record");
    assert_eq!(recorded, Recorded::Forgotten);
    assert!(
        BartenderMemory::list_for_user(&client, patron.id)
            .await
            .expect("list")
            .is_empty()
    );

    let after = say(
        &client,
        room.id,
        patron.id,
        "@forget-keep three",
        secs(t0, 20),
    )
    .await;
    let after_reply = say(&client, room.id, bartender.id, "three", secs(t0, 21)).await;
    let recorded = BartenderMemory::record_exchange(&mut client, &after, &after_reply)
        .await
        .expect("record");
    assert_eq!(recorded, Recorded::Stored { evicted: 0 });
}

#[tokio::test]
async fn a_memory_outlives_the_chat_log_it_came_from() {
    let test_db = test_db().await;
    let patron = create_test_user(&test_db.db, "outlive-patron").await;
    let bartender = create_test_user(&test_db.db, "outlive-keep").await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_public_room(&client, "outlive-room")
        .await
        .expect("room");
    let t0 = Utc::now() - Duration::hours(1);

    let order = say(&client, room.id, patron.id, "@outlive-keep one", t0).await;
    let reply = say(&client, room.id, bartender.id, "one", secs(t0, 1)).await;
    let tail = say(&client, room.id, patron.id, "thanks", secs(t0, 2)).await;
    BartenderMemory::record_exchange(&mut client, &order, &reply)
        .await
        .expect("record");
    assert_eq!(
        BartenderMemory::record_exchange(&mut client, &order, &reply)
            .await
            .expect("record again"),
        Recorded::AlreadyStored,
        "one exchange is stored once"
    );

    for message in [&order, &reply, &tail] {
        ChatMessage::delete_by_admin(&client, message.id)
            .await
            .expect("delete");
    }
    let memories = BartenderMemory::list_for_user(&client, patron.id)
        .await
        .expect("list");
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].order_body, "@outlive-keep one");
    assert_eq!(
        memories[0].kind,
        MemoryKind::Exchange {
            replied_at: reply.created,
            reply_body: "one".to_string(),
            trailing_bodies: vec!["thanks".to_string()],
        }
    );
}

#[tokio::test]
async fn a_requested_memory_keeps_only_what_to_remember_once() {
    let test_db = test_db().await;
    let patron = create_test_user(&test_db.db, "requested-patron").await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_public_room(&client, "requested-room")
        .await
        .expect("room");
    let t0 = Utc::now() - Duration::hours(1);

    let message = say(
        &client,
        room.id,
        patron.id,
        "@requested-keep remember no ice, ever",
        t0,
    )
    .await;
    say(&client, room.id, patron.id, "not part of it", secs(t0, 1)).await;
    assert_eq!(
        BartenderMemory::record_requested(&mut client, &message, "no ice, ever")
            .await
            .expect("record requested"),
        Recorded::Stored { evicted: 0 }
    );
    assert_eq!(
        BartenderMemory::record_requested(&mut client, &message, "no ice, ever")
            .await
            .expect("record requested again"),
        Recorded::AlreadyStored
    );

    let memories = BartenderMemory::list_for_user(&client, patron.id)
        .await
        .expect("list");
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].ordered_at, message.created);
    assert_eq!(memories[0].order_body, "no ice, ever");
    assert_eq!(memories[0].kind, MemoryKind::Requested);
}

#[tokio::test]
async fn a_remember_request_said_before_forget_me_is_turned_away() {
    let test_db = test_db().await;
    let patron = create_test_user(&test_db.db, "requested-forget-patron").await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_public_room(&client, "requested-forget-room")
        .await
        .expect("room");
    let t0 = Utc::now() - Duration::hours(1);

    let message = say(&client, room.id, patron.id, "@k remember stouts", t0).await;
    let wipe = say(&client, room.id, patron.id, "@k forget me", secs(t0, 1)).await;
    BartenderMemory::forget(&mut client, patron.id, wipe.created)
        .await
        .expect("forget");
    assert_eq!(
        BartenderMemory::record_requested(&mut client, &message, "stouts")
            .await
            .expect("record requested"),
        Recorded::Forgotten
    );
    assert!(
        BartenderMemory::list_for_user(&client, patron.id)
            .await
            .expect("list")
            .is_empty()
    );
}

#[tokio::test]
async fn forget_me_spares_what_was_said_after_it() {
    let test_db = test_db().await;
    let patron = create_test_user(&test_db.db, "forget-order-patron").await;
    let mut client = test_db.db.get().await.expect("db client");
    let room = ChatRoom::get_or_create_public_room(&client, "forget-order-room")
        .await
        .expect("room");
    let t0 = Utc::now() - Duration::hours(1);

    let wipe = say(&client, room.id, patron.id, "@k forget me", t0).await;
    let later = say(
        &client,
        room.id,
        patron.id,
        "@k remember stouts",
        secs(t0, 1),
    )
    .await;
    // The later request wins the race to the patron's lock.
    assert_eq!(
        BartenderMemory::record_requested(&mut client, &later, "stouts")
            .await
            .expect("record requested"),
        Recorded::Stored { evicted: 0 }
    );

    let forgotten = BartenderMemory::forget(&mut client, patron.id, wipe.created)
        .await
        .expect("forget");
    assert_eq!(forgotten, 0);
    let memories = BartenderMemory::list_for_user(&client, patron.id)
        .await
        .expect("list");
    assert_eq!(memories.len(), 1);
    assert_eq!(memories[0].order_body, "stouts");
}
