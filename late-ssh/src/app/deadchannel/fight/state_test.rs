use chrono::NaiveDate;
use late_core::models::deadchannel_runner::DeadchannelRunner;
use rand::{SeedableRng, rngs::StdRng};
use uuid::Uuid;

use super::{
    Applied, Call, Command, Drink, Fight, News, Outcome, Pick, Quarry, Refusal, Sheet, SheetError,
    Slot,
};
use crate::app::deadchannel::fight::cards::{Card, DECK, DRAFTS, ENERGY, HAND, STATIC_CAP};
use crate::app::deadchannel::fight::data::{
    BRIGHT_LINE, CRYSTAL_LINE, DRAFT_LINE, FOES, FoeTier, HEARD_LINE, Intent, MARK_BONUS_CAP, MAX_LEVEL,
    NOISE_CARDS, OLD_SIGNAL, RATIONS_PER_DAY, RULES, START_BITS, STATIC_LINE, exp_to_advance,
    exp_to_seek, title,
};
use crate::app::deadchannel::fight::road::{Mark, Node, RoadRun, Trace};
use crate::app::deadchannel::fight::state::MAX_TIER;

fn day(d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, d).expect("a september day")
}

fn fresh() -> Sheet {
    Sheet::fresh(Uuid::nil(), day(24))
}

/// What the fixed-dice fight leaves of a fresh runner's ten signal, and
/// the static it leaves in the deck
/// (`a_fight_to_the_end_with_fixed_dice_lands_on_one_state`).
const FIXED_FIGHT_SIGNAL: i32 = 7;
const FIXED_FIGHT_STATIC: u8 = 2;

/// The first step of every day's road: a glyph in every lane.
const STEP_IN: Command = Command::Step {
    lane: 1,
    call: Call::Fight(Pick::Fair),
};

/// A fight to put on a row by hand: the glyph `kind` with these numbers,
/// no defense, and a clean deck dealt on fixed dice.
fn a_fight(kind: usize, signal: i32, attack: u32, bits: i64, exp: i64) -> Fight {
    Fight::new(
        Quarry::Glyph(kind),
        FoeTier {
            signal,
            attack,
            defense: 0,
            bits,
            exp,
        },
        DECK.to_vec(),
        0,
        &mut StdRng::seed_from_u64(99),
    )
}

/// The hand, dealt by the test instead of the dice.
fn deal(sheet: &mut Sheet, hand: [Card; HAND]) {
    let fight = sheet.fight.as_mut().expect("a fight to deal into");
    fight.piles.hand = hand.into_iter().map(Some).collect();
}

fn the_fight(sheet: &Sheet) -> &Fight {
    sheet.fight.as_ref().expect("a fight on the row")
}

#[test]
fn stepping_in_spends_a_ration_and_meets_the_glyph_of_your_level() {
    let mut sheet = fresh();
    let mut rng = StdRng::seed_from_u64(1);

    let outcome = sheet.apply(STEP_IN, &mut rng);

    assert_eq!(outcome.applied, Applied::Started { pick: Pick::Fair });
    assert_eq!(outcome.lines, vec![FOES[0].arrives.to_string()]);
    assert_eq!(sheet.rations_left, RATIONS_PER_DAY - 1);
    assert_eq!(
        sheet.road.path,
        vec![Trace {
            lane: 1,
            mark: Mark::Fighting
        }]
    );
    let fight = the_fight(&sheet);
    assert_eq!(fight.foe().name, "flicker");
    assert_eq!(fight.foe_signal, fight.foe_max_signal);
    assert_eq!(fight.log, outcome.lines);
    // The deal: five of the ten in hand, five to draw, full energy, and
    // the glyph's first move already on show.
    assert_eq!(
        (
            fight.piles.hand.iter().flatten().count(),
            fight.piles.draw.len(),
            fight.piles.discard.len(),
            fight.energy,
            fight.block,
            fight.turn,
        ),
        (HAND, 5, 0, ENERGY, 0, 0)
    );
    assert_eq!(fight.intent(), Intent::Hit);

    // A second step with a fight waiting spends nothing and resumes it,
    // whatever it asked for.
    let again = sheet.apply(
        Command::Step {
            lane: 0,
            call: Call::Take,
        },
        &mut rng,
    );
    assert_eq!(again.applied, Applied::Resumed);
    assert_eq!(sheet.rations_left, RATIONS_PER_DAY - 1);
    assert_eq!(sheet.road.path.len(), 1);
    let before = sheet.clone();
    assert_eq!(
        sheet.apply(Command::Resume, &mut rng).applied,
        Applied::Resumed
    );
    assert_eq!(sheet, before);
}

#[test]
fn a_fight_to_the_end_with_fixed_dice_lands_on_one_state() {
    let mut sheet = fresh();
    let mut rng = StdRng::seed_from_u64(7);
    sheet.apply(STEP_IN, &mut rng);

    let ended = fight_out(&mut sheet, &mut rng);

    // Whole state: the dice are fixed, so the sheet after the fight is
    // one exact thing. A change here is a rules change; read it.
    let (_, _, flicker) = RULES.foe(1);
    assert_eq!(
        ended,
        Applied::Won {
            foe: "flicker",
            bits: flicker.bits,
            garnished: 0,
            exp: flicker.exp,
            leveled: None,
            bright: false,
            crystals: 0,
        }
    );
    assert_eq!(
        sheet,
        Sheet {
            exp: flicker.exp,
            signal: FIXED_FIGHT_SIGNAL,
            bits: START_BITS + flicker.bits,
            rations_left: RATIONS_PER_DAY - 1,
            kills: 1,
            kills_today: 1,
            road: RoadRun {
                path: vec![Trace {
                    lane: 1,
                    mark: Mark::Won
                }],
                static_cards: FIXED_FIGHT_STATIC,
            },
            ..fresh()
        }
    );
}

