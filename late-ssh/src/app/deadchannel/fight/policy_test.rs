use super::{Table, auto, sharp};
use crate::app::deadchannel::fight::cards::Card;
use crate::app::deadchannel::fight::data::Intent;
use crate::app::deadchannel::fight::state::Powers;

/// A table with strikes for 5, a surge for 13, blocks of 4, a glyph that
/// hits for 6, and room to take it.
fn table(hand: [Card; 5], now: Intent, next: Intent) -> Table {
    Table {
        hand: hand.into_iter().map(Some).collect(),
        energy: 3,
        block: 0,
        foe_signal: 100,
        foe_max_signal: 100,
        signal: 50,
        max_signal: 50,
        powers: Powers {
            strike: 5,
            surge: 13,
            block: 4,
            hit: 6,
            jab: 3,
            mend: 3,
            bulwark: 12,
            burn: 1,
            ground: 5,
            sever: 10,
        },
        now,
        next,
    }
}

fn cards(table: &Table, plays: &[usize]) -> Vec<Card> {
    plays
        .iter()
        .map(|slot| table.hand[*slot].expect("a slot the hand holds"))
        .collect()
}

const MIXED: [Card; 5] = [
    Card::Block,
    Card::Strike,
    Card::Surge,
    Card::Block,
    Card::Strike,
];

/// The `Auto` key: all in against a plain hit and while the glyph
/// gathers, two blocks up on the turn the heavy lands, the kill whenever
/// the hand holds it, and energy left over on a guard when something is
/// landing.
#[test]
fn auto_hits_until_a_heavy_lands_and_takes_the_kill() {
    let hit = table(MIXED, Intent::Hit, Intent::Hit);
    assert_eq!(cards(&hit, &auto(&hit)), vec![Card::Surge, Card::Strike]);

    // Gathering: the key does not think a turn ahead.
    let gathering = table(MIXED, Intent::Charge, Intent::Heavy);
    assert_eq!(
        cards(&gathering, &auto(&gathering)),
        vec![Card::Surge, Card::Strike]
    );

    // The heavy lands for 12: two blocks, and the third energy still hits.
    let landing = table(MIXED, Intent::Heavy, Intent::Hit);
    assert_eq!(
        cards(&landing, &auto(&landing)),
        vec![Card::Block, Card::Block, Card::Strike]
    );

    // Never more than two cards on the wall, however short it is.
    let walls = table(
        [
            Card::Block,
            Card::Block,
            Card::Block,
            Card::Strike,
            Card::Strike,
        ],
        Intent::Heavy,
        Intent::Hit,
    );
    assert_eq!(
        cards(&walls, &auto(&walls)),
        vec![Card::Block, Card::Block, Card::Strike]
    );

    // Block already standing covers it: nothing more goes on the wall.
    let mut covered = table(MIXED, Intent::Heavy, Intent::Hit);
    covered.block = 12;
    assert_eq!(
        cards(&covered, &auto(&covered)),
        vec![Card::Surge, Card::Strike]
    );

    // The kill outranks the wall.
    let mut finishing = table(MIXED, Intent::Heavy, Intent::Hit);
    finishing.foe_signal = 18;
    assert_eq!(
        cards(&finishing, &auto(&finishing)),
        vec![Card::Surge, Card::Strike]
    );

    // What is left over goes on a guard against the hit, and the wipe is
    // the guard that takes the static with it; with nothing landing it
    // shakes static out a card at a time. When a heavy calls for the
    // block anyway, the wipe goes up ahead of the hits.
    let noisy = [
        Card::Static,
        Card::Strike,
        Card::Static,
        Card::Wipe,
        Card::Static,
    ];
    let spare = table(noisy, Intent::Hit, Intent::Hit);
    assert_eq!(cards(&spare, &auto(&spare)), vec![Card::Strike, Card::Wipe]);
    let quiet = table(noisy, Intent::Charge, Intent::Heavy);
    assert_eq!(
        cards(&quiet, &auto(&quiet)),
        vec![Card::Strike, Card::Static, Card::Static]
    );
    let walled = table(noisy, Intent::Heavy, Intent::Hit);
    assert_eq!(
        cards(&walled, &auto(&walled)),
        vec![Card::Wipe, Card::Strike]
    );
}

