use std::fmt::Write as _;
use std::time::Instant;

use super::super::super::classes::{ARCHETYPE_LEVEL, ArchetypeDef, Class, archetypes_for};
use super::super::{LONG_ROAD, TimeOfDay, Weather};
use super::*;

const ARCHDEMON: &str = "the Archdemon Mal'gareth";
const YSSGAR: &str = "Yssgar, the Sundering Deep";
const ASCENDANT: &str = "Kaethyr Ascendant, Who Sang the God Awake";
const KING: &str = "the King Who Was Promised Nothing";
const DUMMY: &str = "a straw training dummy";

/// Every archetype path a character of this level can be on: none below
/// `ARCHETYPE_LEVEL`, else both of the class's paths.
fn paths(class: Class, level: i32) -> Vec<Option<&'static ArchetypeDef>> {
    if level < ARCHETYPE_LEVEL {
        vec![None]
    } else {
        archetypes_for(class).into_iter().map(Some).collect()
    }
}

fn path_label(arch: Option<&'static ArchetypeDef>) -> &'static str {
    match arch {
        Some(a) => a.name,
        None => "no path",
    }
}

/// A character on a named path, three level-appropriate draughts in the bag.
fn recipe_on(
    class: Class,
    archetype: Option<&'static ArchetypeDef>,
    level: i32,
    gear: Gear,
    companion: Companion,
    coat: Coat,
    policy: Policy,
) -> Recipe {
    Recipe {
        class,
        level,
        archetype,
        gear,
        companion,
        coat,
        potions: 3,
        policy,
        build: Build::Neutral,
    }
}

/// A character on the damage path (the DPS archetype once it is offered, else
/// the class's first): the yardsticks and the crown derivation measure this
/// one; the crown contract and the report tables run every path.
fn recipe(
    class: Class,
    level: i32,
    gear: Gear,
    companion: Companion,
    coat: Coat,
    policy: Policy,
) -> Recipe {
    Recipe {
        class,
        level,
        archetype: (level >= ARCHETYPE_LEVEL).then(|| dps_or_first(class)),
        gear,
        companion,
        coat,
        potions: 3,
        policy,
        build: Build::Neutral,
    }
}

/// A bare character on the damage path with a given ability-score build.
fn built(class: Class, level: i32, gear: Gear, build: Build) -> Recipe {
    let mut r = recipe(
        class,
        level,
        gear,
        Companion::None,
        Coat::None,
        Policy::Honest,
    );
    r.build = build;
    r
}

#[test]
fn the_arena_clock_is_a_clear_day() {
    // The whole honest window must be flat: no dark (+25% mob damage), no fog
    // (ambush boost), no storm (caster-bolt boost). Pins ARENA_CLOCK against
    // the real day/weather tables so a change there moves this, not the data.
    for t in ARENA_CLOCK + 1..=ARENA_CLOCK + HONEST_MAX_TICKS as u64 {
        assert!(!TimeOfDay::from_ticks(t).is_dark(), "tick {t} is dark");
        assert!(
            !matches!(Weather::from_ticks(t), Weather::Fog | Weather::Storm),
            "tick {t} has boosting weather"
        );
    }
}

#[test]
fn every_point_of_damage_is_accounted_for() {
    // A naked level-1 Warrior has exactly two sources, the auto and Cleave, so
    // the dummy's whole health must land in those two buckets and nowhere else.
    let mut arena = Arena::new();
    let r = arena.fight(
        recipe(
            Class::Warrior,
            1,
            Gear::Naked,
            Companion::None,
            Coat::None,
            Policy::Honest,
        ),
        DUMMY,
    );
    assert_eq!(r.outcome, Outcome::Won, "{r:?}");
    assert_eq!(r.dealt.total(), r.foe.max_hp, "{r:?}");
    assert!(r.dealt.auto > 0 && r.dealt.ability > 0, "{r:?}");
    assert_eq!((r.dealt.dot, r.dealt.coat, r.dealt.pet), (0, 0, 0), "{r:?}");
}

#[test]
fn a_companion_and_a_coat_land_in_their_own_buckets() {
    // Against a foe that lasts: the same character drops the Archdemon in two
    // ticks, before a coat wound ever festers.
    let mut arena = Arena::new();
    let r = arena.fight(
        recipe(
            Class::Rogue,
            75,
            Gear::Reaches(19),
            Companion::TameBest,
            Coat::BestOil(4),
            Policy::Honest,
        ),
        YSSGAR,
    );
    // Whether the fight is won is the report's business; this only checks
    // that every source lands in its own bucket.
    assert!(r.ticks > 5, "{r:?}");
    assert!(r.dealt.pet > 0, "{r:?}");
    assert!(r.dealt.coat > 0, "{r:?}");
    assert!(
        r.dealt.dot > 0,
        "the Rogue's Envenom should have ticked: {r:?}"
    );
}

// ---- Where the damage comes from ------------------------------------------

#[test]
fn casters_lean_on_abilities_and_martials_on_the_auto() {
    // L55 in Frontier tier 10, bare, against the King: a fight long enough
    // for a rotation to matter, not an opening burst. Casters get most of
    // their output from the school-carrying abilities (a healer spends some
    // of its casts on mending, so its floor is lower); martials from the
    // Physical swing. Hybrids sit between and are not pinned.
    let mut arena = Arena::new();
    let casters = [
        Class::Mage,
        Class::Runemaster,
        Class::Necromancer,
        Class::Warlock,
        Class::Spiritmaster,
        Class::Cleric,
    ];
    let martials = [
        Class::Warrior,
        Class::Rogue,
        Class::Ranger,
        Class::Monk,
        Class::Berserker,
        Class::Valewalker,
    ];
    let fight = |arena: &mut Arena, class: Class| {
        arena.fight(
            recipe(
                class,
                55,
                Gear::Frontier(9),
                Companion::None,
                Coat::None,
                Policy::Honest,
            ),
            KING,
        )
    };
    for class in casters {
        let r = fight(&mut arena, class);
        let total = r.dealt.total().max(1);
        let abilities = (r.dealt.ability + r.dealt.dot) * 100 / total;
        let floor = if class == Class::Cleric { 50 } else { 55 };
        assert!(
            abilities >= floor,
            "{class:?}: abilities are {abilities}% of output: {r:?}"
        );
    }
    for class in martials {
        let r = fight(&mut arena, class);
        let total = r.dealt.total().max(1);
        let auto = r.dealt.auto * 100 / total;
        assert!(
            auto >= 55,
            "{class:?}: the auto is {auto}% of output: {r:?}"
        );
    }
}

/// Ticks the dps yardstick runs: long enough for every cooldown and the
/// resource pool to matter, short enough to stay inside the clear day.
const DPS_TICKS: u32 = 20;
/// The widest spread the yardstick tolerates between the best and the worst
/// calling at one ladder step (bare: no pet, no coat). The old world sat at
/// 2.0 (a Cleric at half a Rogue). The floor is always a healer or the Druid:
/// a C-tier attack curve and a roster that spends its casts on mending is
/// sustain bought with damage, and the yardstick counts only the damage.
const DPS_SPREAD_MAX: f64 = 1.6;

fn dps_ladder() -> [(i32, Gear); 5] {
    [
        (10, Gear::ShopBest),
        (32, Gear::ShopBest),
        (55, Gear::Frontier(9)),
        (75, Gear::Reaches(19)),
        (100, Gear::Kaelmyr(19)),
    ]
}

#[test]
fn classes_kill_at_a_similar_pace_in_the_same_gear() {
    // Damage paths only: a Tank or Healer path trades pace for its role by
    // design, so the class comparison is made on the path built to deal it.
    let mut arena = Arena::new();
    for (level, gear) in dps_ladder() {
        let rows: Vec<(Class, i32)> = Class::ALL
            .iter()
            .map(|&class| {
                let r = recipe(
                    class,
                    level,
                    gear,
                    Companion::None,
                    Coat::None,
                    Policy::Honest,
                );
                (class, arena.measure_dps(r, DPS_TICKS))
            })
            .collect();
        let (best, worst) = rows.iter().fold((rows[0], rows[0]), |(b, w), &r| {
            (if r.1 > b.1 { r } else { b }, if r.1 < w.1 { r } else { w })
        });
        let spread = best.1 as f64 / worst.1.max(1) as f64;
        assert!(
            spread <= DPS_SPREAD_MAX,
            "L{level} {}: {:?} {} dps vs {:?} {} dps is a {spread:.2}x spread; all: {rows:?}",
            gear.label(),
            best.0,
            best.1,
            worst.0,
            worst.1
        );
    }
}

#[test]
#[ignore = "prints the dps yardstick per class and ladder step; the fast tuning loop"]
fn arena_dps_table() {
    let mut arena = Arena::new();
    let ladder = dps_ladder();
    let mut out = String::new();
    let _ = write!(out, "| class |");
    for (level, gear) in ladder {
        let _ = write!(out, " L{level} {} |", gear.label());
    }
    let _ = writeln!(out);
    for class in Class::ALL {
        for arch in archetypes_for(class) {
            let _ = write!(out, "| {class:?} · {} |", arch.name);
            for (level, gear) in ladder {
                let r = recipe_on(
                    class,
                    Some(arch),
                    level,
                    gear,
                    Companion::None,
                    Coat::None,
                    Policy::Honest,
                );
                let d = arena.measure(r, DPS_TICKS);
                let _ = write!(out, " {} {} |", d.total() / DPS_TICKS as i32, d.shares());
            }
            let _ = writeln!(out);
        }
    }
    eprintln!(
        "[arena] dps over {DPS_TICKS} ticks on the neutral dummy, bare, both paths (dps auto/ability/dot/coat/pet):\n{out}"
    );
}

// ---- The companion --------------------------------------------------------

/// The share of a character's output its companion may carry, on a pet the
/// band can actually have (a Stable beast to L40, a maxed tame past it).
/// Below the floor the pet is a pet in name; above the ceiling it is the
/// build, and the class stops mattering. Beastlord's Pack Bond may push it
/// higher, the one class built around the beast.
const PET_SHARE: std::ops::RangeInclusive<i32> = 12..=33;
const BEASTLORD_PET_SHARE_MAX: i32 = 40;

#[test]
fn a_companion_is_a_share_of_the_fight_not_the_fight() {
    let mut arena = Arena::new();
    let bands = [
        (32, Gear::Kit(3), Companion::ShopBest),
        (55, Gear::Frontier(9), Companion::TameBest),
        (100, Gear::Kaelmyr(19), Companion::TameBest),
    ];
    let mut out_of_band: Vec<String> = Vec::new();
    for (level, gear, companion) in bands {
        for class in Class::ALL {
            let d = arena.measure(
                recipe(class, level, gear, companion, Coat::None, Policy::Honest),
                DPS_TICKS,
            );
            let share = d.pet * 100 / d.total().max(1);
            let ceiling = if class == Class::Beastlord {
                BEASTLORD_PET_SHARE_MAX
            } else {
                *PET_SHARE.end()
            };
            if !(*PET_SHARE.start()..=ceiling).contains(&share) {
                out_of_band.push(format!(
                    "{class:?} L{level} {} {}: {share}%",
                    gear.label(),
                    companion.label()
                ));
            }
        }
    }
    assert!(
        out_of_band.is_empty(),
        "the companion is not a share of the fight: {out_of_band:?}"
    );
}

// ---- The crowns --------------------------------------------------------------
//
// The story of the game: the grind to 100 is long by design (75% of the xp
// curve is 50->100), so the last crown falls to a *prepared* character at
// L80 and 80-100 is prestige; the first crown is a real fight at L12 with
// the right prep (the Treant teaches the oil), not a one-shot. "Prepared"
// means the tier's kit, the oil the foe is weak to, three draughts, and
// from the Reaches on a maxed companion. "A walk-in" is a few levels lower
// in the previous tier's kit with none of that. The table is the contract;
// the crowns' numbers in `world.rs` (`CROWNS`) are derived from what the
// arena measures at each kit and pinned by
// `every_crown_falls_to_a_prepared_character_and_not_to_a_walk_in`.

struct CrownTarget {
    boss: &'static str,
    level: i32,
    gear: Gear,
    companion: Companion,
    /// Level and gear of the unprepared character that must still lose.
    below: (i32, Gear),
}

const CROWN_TARGETS: [CrownTarget; 14] = [
    CrownTarget {
        boss: "the Elder Treant",
        level: 12,
        gear: Gear::Kit(0),
        companion: Companion::None,
        below: (6, Gear::Naked),
    },
    CrownTarget {
        boss: "the Bone Tyrant",
        level: 16,
        gear: Gear::Kit(1),
        companion: Companion::None,
        below: (10, Gear::Kit(0)),
    },
    CrownTarget {
        boss: "the Lich Vael",
        level: 20,
        gear: Gear::Kit(2),
        companion: Companion::None,
        below: (14, Gear::Kit(1)),
    },
    CrownTarget {
        boss: "the Magma Colossus",
        level: 24,
        gear: Gear::Kit(2),
        companion: Companion::None,
        below: (18, Gear::Kit(1)),
    },
    CrownTarget {
        boss: "the Wyrm of Frostspire",
        level: 27,
        gear: Gear::Kit(3),
        companion: Companion::None,
        below: (21, Gear::Kit(2)),
    },
    CrownTarget {
        boss: "the Fallen Paladin",
        level: 30,
        gear: Gear::Kit(3),
        companion: Companion::None,
        below: (24, Gear::Kit(2)),
    },
    CrownTarget {
        boss: "the Archdemon Mal'gareth",
        level: 35,
        gear: Gear::Kit(4),
        companion: Companion::None,
        below: (29, Gear::Kit(3)),
    },
    CrownTarget {
        boss: "The Bonewright Lich",
        level: 40,
        gear: Gear::Kit(4),
        companion: Companion::None,
        below: (34, Gear::Kit(3)),
    },
    CrownTarget {
        boss: "the Elder Dryad",
        level: 40,
        gear: Gear::Kit(4),
        companion: Companion::None,
        below: (34, Gear::Kit(3)),
    },
    CrownTarget {
        boss: "the Abyss-Thing",
        level: 40,
        gear: Gear::Kit(4),
        companion: Companion::None,
        below: (34, Gear::Kit(3)),
    },
    CrownTarget {
        boss: "the King Who Was Promised Nothing",
        level: 55,
        gear: Gear::Frontier(9),
        companion: Companion::ShopBest,
        below: (49, Gear::Frontier(4)),
    },
    CrownTarget {
        boss: "Yssgar, the Sundering Deep",
        level: 65,
        gear: Gear::Reaches(9),
        companion: Companion::TameBest,
        below: (59, Gear::Frontier(19)),
    },
    CrownTarget {
        boss: "Kaethyr the Unquenched, Ashen King of Kaelmyr",
        level: 75,
        gear: Gear::Kaelmyr(9),
        companion: Companion::TameBest,
        below: (69, Gear::Reaches(19)),
    },
    CrownTarget {
        boss: "Kaethyr Ascendant, Who Sang the God Awake",
        level: 80,
        gear: Gear::Kaelmyr(14),
        companion: Companion::TameBest,
        below: (74, Gear::Kaelmyr(9)),
    },
];

/// The alchemy tier a character of this level can brew or buy (the crafting
/// gates: tier 0 at L1, then 8/16/26/38/55).
fn oil_tier_for(level: i32) -> usize {
    match level {
        ..=7 => 0,
        8..=15 => 1,
        16..=25 => 2,
        26..=37 => 3,
        38..=54 => 4,
        _ => 5,
    }
}

fn prepared(t: &CrownTarget, class: Class) -> Recipe {
    recipe(
        class,
        t.level,
        t.gear,
        t.companion,
        Coat::BestOil(oil_tier_for(t.level)),
        Policy::Honest,
    )
}

fn prepared_on(t: &CrownTarget, class: Class, arch: Option<&'static ArchetypeDef>) -> Recipe {
    recipe_on(
        class,
        arch,
        t.level,
        t.gear,
        t.companion,
        Coat::BestOil(oil_tier_for(t.level)),
        Policy::Honest,
    )
}

fn walk_in_on(t: &CrownTarget, class: Class, arch: Option<&'static ArchetypeDef>) -> Recipe {
    let mut r = recipe_on(
        class,
        arch,
        t.below.0,
        t.below.1,
        Companion::None,
        Coat::None,
        Policy::Honest,
    );
    r.potions = 0;
    r
}

/// A crown fight is a fight: the median prepared kill sits in this many ticks.
const CROWN_TICKS: std::ops::RangeInclusive<u32> = 8..=40;
/// At most this many of the 17 callings may take a crown as a walk-in (per
/// path; the contract runs both paths, so twice this across them).
const WALK_IN_WINS_MAX: usize = 4;

#[test]
#[ignore = "a minute of real fights; part of `make arena`, not of the suite"]
fn every_crown_falls_to_a_prepared_character_and_not_to_a_walk_in() {
    // Every calling on every path: a crown must fall whatever a player chose
    // at L10, and a walk-in must lose whatever they chose.
    let mut arena = Arena::new();
    for t in CROWN_TARGETS.iter() {
        let mut ticks: Vec<u32> = Vec::new();
        let mut losers: Vec<(Class, &'static str, Outcome, u32)> = Vec::new();
        for class in Class::ALL {
            for arch in paths(class, t.level) {
                let r = arena.fight(prepared_on(t, class, arch), t.boss);
                ticks.push(r.ticks);
                if r.outcome != Outcome::Won {
                    losers.push((class, path_label(arch), r.outcome, r.ticks));
                }
            }
        }
        assert!(
            losers.is_empty(),
            "{} at L{} {}: prepared characters lost: {losers:?}",
            t.boss,
            t.level,
            t.gear.label()
        );
        ticks.sort_unstable();
        let median = ticks[ticks.len() / 2];
        assert!(
            CROWN_TICKS.contains(&median),
            "{} at L{} {}: a median kill of {median} ticks is not a fight ({ticks:?})",
            t.boss,
            t.level,
            t.gear.label()
        );
        let mut walk_in_wins: Vec<(Class, &'static str)> = Vec::new();
        for class in Class::ALL {
            for arch in paths(class, t.below.0) {
                if arena.fight(walk_in_on(t, class, arch), t.boss).outcome == Outcome::Won {
                    walk_in_wins.push((class, path_label(arch)));
                }
            }
        }
        assert!(
            walk_in_wins.len() <= WALK_IN_WINS_MAX * 2,
            "{} falls to a walk-in at L{} {}: {walk_in_wins:?}",
            t.boss,
            t.below.0,
            t.below.1.label()
        );
    }
}

#[test]
#[ignore = "prints, per crown, what a prepared character brings; the input to the CROWNS table"]
fn arena_crown_yardstick() {
    let mut arena = Arena::new();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "| crown | today | target | median dps | median pool | median armor | dps range |"
    );
    for t in CROWN_TARGETS.iter() {
        let mut dps: Vec<i32> = Vec::new();
        let mut pool: Vec<i32> = Vec::new();
        let mut armor: Vec<i32> = Vec::new();
        for class in Class::ALL {
            let r = prepared(t, class);
            dps.push(arena.measure_dps(r, DPS_TICKS));
            let (_, _, _, hp, arm) = arena.sheet(r);
            pool.push(hp);
            armor.push(arm);
        }
        dps.sort_unstable();
        pool.sort_unstable();
        armor.sort_unstable();
        let f = arena.foe(t.boss);
        let _ = writeln!(
            out,
            "| {} | {}hp {}dmg | L{} {} {} | {} | {} | {} | {}..{} |",
            t.boss,
            f.max_hp,
            f.damage,
            t.level,
            t.gear.label(),
            t.companion.label(),
            dps[dps.len() / 2],
            pool[pool.len() / 2],
            armor[armor.len() / 2],
            dps[0],
            dps[dps.len() - 1]
        );
    }
    eprintln!(
        "[arena] crown yardstick ({DPS_TICKS}-tick dps on the neutral dummy, prepared kit):\n{out}"
    );
}

/// The band a crown's doorstep trash must sit in for the prepared character
/// at the crown's target: dead inside this many ticks, and needing at least
/// that many to kill the character with no draught drunk.
const DOORSTEP_KILL_TICKS_MAX: f64 = 4.0;
const DOORSTEP_SURVIVE_TICKS_MIN: f64 = 15.0;

/// Median hp/dmg of the regulars in the crown's zone, and the prepared
/// character's median dps, pool and armor at the crown's target.
fn doorstep_numbers(arena: &mut Arena, t: &CrownTarget) -> Option<(FoeCard, i32, i32, i32)> {
    let mut dps: Vec<i32> = Vec::new();
    let mut pool: Vec<i32> = Vec::new();
    let mut armor: Vec<i32> = Vec::new();
    for class in Class::ALL {
        let r = prepared(t, class);
        dps.push(arena.measure_dps(r, DPS_TICKS));
        let (_, _, _, hp, arm) = arena.sheet(r);
        pool.push(hp);
        armor.push(arm);
    }
    dps.sort_unstable();
    pool.sort_unstable();
    armor.sort_unstable();
    let mut trash = arena.doorstep(t.boss);
    if trash.is_empty() {
        return None;
    }
    trash.sort_by_key(|f| f.max_hp + f.damage * 4);
    let mid = trash[trash.len() / 2];
    Some((
        mid,
        dps[dps.len() / 2],
        pool[pool.len() / 2],
        armor[armor.len() / 2],
    ))
}

fn blunt_for(attack_type: super::super::super::damage::DamageType, armor: i32) -> i32 {
    if attack_type == super::super::super::damage::DamageType::Physical {
        armor / 2
    } else {
        armor / 4
    }
}

#[test]
#[ignore = "half a minute of real fights; part of `make arena`, not of the suite"]
fn the_trash_on_a_crowns_doorstep_is_in_band() {
    // The land must agree with its crown: at the crown's target, a regular on
    // its doorstep dies in a few prepared ticks and needs many to kill you.
    // Lands ride `tune_spawn_balance`'s rows, so an out-of-band land is one
    // row there, never a mob.
    let mut arena = Arena::new();
    for t in CROWN_TARGETS.iter() {
        let Some((mid, dps, pool, armor)) = doorstep_numbers(&mut arena, t) else {
            continue;
        };
        let to_kill = mid.max_hp as f64 / dps.max(1) as f64;
        let to_die = pool as f64 / (mid.damage - blunt_for(mid.attack_type, armor)).max(1) as f64;
        assert!(
            to_kill <= DOORSTEP_KILL_TICKS_MAX,
            "{}'s doorstep: a regular ({} hp) takes {to_kill:.1} prepared ticks to kill",
            t.boss,
            mid.max_hp
        );
        assert!(
            to_die >= DOORSTEP_SURVIVE_TICKS_MIN,
            "{}'s doorstep: a regular ({} dmg) kills the prepared character in {to_die:.1} ticks",
            t.boss,
            mid.damage
        );
    }
}

#[test]
#[ignore = "prints, per crown, the trash on its doorstep against the prepared character"]
fn arena_doorstep_yardstick() {
    let mut arena = Arena::new();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "| crown | median doorstep hp/dmg | ticks to kill one | ticks one needs to kill you |"
    );
    for t in CROWN_TARGETS.iter() {
        let Some((mid, dps, pool, armor)) = doorstep_numbers(&mut arena, t) else {
            let _ = writeln!(out, "| {} | none | - | - |", t.boss);
            continue;
        };
        let _ = writeln!(
            out,
            "| {} | {}/{} | {:.1} | {:.1} |",
            t.boss,
            mid.max_hp,
            mid.damage,
            mid.max_hp as f64 / dps.max(1) as f64,
            pool as f64 / (mid.damage - blunt_for(mid.attack_type, armor)).max(1) as f64
        );
    }
    eprintln!("[arena] doorstep yardstick (prepared character at the crown's target):\n{out}");
}

// ---- The exploits --------------------------------------------------------
//
// The hole players found: a boss kept its wounds between engagements, `flee`
// always worked and cost nothing, mobs never healed, and a stun outlived the
// fight it was cast in. So engage, stun, land the free exchange, flee, wait
// out the cooldown, repeat, took the strongest crown in the game at level 32
// without ever being hit. These pin the fix: a fled foe recovers in full, and
// the loop stalls on a boss that is always at full health.

#[test]
fn stun_and_flee_cannot_take_the_last_crown_at_l32() {
    let mut arena = Arena::new();
    let mut r = recipe(
        Class::Rogue,
        32,
        Gear::ShopBest,
        Companion::None,
        Coat::None,
        Policy::StunAndFlee,
    );
    r.potions = 0;
    let r = arena.fight(r, ASCENDANT);
    assert_ne!(r.outcome, Outcome::Won, "{r:?}");
}

#[test]
fn hit_and_run_cannot_take_the_king_at_l32() {
    // Hit, flee, heal for free at a fountain, walk back: with the foe's
    // wounds gone the moment you run, the trip buys nothing.
    let mut arena = Arena::new();
    let mut r = recipe(
        Class::Cleric,
        32,
        Gear::ShopBest,
        Companion::None,
        Coat::None,
        Policy::HitAndRun,
    );
    r.potions = 0;
    let r = arena.fight(r, KING);
    assert_ne!(r.outcome, Outcome::Won, "{r:?}");
}

// ---- The report ------------------------------------------------------------

/// Level and gear a character would plausibly bring at each step of the road:
/// the rarity-capped kits of the crafting tiers (`Gear::Kit`), then the
/// generated sets of each endgame land.
const LADDER: [(i32, Gear); 8] = [
    (10, Gear::Kit(0)),
    (20, Gear::Kit(1)),
    (32, Gear::Kit(3)),
    (40, Gear::Kit(4)),
    (50, Gear::Frontier(9)),
    (60, Gear::Frontier(19)),
    (75, Gear::Reaches(19)),
    (100, Gear::Kaelmyr(19)),
];

fn ladder_table(
    arena: &mut Arena,
    out: &mut String,
    foe: &str,
    companion: Companion,
    coat: Coat,
    policy: Policy,
) {
    let _ = write!(out, "| class |");
    for (level, gear) in LADDER {
        let _ = write!(out, " L{level} {} |", gear.label());
    }
    let _ = writeln!(out);
    let _ = write!(out, "|---|");
    for _ in LADDER {
        let _ = write!(out, "---|");
    }
    let _ = writeln!(out);
    for class in Class::ALL {
        // One row per path; below ARCHETYPE_LEVEL both rows are the same
        // pathless character, so the second path's cell reads "-".
        for (pi, arch) in archetypes_for(class).into_iter().enumerate() {
            let _ = write!(out, "| {class:?} · {} |", arch.name);
            for (level, gear) in LADDER {
                if level < ARCHETYPE_LEVEL && pi > 0 {
                    let _ = write!(out, " - |");
                    continue;
                }
                let coat = if level < 26 && coat != Coat::None {
                    // Tier-5 oils sit behind Alchemy gates no L10-20 character has.
                    Coat::BestOil(0)
                } else {
                    coat
                };
                let path = (level >= ARCHETYPE_LEVEL).then_some(arch);
                let r = arena.fight(
                    recipe_on(class, path, level, gear, companion, coat, policy),
                    foe,
                );
                let _ = write!(out, " {} |", r.cell());
            }
            let _ = writeln!(out);
        }
    }
}

/// Where a report lands: `LATEANIA_ARENA_REPORT` names the main file and the
/// extra file sits beside it; by default both go under the crate's `target/`.
fn report_path(extra: bool) -> std::path::PathBuf {
    let main = match std::env::var("LATEANIA_ARENA_REPORT") {
        Ok(p) => std::path::PathBuf::from(p),
        Err(_) => std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("lateania-arena.md"),
    };
    if !extra {
        return main;
    }
    let stem = main
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("lateania-arena");
    main.with_file_name(format!("{stem}-extra.md"))
}

fn write_report(path: &std::path::Path, out: &str) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).expect("report dir");
    }
    std::fs::write(path, out).expect("write report");
    eprintln!("[arena] report written to {}", path.display());
}