#[test]
fn winning_past_the_threshold_levels_up() {
    let mut sheet = fresh();
    sheet.exp = 90;
    // A foe that cannot survive you.
    sheet.weapon_tier = 50;
    sheet.fight = Some(a_fight(0, 1, 0, 36, 14));
    let mut rng = StdRng::seed_from_u64(3);

    let outcome = win_out(&mut sheet, &mut rng);

    assert_eq!(
        outcome.applied,
        Applied::Won {
            foe: "flicker",
            bits: 36,
            garnished: 0,
            exp: 14,
            leveled: Some(2),
            bright: false,
            crystals: sheet.crystals,
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

/// No dice in a run: what the glyph meant to do this turn lands on the
/// way out, and then you are out. The step stays spent and pays nothing.
#[test]
fn running_takes_what_the_glyph_meant_to_do_and_gets_away() {
    let mut rng = StdRng::seed_from_u64(1);

    // The flicker only ever hits: the run costs its hit.
    let mut sheet = fresh();
    sheet.apply(STEP_IN, &mut rng);
    let hit = sheet.powers(the_fight(&sheet)).hit;
    let outcome = sheet.apply(Command::Run, &mut rng);
    assert_eq!(outcome.applied, Applied::Escaped);
    assert_eq!(sheet.fight, None);
    assert_eq!(sheet.signal, 10 - hit);
    assert_eq!(sheet.bits, START_BITS, "running earns nothing");
    assert_eq!(sheet.runs_today, 1);
    assert_eq!(sheet.rations_left, RATIONS_PER_DAY - 1);
    assert_eq!(
        sheet.road,
        RoadRun {
            path: vec![Trace {
                lane: 1,
                mark: Mark::Ran
            }],
            static_cards: 1,
        },
        "the hit that landed left its static in the deck"
    );

    // Block standing takes the hit on the way out.
    let mut guarded = fresh();
    guarded.apply(STEP_IN, &mut rng);
    guarded.fight.as_mut().expect("a fight").block = 50;
    guarded.apply(Command::Run, &mut rng);
    assert_eq!((guarded.signal, guarded.road.static_cards), (10, 0));

    // The drift hits, gathers, comes down: running while it gathers is
    // free, and the line is the getaway alone.
    let mut free = fresh();
    free.fight = Some(a_fight(2, 30, 500, 0, 0));
    free.fight.as_mut().expect("a fight").turn = 1;
    assert_eq!(the_fight(&free).intent(), Intent::Charge);
    let outcome = free.apply(Command::Run, &mut rng);
    assert_eq!(outcome.applied, Applied::Escaped);
    assert_eq!(outcome.lines.len(), 1);
    assert_eq!(free.signal, 10);

    // And running into the heavy can be the end of the day.
    let mut caught = fresh();
    caught.bits = 30;
    caught.fight = Some(a_fight(2, 30, 500, 0, 0));
    caught.fight.as_mut().expect("a fight").turn = 2;
    assert_eq!(the_fight(&caught).intent(), Intent::Heavy);
    let outcome = caught.apply(Command::Run, &mut rng);
    assert_eq!(outcome.applied, Applied::Lost { bits_lost: 30 });
    assert!(caught.is_down());
}

#[test]
fn spent_or_down_is_refused_and_changes_nothing() {
    let mut rng = StdRng::seed_from_u64(1);

    let mut spent = fresh();
    spent.rations_left = 0;
    let before = spent.clone();
    let outcome = spent.apply(STEP_IN, &mut rng);
    assert_eq!(outcome.applied, Applied::Refused(Refusal::NoRations));
    assert_eq!(spent, before);

    let mut down = fresh();
    down.signal = 0;
    let before = down.clone();
    let outcome = down.apply(STEP_IN, &mut rng);
    assert_eq!(outcome.applied, Applied::Refused(Refusal::SignalDown));
    assert_eq!(down, before);

    // No fight on the row: nothing to resume, no card, no turn, no auto,
    // no run.
    for command in [
        Command::Resume,
        Command::Play { slot: 0 },
        Command::EndTurn,
        Command::Auto,
        Command::Run,
    ] {
        let mut idle = fresh();
        let outcome = idle.apply(command, &mut rng);
        assert_eq!(outcome.applied, Applied::Refused(Refusal::NoFight));
        assert_eq!(idle, fresh());
    }
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
    sheet.fight = Some(a_fight(2, 32, 5, 148, 34));
    sheet.road = RoadRun {
        path: vec![Trace {
            lane: 2,
            mark: Mark::Fighting,
        }],
        static_cards: 3,
    };

    assert!(!sheet.settle(day(24)), "the same day rolls nothing");
    assert!(sheet.is_down());

    assert!(sheet.settle(day(25)));
    assert_eq!(sheet.day, day(25));
    assert_eq!(sheet.signal, 30);
    assert_eq!(sheet.rations_left, RATIONS_PER_DAY);
    assert_eq!(sheet.fight, None);
    assert_eq!(
        sheet.road,
        RoadRun::default(),
        "a new day, a new road, a clean deck"
    );
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
    let won_as = |leveled, bright| Applied::Won {
        foe: "hiss",
        bits: 1,
        garnished: 0,
        exp: 1,
        leveled,
        bright,
        crystals: i32::from(bright),
    };
    let won = |leveled| won_as(leveled, false);

    // An ordinary kill mid-day: nothing.
    let mut sheet = fresh();
    sheet.kills = 4;
    sheet.rations_left = 5;
    assert_eq!(sheet.news(&won(None)), vec![]);
    assert_eq!(sheet.news(&Applied::Escaped), vec![]);
    assert_eq!(sheet.news(&Applied::Round), vec![]);
    assert_eq!(sheet.news(&Applied::Played { card: Card::Strike }), vec![]);
    assert_eq!(sheet.news(&Applied::Mended { restored: 3 }), vec![]);

    // The first glyph ever.
    sheet.kills = 1;
    assert_eq!(
        sheet.news(&won(None)),
        vec![News::FirstBlood { foe: "hiss" }]
    );

    // A bright glyph put down outranks a near miss; a level outranks it.
    sheet.kills = 4;
    sheet.signal = 2;
    assert_eq!(
        sheet.news(&won_as(None, true)),
        vec![News::BrightDown { foe: "hiss" }]
    );
    assert_eq!(
        sheet.news(&won_as(Some(2), true)),
        vec![News::Leveled { level: 2 }]
    );

    // A win on the last of the signal; a level outranks it.
    assert_eq!(
        sheet.news(&won(None)),
        vec![News::NearMiss {
            foe: "hiss",
            signal: 2
        }]
    );
    assert_eq!(sheet.news(&won(Some(2))), vec![News::Leveled { level: 2 }]);

    // The road's last step, a fight won or escaped or a quiet node taken,
    // closes the day with the card, after whatever story the win was.
    sheet.rations_left = 0;
    sheet.kills_today = 7;
    sheet.runs_today = 2;
    let card = News::RoadWalked {
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
    assert_eq!(sheet.news(&Applied::Escaped), vec![card.clone()]);
    assert_eq!(
        sheet.news(&Applied::Cached {
            bits: 9,
            garnished: 0
        }),
        vec![card]
    );

    // A dropped signal is its own line and the day's last word.
    sheet.signal = 0;
    assert_eq!(
        sheet.news(&Applied::Lost { bits_lost: 30 }),
        vec![News::Dropped {
            bits_lost: 30,
            step: 10
        }]
    );
}

#[test]
fn the_armorer_trades_up_and_refuses_down_or_short() {
    let mut rng = StdRng::seed_from_u64(1);
    let at_four = || {
        let mut sheet = fresh();
        sheet.level = 4;
        sheet.signal = 40;
        sheet
    };
    let mut sheet = at_four();
    sheet.bits = 2000;

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
            paid: 506
        }
    );
    assert_eq!(
        bought.lines,
        vec!["the armorer hands over the box cutter. 506 bits.".to_string()]
    );

    // Tier 3 with the box cutter handed back: 1316 less 75% of 506 (379).
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
            paid: 937
        }
    );
    assert_eq!(
        traded.lines,
        vec![
            "the armorer hands over the tire iron. the box cutter goes back on the wall. 937 bits."
                .to_string()
        ]
    );
    let mut expected = at_four();
    expected.bits = 2000 - 506 - 937;
    expected.weapon_tier = 3;
    assert_eq!(sheet, expected, "the whole sheet after two trades");
    assert_eq!(sheet.attack(), 7);
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
    // 2227 less 75% of 1316 (987) is 1240; 557 on hand.
    assert_eq!(short.applied, Applied::Refused(Refusal::Short { by: 683 }));
    assert_eq!(
        short.lines,
        vec!["you are 683 bits short of the rebar club.".to_string()]
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
            paid: 108
        }
    );
    assert_eq!(sheet.armor_tier, 1);
    assert_eq!(sheet.defense(), 5);
    assert_eq!(sheet.bits, 557 - 108);
}

/// A sheet standing in front of the first bright node of its day's road,
/// and the lane that node is on. Nothing is walked yet, so every lane is
/// in reach.
fn before_a_bright_node() -> (Sheet, u8) {
    let mut sheet = fresh();
    let road = sheet.todays_road();
    let (step, lane) = road
        .steps
        .iter()
        .enumerate()
        .find_map(|(step, lanes)| {
            lanes
                .iter()
                .position(|node| *node == Node::Bright)
                .map(|lane| (step, lane))
        })
        .expect("two bright nodes on every road");
    sheet.rations_left = RATIONS_PER_DAY - step as i32;
    (sheet, lane as u8)
}