/// A runner reading the hand does what the key does not: blocks a hit
/// that would land, wipes instead of paying for static card by card, and
/// never spends a block on a turn nothing is coming.
#[test]
fn sharp_weighs_the_whole_turn() {
    // A plain hit for 6: the key takes it on the chin. Two blocks stop
    // it whole (no static either), and the last energy still hits.
    let hit = table(MIXED, Intent::Hit, Intent::Hit);
    let mut plays = cards(&hit, &sharp(&hit));
    plays.sort_by_key(|card| card.name());
    assert_eq!(plays, vec![Card::Block, Card::Block, Card::Strike]);

    // Noise with a hit after it: block banked now is block for then, and
    // nothing is coming this turn to waste a card on beyond that.
    let quiet = table(MIXED, Intent::Noise, Intent::Noise);
    let mut plays = cards(&quiet, &sharp(&quiet));
    plays.sort_by_key(|card| card.name());
    assert_eq!(
        plays,
        vec![Card::Strike, Card::Surge],
        "nothing lands this turn or the next: all in"
    );

    // Three static in hand: the wipe clears them for one energy.
    let noisy = table(
        [
            Card::Static,
            Card::Strike,
            Card::Static,
            Card::Wipe,
            Card::Static,
        ],
        Intent::Hit,
        Intent::Hit,
    );
    let plays = cards(&noisy, &sharp(&noisy));
    assert!(plays.contains(&Card::Wipe), "{plays:?}");
    assert!(!plays.contains(&Card::Static), "{plays:?}");

    // The kill, by the cheapest cards that make it.
    let mut finishing = table(MIXED, Intent::Heavy, Intent::Hit);
    finishing.foe_signal = 5;
    assert_eq!(cards(&finishing, &sharp(&finishing)), vec![Card::Strike]);

    // A hit that would end the day gets every block there is.
    let mut dying = table(MIXED, Intent::Heavy, Intent::Hit);
    dying.signal = 5;
    let plays = cards(&dying, &sharp(&dying));
    assert_eq!(
        plays.iter().filter(|card| **card == Card::Block).count(),
        2,
        "{plays:?}"
    );
}

/// Neither policy asks for a card the turn cannot pay for or a slot with
/// nothing in it, whatever the hand.
#[test]
fn a_policy_only_plays_what_the_turn_can_pay_for() {
    let hands = [
        MIXED,
        [Card::Surge; 5],
        [Card::Static; 5],
        [Card::Block; 5],
        [
            Card::Wipe,
            Card::Static,
            Card::Surge,
            Card::Static,
            Card::Strike,
        ],
        [
            Card::Burn,
            Card::Mute,
            Card::Bulwark,
            Card::Surge,
            Card::Jab,
        ],
        [
            Card::Ground,
            Card::Static,
            Card::Sever,
            Card::Riposte,
            Card::Siphon,
        ],
    ];
    for hand in hands {
        for now in [Intent::Hit, Intent::Charge, Intent::Heavy, Intent::Noise] {
            for energy in 0..=3 {
                let mut at = table(hand, now, Intent::Hit);
                at.energy = energy;
                at.hand[4] = None;
                for plays in [auto(&at), sharp(&at)] {
                    let mut seen = std::collections::HashSet::new();
                    // Paid card by card, in order: a burn's energy only
                    // buys what is played after it.
                    let mut left = i32::from(energy);
                    for slot in &plays {
                        assert!(seen.insert(*slot), "a slot twice: {plays:?}");
                        let card = at.hand[*slot].expect("a slot with a card in it");
                        left -= i32::from(card.cost());
                        assert!(left >= 0, "{hand:?} {now:?} {energy}: {plays:?}");
                        if card == Card::Burn {
                            // One for nothing; the hands here hold no
                            // static beside a burn.
                            left += 1;
                        }
                    }
                }
            }
        }
    }
}

