use std::collections::HashMap;

use chrono::{DateTime, Duration, TimeZone, Utc};
use late_core::models::deadchannel_bout::{Bout, BoutBet, BoutOutcome, Side};
use rand::SeedableRng;
use rand::rngs::StdRng;
use uuid::Uuid;

use super::{RingView, panel_lines};
use crate::app::deadchannel::fight::cards::{Card, DECK};
use crate::app::deadchannel::fight::data::RULES;
use crate::app::deadchannel::fight::state::Sheet;
use crate::app::deadchannel::ring::data::{BELL_SECS, CALL_SECS, ROUND_SECS};
use crate::app::deadchannel::ring::session::{Contender, seen_by};
use crate::app::deadchannel::ring::state::{Duel, Picks};
use crate::app::deadchannel::ring::svc::{Board, LiveBout};

fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2101, 3, 9, 22, 0, 0).unwrap()
}

struct Ring {
    mira: Uuid,
    tom: Uuid,
    wren: Uuid,
    board: Board,
}

/// A call from mira to tom, open at [`at`].
fn call(mira: Uuid, tom: Uuid) -> Bout {
    Bout {
        id: Uuid::now_v7(),
        day: at().date_naive(),
        caller_user_id: mira,
        called_user_id: tom,
        caller_username: "mira".to_string(),
        called_username: "tom".to_string(),
        caller_deck: None,
        called_deck: None,
        called_at: at(),
        expires_at: at() + Duration::seconds(CALL_SECS),
        accepted_at: None,
        bell_at: None,
        round: 0,
        deadline_at: None,
        state: None,
        pool_caller: 0,
        pool_called: 0,
        ended_at: None,
        outcome: None,
        caller_settled_at: None,
        called_settled_at: None,
    }
}

/// An empty ring with mira, tom and wren around it.
fn ring() -> Ring {
    Ring {
        mira: Uuid::now_v7(),
        tom: Uuid::now_v7(),
        wren: Uuid::now_v7(),
        board: Board::default(),
    }
}

/// The same ring with mira's call taken at [`at`]: decks in, round 1
/// dealt off a fixed seed, the bell set.
fn set() -> Ring {
    let mut ring = ring();
    let bell = at() + Duration::seconds(BELL_SECS);
    let deck = [&DECK[..], &[Card::Jab, Card::Mute]].concat();
    let mut bout = call(ring.mira, ring.tom);
    bout.caller_deck = Some(serde_json::json!(deck));
    bout.called_deck = Some(serde_json::json!(DECK));
    bout.accepted_at = Some(at());
    bout.bell_at = Some(bell);
    bout.round = 1;
    bout.deadline_at = Some(bell + Duration::seconds(ROUND_SECS));
    ring.board.live = Some(LiveBout {
        bout,
        duel: Duel::deal(DECK.to_vec(), DECK.to_vec(), &mut StdRng::seed_from_u64(7)),
        bets: Vec::new(),
    });
    ring
}

fn sheet(marks: i32, level: i32) -> Sheet {
    let mut sheet = Sheet::fresh(Uuid::now_v7(), at().date_naive());
    sheet.marks = marks;
    sheet.level = level;
    sheet.peak_level = level;
    sheet.draft_up();
    sheet
}

/// The panel as `viewer` reads it at `now`, a row a string.
fn panel(ring: &Ring, viewer: Uuid, sheet: &Sheet, now: DateTime<Utc>) -> Vec<String> {
    let board = seen_by(&ring.board, viewer);
    let roster = [("ash", 3), ("tom", 1)]
        .map(|(name, marks)| Contender {
            user_id: match name {
                "tom" => ring.tom,
                _ => Uuid::nil(),
            },
            username: name.to_string(),
            marks,
        })
        .to_vec();
    let view = RingView {
        board: &board,
        picks: Some(Picks::opening().flipped(1)),
        cursor: 1,
        corner: Side::Called,
        roster,
        me: viewer,
        sheet: Some(sheet),
        now,
        word: None,
    };
    panel_lines(&view)
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect()
}

fn row<'a>(rows: &'a [String], holding: &str) -> &'a str {
    rows.iter()
        .find(|row| row.contains(holding))
        .unwrap_or_else(|| panic!("no row holding {holding:?} in:\n{}", rows.join("\n")))
}

fn holds(rows: &[String], text: &str) -> bool {
    rows.iter().any(|row| row.contains(text))
}