#[test]
fn the_bright_glyph_is_harder_pays_double_bits_and_leaves_a_crystal() {
    let mut rng = StdRng::seed_from_u64(1);

    // A plain glyph's node does not answer to it, and the ration is kept.
    let mut plain = fresh();
    let before = plain.clone();
    let refused = plain.apply(
        Command::Step {
            lane: 1,
            call: Call::Fight(Pick::Bright),
        },
        &mut rng,
    );
    assert_eq!(refused.applied, Applied::Refused(Refusal::WrongCall));
    assert_eq!(plain, before);

    let (mut sheet, lane) = before_a_bright_node();
    sheet.level = 8;
    sheet.signal = 80;
    assert_eq!(sheet.node_ahead(lane), Some(Node::Bright));
    let rations = sheet.rations_left;
    // Nor does the bright one answer to a plain fight.
    let refused = sheet.apply(
        Command::Step {
            lane,
            call: Call::Fight(Pick::Fair),
        },
        &mut rng,
    );
    assert_eq!(refused.applied, Applied::Refused(Refusal::WrongCall));
    let outcome = sheet.apply(
        Command::Step {
            lane,
            call: Call::Fight(Pick::Bright),
        },
        &mut rng,
    );
    assert_eq!(outcome.applied, Applied::Started { pick: Pick::Bright });
    assert_eq!(sheet.rations_left, rations - 1);
    assert_eq!(
        outcome.lines,
        vec![FOES[7].arrives.to_string(), BRIGHT_LINE.to_string()]
    );
    let fight = sheet.fight.clone().expect("a fight on the row");
    assert_eq!(fight.name(), "bright dead pixels");
    // Over the dead pixels of level 8: a third more signal, 15% more
    // attack and defense rounded up, twice the bits, the same exp.
    let (_, _, plain) = RULES.foe(8);
    assert_eq!((plain.attack, plain.defense), (15, 11));
    assert_eq!(
        (
            fight.foe_max_signal,
            fight.foe_attack,
            fight.foe_defense,
            fight.foe_bits,
            fight.foe_exp
        ),
        (plain.signal * 135 / 100, 18, 13, plain.bits * 2, plain.exp)
    );

    // Put down, it always leaves one, and the wire hears.
    sheet.kills = 40;
    sheet.weapon_tier = 500;
    sheet.fight.as_mut().expect("a fight").foe_signal = 1;
    let won = win_out(&mut sheet, &mut rng);
    assert_eq!(
        won.applied,
        Applied::Won {
            foe: "dead pixels",
            bits: plain.bits * 2,
            garnished: 0,
            exp: plain.exp,
            leveled: None,
            bright: true,
            crystals: 1,
        }
    );
    assert_eq!(sheet.crystals, 1);
    assert_eq!(won.lines.last().map(String::as_str), Some(CRYSTAL_LINE));
    assert_eq!(
        sheet.news(&won.applied),
        vec![News::BrightDown { foe: "dead pixels" }]
    );
    assert_eq!(
        sheet.road.path.last().map(|trace| trace.mark),
        Some(Mark::BrightWon)
    );
}

#[test]
fn a_fair_kill_leaves_a_crystal_now_and_then_and_a_step_down_never() {
    let mut rng = StdRng::seed_from_u64(11);
    let kills = 1200;

    let mut fair = fresh();
    for _ in 0..kills {
        a_sure_kill(&mut fair, 0);
        win_out(&mut fair, &mut rng);
        fair.exp = 0;
    }
    // One in twelve: a hundred of twelve hundred, give or take the dice.
    assert!(
        (70..=130).contains(&fair.crystals),
        "{} crystals from {kills} fair kills",
        fair.crystals
    );

    // The same glyph met from a level above is a step down: nothing.
    let mut above = fresh();
    above.level = 2;
    above.signal = 20;
    for _ in 0..kills {
        a_sure_kill(&mut above, 0);
        win_out(&mut above, &mut rng);
        above.exp = 0;
    }
    assert_eq!(above.crystals, 0);
}

#[test]
fn dead_air_pours_one_glass_a_day_for_a_crystal() {
    let mut rng = StdRng::seed_from_u64(1);
    let mut sheet = fresh();
    sheet.level = 8;
    sheet.signal = 31;

    // No crystal, no glass.
    let dry = sheet.apply(
        Command::Drink {
            drink: Drink::StaticOnIce,
        },
        &mut rng,
    );
    assert_eq!(
        dry.applied,
        Applied::Refused(Refusal::ShortCrystals { by: 1 })
    );
    assert_eq!(sheet.drink, None);

    sheet.crystals = 3;
    let poured = sheet.apply(
        Command::Drink {
            drink: Drink::StaticOnIce,
        },
        &mut rng,
    );
    assert_eq!(
        poured.applied,
        Applied::Drank {
            drink: Drink::StaticOnIce
        }
    );
    assert_eq!((sheet.crystals, sheet.drink), (2, Some(Drink::StaticOnIce)));
    // Level 8, bare hands: 8, and the glass's 1 + 8 / 4.
    assert_eq!((sheet.attack(), sheet.defense()), (11, 8));
    assert_eq!(sheet.signal, 31, "only the test pattern fills");

    // One a day.
    let second = sheet.apply(
        Command::Drink {
            drink: Drink::DeadAirNeat,
        },
        &mut rng,
    );
    assert_eq!(second.applied, Applied::Refused(Refusal::GlassPoured));
    assert_eq!((sheet.crystals, sheet.drink), (2, Some(Drink::StaticOnIce)));

    // The roll takes it.
    assert!(sheet.settle(day(25)));
    assert_eq!(sheet.drink, None);
    assert_eq!((sheet.attack(), sheet.signal), (8, 80));

    // The other two: defense, and the bars.
    sheet.apply(
        Command::Drink {
            drink: Drink::DeadAirNeat,
        },
        &mut rng,
    );
    assert_eq!((sheet.attack(), sheet.defense()), (8, 11));
    sheet.settle(day(26));
    sheet.signal = 5;
    sheet.apply(
        Command::Drink {
            drink: Drink::TestPattern,
        },
        &mut rng,
    );
    assert_eq!((sheet.signal, sheet.max_signal()), (96, 96));
    assert_eq!(sheet.crystals, 0);

    // Not with the signal down, a glyph waiting, or the day spent.
    let mut down = fresh();
    down.crystals = 1;
    down.signal = 0;
    let mut waiting = fresh();
    waiting.crystals = 1;
    waiting.apply(STEP_IN, &mut rng);
    let mut spent = fresh();
    spent.crystals = 1;
    spent.rations_left = 0;
    for (mut sheet, refusal) in [
        (down, Refusal::SignalDown),
        (waiting, Refusal::FightWaiting),
        (spent, Refusal::NoRations),
    ] {
        let refused = sheet.apply(
            Command::Drink {
                drink: Drink::TestPattern,
            },
            &mut rng,
        );
        assert_eq!(refused.applied, Applied::Refused(refusal));
        assert_eq!((sheet.crystals, sheet.drink), (1, None));
    }
}