/// The Long Road tables alone fill most of nextest's per-test budget, so the
/// report is two tests: this one (the road) and `arena_report_extra` (the
/// composition, the dps yardstick, the exploits, the boss roster).
#[test]
#[ignore = "writes the balance report, part 1: the Long Road (LATEANIA_ARENA_REPORT overrides the path)"]
fn arena_report() {
    let started = Instant::now();
    let mut arena = Arena::new();
    let mut out = String::new();
    let _ = writeln!(out, "# Lateania arena report");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Real engine, one fresh character per fight, scores at flat 10s, clock pinned to a clear day. \
         Honest policy: auto-attack, rotate the roster by value, drink a level-appropriate draught \
         (3 in the bag) under {DRINK_UNDER_PCT}% health. Cell: `outcome ticks hp-left% potions-drunk auto/ability/dot/coat/pet` \
         with shares in percent of damage dealt. W won, D died, S stalemate after {HONEST_MAX_TICKS} ticks, E foe fled."
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "One row per class and archetype path (the L10 column is pathless, so its second row reads `-`)."
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Gear presets: kitN = the smithed weapon and plate of crafting tier N plus the best authored piece per other slot under the tier's rarity cap; frontN/reachN/kaelN = the full 8-piece generated set of that zone tier."
    );

    let road: Vec<&'static str> = LONG_ROAD.iter().map(|m| m.boss).collect();

    let _ = writeln!(out);
    let _ = writeln!(out, "## The Long Road, honest, no companion, no coat");
    for foe in &road {
        let _ = writeln!(out);
        let _ = writeln!(out, "### {}", arena.foe(foe).label());
        let _ = writeln!(out);
        ladder_table(
            &mut arena,
            &mut out,
            foe,
            Companion::None,
            Coat::None,
            Policy::Honest,
        );
        eprintln!("[arena] {foe}: bare table done at {:?}", started.elapsed());
    }

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "## The Long Road, honest, maxed tame + best oil (tier 5, tier 1 under L26)"
    );
    for foe in &road {
        let _ = writeln!(out);
        let _ = writeln!(out, "### {}", arena.foe(foe).label());
        let _ = writeln!(out);
        ladder_table(
            &mut arena,
            &mut out,
            foe,
            Companion::TameBest,
            Coat::BestOil(4),
            Policy::Honest,
        );
        eprintln!(
            "[arena] {foe}: geared table done at {:?}",
            started.elapsed()
        );
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "## The ceiling: routed, maxed tame + best oil");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "The geared character above, but reading the foe: every offensive pick is weighed by the crown's resist/weak multiplier for its school. What a player who looks at the traits line gets."
    );
    for foe in &road {
        let _ = writeln!(out);
        let _ = writeln!(out, "### {}", arena.foe(foe).label());
        let _ = writeln!(out);
        ladder_table(
            &mut arena,
            &mut out,
            foe,
            Companion::TameBest,
            Coat::BestOil(4),
            Policy::Routed,
        );
        eprintln!(
            "[arena] {foe}: routed table done at {:?}",
            started.elapsed()
        );
    }

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Generated in {:?}. Part 2 (composition, dps yardstick, exploits, roster) is the `-extra` file beside this one.",
        started.elapsed()
    );
    write_report(&report_path(false), &out);
}