#[test]
fn an_empty_ring_shows_a_marked_runner_the_deck_the_wire_and_the_calls() {
    let mut ring = ring();
    ring.board.calls = vec![call(ring.mira, ring.tom)];
    ring.board.records = HashMap::from([(ring.tom, (3, 1))]);
    let now = at() + Duration::seconds(19);

    // Mira, marked and at a draft level she has not fought for.
    let rows = panel(&ring, ring.mira, &sheet(1, 6), now);
    assert_eq!(
        row(&rows, "[1]").trim(),
        "[1] ‹jab› not brace   [2] ‹bulwark› not riposte   [3] ‹burn› not ground   [4] ‹sever› not mute"
    );
    assert_eq!(
        row(&rows, "ash").trim_end(),
        "    ash                 marks 3   no bouts yet"
    );
    assert_eq!(
        row(&rows, "▸").trim_end(),
        "  ▸ tom                 marks 1   3 won, 1 lost"
    );
    assert_eq!(row(&rows, "you called"), "  you called tom. 2:41 left.");
    assert!(holds(&rows, "you owe the ring a bout at level 6"));
    assert_eq!(
        row(&rows, "[c]").trim_end(),
        "[↑↓] pick   [c] call   [1-4] flip a pick   [x] take it back"
    );

    // Tom reads the same call as his to answer.
    let rows = panel(&ring, ring.tom, &sheet(1, 2), now);
    assert_eq!(row(&rows, "calls you"), "  mira calls you. 2:41 to answer.");
    assert!(holds(&rows, "[a] take the call   [x] not tonight"));
    assert!(!holds(&rows, "you owe the ring"));

    // Wren has no mark: the call to read, no deck, nobody to call, no keys.
    let rows = panel(&ring, ring.wren, &sheet(0, 6), now);
    assert_eq!(row(&rows, "mira calls"), "  mira calls tom. 2:41 left.");
    assert!(holds(
        &rows,
        "the ropes are for runners who put the Old Signal down"
    ));
    for marked_only in ["[1]", "[c]", "ash", "you owe the ring"] {
        assert!(
            !holds(&rows, marked_only),
            "{marked_only} is for the marked"
        );
    }
}

#[test]
fn a_set_bout_shows_the_bell_the_pool_and_the_press_a_bet_takes() {
    let mut ring = set();
    let live = ring.board.live.as_mut().expect("the bout");
    live.bout.pool_caller = 120;
    live.bout.pool_called = 40;
    live.bets = vec![BoutBet {
        bout_id: live.bout.id,
        user_id: ring.wren,
        username: "wren".to_string(),
        side: Side::Caller,
        bits: 80,
        presses: 2,
        placed_at: at(),
        paid_at: None,
        paid_bits: None,
    }];
    let now = at() + Duration::seconds(48);
    let watcher = sheet(0, 4);
    let stake = RULES.cache(4);

    let rows = panel(&ring, ring.wren, &watcher, now);
    assert!(row(&rows, "the bell in").starts_with("the bell in 4:12."));
    assert_eq!(
        row(&rows, "the pool"),
        "  the pool   120 on mira   40 on tom"
    );
    assert_eq!(
        row(&rows, "your bet"),
        "  your bet   80 on mira, 2 of 3 presses"
    );
    // What each corner drafted and inked, the base ten left unsaid.
    assert_eq!(
        row(&rows, "  mira  ").trim_end().rsplit("  ").next(),
        Some("‹jab› ‹mute›")
    );
    // The corner picked is the one the press names.
    assert_eq!(
        row(&rows, "[b]").trim_end(),
        format!("[←→] corner: tom   [b] put {stake} bits on tom")
    );
    assert!(!holds(&rows, "your slots"), "a watcher lays nothing");

    // A fighter's press goes on their own corner, and round 1 is theirs
    // to lay before the bell.
    let rows = panel(&ring, ring.mira, &sheet(1, 4), now);
    assert_eq!(
        row(&rows, "[b]").trim_end(),
        format!(
            "[b] put {stake} bits on mira   [1-5] lay a card, or take it back   [e] lock the round"
        )
    );
    assert!(holds(&rows, "your slots"));
    assert_eq!(row(&rows, "your bet"), "  your bet   nothing down");
}

