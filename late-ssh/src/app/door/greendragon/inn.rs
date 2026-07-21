//! The Sleeping Stag's entertainments: the bard's nightly gamble
//! (`modules/sethsong.php`) and the romance ladder (`modules/lovers.php` +
//! `modules/lovers/*`). Mechanics are transcribed 1=1 from those modules;
//! **all prose and names are original to late.sh**. The menu wiring lives in
//! `state.rs`; these are the resolvers.

use rand::Rng;

use super::data;
use super::model::{self, AddressStyle, Character};

/// One verse from the bard (`sethsong.php`): a straight `e_rand(0,18)` over
/// the stock outcome table. The caller gates the once-per-day flag; this
/// sets it. Returns the lines to log.
pub fn bard_song(c: &mut Character, rng: &mut impl Rng) -> Vec<String> {
    c.heard_bard_today = true;
    let mut lines = vec![format!(
        "{} takes up his lute, and the room quiets to listen.",
        data::BARD
    )];
    match rng.gen_range(0..=18u32) {
        0 => {
            c.turns += 2;
            lines
                .push("A marching song that won't leave your feet: +2 forest fights today!".into());
        }
        1 | 2 | 6 | 13 | 14 => {
            c.turns += 1;
            lines.push("The tune puts fresh iron in your legs: +1 forest fight.".into());
        }
        3 => {
            let gold = rng.gen_range(10..=50u64);
            c.gold += gold;
            lines.push(format!(
                "The crowd showers the floor with coins and {} waves your share over: +{gold} gold.",
                data::BARD
            ));
        }
        4 => {
            // HP swells to 1.2x the larger of current and max (an overheal
            // the healer will later clip for free).
            c.hitpoints = (c.hitpoints.max(c.max_hitpoints()) as f64 * 1.2).round() as u32;
            lines.push(
                "The ballad swells and your heart with it; you feel larger than life itself."
                    .into(),
            );
        }
        5 | 11 => {
            c.turns = c.turns.saturating_sub(1);
            lines.push("A dirge so heavy your shoulders sag under it: -1 forest fight.".into());
        }
        7 => {
            let loss = (c.max_hitpoints() as f64 * 0.10).round() as u32;
            c.hitpoints = c.hitpoints.saturating_sub(loss).max(1);
            lines.push(format!(
                "A cursed verse! Old wounds reopen as he sings: -{loss} hitpoints."
            ));
        }
        8 => {
            if c.gold >= 5 {
                c.gold -= 5;
                lines.push(
                    "He passes the hat and somehow it stops in front of you: -5 gold.".into(),
                );
            } else {
                lines.push("He passes the hat; your purse is too empty to be embarrassed.".into());
            }
        }
        9 => {
            c.gems += 1;
            lines.push("Mid-verse he flicks something glittering your way. A gem!".into());
        }
        10 | 12 => {
            if c.hitpoints < c.max_hitpoints() {
                c.hitpoints = c.max_hitpoints();
            }
            lines.push("A healing air, old as the hills: your wounds close as he plays.".into());
        }
        15 => {
            if c.style == AddressStyle::Second {
                c.charm += 1;
                lines.push(format!(
                    "{} weaves your name into the verse, and the whole room looks your way: +1 charm.",
                    data::BARD
                ));
            } else {
                c.turns += 1;
                lines.push(
                    "A bawdy number that leaves you laughing and lighter: +1 forest fight.".into(),
                );
            }
        }
        16 => {
            let loss = (c.max_hitpoints() as f64 * 0.20).round() as u32;
            c.hitpoints = c.hitpoints.saturating_sub(loss).max(1);
            lines.push(format!(
                "The song of the fall of Duskmere - grief lands like a blow: -{loss} hitpoints."
            ));
        }
        18 => {
            c.charm = c.charm.saturating_sub(1);
            lines.push(
                "He rhymes your name with something unflattering. The room roars: -1 charm.".into(),
            );
        }
        _ => {
            lines.push("A pleasant enough tune. Nothing comes of it.".into());
        }
    }
    lines
}

/// What a romance action produced: log lines, plus a daily-news item when the
/// evening (or the wedding, or the rejection) makes the paper.
pub struct FlirtOutcome {
    pub lines: Vec<String>,
    pub news: Option<String>,
}

/// The upstream flirt test `e_rand(charm, T) >= T`: a uniform roll between
/// the two (whichever order), so success is certain once charm reaches T.
fn flirt_roll(charm: u32, threshold: u32, rng: &mut impl Rng) -> bool {
    let (lo, hi) = (charm.min(threshold), charm.max(threshold));
    rng.gen_range(lo..=hi) >= threshold
}

