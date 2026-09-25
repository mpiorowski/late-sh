use chrono::NaiveDate;
use late_core::models::deadchannel_runner::DeadchannelRunner;
use rand::{SeedableRng, rngs::StdRng};
use uuid::Uuid;

use super::{Applied, Command, Fight, News, Outcome, Quarry, Refusal, Sheet, SheetError, Slot};
use crate::app::deadchannel::fight::data::{
    FOES, HEARD_LINE, MARK_BONUS_CAP, MAX_LEVEL, OLD_SIGNAL, RATIONS_PER_DAY, START_BITS,
    exp_to_advance, exp_to_seek, title,
};
use crate::app::deadchannel::fight::state::MAX_TIER;

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, d).expect("a september day")
}

fn fresh() -> Sheet {
    Sheet::fresh(Uuid::nil(), day(24))
}

#[test]
fn stepping_in_spends_a_ration_and_meets_the_glyph_of_your_level() {
    let mut sheet = fresh();
    let mut rng = StdRng::seed_from_u64(1);

    let outcome = sheet.apply(Command::Start, &mut rng);

    assert_eq!(outcome.applied, Applied::Started);
    assert_eq!(outcome.lines, vec![FOES[0].arrives.to_string()]);
    assert_eq!(sheet.rations_left, RATIONS_PER_DAY - 1);
    let fight = sheet.fight.as_ref().expect("a fight on the row");
    assert_eq!(fight.foe().name, "flicker");
    assert_eq!(fight.foe_signal, fight.foe_max_signal);
    assert_eq!(fight.log, outcome.lines);

    // A second step in with a fight waiting spends nothing and resumes it.
    let again = sheet.apply(Command::Start, &mut rng);
    assert_eq!(again.applied, Applied::Resumed);
    assert_eq!(sheet.rations_left, RATIONS_PER_DAY - 1);
}

#[test]
fn a_fight_to_the_end_with_fixed_dice_lands_on_one_state() {
    let mut sheet = fresh();
    let mut rng = StdRng::seed_from_u64(7);
    sheet.apply(Command::Start, &mut rng);

    let mut rounds = 0;
    let ended = loop {
        rounds += 1;
        assert!(rounds < 200, "a fight always ends");
        let outcome = sheet.apply(Command::Attack, &mut rng);
        match outcome.applied {
            Applied::Round => continue,
            ended => break ended,
        }
    };

    // Whole state: the dice are fixed, so the sheet after the fight is
    // one exact thing. A change here is a rules change; read it.
    assert_eq!(sheet.fight, None, "the fight leaves the row when it ends");
    match ended {
        Applied::Won {
            foe,
            bits,
            exp,
            leveled,
        } => {
            assert_eq!((foe, bits, exp, leveled), ("flicker", 36, 14, None));
            assert_eq!(sheet.bits, START_BITS + 36);
            assert_eq!(sheet.exp, 14);
            assert!(sheet.signal > 0);
            assert_eq!(
                (sheet.kills, sheet.kills_today, sheet.runs_today),
                (1, 1, 0)
            );
        }
        Applied::Lost { bits_lost } => {
            assert_eq!(bits_lost, START_BITS);
            assert_eq!(sheet.bits, 0);
            assert_eq!(sheet.signal, 0);
            assert!(sheet.is_down());
            assert_eq!(
                (sheet.kills, sheet.kills_today, sheet.runs_today),
                (0, 0, 0)
            );
        }
        other => panic!("a fight ends won or lost, not {other:?}"),
    }
    assert_eq!(sheet.level, 1);
    assert_eq!(sheet.rations_left, RATIONS_PER_DAY - 1);
}