#[test]
#[ignore = "writes the balance report, part 2: composition, dps yardstick, exploits, roster"]
fn arena_report_extra() {
    let started = Instant::now();
    let mut arena = Arena::new();
    let mut out = String::new();
    let _ = writeln!(out, "# Lateania arena report, part 2");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Same rules as part 1 (real engine, fresh character per fight, neutral scores unless the table says otherwise, clear day; cell = `outcome ticks hp-left% potions-drunk auto/ability/dot/coat/pet`)."
    );
    let road: Vec<&'static str> = LONG_ROAD.iter().map(|m| m.boss).collect();
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "## Where the damage comes from: L55, Frontier tier 10, vs {ARCHDEMON}"
    );
    let _ = writeln!(out);
    let kits = [
        (Companion::None, Coat::None),
        (Companion::None, Coat::BestOil(4)),
        (Companion::None, Coat::Poison(4)),
        (Companion::ShopBest, Coat::None),
        (Companion::TameBest, Coat::None),
        (Companion::TameBest, Coat::BestOil(4)),
    ];
    for (companion, coat) in kits {
        let _ = writeln!(
            out,
            "- {}",
            recipe(
                Class::Rogue,
                55,
                Gear::Frontier(9),
                companion,
                coat,
                Policy::Honest
            )
            .label()
        );
    }
    let _ = writeln!(out);
    let _ = write!(out, "| class | attack | swing | spell | max hp |");
    for (companion, coat) in kits {
        let _ = write!(out, " {} {} |", companion.label(), coat.label());
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "|---|---|---|---|---|---|---|---|---|---|---|");
    for class in Class::ALL {
        for arch in archetypes_for(class) {
            let mut row = String::new();
            let mut sheet = (0, 0, 0, 0);
            for (companion, coat) in kits {
                let r = arena.fight(
                    recipe_on(
                        class,
                        Some(arch),
                        55,
                        Gear::Frontier(9),
                        companion,
                        coat,
                        Policy::Honest,
                    ),
                    ARCHDEMON,
                );
                sheet = (r.attack, r.swing, r.spell_power, r.max_hp);
                let _ = write!(row, " {} |", r.cell());
            }
            let _ = writeln!(
                out,
                "| {class:?} · {} | {} | {} | {} | {} |{row}",
                arch.name, sheet.0, sheet.1, sheet.2, sheet.3
            );
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "## Honest vs routed: L55 Frontier-10, bare, vs {KING} (weak to shadow)"
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Damage per tick over the fight, and the outcome. The routed column is the school-game ceiling; the gap is what reading the foe is worth to that path."
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "| path | honest dps | routed dps | gain | honest | routed |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|");
    for class in Class::ALL {
        for arch in archetypes_for(class) {
            let h = arena.fight(
                recipe_on(
                    class,
                    Some(arch),
                    55,
                    Gear::Frontier(9),
                    Companion::None,
                    Coat::None,
                    Policy::Honest,
                ),
                KING,
            );
            let r = arena.fight(
                recipe_on(
                    class,
                    Some(arch),
                    55,
                    Gear::Frontier(9),
                    Companion::None,
                    Coat::None,
                    Policy::Routed,
                ),
                KING,
            );
            let dps = |f: &FightResult| f.dealt.total() / f.ticks.max(1) as i32;
            let (hd, rd) = (dps(&h), dps(&r));
            let _ = writeln!(
                out,
                "| {class:?} · {} | {hd} | {rd} | {:+}% | {} | {} |",
                arch.name,
                (rd - hd) * 100 / hd.max(1),
                h.cell(),
                r.cell()
            );
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "## Damage per tick, {DPS_TICKS} ticks on the neutral training dummy, bare"
    );
    let _ = writeln!(out);
    let _ = write!(out, "| class |");
    for (level, gear) in dps_ladder() {
        let _ = write!(out, " L{level} {} |", gear.label());
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "|---|---|---|---|---|---|");
    for class in Class::ALL {
        for arch in archetypes_for(class) {
            let _ = write!(out, "| {class:?} · {} |", arch.name);
            for (level, gear) in dps_ladder() {
                let r = recipe_on(
                    class,
                    Some(arch),
                    level,
                    gear,
                    Companion::None,
                    Coat::None,
                    Policy::Honest,
                );
                let d = arena.measure(r, DPS_TICKS);
                let _ = write!(out, " {} {} |", d.total() / DPS_TICKS as i32, d.shares());
            }
            let _ = writeln!(out);
        }
    }

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "## The builds: {BUILD_TICKS} ticks on the neutral dummy, bare, damage path (dps vs neutral, max hp vs neutral)"
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Peak = 18 in that score, 10 elsewhere. Focused = 20 in the class primary and 20 CON. Blessed = all 18, cursed = all 3. Glass cannon = STR/DEX/INT 20 and the rest 3; tortoise = CON/WIS 20 and the rest 3; merchant = CHA 20 and the rest 3."
    );
    let _ = writeln!(out);
    let _ = write!(out, "{}", build_table(&mut arena));

    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "## The exploits: L32, shop gear, no potions, vs {ARCHDEMON}"
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Stun-and-flee needs a stun in the roster; hit-and-run needs nothing. `taken` is net health lost over the whole fight."
    );
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "| class | stun-and-flee | taken | hit-and-run | taken |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|");
    for class in Class::ALL {
        let stun = if has_stun(class, 32) {
            let mut r = recipe(
                class,
                32,
                Gear::ShopBest,
                Companion::None,
                Coat::None,
                Policy::StunAndFlee,
            );
            r.potions = 0;
            let r = arena.fight(r, ARCHDEMON);
            format!("{} {}t | {}", r.outcome.glyph(), r.ticks, r.taken)
        } else {
            "no stun | -".to_string()
        };
        let mut run = recipe(
            class,
            32,
            Gear::ShopBest,
            Companion::None,
            Coat::None,
            Policy::HitAndRun,
        );
        run.potions = 0;
        let run = arena.fight(run, ARCHDEMON);
        let _ = writeln!(
            out,
            "| {class:?} | {stun} | {} {}t | {} |",
            run.outcome.glyph(),
            run.ticks,
            run.taken
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "### The L32 Rogue against every crown");
    let _ = writeln!(out);
    let _ = writeln!(out, "| foe | stun-and-flee | taken | hit-and-run | taken |");
    let _ = writeln!(out, "|---|---|---|---|---|");
    for foe in &road {
        let mut stun = recipe(
            Class::Rogue,
            32,
            Gear::ShopBest,
            Companion::None,
            Coat::None,
            Policy::StunAndFlee,
        );
        stun.potions = 0;
        let mut run = recipe(
            Class::Rogue,
            32,
            Gear::ShopBest,
            Companion::None,
            Coat::None,
            Policy::HitAndRun,
        );
        run.potions = 0;
        let a = arena.fight(stun, foe);
        let b = arena.fight(run, foe);
        let _ = writeln!(
            out,
            "| {foe} | {} {}t | {} | {} {}t | {} |",
            a.outcome.glyph(),
            a.ticks,
            a.taken,
            b.outcome.glyph(),
            b.ticks,
            b.taken
        );
    }

    let _ = writeln!(out);
    let _ = writeln!(out, "## Every boss as the engine fields it");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "| boss | level | hp | dmg | strikes | weak | resists |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|");
    for name in arena.bosses() {
        let f = arena.foe(name);
        let school = |s: Option<_>| {
            s.map(|d: super::super::super::damage::DamageType| d.label())
                .unwrap_or("-")
        };
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} |",
            f.name,
            f.level,
            f.max_hp,
            f.damage,
            f.attack_type.label(),
            school(f.weak),
            school(f.resist)
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "Generated in {:?}.", started.elapsed());
    write_report(&report_path(true), &out);
}