#[test]
fn the_blade_cart_sells_the_next_tier_up_for_crystals_alone() {
    let mut rng = StdRng::seed_from_u64(1);
    let mut sheet = fresh();
    sheet.weapon_tier = 3;
    sheet.bits = 7;
    sheet.crystals = 2;

    let short = sheet.apply(Command::Cart { slot: Slot::Weapon }, &mut rng);
    assert_eq!(
        short.applied,
        Applied::Refused(Refusal::ShortCrystals { by: 1 })
    );
    assert_eq!((sheet.weapon_tier, sheet.bits, sheet.crystals), (3, 7, 2));

    // The next tier up from the one carried, and not a bit changes hands.
    sheet.crystals = 7;
    let sold = sheet.apply(Command::Cart { slot: Slot::Weapon }, &mut rng);
    assert_eq!(
        sold.applied,
        Applied::Carted {
            slot: Slot::Weapon,
            tier: 4,
            crystals: 3
        }
    );
    assert_eq!((sheet.weapon_tier, sheet.bits, sheet.crystals), (4, 7, 4));

    // Each slot climbs its own ladder, a tier a visit.
    let armor = sheet.apply(Command::Cart { slot: Slot::Armor }, &mut rng);
    assert_eq!(
        armor.applied,
        Applied::Carted {
            slot: Slot::Armor,
            tier: 1,
            crystals: 3
        }
    );
    assert_eq!((sheet.armor_tier, sheet.crystals), (1, 1));

    // At the top of the ladder there is nothing past the wall.
    let mut top = at_the_top(0);
    top.crystals = 9;
    let past = top.apply(Command::Cart { slot: Slot::Weapon }, &mut rng);
    assert_eq!(past.applied, Applied::Refused(Refusal::PastTheWall));
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
    waiting.apply(STEP_IN, &mut rng);
    assert!(waiting.fight.is_some());
    let before = waiting.clone();
    let outcome = waiting.apply(Command::Patch, &mut rng);
    assert_eq!(outcome.applied, Applied::Refused(Refusal::FightWaiting));
    assert_eq!(waiting, before);

    let mut short = fresh();
    short.signal = 1;
    short.bits = 1;
    let before = short.clone();
    let outcome = short.apply(Command::Patch, &mut rng);
    assert_eq!(outcome.applied, Applied::Refused(Refusal::Short { by: 8 }));
    assert_eq!(
        outcome.lines,
        vec!["you are 8 bits short of a patch.".to_string()]
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
    sheet.fight = Some(a_fight(0, 100, 0, 36, 14));
    deal(
        &mut sheet,
        [
            Card::Strike,
            Card::Surge,
            Card::Block,
            Card::Block,
            Card::Block,
        ],
    );
    let mut rng = StdRng::seed_from_u64(3);

    let strike = sheet.apply(Command::Play { slot: 0 }, &mut rng);
    assert_eq!(
        strike.lines,
        vec!["your tire iron hits the flicker for 4.".to_string()]
    );
    let surge = sheet.apply(Command::Play { slot: 1 }, &mut rng);
    assert_eq!(
        surge.lines,
        vec!["you surge. your tire iron tears 10 out of the flicker.".to_string()]
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
        unpaid_mark: None,
        stash: 0,
        debt: 0,
        crystals: 2,
        drink: Some("dead_air_neat".to_string()),
        reset_generation: 0,
        road: None,
        cards: None,
        fight: Some(serde_json::json!({
            "quarry": {"glyph": 99}, "foe_signal": 1, "foe_max_signal": 1, "foe_attack": 1,
            "foe_defense": 1, "foe_bits": 1, "foe_exp": 1, "log": [], "bright": false,
            "turn": 0, "energy": 3, "block": 0, "muted": false,
            "piles": {"draw": [], "hand": [], "discard": []}
        })),
        user_id: Uuid::nil(),
        look: serde_json::json!({}),
    };
    assert_eq!(
        Sheet::from_row(&row),
        Err(SheetError::Fight("unknown glyph kind 99".to_string()))
    );

    // A fight from before the round has no deck to play: the row is
    // wrong, loudly (migration 225 dropped every one of them).
    let deckless = DeadchannelRunner {
        fight: Some(serde_json::json!({
            "quarry": {"glyph": 1}, "foe_signal": 1, "foe_max_signal": 1, "foe_attack": 1,
            "foe_defense": 1, "foe_bits": 1, "foe_exp": 1, "log": []
        })),
        ..row.clone()
    };
    assert!(matches!(
        Sheet::from_row(&deckless),
        Err(SheetError::Fight(_))
    ));

    // A fight, a hand, and a road round-trip through the row, with the
    // glass and the crystals.
    let ok = DeadchannelRunner {
        fight: Some(serde_json::json!({
            "quarry": {"glyph": 1}, "foe_signal": 1, "foe_max_signal": 1, "foe_attack": 1,
            "foe_defense": 1, "foe_bits": 1, "foe_exp": 1, "log": [], "bright": true,
            "turn": 2, "energy": 1, "block": 4, "muted": false,
            "piles": {"draw": ["strike"], "hand": ["block", null, "static"], "discard": ["surge", "wipe"]}
        })),
        road: Some(serde_json::json!({
            "path": [{"lane": 2, "mark": "fighting"}], "static_cards": 1
        })),
        ..row
    };
    let sheet = Sheet::from_row(&ok).expect("a sheet");
    let fight = sheet.fight.as_ref().expect("the fight");
    assert!(fight.bright);
    assert_eq!((fight.turn, fight.energy, fight.block), (2, 1, 4));
    assert_eq!(
        fight.piles.hand,
        vec![Some(Card::Block), None, Some(Card::Static)]
    );
    assert_eq!(
        sheet.road,
        RoadRun {
            path: vec![Trace {
                lane: 2,
                mark: Mark::Fighting
            }],
            static_cards: 1,
        }
    );
    assert_eq!(sheet.rations_left, 9);
    assert_eq!((sheet.crystals, sheet.drink), (2, Some(Drink::DeadAirNeat)));
    let write = sheet.to_write();
    assert_eq!(
        (write.crystals, write.drink.as_deref()),
        (2, Some("dead_air_neat"))
    );
    assert_eq!(
        (write.fight, write.road),
        (ok.fight.clone(), ok.road.clone())
    );

    // A road that is not a road is the row being wrong too.
    let off_road = DeadchannelRunner {
        fight: None,
        road: Some(serde_json::json!({"path": "north"})),
        ..ok.clone()
    };
    assert!(matches!(
        Sheet::from_row(&off_road),
        Err(SheetError::Road(_))
    ));

    // The drafted cards round-trip, and a card its draft never offered
    // is the row being wrong.
    let drafted = DeadchannelRunner {
        fight: None,
        level: 6,
        cards: Some(serde_json::json!(["siphon", "bulwark"])),
        ..ok.clone()
    };
    let sheet = Sheet::from_row(&drafted).expect("a sheet");
    assert_eq!(sheet.cards, vec![Card::Siphon, Card::Bulwark]);
    assert_eq!(sheet.to_write().cards, drafted.cards);
    assert_eq!(fresh().to_write().cards, None, "no picks is no column");
    let never_offered = DeadchannelRunner {
        cards: Some(serde_json::json!(["sever"])),
        ..drafted.clone()
    };
    assert!(matches!(
        Sheet::from_row(&never_offered),
        Err(SheetError::Cards(_))
    ));
    let not_cards = DeadchannelRunner {
        cards: Some(serde_json::json!({"jab": true})),
        ..drafted
    };
    assert!(matches!(
        Sheet::from_row(&not_cards),
        Err(SheetError::Cards(_))
    ));

    // A glass the menu does not have is the row being wrong, not a blank.
    let off_menu = DeadchannelRunner {
        fight: None,
        drink: Some("moonshine".to_string()),
        ..ok
    };
    assert_eq!(
        Sheet::from_row(&off_menu),
        Err(SheetError::Drink("moonshine".to_string()))
    );
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

/// The `Auto` key until the fight ends; the ending.
fn fight_out(sheet: &mut Sheet, rng: &mut StdRng) -> Applied {
    win_out(sheet, rng).applied
}

/// Through the real rules, on the `Auto` key from the top of the wall: the
/// Old Signal is a fight a first-timer can lose, and marks make it easier
/// up to the cap and no further. The odds themselves are the arena's
/// (`fight/BALANCE.md`); this holds the shape.
#[test]
fn the_old_signal_is_a_real_fight_that_marks_make_easier() {
    let win_rate = |marks: i32| {
        let mut wins = 0;
        for seed in 0..600 {
            let mut sheet = at_the_top(marks);
            let mut rng = StdRng::seed_from_u64(seed);
            sheet.apply(STEP_IN, &mut rng);
            if matches!(fight_out(&mut sheet, &mut rng), Applied::Slain { .. }) {
                wins += 1;
            }
        }
        wins * 100 / 600
    };
    let first = win_rate(0);
    let capped = win_rate(MARK_BONUS_CAP);
    assert!((10..=60).contains(&first), "first kill wins {first}%");
    assert!(
        capped >= first + 15,
        "capped marks win {capped}% against {first}%"
    );
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
    short.apply(STEP_IN, &mut rng);
    assert_eq!(
        short.fight.as_ref().map(|fight| fight.quarry),
        Some(Quarry::Glyph(14))
    );

    let mut ready = at_the_top(0);
    let outcome = ready.apply(STEP_IN, &mut rng);
    let fight = ready.fight.as_ref().expect("a fight on the row");
    assert_eq!(fight.quarry, Quarry::OldSignal);
    assert_eq!(fight.foe().name, "Old Signal");
    assert_eq!(outcome.lines, vec![OLD_SIGNAL.arrives.to_string()]);

    // Marks scale the exp it takes: last time's threshold is not enough.
    let mut again = at_the_top(1);
    again.exp = exp_to_seek(0);
    assert!(!again.signal_hears());
}

/// Whole state: the kill leaves a mark and a fresh runner's sheet with an
/// empty locker, and keeps the peak, the kill count, today's rations, and
/// the debt.
#[test]
fn putting_the_old_signal_down_leaves_a_mark_and_starts_the_climb_over() {
    let mut sheet = at_the_top(0);
    let mut rng = StdRng::seed_from_u64(1);
    sheet.stash = 4000;
    sheet.debt = 120;
    sheet.crystals = 3;
    sheet.road.static_cards = 2;
    sheet.apply(STEP_IN, &mut rng);
    sheet.fight.as_mut().expect("a fight").foe_signal = 1;
    deal(
        &mut sheet,
        [
            Card::Strike,
            Card::Static,
            Card::Block,
            Card::Block,
            Card::Block,
        ],
    );
    sheet.kills = 900;
    sheet.kills_today = 4;
    sheet.cards = vec![Card::Jab, Card::Bulwark, Card::Ground, Card::Sever];

    let outcome = sheet.apply(Command::Play { slot: 0 }, &mut rng);

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
        unpaid_mark: Some(1),
        stash: 0,
        debt: 120,
        crystals: 0,
        drink: None,
        // The deck comes clean with the rest; the day's road keeps the
        // step it was put down on.
        road: RoadRun {
            path: vec![Trace {
                lane: 1,
                mark: Mark::Won,
            }],
            static_cards: 0,
        },
        // The drafted cards go with the level.
        cards: Vec::new(),
    };
    assert_eq!(sheet, expected);
    assert_eq!(outcome.lines.len(), 4, "{:?}", outcome.lines);
    assert!(outcome.lines[2].contains("mark 1"), "{:?}", outcome.lines);
    assert!(
        outcome.lines[3].contains("4000 bits"),
        "{:?}",
        outcome.lines
    );
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
    sheet.apply(STEP_IN, &mut StdRng::seed_from_u64(2));
    sheet.fight.as_mut().expect("a fight").foe_signal = 1;
    let outcome = win_out(&mut sheet, &mut StdRng::seed_from_u64(2));
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

/// A glyph at the top that cannot survive you, worth one exp: the step
/// that crosses the seek threshold, and no more.
fn harmless_glyph_at_the_top() -> Fight {
    a_fight(14, 1, 0, 0, 1)
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

/// The `Auto` key until the fight ends; the ending outcome, lines and all.
fn win_out(sheet: &mut Sheet, rng: &mut StdRng) -> Outcome {
    for _ in 0..200 {
        let outcome = sheet.apply(Command::Auto, rng);
        if outcome.applied != Applied::Round {
            return outcome;
        }
    }
    panic!("a fight always ends");
}

/// A foe that cannot survive you, paying `bits`.
fn a_sure_kill(sheet: &mut Sheet, bits: i64) {
    sheet.weapon_tier = 50;
    sheet.fight = Some(a_fight(0, 1, 0, bits, 1));
}

#[test]
fn stepping_down_meets_the_glyph_a_level_below_at_half_pay() {
    let mut sheet = fresh();
    sheet.level = 3;
    let mut rng = StdRng::seed_from_u64(1);
    let step_down = Command::Step {
        lane: 1,
        call: Call::Fight(Pick::Lower),
    };

    let outcome = sheet.apply(step_down, &mut rng);

    assert_eq!(outcome.applied, Applied::Started { pick: Pick::Lower });
    assert_eq!(sheet.rations_left, RATIONS_PER_DAY - 1);
    let fight = the_fight(&sheet);
    // The hiss, level 2, at half of what it pays a level-2 runner.
    let (_, _, hiss) = RULES.foe(2);
    assert_eq!(fight.foe().name, "hiss");
    assert_eq!(fight.foe_level(), Some(2));
    assert_eq!(
        (fight.foe_bits, fight.foe_exp),
        (hiss.bits / 2, hiss.exp / 2)
    );
    assert_eq!(outcome.lines.len(), 2, "the step down is said");

    // Nothing is below the flicker, and asking spends nothing.
    let mut first = fresh();
    let refused = first.apply(step_down, &mut rng);
    assert_eq!(refused.applied, Applied::Refused(Refusal::NoLowerGlyph));
    assert_eq!(first, fresh());
}

#[test]
fn the_locker_takes_its_cut_going_in_and_keeps_the_rest_from_the_street() {
    let mut sheet = fresh();
    sheet.bits = 101;
    let mut rng = StdRng::seed_from_u64(1);

    let deposit = sheet.apply(Command::Deposit, &mut rng);

    // A tenth of 101, rounded up: no deposit is free.
    assert_eq!(
        deposit.applied,
        Applied::Deposited {
            stored: 90,
            fee: 11
        }
    );
    assert_eq!((sheet.bits, sheet.stash), (0, 90));
    assert_eq!(
        sheet.apply(Command::Deposit, &mut rng).applied,
        Applied::Refused(Refusal::NothingOnHand)
    );

    // A drop takes what is on hand, never what is locked up.
    sheet.bits = 30;
    sheet.signal = 1;
    sheet.apply(STEP_IN, &mut rng);
    let fight = sheet.fight.as_mut().expect("a fight");
    fight.foe_attack = 500;
    fight.foe_signal = 500;
    fight.foe_max_signal = 500;
    assert!(matches!(
        fight_out(&mut sheet, &mut rng),
        Applied::Lost { bits_lost: 30 }
    ));
    assert_eq!((sheet.bits, sheet.stash), (0, 90));

    // Out is free, and not with a glyph waiting.
    sheet.signal = 10;
    sheet.fight = Some(a_fight(0, 10, 1, 0, 0));
    assert_eq!(
        sheet.apply(Command::Withdraw, &mut rng).applied,
        Applied::Refused(Refusal::FightWaiting)
    );
    sheet.fight = None;
    assert_eq!(
        sheet.apply(Command::Withdraw, &mut rng).applied,
        Applied::Withdrew { amount: 90 }
    );
    assert_eq!((sheet.bits, sheet.stash), (90, 0));
    assert_eq!(
        sheet.apply(Command::Withdraw, &mut rng).applied,
        Applied::Refused(Refusal::LockerEmpty)
    );

    // A single bit is all cut: the locker refuses it instead of keeping it
    // for nothing.
    let mut poor = fresh();
    poor.bits = 1;
    let before = poor.clone();
    assert_eq!(
        poor.apply(Command::Deposit, &mut rng).applied,
        Applied::Refused(Refusal::DepositAllCut)
    );
    assert_eq!(poor, before);
}

#[test]
fn the_bits_machine_lends_to_the_cap_charges_its_fee_once_and_takes_its_share() {
    let mut sheet = fresh();
    sheet.bits = 0;
    let mut rng = StdRng::seed_from_u64(1);

    let loan = sheet.apply(Command::Borrow, &mut rng);

    // A tenth of the loan on top, charged when it lends.
    assert_eq!(loan.applied, Applied::Borrowed { amount: 50, fee: 5 });
    assert_eq!((sheet.bits, sheet.debt), (50, 55));
    assert_eq!(
        sheet.apply(Command::Borrow, &mut rng).applied,
        Applied::Refused(Refusal::LoanCapped)
    );

    // A level up opens the cap by what the debt has not taken of it, the
    // fee included, and the fee on the new loan rounds up.
    sheet.level = 2;
    let more = sheet.apply(Command::Borrow, &mut rng);
    assert_eq!(more.applied, Applied::Borrowed { amount: 45, fee: 5 });
    assert_eq!((sheet.bits, sheet.debt), (95, 105));

    // The debt never grows by itself: a roll adds nothing, however many.
    sheet.settle(day(25));
    sheet.settle(day(30));
    assert_eq!(sheet.debt, 105);

    // A kill pays half its bits to the machine first.
    sheet.bits = 0;
    a_sure_kill(&mut sheet, 60);
    let won = fight_out(&mut sheet, &mut rng);
    assert!(
        matches!(
            won,
            Applied::Won {
                bits: 60,
                garnished: 30,
                ..
            }
        ),
        "{won:?}"
    );
    assert_eq!((sheet.bits, sheet.debt), (30, 75));

    // Repaying takes what the hand holds, up to the debt.
    assert_eq!(
        sheet.apply(Command::Repay, &mut rng).applied,
        Applied::Repaid { amount: 30 }
    );
    assert_eq!((sheet.bits, sheet.debt), (0, 45));
    assert_eq!(
        sheet.apply(Command::Repay, &mut rng).applied,
        Applied::Refused(Refusal::NothingOnHand)
    );
    sheet.bits = 500;
    assert_eq!(
        sheet.apply(Command::Repay, &mut rng).applied,
        Applied::Repaid { amount: 45 }
    );
    assert_eq!((sheet.bits, sheet.debt), (455, 0));
    assert_eq!(
        sheet.apply(Command::Repay, &mut rng).applied,
        Applied::Refused(Refusal::NoDebt)
    );

    // Paid off, a kill is the runner's own again.
    a_sure_kill(&mut sheet, 60);
    assert!(matches!(
        fight_out(&mut sheet, &mut rng),
        Applied::Won { garnished: 0, .. }
    ));
    assert_eq!(sheet.bits, 515);
}

/// Whole state: a step off the ledge wipes the climb, the purse, and the
/// locker, and keeps the marks, the peak, the kills, today's rations, and
/// the debt.
#[test]
fn stepping_off_the_ledge_starts_the_runner_over_and_keeps_the_debt() {
    let mut sheet = at_the_top(2);
    sheet.stash = 900;
    sheet.debt = 70;
    sheet.crystals = 2;
    sheet.kills = 300;
    sheet.rations_left = 4;
    sheet.signal = 12;
    sheet.road.static_cards = 2;
    sheet.cards = vec![Card::Siphon];
    let mut rng = StdRng::seed_from_u64(1);

    let outcome = sheet.apply(Command::Reset, &mut rng);

    assert_eq!(outcome.applied, Applied::Reset);
    let expected = Sheet {
        user_id: Uuid::nil(),
        level: 1,
        exp: 0,
        signal: 10,
        weapon_tier: 0,
        armor_tier: 0,
        bits: 0,
        rations_left: 4,
        day: day(24),
        fight: None,
        kills: 300,
        kills_today: 0,
        runs_today: 0,
        peak_level: MAX_LEVEL,
        marks: 2,
        unpaid_mark: None,
        stash: 0,
        debt: 70,
        crystals: 0,
        drink: None,
        // The day is the day: the static the deck picked up on the road
        // comes down with you.
        road: RoadRun {
            path: Vec::new(),
            static_cards: 2,
        },
        // The drafted cards go with the level.
        cards: Vec::new(),
    };
    assert_eq!(sheet, expected);
    assert!(
        outcome.lines[2].contains("70 bits owed"),
        "{:?}",
        outcome.lines
    );
    assert_eq!(sheet.news(&outcome.applied), vec![News::SteppedOff]);

    // A runner with nothing to lose has no fall to take: the second step
    // changes nothing and the wire hears nothing.
    let before = sheet.clone();
    let again = sheet.apply(Command::Reset, &mut rng);
    assert_eq!(again.applied, Applied::Refused(Refusal::NothingToLose));
    assert_eq!(sheet, before);
    assert_eq!(sheet.news(&again.applied), Vec::new());

    // Not as a way back on the wire before the roll.
    let mut down = fresh();
    down.signal = 0;
    assert_eq!(
        down.apply(Command::Reset, &mut rng).applied,
        Applied::Refused(Refusal::SignalDown)
    );
}

/// One turn, card by card, on a hand the test dealt: what each card does,
/// what it costs, and what the row refuses.
#[test]
fn a_turn_is_three_energy_of_cards_played_from_their_slots() {
    let mut rng = StdRng::seed_from_u64(1);
    let mut sheet = fresh();
    sheet.level = 5;
    sheet.signal = 50;
    sheet.weapon_tier = 5;
    sheet.armor_tier = 5;
    sheet.fight = Some(a_fight(4, 500, 9, 0, 0));
    deal(
        &mut sheet,
        [
            Card::Strike,
            Card::Surge,
            Card::Block,
            Card::Wipe,
            Card::Static,
        ],
    );
    let powers = sheet.powers(the_fight(&sheet));
    // Attack 10 against no defense; a surge is two strikes and half a
    // third; a block is 60% of defense 10.
    assert_eq!((powers.strike, powers.surge, powers.block), (10, 25, 6));

    // The wipe: a block, and the static in the hand gone for good.
    let wipe = sheet.apply(Command::Play { slot: 3 }, &mut rng);
    assert_eq!(wipe.applied, Applied::Played { card: Card::Wipe });
    let fight = the_fight(&sheet);
    assert_eq!((fight.energy, fight.block), (2, 6));
    assert_eq!(
        fight.piles.hand,
        vec![
            Some(Card::Strike),
            Some(Card::Surge),
            Some(Card::Block),
            None,
            None
        ],
        "the slots hold still: a played card leaves a hole"
    );
    assert_eq!(fight.piles.static_cards(), 0);

    // An empty slot, then the surge for the two energy left.
    let empty = sheet.apply(Command::Play { slot: 3 }, &mut rng);
    assert_eq!(empty.applied, Applied::Refused(Refusal::NoCard));
    let surge = sheet.apply(Command::Play { slot: 1 }, &mut rng);
    assert_eq!(surge.applied, Applied::Played { card: Card::Surge });
    assert_eq!(
        (the_fight(&sheet).energy, the_fight(&sheet).foe_signal),
        (0, 475)
    );

    // Nothing left to pay for the strike: refused, and nothing moves.
    let before = sheet.clone();
    let broke = sheet.apply(Command::Play { slot: 0 }, &mut rng);
    assert_eq!(broke.applied, Applied::Refused(Refusal::NoEnergy));
    assert_eq!(sheet, before);
}

/// The glyph's turn: a hit eats the block standing, what gets through
/// lands and leaves static, and block nobody hit is still there next
/// turn. Then a fresh hand and full energy.
#[test]
fn the_glyph_takes_the_turn_it_showed_and_block_holds_until_a_hit_eats_it() {
    let mut rng = StdRng::seed_from_u64(1);
    let mut sheet = fresh();
    sheet.level = 5;
    sheet.signal = 50;
    // The drift: hits, gathers, comes down for double.
    sheet.fight = Some(a_fight(2, 500, 8, 0, 0));
    let hit = sheet.powers(the_fight(&sheet)).hit;
    assert_eq!(hit, 11, "170% of 8 attack less a quarter of defense 5");

    // Block over the hit: it holds, nothing lands, no static, and what
    // is left of it stands.
    sheet.fight.as_mut().expect("a fight").block = 15;
    let held = sheet.apply(Command::EndTurn, &mut rng);
    assert_eq!(held.applied, Applied::Round);
    assert_eq!(
        held.lines,
        vec!["it hits you for 11. your block holds.".to_string()]
    );
    let fight = the_fight(&sheet);
    assert_eq!((sheet.signal, fight.block, fight.turn), (50, 4, 1));
    assert_eq!(fight.piles.static_cards(), 0);
    assert_eq!(
        (fight.energy, fight.piles.hand.iter().flatten().count()),
        (ENERGY, HAND)
    );

    // It gathers: nothing lands, and the block is still standing.
    assert_eq!(fight.intent(), Intent::Charge);
    let gathered = sheet.apply(Command::EndTurn, &mut rng);
    assert_eq!(
        gathered.lines,
        vec!["the drift gathers itself. the next one lands for 22.".to_string()]
    );
    assert_eq!((sheet.signal, the_fight(&sheet).block), (50, 4));

    // The heavy: the block takes its four, the rest lands, static follows.
    let heavy = sheet.apply(Command::EndTurn, &mut rng);
    assert_eq!(
        heavy.lines,
        vec![
            "it comes down on you for 22. your block takes 4, you take 18.".to_string(),
            STATIC_LINE.to_string()
        ]
    );
    let fight = the_fight(&sheet);
    assert_eq!((sheet.signal, fight.block), (32, 0));
    assert_eq!(fight.piles.static_cards(), 1);
}

/// Static rides the deck: a noise turn floods it up to the cap, playing a
/// static card throws it out for good, and what is left when the fight
/// ends is in the next fight's deck.
#[test]
fn static_fills_the_deck_to_the_cap_and_rides_it_to_the_next_fight() {
    let mut rng = StdRng::seed_from_u64(1);
    let mut sheet = fresh();
    sheet.level = 6;
    sheet.signal = 60;
    // The ghost frame opens with noise.
    sheet.fight = Some(a_fight(5, 500, 1, 0, 0));
    assert_eq!(the_fight(&sheet).intent(), Intent::Noise);
    let flooded = sheet.apply(Command::EndTurn, &mut rng);
    assert_eq!(
        flooded.lines,
        vec![format!(
            "it floods you with noise. {NOISE_CARDS} static cards into your deck."
        )]
    );
    assert_eq!(sheet.signal, 60, "noise is not a hit");
    assert_eq!(the_fight(&sheet).piles.static_cards(), NOISE_CARDS);

    // Past the cap there is nowhere left to put it.
    sheet
        .fight
        .as_mut()
        .expect("a fight")
        .piles
        .add_static(STATIC_CAP);
    assert_eq!(the_fight(&sheet).piles.static_cards(), STATIC_CAP);

    // A static card costs one and does nothing but leave.
    deal(
        &mut sheet,
        [
            Card::Static,
            Card::Strike,
            Card::Strike,
            Card::Strike,
            Card::Strike,
        ],
    );
    let before = the_fight(&sheet).piles.static_cards();
    let shaken = sheet.apply(Command::Play { slot: 0 }, &mut rng);
    assert_eq!(shaken.applied, Applied::Played { card: Card::Static });
    let fight = the_fight(&sheet);
    assert_eq!((fight.energy, fight.piles.static_cards()), (2, before - 1));

    // Out of the fight, the static stays in the deck, and the next deal
    // has it.
    sheet.fight.as_mut().expect("a fight").turn = 1;
    sheet.apply(Command::Run, &mut rng);
    let carried = usize::from(sheet.road.static_cards);
    assert_eq!(carried, before - 1);
    sheet.apply(STEP_IN, &mut rng);
    let fight = the_fight(&sheet);
    assert_eq!(fight.piles.static_cards(), carried);
    assert_eq!(
        fight.piles.draw.len() + fight.piles.hand.iter().flatten().count(),
        10 + carried
    );
}

/// The `Auto` key is the obvious turn and the turn's end in one command.
#[test]
fn auto_plays_the_obvious_turn_and_ends_it() {
    let mut rng = StdRng::seed_from_u64(1);
    let mut sheet = fresh();
    sheet.level = 5;
    sheet.signal = 50;
    sheet.fight = Some(a_fight(0, 500, 9, 0, 0));
    deal(
        &mut sheet,
        [
            Card::Block,
            Card::Strike,
            Card::Surge,
            Card::Static,
            Card::Strike,
        ],
    );
    let powers = sheet.powers(the_fight(&sheet));

    let turn = sheet.apply(Command::Auto, &mut rng);

    // Against a plain hit: the surge and a strike, no block, and the
    // flicker's hit lands whole.
    assert_eq!(turn.applied, Applied::Round);
    let fight = the_fight(&sheet);
    assert_eq!(fight.foe_signal, 500 - powers.surge - powers.strike);
    assert_eq!(sheet.signal, 50 - powers.hit);
    assert_eq!((fight.turn, fight.energy), (1, ENERGY));
    assert_eq!(turn.lines.len(), 4, "two cards, the hit, the static");
}

/// The road: a step goes to the lane you stand in or the one beside it,
/// and each node answers only its own calls.
#[test]
fn a_step_reaches_the_lane_beside_and_a_node_answers_only_its_own_call() {
    let mut rng = StdRng::seed_from_u64(1);
    // Standing in the top lane after one step: the bottom lane is out of
    // reach, and asking spends nothing.
    let mut sheet = fresh();
    sheet.weapon_tier = 50;
    sheet.apply(
        Command::Step {
            lane: 0,
            call: Call::Fight(Pick::Fair),
        },
        &mut rng,
    );
    sheet.fight.as_mut().expect("a fight").foe_signal = 1;
    win_out(&mut sheet, &mut rng);
    assert_eq!(sheet.road.open_lanes(), vec![0, 1]);
    let before = sheet.clone();
    let node = sheet.node_ahead(2).expect("a node on the road");
    let call = match node {
        Node::Glyph => Call::Fight(Pick::Fair),
        Node::Bright => Call::Fight(Pick::Bright),
        Node::Rest => Call::Mend,
        Node::Cache => Call::Take,
    };
    let refused = sheet.apply(Command::Step { lane: 2, call }, &mut rng);
    assert_eq!(refused.applied, Applied::Refused(Refusal::NotThatWay));
    let off_road = sheet.apply(Command::Step { lane: 3, call }, &mut rng);
    assert_eq!(off_road.applied, Applied::Refused(Refusal::NotThatWay));
    assert_eq!(sheet, before);

    // A glyph's node is not a rest and not a cache.
    for call in [Call::Mend, Call::Clear, Call::Take] {
        let mut fresh_day = fresh();
        let refused = fresh_day.apply(Command::Step { lane: 1, call }, &mut rng);
        assert_eq!(refused.applied, Applied::Refused(Refusal::WrongCall));
        assert_eq!(fresh_day, fresh());
    }
}

/// A sheet standing in front of the first `node` of its day's road, and
/// the lane it is on, nothing walked yet.
fn before_a(node: Node) -> (Sheet, u8) {
    let mut sheet = fresh();
    let road = sheet.todays_road();
    let (step, lane) = road
        .steps
        .iter()
        .enumerate()
        .find_map(|(step, lanes)| {
            lanes
                .iter()
                .position(|here| *here == node)
                .map(|lane| (step, lane))
        })
        .expect("the day's road has one");
    sheet.rations_left = RATIONS_PER_DAY - step as i32;
    (sheet, lane as u8)
}

/// A rest is one or the other: the signal, or the deck.
#[test]
fn a_rest_mends_the_signal_or_clears_the_deck() {
    let mut rng = StdRng::seed_from_u64(1);
    let (mut sheet, lane) = before_a(Node::Rest);
    sheet.level = 4;
    sheet.signal = 10;
    sheet.road.static_cards = 3;
    let rations = sheet.rations_left;

    let mut mended = sheet.clone();
    let outcome = mended.apply(
        Command::Step {
            lane,
            call: Call::Mend,
        },
        &mut rng,
    );
    // 35% of forty.
    assert_eq!(outcome.applied, Applied::Mended { restored: 14 });
    assert_eq!(
        (mended.signal, mended.road.static_cards, mended.rations_left),
        (24, 3, rations - 1)
    );
    assert_eq!(
        mended.road.path,
        vec![Trace {
            lane,
            mark: Mark::Mended
        }]
    );

    let mut cleared = sheet.clone();
    let outcome = cleared.apply(
        Command::Step {
            lane,
            call: Call::Clear,
        },
        &mut rng,
    );
    assert_eq!(outcome.applied, Applied::Cleared { cards: 3 });
    assert_eq!(
        (
            cleared.signal,
            cleared.road.static_cards,
            cleared.rations_left
        ),
        (10, 0, rations - 1)
    );

    // A mend never goes past the max, and is still a step for a runner
    // who is whole; a clear with nothing to clear keeps the ration.
    let mut whole = sheet.clone();
    whole.signal = 38;
    whole.road.static_cards = 0;
    let topped = whole.apply(
        Command::Step {
            lane,
            call: Call::Mend,
        },
        &mut rng,
    );
    assert_eq!(topped.applied, Applied::Mended { restored: 2 });
    let mut clean = sheet.clone();
    clean.road.static_cards = 0;
    let before = clean.clone();
    let refused = clean.apply(
        Command::Step {
            lane,
            call: Call::Clear,
        },
        &mut rng,
    );
    assert_eq!(refused.applied, Applied::Refused(Refusal::NoStatic));
    assert_eq!(clean, before);
}

/// A cache pays a share of what the glyph of your level pays, and the
/// bits machine takes its half of that like of anything else.
#[test]
fn a_cache_pays_bits_and_the_machine_takes_its_share() {
    let mut rng = StdRng::seed_from_u64(1);
    let (mut sheet, lane) = before_a(Node::Cache);
    sheet.level = 3;
    sheet.signal = 30;
    sheet.debt = 40;
    let bits = RULES.cache(3);
    let take = Command::Step {
        lane,
        call: Call::Take,
    };

    let outcome = sheet.apply(take, &mut rng);

    assert_eq!(
        outcome.applied,
        Applied::Cached {
            bits,
            garnished: 40
        }
    );
    assert_eq!((sheet.bits, sheet.debt), (START_BITS + bits - 40, 0));
    assert_eq!(
        sheet.road.path,
        vec![Trace {
            lane,
            mark: Mark::Cached
        }]
    );
    assert_eq!(outcome.lines.len(), 2, "the cache, and the machine's cut");
}

/// A draft: the kill that reaches its level says a card is waiting, the
/// road holds until one of the two is taken, and the one taken is in the
/// deck of the next fight.
#[test]
fn a_level_that_owes_a_card_holds_the_road_until_one_is_taken() {
    let mut rng = StdRng::seed_from_u64(3);
    let mut sheet = fresh();
    sheet.level = 2;
    sheet.signal = 20;
    sheet.exp = exp_to_advance(2, 0).expect("a level under the top") - 1;
    a_sure_kill(&mut sheet, 10);
    sheet.weapon_tier = 0;
    sheet.level = 2;

    let won = win_out(&mut sheet, &mut rng);
    assert!(
        matches!(
            won.applied,
            Applied::Won {
                leveled: Some(3),
                ..
            }
        ),
        "{:?}",
        won.applied
    );
    assert_eq!(won.lines.last().map(String::as_str), Some(DRAFT_LINE));
    assert_eq!(sheet.draft(), Some(&DRAFTS[0]));

    // The road waits on the card, and a refusal spends nothing.
    let before = sheet.clone();
    let step = sheet.apply(STEP_IN, &mut rng);
    assert_eq!(step.applied, Applied::Refused(Refusal::CardWaiting));
    // Only what this draft offers: not a later draft's card, not a card
    // from the starting deck.
    for card in [Card::Sever, Card::Strike, Card::Static] {
        assert_eq!(
            sheet.apply(Command::Draft { card }, &mut rng).applied,
            Applied::Refused(Refusal::NotOffered),
            "{card:?}"
        );
    }
    assert_eq!(sheet, before);

    let drafted = sheet.apply(Command::Draft { card: Card::Siphon }, &mut rng);
    assert_eq!(
        drafted,
        Outcome {
            applied: Applied::Drafted {
                card: Card::Siphon,
                replaces: Card::Strike,
            },
            lines: vec!["siphon is in your deck now, in place of a strike.".to_string()],
        }
    );
    assert_eq!(
        sheet,
        Sheet {
            cards: vec![Card::Siphon],
            ..before
        },
        "a pick moves the deck and nothing else"
    );
    assert_eq!(sheet.news(&drafted.applied), vec![]);
    assert_eq!(sheet.draft(), None);
    assert_eq!(
        sheet
            .apply(Command::Draft { card: Card::Jab }, &mut rng)
            .applied,
        Applied::Refused(Refusal::NoDraft)
    );

    // The road is open again, and the fight is dealt from the new deck.
    assert_eq!(
        sheet.apply(STEP_IN, &mut rng).applied,
        Applied::Started { pick: Pick::Fair }
    );
    let piles = &the_fight(&sheet).piles;
    let mut dealt: Vec<Card> = piles
        .draw
        .iter()
        .chain(piles.hand.iter().flatten())
        .copied()
        .collect();
    dealt.sort_by_key(|card| card.name());
    let mut deck = sheet.deck();
    deck.sort_by_key(|card| card.name());
    assert_eq!(dealt, deck);
    assert_eq!(
        dealt.iter().filter(|card| **card == Card::Siphon).count(),
        1
    );

    // A draft that comes due with a glyph waiting waits for the fight.
    sheet.level = 6;
    assert_eq!(sheet.draft(), Some(&DRAFTS[1]));
    assert_eq!(
        sheet
            .apply(
                Command::Draft {
                    card: Card::Bulwark
                },
                &mut rng
            )
            .applied,
        Applied::Refused(Refusal::FightWaiting)
    );
}

/// The eight drafted cards through the machine, three turns of one
/// fight: what each moves, and what it leaves alone.
#[test]
fn the_drafted_cards_do_what_they_print() {
    let mut rng = StdRng::seed_from_u64(1);
    let mut sheet = fresh();
    sheet.level = 5;
    sheet.signal = 30;
    sheet.weapon_tier = 5;
    sheet.armor_tier = 5;
    // The drift: hits, gathers, comes down for double.
    sheet.fight = Some(a_fight(2, 500, 8, 0, 0));
    let powers = sheet.powers(the_fight(&sheet));
    assert_eq!(
        (powers.strike, powers.block, powers.hit),
        (10, 6, 10),
        "attack 10, 60% of defense 10, 170% of 8 attack less a quarter of defense"
    );
    let play = |sheet: &mut Sheet, rng: &mut StdRng, slot: u8| {
        sheet.apply(Command::Play { slot }, rng)
    };
    // The glyph's signal, the block standing, the energy, your signal.
    let table = |sheet: &Sheet| {
        let fight = the_fight(sheet);
        (fight.foe_signal, fight.block, fight.energy, sheet.signal)
    };

    deal(
        &mut sheet,
        [
            Card::Jab,
            Card::Siphon,
            Card::Riposte,
            Card::Bulwark,
            Card::Burn,
        ],
    );
    // Burn: free, two more energy, and a static card into the discard.
    let burn = play(&mut sheet, &mut rng, 4);
    assert_eq!(
        burn.lines,
        vec!["you burn hot. +2 energy, and a static card into your deck.".to_string()]
    );
    assert_eq!(table(&sheet), (500, 0, 5, 30));
    assert_eq!(the_fight(&sheet).piles.static_cards(), 1);
    // Jab: free, half a strike.
    play(&mut sheet, &mut rng, 0);
    assert_eq!(table(&sheet), (495, 0, 5, 30));
    // Siphon: a strike, and half of it back.
    let siphon = play(&mut sheet, &mut rng, 1);
    assert_eq!(
        siphon.lines,
        vec!["you siphon 10 out of the drift. +5 signal.".to_string()]
    );
    assert_eq!(table(&sheet), (485, 0, 4, 35));
    // Bulwark: two blocks and a half, for two.
    play(&mut sheet, &mut rng, 3);
    assert_eq!(table(&sheet), (485, 15, 2, 35));
    // Riposte: a block's worth and the fifteen standing, which stay.
    play(&mut sheet, &mut rng, 2);
    assert_eq!(table(&sheet), (464, 15, 1, 35));

    // The hit for 10 eats ten of the block; the glyph gathers next.
    sheet.apply(Command::EndTurn, &mut rng);
    assert_eq!(table(&sheet), (464, 5, ENERGY, 35));
    deal(
        &mut sheet,
        [
            Card::Ground,
            Card::Static,
            Card::Static,
            Card::Sever,
            Card::Mute,
        ],
    );
    // Ground: a strike and one more for each static card in hand, and
    // they are gone. The burn's is in the discard pile, out of reach.
    let ground = play(&mut sheet, &mut rng, 0);
    assert_eq!(
        ground.lines,
        vec!["you ground 2 static into the drift. 30.".to_string()]
    );
    assert_eq!(table(&sheet), (434, 5, 2, 35));
    assert_eq!(
        the_fight(&sheet).piles.hand,
        vec![None, None, None, Some(Card::Sever), Some(Card::Mute)]
    );
    assert_eq!(the_fight(&sheet).piles.static_cards(), 1);
    // Sever: a strike over half, two under it.
    sheet.fight.as_mut().expect("a fight").foe_signal = 250;
    let sever = play(&mut sheet, &mut rng, 3);
    assert_eq!(
        sever.lines,
        vec!["you sever the drift's feed. 20.".to_string()]
    );
    assert_eq!(table(&sheet), (230, 5, 1, 35));
    // A mute is two energy, and one is left.
    assert_eq!(
        play(&mut sheet, &mut rng, 4).applied,
        Applied::Refused(Refusal::NoEnergy)
    );

    // It gathers, then means to come down for 20. A mute and nothing
    // lands: no damage, no static, the block untouched, and the mute is
    // spent with the turn.
    sheet.apply(Command::EndTurn, &mut rng);
    assert_eq!(the_fight(&sheet).intent(), Intent::Heavy);
    deal(
        &mut sheet,
        [
            Card::Mute,
            Card::Strike,
            Card::Strike,
            Card::Block,
            Card::Block,
        ],
    );
    play(&mut sheet, &mut rng, 0);
    assert!(the_fight(&sheet).muted);
    let statics = the_fight(&sheet).piles.static_cards();
    let muted = sheet.apply(Command::EndTurn, &mut rng);
    assert_eq!(
        muted.lines,
        vec!["the drift moves, and nothing comes out.".to_string()]
    );
    assert_eq!(table(&sheet), (230, 5, ENERGY, 35));
    let fight = the_fight(&sheet);
    assert_eq!((fight.muted, fight.piles.static_cards()), (false, statics));
}