#[test]
fn winning_past_the_threshold_levels_up() {
    let mut sheet = fresh();
    sheet.exp = 90;
    // A foe that cannot hurt you and cannot survive you.
    sheet.weapon_tier = 50;
    sheet.fight = Some(Fight {
        quarry: Quarry::Glyph(0),
        foe_signal: 1,
        foe_max_signal: 1,
        foe_attack: 0,
        foe_defense: 0,
        foe_bits: 36,
        foe_exp: 14,
        log: Vec::new(),
    });
    let mut rng = StdRng::seed_from_u64(3);

    let mut rounds = 0;
    let outcome = loop {
        rounds += 1;
        assert!(rounds < 50);
        let outcome = sheet.apply(Command::Attack, &mut rng);
        if outcome.applied != Applied::Round {
            break outcome;
        }
    };

    assert_eq!(
        outcome.applied,
        Applied::Won {
            foe: "flicker",
            bits: 36,
            exp: 14,
            leveled: Some(2)
        }
    );
    assert_eq!(sheet.level, 2);
    assert_eq!(sheet.exp, 104);
    assert_eq!(sheet.max_signal(), 20);
    assert_eq!(sheet.signal, 20, "the new level's signal comes with it");
    assert!(
        outcome
            .lines
            .last()
            .expect("a line")
            .starts_with("you are level 2.")
    );
}

#[test]
fn running_gets_away_or_takes_a_free_strike() {
    let mut escaped = 0;
    let mut caught = 0;
    for seed in 0..30u64 {
        let mut sheet = fresh();
        let mut rng = StdRng::seed_from_u64(seed);
        sheet.apply(Command::Start, &mut rng);
        let outcome = sheet.apply(Command::Run, &mut rng);
        match outcome.applied {
            Applied::Escaped => {
                escaped += 1;
                assert_eq!(sheet.fight, None);
                assert_eq!(outcome.lines.len(), 1);
                assert_eq!(sheet.bits, START_BITS, "running earns nothing");
                assert_eq!(sheet.runs_today, 1);
            }
            Applied::Round => {
                caught += 1;
                assert!(sheet.fight.is_some(), "caught: the fight is still on");
                assert_eq!(outcome.lines.len(), 2, "the catch line and the strike");
            }
            Applied::Lost { .. } => {
                caught += 1;
                assert_eq!(sheet.fight, None);
            }
            other => panic!("a run escapes or is caught, not {other:?}"),
        }
        assert_eq!(
            sheet.rations_left,
            RATIONS_PER_DAY - 1,
            "the ration stays spent"
        );
    }
    assert!(
        escaped > 0 && caught > 0,
        "both outcomes happen: {escaped} / {caught}"
    );
}

#[test]
fn spent_or_down_is_refused_and_changes_nothing() {
    let mut rng = StdRng::seed_from_u64(1);

    let mut spent = fresh();
    spent.rations_left = 0;
    let before = spent.clone();
    let outcome = spent.apply(Command::Start, &mut rng);
    assert_eq!(outcome.applied, Applied::Refused(Refusal::NoRations));
    assert_eq!(spent, before);

    let mut down = fresh();
    down.signal = 0;
    let before = down.clone();
    let outcome = down.apply(Command::Start, &mut rng);
    assert_eq!(outcome.applied, Applied::Refused(Refusal::SignalDown));
    assert_eq!(down, before);

    let mut idle = fresh();
    let outcome = idle.apply(Command::Attack, &mut rng);
    assert_eq!(outcome.applied, Applied::Refused(Refusal::NoFight));
    assert_eq!(idle, fresh());
}