#[test]
fn the_rounds_show_both_hands_and_only_your_own_slots() {
    let mut ring = set();
    let live = ring.board.live.as_mut().expect("the bout");
    let hand = |duel: &Duel, side: Side, slot: usize| {
        duel.corner(side).piles.card(slot).expect("a full hand")
    };
    let (mira_first, tom_second) = (
        hand(&live.duel, Side::Caller, 0),
        hand(&live.duel, Side::Called, 1),
    );
    live.duel.place(Side::Caller, 0, 0).expect("mira lays");
    live.duel.place(Side::Called, 0, 1).expect("tom lays");
    live.duel.lock(Side::Called).expect("tom locks");
    let now = at() + Duration::seconds(BELL_SECS + 9);

    // Mira: her card sits in her slot, her hand carries its keys, and
    // tom's laid card is still in his open hand like the rest.
    let rows = panel(&ring, ring.mira, &sheet(1, 4), now);
    assert!(row(&rows, "round 1 of 4").ends_with("the clock 0:21"));
    assert!(
        row(&rows, "your slots").starts_with(&format!("  your slots  1:{}", mira_first.marked()))
    );
    assert!(row(&rows, "[1] ").contains(&format!("[1] {} in slot 1", mira_first.name())));
    assert!(row(&rows, "  tom  ").ends_with("locked"));
    assert!(row(&rows, "  mira (you)").ends_with("laying..."));
    let toms_hand = rows
        .iter()
        .find(|row| row.starts_with("  tom  ") && row.contains('‹'))
        .expect("tom's hand");
    assert!(toms_hand.contains(&tom_second.marked()));
    assert!(!toms_hand.contains("in slot"), "tom's slots are his own");

    // A watcher gets both hands and nobody's slots.
    let rows = panel(&ring, ring.wren, &sheet(0, 4), now);
    assert!(!holds(&rows, "your slots"));
    assert!(!holds(&rows, "in slot"));
    assert!(!holds(&rows, "[1-5]"));

    // The round comes out: the tape names what each corner played and
    // what got through.
    let live = ring.board.live.as_mut().expect("the bout");
    live.duel.lock(Side::Caller).expect("mira locks");
    live.duel.resolve(&mut StdRng::seed_from_u64(7));
    let rows = panel(&ring, ring.wren, &sheet(0, 4), now);
    assert!(holds(&rows, "round 1 came out"));
    let first = row(&rows, "slot 1");
    assert!(first.contains(&mira_first.marked()) && first.contains(&tom_second.marked()));
    assert!(row(&rows, "slot 2").contains("held"));
}

#[test]
fn a_finished_bout_shows_the_result_the_split_and_your_pay() {
    let mut ring = set();
    let live = ring.board.live.as_mut().expect("the bout");
    // Tom holds every round; mira is put down by hand so the test owns
    // the ending: three strikes of ten into nothing is not a knockout,
    // so the rounds run out and the points decide.
    for _ in 0..4 {
        let strike = (0..5u8).find(|hand| {
            live.duel
                .corner(Side::Caller)
                .piles
                .card(usize::from(*hand))
                == Some(Card::Strike)
        });
        if let Some(strike) = strike {
            live.duel.place(Side::Caller, 0, strike).expect("a strike");
        }
        live.duel.lock(Side::Caller).expect("mira locks");
        live.duel.lock(Side::Called).expect("tom locks");
        live.duel.resolve(&mut StdRng::seed_from_u64(7));
    }
    let ending = live.duel.ending.expect("four rounds end it");
    assert_eq!(ending.winner(), Some(Side::Caller), "mira hit, tom held");
    let ended = at() + Duration::seconds(BELL_SECS + 150);
    live.bout.ended_at = Some(ended);
    live.bout.outcome = Some(BoutOutcome::Caller);
    live.bout.pool_caller = 100;
    live.bout.pool_called = 100;
    let bet = |user_id: Uuid, side: Side| BoutBet {
        bout_id: Uuid::nil(),
        user_id,
        username: String::new(),
        side,
        bits: 100,
        presses: 1,
        placed_at: at(),
        paid_at: None,
        paid_bits: None,
    };
    live.bets = vec![bet(ring.wren, Side::Caller), bet(ring.tom, Side::Called)];
    let now = ended + Duration::seconds(18);

    let rows = panel(&ring, ring.wren, &sheet(0, 4), now);
    assert!(rows[0].starts_with("mira took it on points, "));
    assert_eq!(
        row(&rows, "the pool was"),
        "  the pool was 200 bits: 20 burned, 20 to mira, 160 to the bets on them."
    );
    assert_eq!(row(&rows, "your 100"), "  your 100 on mira pays 160.");
    assert_eq!(row(&rows, "the ring clears"), "the ring clears in 0:42.");
    assert!(!holds(&rows, "you fought"));

    let rows = panel(&ring, ring.tom, &sheet(1, 4), now);
    assert_eq!(row(&rows, "your 100"), "  your 100 on tom is gone.");
    assert!(holds(
        &rows,
        "you fought. the ring is done with you: the road is open."
    ));
}