// ---- The builds --------------------------------------------------------------
//
// Every yardstick above runs the neutral build. These measure what the scores
// do to the same character: one peak per score, the way players will really
// spend (`Focused`), and the strange shapes, on a long window so a crit
// build's dice average out.

/// Crits are dice; 200 swings put a 10% crit chance within a point or two.
const BUILD_TICKS: u32 = 200;

/// The callings the build yardstick reads: one martial, one crit-leaning
/// martial, one caster, one resource-bound healer, at the Frontier's kit.
const BUILD_STEPS: [(Class, i32, Gear); 4] = [
    (Class::Warrior, 55, Gear::Frontier(9)),
    (Class::Rogue, 55, Gear::Frontier(9)),
    (Class::Mage, 55, Gear::Frontier(9)),
    (Class::Cleric, 55, Gear::Frontier(9)),
];

const BUILDS: [Build; 13] = [
    Build::Neutral,
    Build::Peak(Score::Strength),
    Build::Peak(Score::Dexterity),
    Build::Peak(Score::Constitution),
    Build::Peak(Score::Intelligence),
    Build::Peak(Score::Wisdom),
    Build::Peak(Score::Charisma),
    Build::Focused,
    Build::Blessed,
    Build::Cursed,
    Build::GlassCannon,
    Build::Tortoise,
    Build::Merchant,
];