#[test]
fn the_day_roll_refills_everything_and_drops_a_hanging_fight() {
    let mut sheet = fresh();
    sheet.level = 3;
    sheet.signal = 0;
    sheet.rations_left = 0;
    sheet.kills = 12;
    sheet.kills_today = 7;
    sheet.runs_today = 2;
    sheet.fight = Some(Fight {
        quarry: Quarry::Glyph(2),
        foe_signal: 5,
        foe_max_signal: 32,
        foe_attack: 5,
        foe_defense: 4,
        foe_bits: 148,
        foe_exp: 34,
        log: vec!["it hits you for 9.".to_string()],
    });

    assert!(!sheet.settle(day(24)), "the same day rolls nothing");
    assert!(sheet.is_down());

    assert!(sheet.settle(day(25)));
    assert_eq!(sheet.day, day(25));
    assert_eq!(sheet.signal, 30);
    assert_eq!(sheet.rations_left, RATIONS_PER_DAY);
    assert_eq!(sheet.fight, None);
    assert_eq!(
        (sheet.kills_today, sheet.runs_today),
        (0, 0),
        "the day's tally rolls"
    );
    assert_eq!(sheet.kills, 12, "the lifetime count does not");
    assert!(!sheet.is_down());
    assert!(!sheet.settle(day(25)));
}

#[test]
fn the_wire_hears_only_the_results_worth_a_story() {
    let won = |leveled| Applied::Won {
        foe: "hiss",
        bits: 1,
        exp: 1,
        leveled,
    };

    // An ordinary kill mid-day: nothing.
    let mut sheet = fresh();
    sheet.kills = 4;
    sheet.rations_left = 5;
    assert_eq!(sheet.news(&won(None)), vec![]);
    assert_eq!(sheet.news(&Applied::Escaped), vec![]);
    assert_eq!(sheet.news(&Applied::Round), vec![]);

    // The first glyph ever.
    sheet.kills = 1;
    assert_eq!(
        sheet.news(&won(None)),
        vec![News::FirstBlood { foe: "hiss" }]
    );

    // A win on the last of the signal; a level outranks it.
    sheet.kills = 4;
    sheet.signal = 2;
    assert_eq!(
        sheet.news(&won(None)),
        vec![News::NearMiss {
            foe: "hiss",
            signal: 2
        }]
    );
    assert_eq!(sheet.news(&won(Some(2))), vec![News::Leveled { level: 2 }]);

    // The last ration's fight, won or escaped, closes the day with the
    // card, after whatever story the win itself was.
    sheet.rations_left = 0;
    sheet.kills_today = 7;
    sheet.runs_today = 2;
    let card = News::LastRation {
        kills: 7,
        runs: 2,
        signal: 2,
        max_signal: 10,
    };
    assert_eq!(
        sheet.news(&won(None)),
        vec![
            News::NearMiss {
                foe: "hiss",
                signal: 2
            },
            card.clone()
        ]
    );
    assert_eq!(sheet.news(&Applied::Escaped), vec![card]);

    // A dropped signal is its own line and the day's last word.
    sheet.signal = 0;
    assert_eq!(
        sheet.news(&Applied::Lost { bits_lost: 30 }),
        vec![News::Dropped { bits_lost: 30 }]
    );
}