/// The key with the drafted cards: the free ones always, the burn ahead
/// of everything for its energy, a mute over the wall when a heavy lands,
/// the best hit for the energy first.
#[test]
fn auto_plays_the_drafted_cards_the_obvious_way() {
    // A jab is free: it goes first and the three energy still hit.
    let jab = table(
        [
            Card::Strike,
            Card::Jab,
            Card::Block,
            Card::Strike,
            Card::Strike,
        ],
        Intent::Hit,
        Intent::Hit,
    );
    assert_eq!(
        cards(&jab, &auto(&jab)),
        vec![Card::Jab, Card::Strike, Card::Strike, Card::Strike]
    );

    // A burn goes first: a fourth energy, and the static beside it
    // burned up for a fifth. That pays for the surge and all three
    // strikes, and nothing is left to shake out.
    let burn = table(
        [
            Card::Burn,
            Card::Surge,
            Card::Strike,
            Card::Static,
            Card::Strike,
        ],
        Intent::Hit,
        Intent::Hit,
    );
    assert_eq!(
        cards(&burn, &auto(&burn)),
        vec![Card::Burn, Card::Surge, Card::Strike, Card::Strike]
    );

    // The heavy lands: the mute takes it whole, no block goes up, and
    // the energy left still hits. Against a plain hit it stays put, and
    // the energy the two strikes leave goes on a block.
    let mute = [
        Card::Block,
        Card::Mute,
        Card::Strike,
        Card::Block,
        Card::Strike,
    ];
    let landing = table(mute, Intent::Heavy, Intent::Hit);
    assert_eq!(
        cards(&landing, &auto(&landing)),
        vec![Card::Mute, Card::Strike]
    );
    let plain = table(mute, Intent::Hit, Intent::Hit);
    assert_eq!(
        cards(&plain, &auto(&plain)),
        vec![Card::Strike, Card::Strike, Card::Block]
    );

    // A ground with static in hand is three strikes for one energy: it
    // goes ahead of the surge, and nothing is left to shake out. The
    // riposte hits for a block's worth, under a strike, so it goes last.
    let ground = table(
        [
            Card::Static,
            Card::Ground,
            Card::Static,
            Card::Riposte,
            Card::Strike,
        ],
        Intent::Hit,
        Intent::Hit,
    );
    assert_eq!(
        cards(&ground, &auto(&ground)),
        vec![Card::Ground, Card::Strike, Card::Riposte]
    );

    // A sever is a strike until the glyph is at half, so a strike goes
    // first: 28 of 50 is 23 after it, and the sever then lands twice
    // over, ahead of the strike still in the hand.
    let mut sever = table(
        [
            Card::Sever,
            Card::Strike,
            Card::Block,
            Card::Strike,
            Card::Block,
        ],
        Intent::Hit,
        Intent::Hit,
    );
    sever.foe_signal = 28;
    sever.foe_max_signal = 50;
    assert_eq!(
        cards(&sever, &auto(&sever)),
        vec![Card::Strike, Card::Sever, Card::Strike]
    );
}

/// Reading the hand with the drafted cards: the order is part of the
/// turn. Block goes up before the riposte that hits for it, and a mute
/// is worth its two energy when it is the heavy it stops.
#[test]
fn sharp_plays_the_drafted_cards_in_the_order_that_counts() {
    // Blocks of 4 against a hit for 6: two blocks stop it and make the
    // riposte a hit for 12, where riposte first would be a hit for 4.
    let riposte = table(
        [
            Card::Riposte,
            Card::Block,
            Card::Strike,
            Card::Block,
            Card::Strike,
        ],
        Intent::Hit,
        Intent::Hit,
    );
    assert_eq!(
        cards(&riposte, &sharp(&riposte)),
        vec![Card::Block, Card::Block, Card::Riposte]
    );

    // The heavy for 12: a mute stops all of it for two energy, where two
    // blocks would stop 8 and leave no energy for anything else.
    let mute = table(
        [
            Card::Block,
            Card::Mute,
            Card::Strike,
            Card::Block,
            Card::Strike,
        ],
        Intent::Heavy,
        Intent::Hit,
    );
    let plays = cards(&mute, &sharp(&mute));
    assert!(plays.contains(&Card::Mute), "{plays:?}");
    assert!(
        plays.iter().filter(|card| **card == Card::Block).count() < 2,
        "the wall does not go up behind a mute: {plays:?}"
    );

    // Static in hand and a ground: it throws both for a hit of 15,
    // instead of paying an energy each to shake them out.
    let ground = table(
        [
            Card::Static,
            Card::Ground,
            Card::Static,
            Card::Strike,
            Card::Strike,
        ],
        Intent::Noise,
        Intent::Noise,
    );
    let plays = cards(&ground, &sharp(&ground));
    assert_eq!(plays[0], Card::Ground, "{plays:?}");
    assert!(!plays.contains(&Card::Static), "{plays:?}");

    // A siphon over a strike when the signal is short: the same hit, and
    // it mends.
    let mut hurt = table(
        [
            Card::Strike,
            Card::Strike,
            Card::Strike,
            Card::Siphon,
            Card::Strike,
        ],
        Intent::Noise,
        Intent::Noise,
    );
    hurt.signal = 30;
    assert!(
        cards(&hurt, &sharp(&hurt)).contains(&Card::Siphon),
        "a mend is worth a strike's slot"
    );
}