/// Attempt flirt rung `rung` (0-5 on [`model::FLIRT_LADDER`], 6 = the
/// marriage proposal). The caller gates the once-per-day flag; this sets it.
pub fn flirt(c: &mut Character, rung: usize, rng: &mut impl Rng) -> FlirtOutcome {
    c.flirted_today = true;
    let partner = data::partner(c.style);
    let who = c.titled_name();

    // Rung 7: the proposal. No roll — the heart knows (charm >= 22), and a
    // rejection is so crushing the day is over (turns = 0).
    if rung >= model::FLIRT_LADDER.len() {
        if c.charm >= model::MARRY_CHARM_REQUIRED {
            c.married = true;
            c.apply_persistent_buff(model::lover_buff(partner));
            return FlirtOutcome {
                lines: vec![format!(
                    "{partner} laughs, cries, and says yes! The whole inn drinks to your health."
                )],
                news: Some(format!(
                    "{who} and {partner} were joined in matrimony at {}!",
                    data::INN_NAME
                )),
            };
        }
        c.turns = 0;
        return FlirtOutcome {
            lines: vec![format!(
                "{partner} goes very quiet, and lets you down as gently as anyone can. You haven't the heart for anything else today."
            )],
            news: None,
        };
    }

    let (threshold, cap) = model::FLIRT_LADDER[rung];
    if flirt_roll(c.charm, threshold, rng) {
        let mut lines = Vec::new();
        if c.charm < cap {
            c.charm += 1;
            lines.push(format!("{partner} warms to you. You gain a charm point!"));
        } else {
            lines.push(format!("{partner} warms to you, as ever."));
        }
        let mut news = None;
        // The sixth rung is an evening upstairs: it costs two turns and the
        // whole village hears about it by morning.
        if rung == 5 {
            if c.turns > 0 {
                c.turns = c.turns.saturating_sub(2);
            }
            lines.push("The evening runs long and wonderful (-2 forest fights).".into());
            news = Some(format!(
                "{who} and {partner} were seen slipping upstairs together at {}.",
                data::INN_NAME
            ));
        }
        FlirtOutcome { lines, news }
    } else {
        // Failure stings on the upper rungs only: rung 4 while 0<charm<10,
        // rung 5 while 0<charm<13, rung 6 whenever charm > 0.
        let stings = match rung {
            3 => c.charm > 0 && c.charm < 10,
            4 => c.charm > 0 && c.charm < 13,
            5 => c.charm > 0,
            _ => false,
        };
        let mut lines = vec![format!("{partner} pretends, kindly, not to have noticed.")];
        if stings {
            c.charm -= 1;
            lines.push("The sting shows on your face. You lose a charm point.".into());
        }
        FlirtOutcome { lines, news: None }
    }
}

/// The married daily visit (`lovers_violet/seth.php`, marriedto set): one in
/// four is a rebuff (-1 charm), the rest gain a point and the lover's ward.
/// The caller gates the once-per-day flag; this sets it.
pub fn married_visit(c: &mut Character, rng: &mut impl Rng) -> Vec<String> {
    c.flirted_today = true;
    let partner = data::partner(c.style);
    if rng.gen_range(1..=4) == 1 {
        c.charm = c.charm.saturating_sub(1);
        vec![format!(
            "{partner} is run off their feet tonight and waves you off mid-sentence. You lose a charm point."
        )]
    } else {
        c.charm += 1;
        c.apply_persistent_buff(model::lover_buff(partner));
        vec![format!(
            "{partner} steals an hour for you alone. You gain a charm point, and their worry wraps you like armor (Lover's Ward)."
        )]
    }
}

/// Idle chat with the partner: pure flavor bucketed by `charm + e_rand(-1,1)`
/// (the upstream chat switch's eight bands). Costs nothing, changes nothing.
pub fn chat(c: &Character, rng: &mut impl Rng) -> String {
    let lines = match c.style {
        AddressStyle::Second => &data::CHAT_BARD,
        _ => &data::CHAT_BARMAID,
    };
    let bucket = c.charm as i64 + rng.gen_range(-1..=1);
    let idx = if bucket <= 0 {
        0
    } else if bucket >= 19 {
        7
    } else {
        ((bucket + 2) / 3) as usize
    };
    lines[idx.min(lines.len() - 1)].to_string()
}