#[test]
fn the_armorer_trades_up_and_refuses_down_or_short() {
    let mut rng = StdRng::seed_from_u64(1);
    let mut sheet = fresh();
    sheet.bits = 1000;

    // Tier 2 off bare hands: the full price on the wall.
    let bought = sheet.apply(
        Command::Outfit {
            slot: Slot::Weapon,
            tier: 2,
        },
        &mut rng,
    );
    assert_eq!(
        bought.applied,
        Applied::Outfitted {
            slot: Slot::Weapon,
            tier: 2,
            paid: 225
        }
    );
    assert_eq!(
        bought.lines,
        vec!["the armorer hands over the box cutter. 225 bits.".to_string()]
    );

    // Tier 3 with the box cutter handed back: 585 less 75% of 225.
    let traded = sheet.apply(
        Command::Outfit {
            slot: Slot::Weapon,
            tier: 3,
        },
        &mut rng,
    );
    assert_eq!(
        traded.applied,
        Applied::Outfitted {
            slot: Slot::Weapon,
            tier: 3,
            paid: 417
        }
    );
    assert_eq!(
        traded.lines,
        vec![
            "the armorer hands over the tire iron. the box cutter goes back on the wall. 417 bits."
                .to_string()
        ]
    );
    let mut expected = fresh();
    expected.bits = 1000 - 225 - 417;
    expected.weapon_tier = 3;
    assert_eq!(sheet, expected, "the whole sheet after two trades");
    assert_eq!(sheet.attack(), 4);
    assert_eq!(sheet.gear_name(Slot::Weapon), Some("tire iron"));
    assert_eq!(sheet.gear_name(Slot::Armor), None);

    // Down, sideways, or off the wall: nothing moves.
    for tier in [3, 1, 0, 16] {
        let refused = sheet.apply(
            Command::Outfit {
                slot: Slot::Weapon,
                tier,
            },
            &mut rng,
        );
        assert_eq!(
            refused.applied,
            Applied::Refused(Refusal::NotAnUpgrade),
            "tier {tier}"
        );
        assert_eq!(sheet, expected);
    }

    // Short: the refusal names the gap, and the trade-in counts toward it.
    let short = sheet.apply(
        Command::Outfit {
            slot: Slot::Weapon,
            tier: 4,
        },
        &mut rng,
    );
    // 990 less 75% of 585 (438) is 552; 358 on hand.
    assert_eq!(short.applied, Applied::Refused(Refusal::Short { by: 194 }));
    assert_eq!(
        short.lines,
        vec!["you are 194 bits short of the rebar club.".to_string()]
    );
    assert_eq!(sheet, expected);

    // The other slot has its own ladder.
    let armor = sheet.apply(
        Command::Outfit {
            slot: Slot::Armor,
            tier: 1,
        },
        &mut rng,
    );
    assert_eq!(
        armor.applied,
        Applied::Outfitted {
            slot: Slot::Armor,
            tier: 1,
            paid: 48
        }
    );
    assert_eq!(sheet.armor_tier, 1);
    assert_eq!(sheet.defense(), 2);
    assert_eq!(sheet.bits, 358 - 48);
}

/// Patch sells the whole gap at a bit a point times the level, and turns
/// away a dropped signal (the roll's business), a fight left waiting on
/// the row, a full signal, and a short purse, changing nothing each time.
#[test]
fn patch_buys_the_signal_back_and_refuses_down_waiting_full_or_short() {
    let mut rng = StdRng::seed_from_u64(1);

    let mut sheet = fresh();
    sheet.level = 3;
    sheet.signal = 12;
    sheet.bits = 100;
    assert_eq!(sheet.patch_price(), 54);
    let patched = sheet.apply(Command::Patch, &mut rng);
    assert_eq!(
        patched.applied,
        Applied::Patched {
            restored: 18,
            paid: 54
        }
    );
    assert_eq!(
        patched.lines,
        vec!["patch works fast. +18 signal, back to full. 54 bits.".to_string()]
    );
    assert_eq!((sheet.signal, sheet.bits), (30, 46));
    assert!(sheet.news(&patched.applied).is_empty());

    let full = sheet.apply(Command::Patch, &mut rng);
    assert_eq!(full.applied, Applied::Refused(Refusal::NothingToPatch));
    assert_eq!((sheet.signal, sheet.bits), (30, 46));

    let mut down = fresh();
    down.signal = 0;
    let before = down.clone();
    let outcome = down.apply(Command::Patch, &mut rng);
    assert_eq!(outcome.applied, Applied::Refused(Refusal::SignalDown));
    assert_eq!(down, before);

    let mut waiting = fresh();
    waiting.signal = 4;
    waiting.apply(Command::Start, &mut rng);
    assert!(waiting.fight.is_some());
    let before = waiting.clone();
    let outcome = waiting.apply(Command::Patch, &mut rng);
    assert_eq!(outcome.applied, Applied::Refused(Refusal::FightWaiting));
    assert_eq!(waiting, before);

    let mut short = fresh();
    short.signal = 1;
    short.bits = 5;
    let before = short.clone();
    let outcome = short.apply(Command::Patch, &mut rng);
    assert_eq!(outcome.applied, Applied::Refused(Refusal::Short { by: 4 }));
    assert_eq!(
        outcome.lines,
        vec!["you are 4 bits short of a patch.".to_string()]
    );
    assert_eq!(short, before);

    // Spent for the day with no glyph waiting: nothing can spend the
    // signal before the roll refills it for free, so patch sells nothing.
    let mut spent = fresh();
    spent.signal = 4;
    spent.rations_left = 0;
    let before = spent.clone();
    let outcome = spent.apply(Command::Patch, &mut rng);
    assert_eq!(outcome.applied, Applied::Refused(Refusal::NoRations));
    assert_eq!(
        outcome.lines,
        vec!["you are spent for today. the roll brings the signal back for nothing.".to_string()]
    );
    assert_eq!(spent, before);
}