/// The build table: dps per build as a percent of the neutral build, the
/// sheet's max hp beside it. Shared by the yardstick and the report.
fn build_table(arena: &mut Arena) -> String {
    let mut out = String::new();
    let _ = write!(out, "| build |");
    for (class, level, gear) in BUILD_STEPS {
        let _ = write!(out, " {class:?} L{level} {} |", gear.label());
    }
    let _ = writeln!(out);
    let neutral: Vec<(i32, i32)> = BUILD_STEPS
        .iter()
        .map(|&(class, level, gear)| {
            let r = built(class, level, gear, Build::Neutral);
            let hp = arena.sheet(r).3;
            (arena.measure_dps(r, BUILD_TICKS), hp)
        })
        .collect();
    for build in BUILDS {
        let _ = write!(out, "| {} |", build.label());
        for (i, &(class, level, gear)) in BUILD_STEPS.iter().enumerate() {
            let r = built(class, level, gear, build);
            let dps = arena.measure_dps(r, BUILD_TICKS);
            let hp = arena.sheet(r).3;
            let (n_dps, n_hp) = neutral[i];
            let _ = write!(
                out,
                " {dps} ({:+}%) hp {hp} ({:+}%) |",
                dps * 100 / n_dps.max(1) - 100,
                hp * 100 / n_hp.max(1) - 100
            );
        }
        let _ = writeln!(out);
    }
    out
}

