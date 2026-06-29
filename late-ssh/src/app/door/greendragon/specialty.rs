//! The twelve specialty combat skills.
//!
//! Mechanics (use-costs, durations, buff parameters) are transcribed 1=1 from
//! LoGD's three specialty modules — `specialtymysticpower`, `specialtydarkarts`,
//! `specialtythiefskills` — which are pure numbers and so uncopyrightable. The
//! skill **names and flavor are original to late.sh**; no module prose is
//! copied. Each skill spends "uses" from the per-day pool (see
//! [`super::model::Character::spend_specialty_uses`]) and applies a
//! [`Buff`] resolved by [`super::combat::resolve_round_buffed`].

use super::combat::Buff;
use super::model::Specialty;

/// Integer rounding matching PHP's `round()` (half away from zero, positive
/// inputs here so half-up), used by the level/attack-scaled skills.
fn iround(x: f32) -> u32 {
    x.round() as u32
}

/// One castable specialty skill: a label, its use-cost, and a builder for the
/// buff it applies (scaled by the player's level and attack at cast time).
pub struct Skill {
    pub name: &'static str,
    pub cost: u32,
    build: fn(level: u32, attack: u32) -> Buff,
}

impl Skill {
    /// The buff this skill applies, given the caster's current level and attack.
    pub fn buff(&self, level: u32, attack: u32) -> Buff {
        (self.build)(level, attack)
    }
}

/// The skills available to `specialty`, in unlock order (cheapest first). Empty
/// for [`Specialty::None`].
pub fn skills(specialty: Specialty) -> &'static [Skill] {
    match specialty {
        Specialty::None => &[],
        Specialty::Mystical => MYSTICAL,
        Specialty::DarkArts => DARK_ARTS,
        Specialty::Thief => THIEF,
    }
}

// ── Mystical Powers (LoGD `MP`) ─────────────────────────────────────────────

const MYSTICAL: &[Skill] = &[
    Skill {
        name: "Mending Flow",
        cost: 1,
        build: |level, _atk| {
            let mut b = Buff::new("Mending Flow", 5);
            b.regen = level;
            b.wearoff = "the mending current ebbs away.".into();
            b
        },
    },
    Skill {
        name: "Stonefist",
        cost: 2,
        build: |level, _atk| {
            let mut b = Buff::new("Stonefist", 5);
            b.minion_count = 1;
            b.minion_min = 1;
            b.minion_max = level * 3;
            b.round_msg = Some("a fist of living rock hammers your foe.".into());
            b.wearoff = "the stone fist crumbles back to gravel.".into();
            b
        },
    },
    Skill {
        name: "Lifedrink",
        cost: 3,
        build: |_level, _atk| {
            let mut b = Buff::new("Lifedrink", 5);
            b.lifetap = 1.0;
            b.round_msg = Some("your blade drinks deep and your wounds knit closed.".into());
            b.wearoff = "your weapon's thirst is sated.".into();
            b
        },
    },
    Skill {
        name: "Stormskin",
        cost: 5,
        build: |_level, _atk| {
            let mut b = Buff::new("Stormskin", 5);
            b.damage_shield = 2.0;
            b.round_msg = Some("lightning arcs off your skin into your attacker.".into());
            b.wearoff = "the crackling aura earths out and fades.".into();
            b
        },
    },
];

// ── Dark Arts (LoGD `DA`) ───────────────────────────────────────────────────

const DARK_ARTS: &[Skill] = &[
    Skill {
        name: "Bonecall",
        cost: 1,
        build: |level, _atk| {
            let mut b = Buff::new("Bonecall", 5);
            b.minion_count = iround(level as f32 / 3.0) + 1;
            b.minion_min = 0;
            b.minion_max = iround(level as f32 / 2.0) + 1;
            b.round_msg = Some("your risen bonewalkers tear at the enemy.".into());
            b.wearoff = "your skeletons collapse into loose bones.".into();
            b
        },
    },
    Skill {
        name: "Effigy",
        cost: 2,
        build: |_level, attack| {
            // One vicious one-round strike scaled by your attack.
            let mut b = Buff::new("Effigy", 1);
            b.minion_count = 1;
            b.minion_min = iround(attack as f32 * 1.5);
            b.minion_max = iround(attack as f32 * 3.0);
            b.round_msg = Some("you drive a needle into the effigy and your foe convulses.".into());
            b
        },
    },
    Skill {
        name: "Hexweight",
        cost: 3,
        build: |_level, _atk| {
            let mut b = Buff::new("Hexweight", 5);
            b.enemy_dmg_mod = 0.5;
            b.round_msg = Some("your foe sags under the hex and strikes at half force.".into());
            b.wearoff = "the hex lifts from your enemy.".into();
            b
        },
    },
    Skill {
        name: "Soulwither",
        cost: 5,
        build: |_level, _atk| {
            let mut b = Buff::new("Soulwither", 5);
            b.enemy_atk_mod = 0.0;
            b.enemy_def_mod = 0.0;
            b.round_msg = Some("your foe claws at its own withering soul, unable to fight.".into());
            b.wearoff = "your enemy's soul settles back into its body.".into();
            b
        },
    },
];

// ── Thief Skills (LoGD `TS`) ────────────────────────────────────────────────

const THIEF: &[Skill] = &[
    Skill {
        name: "Taunt",
        cost: 1,
        build: |_level, _atk| {
            let mut b = Buff::new("Taunt", 5);
            b.enemy_atk_mod = 0.5;
            b.round_msg = Some("stung by your jeering, your foe swings half-heartedly.".into());
            b.wearoff = "your foe shakes off the insult.".into();
            b
        },
    },
    Skill {
        name: "Venom Edge",
        cost: 2,
        build: |_level, _atk| {
            let mut b = Buff::new("Venom Edge", 5);
            b.player_atk_mod = 2.0;
            b.round_msg = Some("venom on your blade doubles every cut.".into());
            b.wearoff = "the last of the venom dries on your blade.".into();
            b
        },
    },
    Skill {
        name: "Vanish",
        cost: 3,
        build: |_level, _atk| {
            let mut b = Buff::new("Vanish", 5);
            b.enemy_atk_mod = 0.0;
            b.round_msg = Some("you melt into shadow; your foe swings at empty air.".into());
            b.wearoff = "you step back into plain sight.".into();
            b
        },
    },
    Skill {
        name: "Shadowstrike",
        cost: 5,
        build: |_level, _atk| {
            let mut b = Buff::new("Shadowstrike", 5);
            b.player_atk_mod = 3.0;
            b.player_def_mod = 3.0;
            b.round_msg = Some("striking from the blind side, you hit harder and guard tighter.".into());
            b.wearoff = "your advantage of surprise is spent.".into();
            b
        },
    },
];