#[test]
fn a_carried_weapon_is_named_in_the_hit_line() {
    let mut sheet = fresh();
    sheet.weapon_tier = 3;
    sheet.fight = Some(Fight {
        quarry: Quarry::Glyph(0),
        foe_signal: 1,
        foe_max_signal: 1,
        foe_attack: 0,
        foe_defense: 0,
        foe_bits: 36,
        foe_exp: 14,
        log: Vec::new(),
    });
    let mut rng = StdRng::seed_from_u64(3);
    let outcome = sheet.apply(Command::Attack, &mut rng);
    assert!(
        outcome.lines[0].starts_with("your tire iron hits the flicker for "),
        "{:?}",
        outcome.lines
    );
}

#[test]
fn a_row_naming_an_unknown_glyph_is_rejected() {
    let now = chrono::Utc::now();
    let row = DeadchannelRunner {
        id: Uuid::nil(),
        created: now,
        updated: now,
        left_at: None,
        guide_seen_at: None,
        level: 1,
        exp: 0,
        signal: 10,
        weapon_tier: 0,
        armor_tier: 0,
        bits: 50,
        rations_left: 9,
        day: day(24),
        kills: 0,
        kills_today: 0,
        runs_today: 0,
        peak_level: 1,
        marks: 0,
        fight: Some(serde_json::json!({
            "quarry": {"glyph": 99}, "foe_signal": 1, "foe_max_signal": 1, "foe_attack": 1,
            "foe_defense": 1, "foe_bits": 1, "foe_exp": 1, "log": []
        })),
        user_id: Uuid::nil(),
        look: serde_json::json!({}),
    };
    assert_eq!(
        Sheet::from_row(&row),
        Err(SheetError::Fight("unknown glyph kind 99".to_string()))
    );

    let ok = DeadchannelRunner { fight: None, ..row };
    let sheet = Sheet::from_row(&ok).expect("a sheet");
    assert_eq!(sheet.to_write().fight, None);
    assert_eq!(sheet.rations_left, 9);
}

#[test]
fn every_glyph_wears_a_five_by_three_face() {
    for foe in FOES.iter() {
        assert_eq!(foe.portrait.len(), 3, "{}", foe.name);
        for row in foe.portrait {
            assert_eq!(row.chars().count(), 5, "{}: {row:?}", foe.name);
            for ch in row.chars() {
                assert!(
                    unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0) <= 1,
                    "{}: wide glyph {ch:?}",
                    foe.name
                );
            }
        }
    }
}

/// A level-15 runner at the top of the armorer's wall, full signal, with
/// the exp that makes the Old Signal hear them.
fn at_the_top(marks: i32) -> Sheet {
    let mut sheet = fresh();
    sheet.level = MAX_LEVEL;
    sheet.peak_level = MAX_LEVEL;
    sheet.marks = marks;
    sheet.exp = exp_to_seek(marks);
    sheet.weapon_tier = MAX_TIER;
    sheet.armor_tier = MAX_TIER;
    sheet.signal = sheet.max_signal();
    sheet.bits = 1234;
    sheet
}