#[test]
#[ignore = "a yardstick print for the tuning loop"]
fn arena_build_table() {
    let mut arena = Arena::new();
    eprintln!(
        "[arena] builds, {BUILD_TICKS}-tick dps on the neutral dummy, bare, damage path (dps vs neutral, max hp vs neutral):\n{}",
        build_table(&mut arena)
    );
}

/// Percent change of `dps` against `neutral`.
fn pct_vs(dps: i32, neutral: i32) -> i32 {
    dps * 100 / neutral.max(1) - 100
}

#[test]
fn a_peak_score_moves_its_own_axis_and_nothing_else() {
    // Deterministic hooks only (a crit build is dice, see the next contract):
    // CHA changes no fight at all, CON changes the pool and not the pace,
    // and the damage scores land in the band a lucky 18 was designed to be
    // worth, a share of the fight and never the fight.
    let mut arena = Arena::new();
    let mut wrong: Vec<String> = Vec::new();
    for (class, level, gear) in BUILD_STEPS {
        let n = built(class, level, gear, Build::Neutral);
        let n_dps = arena.measure_dps(n, DPS_TICKS);
        let n_hp = arena.sheet(n).3;
        let cha = built(class, level, gear, Build::Peak(Score::Charisma));
        if arena.measure_dps(cha, DPS_TICKS) != n_dps || arena.sheet(cha).3 != n_hp {
            wrong.push(format!("{class:?}: CHA changed the fight"));
        }
        let con = built(class, level, gear, Build::Peak(Score::Constitution));
        let con_hp = pct_vs(arena.sheet(con).3, n_hp);
        let con_dps = pct_vs(arena.measure_dps(con, DPS_TICKS), n_dps);
        if !(8..=14).contains(&con_hp) || con_dps.abs() > 2 {
            wrong.push(format!("{class:?}: CON hp {con_hp:+}% dps {con_dps:+}%"));
        }
    }
    // Regen only tells once the pool has drained, so the Wisdom axis reads
    // on a window long enough to run a Cleric dry (`REGEN_TICKS`).
    let axes = [
        (Class::Warrior, Score::Strength, DPS_TICKS, 3..=10),
        (Class::Rogue, Score::Strength, DPS_TICKS, 3..=10),
        (Class::Mage, Score::Intelligence, DPS_TICKS, 2..=8),
        (Class::Mage, Score::Wisdom, REGEN_TICKS, 5..=20),
        (Class::Cleric, Score::Wisdom, REGEN_TICKS, 3..=16),
    ];
    for (class, score, ticks, band) in axes {
        let n = arena.measure_dps(built(class, 55, Gear::Frontier(9), Build::Neutral), ticks);
        let peak = arena.measure_dps(
            built(class, 55, Gear::Frontier(9), Build::Peak(score)),
            ticks,
        );
        let gain = pct_vs(peak, n);
        if !band.contains(&gain) {
            wrong.push(format!(
                "{class:?} peak {}: {gain:+}% (want {band:?})",
                score.label()
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "a peak score is out of its band: {wrong:?}"
    );
}

/// Long enough to run a Cleric's pool dry, so Wisdom's regen is in the number.
const REGEN_TICKS: u32 = 100;

/// Crits are dice: this many swings put an 8% crit chance's gain within a
/// few points of its expectation, and make a zero-crit window impossible.
const CRIT_TICKS: u32 = 150;

#[test]
fn a_crit_build_lands_its_share_over_a_long_window() {
    let mut arena = Arena::new();
    let n = arena.measure_dps(
        built(Class::Rogue, 55, Gear::Frontier(9), Build::Neutral),
        CRIT_TICKS,
    );
    let dex = arena.measure_dps(
        built(
            Class::Rogue,
            55,
            Gear::Frontier(9),
            Build::Peak(Score::Dexterity),
        ),
        CRIT_TICKS,
    );
    let gain = pct_vs(dex, n);
    assert!(
        (1..=12).contains(&gain),
        "an 18 DEX Rogue should out-damage a neutral one by a few percent, not {gain:+}%"
    );
}

/// What the whole roll may be worth, every score going one way: less than a
/// kit tier, more than a rounding error. Blessed reads +17..+24% on the
/// 200-tick yardstick and Cursed -13..-17%; the bands leave room for the
/// crit and glance dice of a short window.
const BLESSED_GAIN: std::ops::RangeInclusive<i32> = 5..=40;
const CURSED_LOSS: std::ops::RangeInclusive<i32> = -30..=-3;

#[test]
fn the_whole_roll_is_a_share_of_the_fight_not_the_fight() {
    let mut arena = Arena::new();
    let mut wrong: Vec<String> = Vec::new();
    for (class, level, gear) in BUILD_STEPS {
        let n = built(class, level, gear, Build::Neutral);
        let n_dps = arena.measure_dps(n, DPS_TICKS);
        let n_hp = arena.sheet(n).3;
        let blessed = built(class, level, gear, Build::Blessed);
        let b_gain = pct_vs(arena.measure_dps(blessed, DPS_TICKS), n_dps);
        if !BLESSED_GAIN.contains(&b_gain) || arena.sheet(blessed).3 <= n_hp {
            wrong.push(format!("{class:?} blessed: {b_gain:+}%"));
        }
        let cursed = built(class, level, gear, Build::Cursed);
        let c_gain = pct_vs(arena.measure_dps(cursed, DPS_TICKS), n_dps);
        if !CURSED_LOSS.contains(&c_gain) || arena.sheet(cursed).3 >= n_hp {
            wrong.push(format!("{class:?} cursed: {c_gain:+}%"));
        }
    }
    assert!(
        wrong.is_empty(),
        "the roll is worth the wrong amount: {wrong:?}"
    );
}

#[test]
fn the_strange_builds_trade_what_they_say_they_trade() {
    // Glass cannon: more damage, less body. Tortoise: more body, no more
    // damage on a martial (a caster's Wisdom does buy casts, so it may edge
    // up). Merchant: a worse fighter on every axis, richer at the counter
    // (the counter is pinned in svc_test, the arena has no shop).
    let mut arena = Arena::new();
    let mut wrong: Vec<String> = Vec::new();
    for (class, level, gear) in BUILD_STEPS {
        let n = built(class, level, gear, Build::Neutral);
        let n_dps = arena.measure_dps(n, DPS_TICKS);
        let n_hp = arena.sheet(n).3;
        let glass = built(class, level, gear, Build::GlassCannon);
        let g_hp = arena.sheet(glass).3;
        if g_hp >= n_hp {
            wrong.push(format!("{class:?} glass cannon kept its body"));
        }
        if matches!(class, Class::Warrior | Class::Rogue)
            && arena.measure_dps(glass, DPS_TICKS) <= n_dps
        {
            wrong.push(format!("{class:?} glass cannon lost its edge"));
        }
        let tortoise = built(class, level, gear, Build::Tortoise);
        let t_dps = pct_vs(arena.measure_dps(tortoise, DPS_TICKS), n_dps);
        if arena.sheet(tortoise).3 <= n_hp || t_dps > 10 {
            wrong.push(format!("{class:?} tortoise: dps {t_dps:+}%"));
        }
        let merchant = built(class, level, gear, Build::Merchant);
        if arena.measure_dps(merchant, DPS_TICKS) >= n_dps || arena.sheet(merchant).3 >= n_hp {
            wrong.push(format!("{class:?} merchant fights as well as anyone"));
        }
    }
    assert!(
        wrong.is_empty(),
        "a strange build is not the trade it claims: {wrong:?}"
    );
}
