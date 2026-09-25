use chrono::NaiveDate;
use late_core::models::deadchannel_runner::DeadchannelRunner;
use rand::{SeedableRng, rngs::StdRng};
use uuid::Uuid;

use super::{Applied, Command, Fight, News, Refusal, Sheet, SheetError, Slot};
use crate::app::deadchannel::fight::data::{FOES, RATIONS_PER_DAY, START_BITS};

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
        kind: 0,
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
        kind: 2,
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
}

#[test]
fn a_carried_weapon_is_named_in_the_hit_line() {
    let mut sheet = fresh();
    sheet.weapon_tier = 3;
    sheet.fight = Some(Fight {
        kind: 0,
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
        fight: Some(serde_json::json!({
            "kind": 99, "foe_signal": 1, "foe_max_signal": 1, "foe_attack": 1,
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