/// Attack until the fight ends; the ending.
fn fight_out(sheet: &mut Sheet, rng: &mut StdRng) -> Applied {
    for _ in 0..500 {
        match sheet.apply(Command::Attack, rng).applied {
            Applied::Round => continue,
            ended => return ended,
        }
    }
    panic!("a fight always ends");
}

/// The balance target, through the real rules: a runner at the top of the
/// wall with full signal who only attacks puts the Old Signal down about
/// two times in five on the first try, and about four in five with the
/// mark bonus at its cap. A change here is a balance change; read it.
#[test]
fn the_old_signal_is_a_real_fight_that_marks_make_easier() {
    let win_rate = |marks: i32| {
        let mut wins = 0;
        for seed in 0..2000 {
            let mut sheet = at_the_top(marks);
            let mut rng = StdRng::seed_from_u64(seed);
            sheet.apply(Command::Start, &mut rng);
            if matches!(fight_out(&mut sheet, &mut rng), Applied::Slain { .. }) {
                wins += 1;
            }
        }
        wins * 100 / 2000
    };
    let first = win_rate(0);
    let capped = win_rate(MARK_BONUS_CAP);
    assert!((33..=45).contains(&first), "first kill wins {first}%");
    assert!((72..=86).contains(&capped), "capped marks win {capped}%");
    assert_eq!(
        win_rate(MARK_BONUS_CAP + 3),
        capped,
        "the bonus stops at the cap"
    );
}

#[test]
fn the_old_signal_answers_only_at_the_top_with_the_exp_to_leave_it() {
    let mut rng = StdRng::seed_from_u64(1);

    let mut short = at_the_top(0);
    short.exp -= 1;
    assert!(!short.signal_hears());
    short.apply(Command::Start, &mut rng);
    assert_eq!(
        short.fight.as_ref().map(|fight| fight.quarry),
        Some(Quarry::Glyph(14))
    );

    let mut ready = at_the_top(0);
    let outcome = ready.apply(Command::Start, &mut rng);
    let fight = ready.fight.as_ref().expect("a fight on the row");
    assert_eq!(fight.quarry, Quarry::OldSignal);
    assert_eq!(fight.foe().name, "Old Signal");
    assert_eq!(outcome.lines, vec![OLD_SIGNAL.arrives.to_string()]);

    // Marks scale the exp it takes: last time's threshold is not enough.
    let mut again = at_the_top(1);
    again.exp = exp_to_seek(0);
    assert!(!again.signal_hears());
}

/// Whole state: the kill leaves a mark and a fresh runner's sheet, and
/// keeps the peak, the kill count, and today's rations.
#[test]
fn putting_the_old_signal_down_leaves_a_mark_and_starts_the_climb_over() {
    let mut sheet = at_the_top(0);
    let mut rng = StdRng::seed_from_u64(1);
    sheet.apply(Command::Start, &mut rng);
    sheet.fight.as_mut().expect("a fight").foe_signal = 1;
    sheet.kills = 900;
    sheet.kills_today = 4;

    let outcome = sheet.apply(Command::Attack, &mut rng);

    assert_eq!(outcome.applied, Applied::Slain { marks: 1 });
    let expected = Sheet {
        user_id: Uuid::nil(),
        level: 1,
        exp: 0,
        signal: 10,
        weapon_tier: 0,
        armor_tier: 0,
        bits: START_BITS,
        rations_left: RATIONS_PER_DAY - 1,
        day: day(24),
        fight: None,
        kills: 901,
        kills_today: 5,
        runs_today: 0,
        peak_level: MAX_LEVEL,
        marks: 1,
    };
    assert_eq!(sheet, expected);
    assert_eq!(outcome.lines.len(), 3, "{:?}", outcome.lines);
    assert!(outcome.lines[2].contains("mark 1"), "{:?}", outcome.lines);
    assert_eq!(sheet.news(&outcome.applied), vec![News::Slain { marks: 1 }]);
    assert_eq!(
        (sheet.attack(), sheet.defense()),
        (2, 2),
        "level 1 plus the mark"
    );
}

