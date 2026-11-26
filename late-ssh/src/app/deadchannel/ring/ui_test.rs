use std::collections::HashMap;

use chrono::{DateTime, Duration, TimeZone, Utc};
use late_core::models::deadchannel_bout::{Bout, BoutBet, BoutOutcome, Side};
use rand::SeedableRng;
use rand::rngs::StdRng;
use uuid::Uuid;

use super::{RingView, notice, panel_lines};
use crate::app::deadchannel::fight::cards::{Card, DECK, DRAFTS};
use crate::app::deadchannel::fight::data::RULES;
use crate::app::deadchannel::fight::state::Sheet;
use crate::app::deadchannel::ring::data::{BELL_SECS, CALL_SECS, ROUND_SECS, SIGNAL, powers};
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

/// The panel as `viewer` reads it at `now`, a row a string, on a screen
/// with room for all of it.
fn panel(ring: &Ring, viewer: Uuid, sheet: &Sheet, now: DateTime<Utc>) -> Vec<String> {
    panel_short_by(ring, viewer, sheet, now, 0)
}

/// [`panel`] on a screen `short_by` rows short of all of it.
fn panel_short_by(
    ring: &Ring,
    viewer: Uuid,
    sheet: &Sheet,
    now: DateTime<Utc>,
    short_by: usize,
) -> Vec<String> {
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
    panel_lines(&view, short_by)
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

/// The row of the tape for `slot`, from 1.
fn tape_row(rows: &[String], slot: usize) -> &str {
    let label = format!("  {slot}  ");
    rows.iter()
        .find(|row| row.starts_with(&label))
        .unwrap_or_else(|| panic!("no tape row for slot {slot} in:\n{}", rows.join("\n")))
}

/// What the street's notice tells `viewer` at `now`, and the verb beside
/// the city's key.
fn noticed(ring: &Ring, viewer: Uuid, now: DateTime<Utc>) -> Option<(String, &'static str)> {
    let board = seen_by(&ring.board, viewer);
    let view = RingView {
        board: &board,
        picks: None,
        cursor: 0,
        corner: Side::Caller,
        roster: Vec::new(),
        me: viewer,
        sheet: None,
        now,
        word: None,
    };
    notice(&view).map(|notice| {
        let told = notice
            .told
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        (told, notice.verb)
    })
}

#[test]
fn an_empty_ring_shows_a_marked_runner_the_deck_the_wire_and_the_calls() {
    let mut ring = ring();
    ring.board.calls = vec![call(ring.mira, ring.tom)];
    ring.board.records = HashMap::from([(ring.tom, (3, 1))]);
    let now = at() + Duration::seconds(19);

    // Mira, marked and at a draft level she has not fought for.
    let rows = panel(&ring, ring.mira, &sheet(1, 6), now);
    // Each pick with the card it was taken over and what it does here.
    assert_eq!(
        row(&rows, "[1]"),
        "  [1] ‹jab› not brace         free. hits for three quarters of a strike"
    );
    assert_eq!(
        row(&rows, "[2]"),
        "  [2] ‹bulwark› not riposte   two energy. holds two blocks and a half"
    );
    // The road's line names the glyph; the ring's names the other corner.
    assert_eq!(
        row(&rows, "[4]"),
        "  [4] ‹sever› not mute        a strike. hits for 2.5 strikes once they are at half"
    );
    assert_eq!(
        row(&rows, "ash").trim_end(),
        "    ash                  marks 3   no bouts yet"
    );
    assert_eq!(
        row(&rows, "▸").trim_end(),
        "  ▸ tom                  marks 1   3 won, 1 lost"
    );
    assert_eq!(row(&rows, "you called"), "  you called tom. 9:41 left.");
    assert!(holds(&rows, "you owe the ring a bout at level 6"));
    assert_eq!(
        row(&rows, "[c]").trim_end(),
        "[↑↓] pick   [c] call   [1-4] flip a pick   [x] take it back   [?] guide"
    );
    // The minutes a call waits are when there is time to read how it
    // goes: the bell and the rounds, then the round, with the ring's own
    // numbers in the example.
    let p = powers();
    assert!(holds(&rows, "how a bout goes"));
    assert!(holds(
        &rows,
        "  taking a call sets the bell 5 minutes out. the street bets until then."
    ));
    assert!(holds(
        &rows,
        &format!(
            "  a ‹block› holding {} meets a ‹strike› hitting {}: {} gets through.",
            p.block,
            p.strike,
            p.strike - p.block
        )
    ));
    assert!(rows.last().expect("a row").contains("past round 4"));
    let short = panel_short_by(&ring, ring.mira, &sheet(1, 6), now, 3);
    assert!(!holds(&short, "how a bout goes"));
    assert_eq!(short.last().map(String::as_str), Some(row(&rows, "[c]")));
    assert_eq!(short[..], rows[..short.len()], "nothing else gives");

    // Tom reads the same call as his to answer.
    let rows = panel(&ring, ring.tom, &sheet(1, 2), now);
    assert_eq!(row(&rows, "calls you"), "  mira calls you. 9:41 to answer.");
    assert!(holds(&rows, "[a] take the call   [x] not tonight"));
    assert!(!holds(&rows, "you owe the ring"));

    // Wren has no mark: the call to read, no deck, nobody to call, no keys.
    let rows = panel(&ring, ring.wren, &sheet(0, 6), now);
    assert_eq!(row(&rows, "mira calls"), "  mira calls tom. 9:41 left.");
    assert!(holds(
        &rows,
        "the ropes are for runners who put the Old Signal down"
    ));
    assert!(holds(&rows, "how a bout goes"), "a watcher bets on it");
    for marked_only in ["[1]", "[c]", "ash", "you owe the ring"] {
        assert!(
            !holds(&rows, marked_only),
            "{marked_only} is for the marked"
        );
    }
}

#[test]
fn a_screen_too_short_for_the_empty_ring_folds_the_deck_and_keeps_the_call() {
    let mut ring = ring();
    ring.board.calls = vec![call(ring.mira, ring.tom)];
    let now = at() + Duration::seconds(19);
    // Tom, called, owing a gate: the tallest the empty ring gets for him.
    let whole = panel(&ring, ring.tom, &sheet(1, 6), now);
    let explained = whole
        .iter()
        .rev()
        .position(|row| row == "how a bout goes")
        .expect("how a bout goes")
        + 2;
    let unexplained = &whole[..whole.len() - explained];

    // Short by the explanation or less: only the explanation goes.
    let short = panel_short_by(&ring, ring.tom, &sheet(1, 6), now, explained);
    assert_eq!(short[..], *unexplained);
    assert!(holds(&short, "level and kit stay outside."));

    // Still short with that gone: the four picks take one row and what
    // they do is left to the guide, so the call and the keys that answer
    // it are still on the screen.
    let folded = panel_short_by(&ring, ring.tom, &sheet(1, 6), now, explained + 1);
    assert_eq!(
        row(&folded, "[1]"),
        "  [1] ‹jab› [2] ‹bulwark› [3] ‹burn› [4] ‹sever›"
    );
    assert!(!holds(&folded, "level and kit stay outside."));
    assert_eq!(folded.len(), unexplained.len() - 5);
    assert_eq!(
        row(&folded, "calls you"),
        "  mira calls you. 9:41 to answer."
    );
    assert!(
        folded
            .last()
            .expect("a row")
            .contains("[a] take the call   [x] not tonight")
    );
    assert!(holds(&folded, "you owe the ring a bout at level 6"));
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
    // What each corner drafted and inked, the base ten left unsaid, on
    // the row under its name.
    assert_eq!(
        row(&rows, "mira").trim_end(),
        "  mira                 no bouts yet"
    );
    assert_eq!(
        row(&rows, "drafted").trim_end(),
        "    drafted  ‹jab› ‹mute›"
    );
    // The corner picked is the one the press names.
    assert_eq!(
        row(&rows, "[b]").trim_end(),
        format!("  [←→] corner: tom   [b] put {stake} bits on tom")
    );
    for fighters_only in ["comes out", "╭", "their hand", "[1-5]"] {
        assert!(!holds(&rows, fighters_only), "a watcher lays nothing");
    }
    assert!(holds(&rows, "how a round goes"));

    // A fighter's press goes on their own corner, and round 1 is theirs
    // to lay before the bell: the hand as cards, the other hand open.
    let rows = panel(&ring, ring.mira, &sheet(1, 4), now);
    assert_eq!(
        row(&rows, "[b]").trim_end(),
        format!("  [b] put {stake} bits on mira")
    );
    assert_eq!(
        row(&rows, "[1-5]").trim_end(),
        "  [1-5] lay a card, or take it back   [e] lock the round   [?] guide"
    );
    assert_eq!(row(&rows, "your bet"), "  your bet   nothing down");
    let toms = &ring.board.live.as_ref().expect("the bout").duel;
    let toms_first = toms.corner(Side::Called).piles.card(0).expect("a hand");
    assert!(row(&rows, "their hand").contains(&toms_first.marked()));
    // Her keys come before the book, which is where a short screen cuts
    // the panel.
    let at_row = |holding: &str| {
        rows.iter()
            .position(|row| row.contains(holding))
            .unwrap_or_else(|| panic!("no row holding {holding:?}"))
    };
    assert!(at_row("lock the round") < at_row("the pool"));
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

    // Mira: her card sits in her slot and says so on its own frame, her
    // hand carries its keys, and tom's laid card is still in his open
    // hand like the rest.
    let rows = panel(&ring, ring.mira, &sheet(1, 4), now);
    assert!(row(&rows, "round 1 of 4").ends_with("the clock 0:21"));
    assert!(
        row(&rows, "comes out").starts_with(&format!("  comes out  1 {}", mira_first.marked()))
    );
    assert!(row(&rows, "╭").starts_with(&format!("  ╭ 1 {} ", mira_first.name())));
    assert!(row(&rows, "╰").starts_with("  ╰ slot 1 "));
    assert!(row(&rows, "  tom  ").ends_with("locked"));
    assert!(row(&rows, "  mira (you)").ends_with("laying..."));
    let toms_hand = row(&rows, "their hand");
    assert!(toms_hand.contains(&tom_second.marked()));
    assert!(!toms_hand.contains("slot"), "tom's slots are his own");
    // Her keys come before the pool at the foot, which is where a short
    // screen cuts the panel.
    let at_row = |holding: &str| {
        rows.iter()
            .position(|row| row.contains(holding))
            .unwrap_or_else(|| panic!("no row holding {holding:?}"))
    };
    assert!(at_row("lock the round") < at_row("the pool"));

    // Tom locked: his keys give way to what the round waits for.
    let rows = panel(&ring, ring.tom, &sheet(1, 4), now);
    assert!(!holds(&rows, "[1-5]"));
    assert!(holds(&rows, "locked. it comes out when both corners are"));

    // A watcher gets both hands and nobody's slots.
    let rows = panel(&ring, ring.wren, &sheet(0, 4), now);
    for fighters_only in ["comes out", "╭", "╰ slot", "[1-5]"] {
        assert!(
            !holds(&rows, fighters_only),
            "{fighters_only} is a fighter's"
        );
    }
    let mira_hand = rows
        .iter()
        .find(|row| row.starts_with("  mira ") && row.contains('‹'))
        .expect("mira's open hand");
    assert!(mira_hand.contains(&mira_first.marked()));
    assert!(row(&rows, "  tom  ").ends_with("locked"));
}

#[test]
fn the_round_that_came_out_is_told_slot_by_slot() {
    let p = powers();
    let mut ring = set();
    let live = ring.board.live.as_mut().expect("the bout");
    let in_hand = |duel: &Duel, side: Side, card: Card| {
        (0..5u8)
            .find(|hand| duel.corner(side).piles.card(usize::from(*hand)) == Some(card))
            .unwrap_or_else(|| panic!("no {} in the hand", card.name()))
    };
    // Mira throws a strike into tom's block, then a siphon into nothing.
    let strike = in_hand(&live.duel, Side::Caller, Card::Strike);
    let siphon = in_hand(&live.duel, Side::Caller, Card::Siphon);
    let block = in_hand(&live.duel, Side::Called, Card::Block);
    live.duel
        .place(Side::Caller, 0, strike)
        .expect("the strike");
    live.duel
        .place(Side::Caller, 1, siphon)
        .expect("the siphon");
    live.duel.place(Side::Called, 0, block).expect("the block");
    live.duel.lock(Side::Caller).expect("mira locks");
    live.duel.lock(Side::Called).expect("tom locks");
    live.duel.resolve(&mut StdRng::seed_from_u64(7));
    let now = at() + Duration::seconds(BELL_SECS + 40);

    let rows = panel(&ring, ring.wren, &sheet(0, 4), now);
    assert_eq!(row(&rows, "came out"), "round 1 came out, slot by slot");
    assert_eq!(
        row(&rows, "signal").trim_end(),
        format!("     {:<31}{:<31}signal", "mira", "tom")
    );
    // What was thrown, what the block ate, and where both signals stood
    // once the slot was over.
    let through = p.strike - p.block;
    assert_eq!(
        tape_row(&rows, 1),
        format!(
            "  1  {:<31}{:<31}{SIGNAL} · {}",
            format!("‹strike› hits {}, {} blocked", p.strike, p.block),
            format!("‹block› holds {}", p.block),
            SIGNAL - through
        )
    );
    // A siphon at full signal has nothing to mend, and says nothing of it.
    assert_eq!(
        tape_row(&rows, 2),
        format!(
            "  2  {:<31}{:<31}{SIGNAL} · {}",
            format!("‹siphon› hits {}", p.strike),
            "nothing laid",
            SIGNAL - through - p.strike
        )
    );
    assert!(tape_row(&rows, 4).contains("nothing laid"));
    // The bars over the tape are the sum of its rows.
    assert!(row(&rows, "  tom  ").contains(&format!("{}/{SIGNAL}", SIGNAL - through - p.strike)));
}

#[test]
fn a_bout_closes_with_how_a_round_goes_unless_the_screen_is_short() {
    let ring = set();
    let now = at() + Duration::seconds(BELL_SECS + 9);
    for viewer in [ring.mira, ring.wren] {
        let rows = panel(&ring, viewer, &sheet(1, 4), now);
        assert!(holds(&rows, "how a round goes"));
        assert!(holds(&rows, "guards go up, then both hits land"));
        assert!(rows.last().expect("a row").contains("past round 4"));

        // Short of all of it, the explanation goes whole and nothing of
        // the round does.
        let short = panel_short_by(&ring, viewer, &sheet(1, 4), now, 2);
        assert!(!holds(&short, "how a round goes"));
        assert_eq!(short[..], rows[..short.len()]);
        assert_eq!(
            short.last().map(String::as_str),
            Some(row(&rows, "the pool"))
        );
    }
}

#[test]
fn no_row_is_wider_than_the_panel_whoever_fights() {
    // The widest a bout gets: two names at the limit, both decks with
    // every draft card in them, a pool in the hundreds of thousands.
    let long = |tag: &str| format!("{tag}{}", "x".repeat(32 - tag.len()));
    let every_draft: Vec<Card> = DRAFTS.iter().flat_map(|draft| draft.options).collect();
    let deck = [&DECK[..], &every_draft[..]].concat();
    let mut ring = set();
    let live = ring.board.live.as_mut().expect("the bout");
    live.bout.caller_username = long("mira");
    live.bout.called_username = long("tom");
    live.bout.caller_deck = Some(serde_json::json!(deck));
    live.bout.called_deck = Some(serde_json::json!(deck));
    live.bout.pool_caller = 987_654;
    live.bout.pool_called = 123_456;
    live.bets = vec![BoutBet {
        bout_id: live.bout.id,
        user_id: ring.wren,
        username: "wren".to_string(),
        side: Side::Called,
        bits: 123_456,
        presses: 3,
        placed_at: at(),
        paid_at: None,
        paid_bits: None,
    }];
    let widest = |rows: &[String]| {
        rows.iter()
            .max_by_key(|row| row.chars().count())
            .cloned()
            .unwrap_or_default()
    };
    let check = |ring: &Ring, phase: &str, now: DateTime<Utc>| {
        for viewer in [ring.mira, ring.tom, ring.wren] {
            let rows = panel(ring, viewer, &sheet(1, 12), now);
            let wide = widest(&rows);
            assert!(
                wide.chars().count() <= 94,
                "{phase}: a row of {} cells wraps in the panel: {wide}",
                wide.chars().count()
            );
            // The rows that are columns fit the panel on an 80 column
            // terminal, 74 cells inside: the hand's cards, the slots, a
            // hand as chips, the tape under its two names.
            let columns = [
                "  ╭",
                "  │",
                "  ╰",
                "  comes out",
                "  their hand",
                "     ",
                "  1  ",
                "  2  ",
                "  3  ",
                "  4  ",
            ];
            for column_row in rows.iter().filter(|row| {
                columns.iter().any(|start| row.starts_with(start))
                    || (row.contains('‹')
                        && (row.starts_with("  mira") || row.starts_with("  tom")))
            }) {
                assert!(
                    column_row.chars().count() <= 74,
                    "{phase}: a row of {} cells wraps at 80 columns: {column_row}",
                    column_row.chars().count()
                );
            }
            // How the game goes reads whole there too.
            let rules = rows
                .iter()
                .skip_while(|row| !row.starts_with("how a "))
                .max_by_key(|row| row.chars().count());
            assert!(
                rules.is_none_or(|row| row.chars().count() <= 74),
                "{phase}: {rules:?}"
            );
        }
    };
    check(&ring, "set", at() + Duration::seconds(48));
    check(&ring, "on", at() + Duration::seconds(BELL_SECS + 9));
    let mut now = at() + Duration::seconds(BELL_SECS + 40);
    while ring
        .board
        .live
        .as_ref()
        .expect("the bout")
        .duel
        .ending
        .is_none()
    {
        let live = ring.board.live.as_mut().expect("the bout");
        live.duel.call_time();
        live.duel.resolve(&mut StdRng::seed_from_u64(7));
        check(&ring, "a round out", now);
        now += Duration::seconds(36);
    }
    let live = ring.board.live.as_mut().expect("the bout");
    live.bout.ended_at = Some(now);
    check(&ring, "over", now + Duration::seconds(5));

    // The empty ring, with calls between the same two names.
    let mut empty = self::ring();
    let mut call = call(empty.mira, empty.tom);
    call.caller_username = long("mira");
    call.called_username = long("tom");
    empty.board.calls = vec![call];
    check(&empty, "empty", at() + Duration::seconds(19));
}

#[test]
fn the_street_hears_of_the_ring_only_what_concerns_the_runner() {
    let mut ring = ring();
    let now = at() + Duration::seconds(19);
    assert_eq!(
        noticed(&ring, ring.wren, now),
        None,
        "an empty ring is no news"
    );

    // A call is news to the two it names, and to nobody else.
    ring.board.calls = vec![call(ring.mira, ring.tom)];
    assert_eq!(
        noticed(&ring, ring.tom, now),
        Some((
            "mira calls you to the ring. 9:41 to answer.".to_string(),
            "answer"
        ))
    );
    assert_eq!(
        noticed(&ring, ring.mira, now),
        Some(("you called tom. 9:41 left.".to_string(), "step in"))
    );
    assert_eq!(noticed(&ring, ring.wren, now), None);

    // A bout set is everybody's: the book for the street, the bell for
    // the two in it.
    let mut ring = set();
    let now = at() + Duration::seconds(48);
    assert_eq!(
        noticed(&ring, ring.wren, now),
        Some((
            "mira against tom. the bell in 4:12, the book is open.".to_string(),
            "watch, and bet"
        ))
    );
    assert_eq!(
        noticed(&ring, ring.tom, now),
        Some((
            "your bout with mira. the bell in 4:12.".to_string(),
            "step in"
        ))
    );

    // The rounds on.
    let now = at() + Duration::seconds(BELL_SECS + 9);
    assert_eq!(
        noticed(&ring, ring.wren, now),
        Some(("mira against tom, round 1 of 4.".to_string(), "watch"))
    );
    assert_eq!(
        noticed(&ring, ring.mira, now),
        Some((
            "your bout is on. round 1 of 4, 0:21 on the clock.".to_string(),
            "step in"
        ))
    );

    // The result, while it is still on the board.
    let live = ring.board.live.as_mut().expect("the bout");
    while live.duel.ending.is_none() {
        live.duel.call_time();
        live.duel.resolve(&mut StdRng::seed_from_u64(7));
    }
    let ended = now + Duration::seconds(90);
    live.bout.ended_at = Some(ended);
    let (told, verb) = noticed(&ring, ring.wren, ended + Duration::seconds(5)).expect("the result");
    assert_eq!(verb, "see it");
    assert_eq!(
        told,
        panel(&ring, ring.wren, &sheet(0, 4), ended + Duration::seconds(5))[0]
    );
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