#[test]
fn marks_scale_the_ladder_and_the_peak_only_climbs() {
    assert_eq!(exp_to_advance(1, 0), Some(100));
    assert_eq!(exp_to_advance(1, 4), Some(200));
    assert_eq!(exp_to_advance(MAX_LEVEL, 4), None);
    assert_eq!(exp_to_seek(4), 43930 + 1500);
    assert_eq!(title(0), None);
    assert_eq!(title(1), Some("heard"));
    assert_eq!(title(99), Some("old voice"));

    let mut sheet = fresh();
    sheet.peak_level = 9;
    sheet.marks = 1;
    sheet.exp = exp_to_advance(1, 1).expect("a threshold") - 1;
    sheet.apply(Command::Start, &mut StdRng::seed_from_u64(2));
    sheet.fight.as_mut().expect("a fight").foe_signal = 1;
    let mut rng = StdRng::seed_from_u64(2);
    let outcome = loop {
        let outcome = sheet.apply(Command::Attack, &mut rng);
        if outcome.applied != Applied::Round {
            break outcome;
        }
    };
    assert!(
        matches!(
            outcome.applied,
            Applied::Won {
                leveled: Some(2),
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(sheet.peak_level, 9, "a climb under the peak leaves it");
}

/// A glyph at the top that cannot hurt you and cannot survive you, worth
/// one exp: the step that crosses the seek threshold, and no more.
fn harmless_glyph_at_the_top() -> Fight {
    Fight {
        quarry: Quarry::Glyph(14),
        foe_signal: 1,
        foe_max_signal: 1,
        foe_attack: 0,
        foe_defense: 0,
        foe_bits: 0,
        foe_exp: 1,
        log: Vec::new(),
    }
}

/// The glyph kill that lifts a level-15 runner over the seek threshold
/// says so, once: the kill that crosses it prints the heard line, the
/// next glyph kill past it does not.
#[test]
fn the_kill_that_crosses_the_seek_threshold_says_so_once() {
    let mut sheet = at_the_top(0);
    sheet.exp = exp_to_seek(0) - 1;
    sheet.weapon_tier = 50;
    assert!(!sheet.signal_hears());
    let mut rng = StdRng::seed_from_u64(4);

    sheet.fight = Some(harmless_glyph_at_the_top());
    let crossing = win_out(&mut sheet, &mut rng);
    assert!(
        matches!(crossing.applied, Applied::Won { leveled: None, .. }),
        "{crossing:?}"
    );
    assert_eq!(sheet.exp, exp_to_seek(0));
    assert!(sheet.signal_hears());
    assert_eq!(
        crossing
            .lines
            .iter()
            .filter(|line| *line == HEARD_LINE)
            .count(),
        1,
        "{:?}",
        crossing.lines
    );

    sheet.fight = Some(harmless_glyph_at_the_top());
    let past = win_out(&mut sheet, &mut rng);
    assert!(matches!(past.applied, Applied::Won { .. }), "{past:?}");
    assert!(
        sheet.signal_hears(),
        "still heard, one exp past the threshold"
    );
    assert!(
        !past.lines.iter().any(|line| *line == HEARD_LINE),
        "{:?}",
        past.lines
    );
}

/// Attack until the fight ends; the ending outcome, lines and all.
fn win_out(sheet: &mut Sheet, rng: &mut StdRng) -> Outcome {
    for _ in 0..500 {
        let outcome = sheet.apply(Command::Attack, rng);
        if outcome.applied != Applied::Round {
            return outcome;
        }
    }
    panic!("a fight always ends");
}
