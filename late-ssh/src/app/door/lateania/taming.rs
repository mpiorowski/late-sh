// The Animal Taming trade for Lateania.
//
// Broceliande, the Greenwood, is home to fifty wild beasts a beastmaster can
// tame into a true combat companion. This module holds:
//
//   * `TAMEABLE` - the fifty tameable species (as `PetSpecies`, ordered small ->
//     large), each with a rising `tame_level` so the trade gets harder and
//     harder; the biggest beasts need a near-max Animal Taming skill.
//   * `WILD_BEASTS` - where each beast roams, keyed to Broceliande rooms exactly
//     like `world::WILDLIFE` / `world::NODES` (static data + a per-beast service
//     cooldown on a failed tame).
//   * The taming success mechanic (`tame_chance`), driven by how far the tamer's
//     Animal Taming level exceeds the beast's required level.
//   * The pet **auto-skills** - abilities keyed to a pet's level (by size class)
//     that fire automatically in the combat round (see `svc.rs`).
//
// The world wiring (the taming action, the panel, and the pet auto-skill combat
// step) lives in `svc.rs` / `state.rs` / `ui.rs`; only the data and the pure
// maths live here.

use super::pets::PetSpecies;
use super::skills::{TamingSkill, skill_level_for_xp};
use super::world::{BROCELIANDE_BASE, BROCELIANDE_ZONE_COUNT, BROCELIANDE_ZONE_STRIDE, RoomId};

/// The fifty tameable beasts of Broceliande, ordered smallest to largest. The
/// `tame_level` climbs from 1 to 50 across the list, so early beasts fall to a
/// novice and the great forest wyrm needs a near-master tamer. Health and attack
/// scale with size, so a bigger beast is a stronger companion. `price` is unused
/// for tameables (they are earned, not bought).
///
/// KEYS ARE PERSISTED - never reorder or rename an existing key.
pub const TAMEABLE: &[PetSpecies] = &[
    beast(
        "wt_hare",
        "Greenwood Hare",
        "\u{1F407}",
        1,
        34,
        5,
        "a quick brown hare of the forest eaves",
    ),
    beast(
        "wt_hedgehog",
        "Bristleback Hedgehog",
        "\u{1F994}",
        2,
        40,
        5,
        "a spiny little forager, all quills and courage",
    ),
    beast(
        "wt_squirrel",
        "Red Pine-Squirrel",
        "\u{1F43F}",
        3,
        30,
        6,
        "a darting red squirrel of the oak canopy",
    ),
    beast(
        "wt_ferret",
        "Fen Ferret",
        "\u{1F9A6}",
        4,
        38,
        7,
        "a sinuous ferret that hunts the reed-roots",
    ),
    beast(
        "wt_pinemarten",
        "Pine Marten",
        "\u{1F43E}",
        5,
        44,
        8,
        "a bold marten with a hunter's bright eyes",
    ),
    beast(
        "wt_wildcat",
        "Green-Eyed Wildcat",
        "\u{1F408}",
        6,
        52,
        9,
        "a lean forest wildcat, half shadow",
    ),
    beast(
        "wt_foxred",
        "Briar Fox",
        "\u{1F98A}",
        7,
        50,
        10,
        "a russet fox that knows every run of the thicket",
    ),
    beast(
        "wt_badger",
        "Grove Badger",
        "\u{1F9A1}",
        8,
        70,
        9,
        "a stout badger, slow to anger and hard to stop",
    ),
    beast(
        "wt_owl",
        "Moonshadow Owl",
        "\u{1F989}",
        9,
        46,
        12,
        "a silent owl that strikes from the dark",
    ),
    beast(
        "wt_hawk",
        "Green Goshawk",
        "\u{1F985}",
        10,
        48,
        13,
        "a fierce goshawk of the forest clearings",
    ),
    beast(
        "wt_lynx",
        "Fernlight Lynx",
        "\u{1F408}",
        12,
        74,
        12,
        "a tuft-eared lynx that stalks the fern",
    ),
    beast(
        "wt_boar",
        "Forest Boar",
        "\u{1F417}",
        14,
        96,
        12,
        "a bristling boar with tusks like sabres",
    ),
    beast(
        "wt_stag",
        "Moss-Antler Stag",
        "\u{1F98C}",
        16,
        88,
        14,
        "a great stag crowned in moss-hung antler",
    ),
    beast(
        "wt_wolf",
        "Greenwood Wolf",
        "\u{1F43A}",
        18,
        100,
        15,
        "a grey wolf of the deep wood, patient and deadly",
    ),
    beast(
        "wt_panther",
        "Shadow Panther",
        "\u{1F406}",
        20,
        108,
        17,
        "a black panther that flows through the ruins",
    ),
    beast(
        "wt_boarking",
        "Tuskgore Boar-King",
        "\u{1F417}",
        22,
        138,
        15,
        "a monstrous boar-king, scarred and unkillable",
    ),
    beast(
        "wt_direwolf",
        "Direwolf",
        "\u{1F43A}",
        24,
        132,
        18,
        "a horse-high direwolf that leads the pack",
    ),
    beast(
        "wt_cavebear",
        "Barrow Cave-Bear",
        "\u{1F43B}",
        26,
        176,
        16,
        "a shaggy cave-bear roused from the barrows",
    ),
    beast(
        "wt_adder",
        "Great Fen-Adder",
        "\u{1F40D}",
        28,
        118,
        20,
        "a venom-fanged adder longer than a spear",
    ),
    beast(
        "wt_constrictor",
        "Jungle Constrictor",
        "\u{1F40D}",
        30,
        150,
        18,
        "a green constrictor that drops from the boughs",
    ),
    beast(
        "wt_wisp",
        "Moor-Wisp",
        "\u{1F526}",
        32,
        96,
        24,
        "a cold drifting wisp bound to a tamer's will",
    ),
    beast(
        "wt_direboar",
        "Thornhide Direboar",
        "\u{1F417}",
        33,
        196,
        19,
        "a thorn-armoured direboar of the deep briar",
    ),
    beast(
        "wt_greatstag",
        "Cernun Great-Stag",
        "\u{1F98C}",
        35,
        170,
        22,
        "a vast stag of the Horned One's own herd",
    ),
    beast(
        "wt_direpanther",
        "Ruin Dire-Panther",
        "\u{1F406}",
        36,
        188,
        23,
        "a dire-panther that haunts the ivy-halls",
    ),
    beast(
        "wt_wildboar_king",
        "Greenmoor Tusker",
        "\u{1F417}",
        38,
        228,
        20,
        "the great tusker whose charge fells oaks",
    ),
    beast(
        "wt_direbear",
        "Greenmantle Dire-Bear",
        "\u{1F43B}",
        40,
        260,
        22,
        "a dire-bear mantled in moss like a keep",
    ),
    beast(
        "wt_jaguar",
        "Steamwood Jaguar",
        "\u{1F406}",
        41,
        214,
        26,
        "a jungle jaguar, fever-fast and merciless",
    ),
    beast(
        "wt_drake",
        "Fernwyrm Drakeling",
        "\u{1F432}",
        42,
        240,
        25,
        "a scaled drakeling of the wyrm-fern hollows",
    ),
    beast(
        "wt_hunthound",
        "Hound of the Wild Hunt",
        "\u{1F415}",
        43,
        232,
        27,
        "a spectral hunt-hound with eyes like coals",
    ),
    beast(
        "wt_wisent",
        "Green Wisent",
        "\u{1F9AC}",
        44,
        300,
        21,
        "a mountainous wisent, a wall of horn and muscle",
    ),
    beast(
        "wt_direwyrm_small",
        "Thornwyrd Serpent",
        "\u{1F40D}",
        45,
        256,
        28,
        "a black maze-serpent that drinks the light",
    ),
    beast(
        "wt_greatdrake",
        "Steaming Jungle-Drake",
        "\u{1F409}",
        46,
        300,
        27,
        "a true drake of the steaming jungle deeps",
    ),
    beast(
        "wt_rootbeast",
        "Worldroot Delver",
        "\u{1F994}",
        46,
        320,
        24,
        "a huge burrowing root-beast of the deep caverns",
    ),
    beast(
        "wt_stormstag",
        "Storm-Crowned Elk",
        "\u{1F98C}",
        47,
        288,
        29,
        "a lightning-antlered elk of the standing kings",
    ),
    beast(
        "wt_direwolf_alpha",
        "Greenwood Alpha",
        "\u{1F43A}",
        47,
        300,
        28,
        "the grey alpha whose howl empties a valley",
    ),
    beast(
        "wt_wyrmling",
        "Fern-Wyrm",
        "\u{1F409}",
        48,
        340,
        28,
        "a young forest-wyrm, coiled and cunning",
    ),
    beast(
        "wt_treantling",
        "Oakheart Treantling",
        "\u{1F333}",
        48,
        400,
        24,
        "a walking oak-child of the Oakheart grove",
    ),
    beast(
        "wt_diredrake",
        "Vine-Choked Dire-Drake",
        "\u{1F409}",
        49,
        360,
        30,
        "a dire-drake wound about with strangler-vine",
    ),
    beast(
        "wt_greatwyrm",
        "Barrowgreen Great-Wyrm",
        "\u{1F409}",
        49,
        380,
        31,
        "a barrow-wyrm risen green from the burial mounds",
    ),
    beast(
        "wt_fae_lord",
        "Erlking's Great Hart",
        "\u{1F98C}",
        49,
        340,
        33,
        "the Erlking's own hart, antlers hung with gold",
    ),
    beast(
        "wt_hunt_master",
        "Cernun Hunt-Beast",
        "\u{1F43A}",
        50,
        360,
        34,
        "the lead beast of the Wild Hunt itself",
    ),
    beast(
        "wt_ruin_wyrm",
        "Greenmantle Guard-Wyrm",
        "\u{1F409}",
        50,
        420,
        30,
        "the coiled wyrm that guards the taken keep",
    ),
    beast(
        "wt_stormwyrm",
        "Storm-Wyrm of the Kings",
        "\u{1F409}",
        50,
        400,
        33,
        "a wyrm crowned in the standing-stones' storm",
    ),
    beast(
        "wt_deepdrake",
        "Worldroot Deep-Drake",
        "\u{1F409}",
        50,
        440,
        32,
        "a pale eyeless drake of the World-Oak's roots",
    ),
    beast(
        "wt_greattreant",
        "Greenmarch Treant",
        "\u{1F332}",
        50,
        520,
        28,
        "a great treant that walks the wood's still heart",
    ),
    beast(
        "wt_heartwyrm",
        "Heart-Oak Wyrm",
        "\u{1F409}",
        50,
        460,
        34,
        "the green wyrm coiled in the Heart-Oak's shade",
    ),
    beast(
        "wt_ancient_drake",
        "Ancient Forest-Drake",
        "\u{1F409}",
        50,
        500,
        36,
        "an ancient drake, old as the first wood",
    ),
    beast(
        "wt_worldtreant",
        "Elder World-Treant",
        "\u{1F332}",
        50,
        620,
        30,
        "an elder treant, a moving hill of ancient oak",
    ),
    beast(
        "wt_greenwyrm",
        "Green Wyrm of the World-Oak",
        "\u{1F409}",
        50,
        560,
        38,
        "a great green wyrm coiled through the World-Oak's crown",
    ),
    beast(
        "wt_worldoak",
        "Scion of the World-Oak",
        "\u{1F333}",
        50,
        700,
        34,
        "a living scion of the World-Oak, oldest and mightiest of all beasts",
    ),
];

/// Number of tameable beasts (the design target is fifty).
pub const TAMEABLE_COUNT: usize = TAMEABLE.len();

/// A `const` constructor for a tameable species (keeps the table readable).
const fn beast(
    key: &'static str,
    name: &'static str,
    glyph: &'static str,
    tame_level: i32,
    base_hp: i32,
    base_attack: i32,
    desc: &'static str,
) -> PetSpecies {
    PetSpecies {
        key,
        name,
        glyph,
        price: 0,
        base_hp,
        base_attack,
        desc,
        tame_level,
    }
}

/// Look up a tameable species by key.
pub fn tameable_by_key(key: &str) -> Option<&'static PetSpecies> {
    TAMEABLE.iter().find(|s| s.key == key)
}

/// A wild beast roaming a specific Broceliande room. Modelled like
/// `world::WILDLIFE` / `world::NODES`: static data keyed to a home room, with a
/// per-spot cooldown after a *failed* tame tracked on the service.
#[derive(Clone, Copy, Debug)]
pub struct WildBeast {
    pub home: RoomId,
    /// Index into `TAMEABLE` of the beast that roams here.
    pub species: usize,
}

/// Every place a tameable beast roams, keyed to a Broceliande room. Built once
/// and cached: each of the fifty beasts is placed in the zone whose depth suits
/// its taming difficulty (small easy beasts near the eaves, the great wyrms in
/// the deep). Beasts gather at that zone's **forest gate** - the entrance room,
/// which is always real and safe (offset 0), so every beast is guaranteed a live
/// home room a tamer can reach and work at in peace. Several beasts share a
/// gate, reading as a menagerie at each woodward-holt.
pub fn wild_beasts() -> &'static [WildBeast] {
    use std::sync::OnceLock;
    static BEASTS: OnceLock<Vec<WildBeast>> = OnceLock::new();
    BEASTS
        .get_or_init(|| {
            // Map each beast onto a zone by its rank (0..50 -> zone 0..N), and
            // home it at that zone's entrance gate (offset 0), which always
            // exists whether the zone was carved as a maze or a sparse cavern.
            let zones = BROCELIANDE_ZONE_COUNT.max(1);
            TAMEABLE
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let zone = (i * zones / TAMEABLE_COUNT).min(zones - 1);
                    let home = BROCELIANDE_BASE + zone as u32 * BROCELIANDE_ZONE_STRIDE;
                    WildBeast { home, species: i }
                })
                .collect()
        })
        .as_slice()
}

/// The tameable beasts roaming a given room (usually zero or one).
pub fn beasts_at(room: RoomId) -> Vec<&'static WildBeast> {
    wild_beasts().iter().filter(|b| b.home == room).collect()
}

/// The success chance (0..=95%) of taming a beast, given the tamer's total
/// Animal Taming xp. Driven by how far the tamer's level exceeds the beast's
/// required level: at the exact required level it is a coin-toss-minus; each
/// level of surplus adds a solid margin; being under-level is refused entirely
/// (returns 0). Capped below certainty so even a master can be thrown.
pub fn tame_chance(taming_xp: i64, beast: &PetSpecies) -> u32 {
    let level = skill_level_for_xp(taming_xp);
    if level < beast.tame_level {
        return 0;
    }
    let surplus = level - beast.tame_level;
    // 40% at exactly the required level, +9% per level of surplus, capped at 95.
    (40 + surplus * 9).clamp(0, 95) as u32
}

/// Xp awarded for a *successful* tame: scales with the beast's difficulty, so
/// taming a great wyrm is worth far more than a hare. Kept generous enough that
/// working up the fifty beasts is a real, rewarding progression on the shared
/// 1..=50 curve.
pub fn tame_xp(beast: &PetSpecies) -> i32 {
    30 + beast.tame_level * beast.tame_level / 2
}

// ---- Pet auto-skills ------------------------------------------------------
//
// A companion (bought or tamed) unlocks abilities as it gains levels, and they
// fire automatically in the combat round on their own cooldowns. The set a pet
// gets is keyed to its **size class** (derived from base health), so a hare
// learns light, quick tricks and a wyrm learns devastating ones - but every pet
// walks the same unlock ladder (L3 / L8 / L15 / L22 / L30) surfaced in the pet
// view so the player sees what is coming.

/// What an auto-skill does when it fires in the combat round. Resolved in
/// `svc.rs` against the existing combat machinery (bonus damage, mob DoTs via
/// `seed_mob_dot`, owner empower, splash mitigation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PetSkillEffect {
    /// A savage bite: bonus physical damage on top of the normal bite.
    SavageBite,
    /// A rend: seeds a bleeding damage-over-time on the foe.
    Rend,
    /// An intimidating roar: empowers the owner's next blows for a few ticks.
    Roar,
    /// A loyal guard: shields the owner for a few ticks (splash mitigation).
    Guard,
    /// A killing pounce: a heavy burst of bonus damage.
    Pounce,
}

/// One unlockable pet auto-skill.
#[derive(Clone, Copy, Debug)]
pub struct PetSkill {
    /// Pet level at which the skill unlocks.
    pub level: i32,
    pub name: &'static str,
    pub effect: PetSkillEffect,
    /// Combat rounds between firings.
    pub cooldown: u8,
    /// Base magnitude (bonus damage / shield / empower / DoT-per-tick); scaled by
    /// the pet's own attack in `svc.rs`.
    pub power: i32,
}

/// The unlock ladder shared by every companion. The five rungs unlock at L3, L8,
/// L15, L22 and L30. (Pets currently cap at level 10 via loyalty, so the higher
/// rungs reward the most-fed, most-loyal companions.)
pub const PET_SKILLS: &[PetSkill] = &[
    PetSkill {
        level: 3,
        name: "Savage Bite",
        effect: PetSkillEffect::SavageBite,
        cooldown: 3,
        power: 6,
    },
    PetSkill {
        level: 8,
        name: "Rend",
        effect: PetSkillEffect::Rend,
        cooldown: 4,
        power: 4,
    },
    PetSkill {
        level: 15,
        name: "Intimidating Roar",
        effect: PetSkillEffect::Roar,
        cooldown: 6,
        power: 5,
    },
    PetSkill {
        level: 22,
        name: "Loyal Guard",
        effect: PetSkillEffect::Guard,
        cooldown: 6,
        power: 12,
    },
    PetSkill {
        level: 30,
        name: "Killing Pounce",
        effect: PetSkillEffect::Pounce,
        cooldown: 7,
        power: 18,
    },
];

/// The pet auto-skills unlocked at a given pet level (those with `level <= lvl`).
pub fn pet_skills_at(level: i32) -> impl Iterator<Item = &'static PetSkill> {
    PET_SKILLS.iter().filter(move |s| s.level <= level)
}

/// The Animal Taming trade's stable key (for persistence/display parity).
pub fn taming_key() -> &'static str {
    TamingSkill::key()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_fifty_tameable_beasts_ordered_small_to_large() {
        assert_eq!(TAMEABLE_COUNT, 50, "fifty tameable beasts");
        // The taming difficulty is non-decreasing across the list (small -> large
        // -> harder and harder), and spans the whole 1..=50 range.
        for w in TAMEABLE.windows(2) {
            assert!(
                w[1].tame_level >= w[0].tame_level,
                "tame level must not fall going down the list ({} -> {})",
                w[0].name,
                w[1].name
            );
        }
        assert_eq!(
            TAMEABLE[0].tame_level, 1,
            "the first beast is a novice tame"
        );
        assert_eq!(
            TAMEABLE[TAMEABLE_COUNT - 1].tame_level,
            50,
            "the last beast needs a master tamer"
        );
        // Every tameable is marked tameable, has a name/glyph, and non-trivial
        // stats that trend up with size.
        for s in TAMEABLE {
            assert!(s.is_tameable(), "{} should be tameable", s.name);
            assert!(s.base_hp > 0 && s.base_attack > 0, "{} has stats", s.name);
        }
        // Bigger beasts are stronger companions: the largest out-muscles the
        // smallest by a wide margin.
        assert!(TAMEABLE[TAMEABLE_COUNT - 1].base_hp > TAMEABLE[0].base_hp * 5);
    }

    #[test]
    fn tameable_keys_are_unique_and_resolve() {
        let mut keys: Vec<&str> = TAMEABLE.iter().map(|s| s.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), TAMEABLE_COUNT, "tameable keys are unique");
        for s in TAMEABLE {
            assert_eq!(tameable_by_key(s.key).map(|x| x.key), Some(s.key));
        }
    }

    #[test]
    fn every_beast_has_a_roaming_spot_in_broceliande() {
        let beasts = wild_beasts();
        assert_eq!(beasts.len(), TAMEABLE_COUNT, "one roaming spot per beast");
        // Every spot points at a real species index, and all fifty species appear.
        let mut seen = std::collections::HashSet::new();
        for b in beasts {
            assert!(b.species < TAMEABLE_COUNT);
            seen.insert(b.species);
        }
        assert_eq!(seen.len(), TAMEABLE_COUNT, "all fifty beasts are placed");
    }

    #[test]
    fn tame_chance_rises_with_surplus_and_refuses_under_level() {
        let beast = &TAMEABLE[TAMEABLE_COUNT - 1]; // needs level 50
        // A novice cannot tame the greatest beast.
        assert_eq!(tame_chance(0, beast), 0);
        // The first beast (level 1) is a coin-toss for a rank beginner and a near
        // sure thing for a trained tamer.
        let easy = &TAMEABLE[0];
        assert_eq!(tame_chance(0, easy), 40, "at exactly the required level");
        let trained = super::super::skills::xp_for_skill_level(10);
        assert!(
            tame_chance(trained, easy) > tame_chance(0, easy),
            "surplus level raises the odds"
        );
        // The chance is capped below certainty.
        let master = super::super::skills::xp_for_skill_level(50);
        assert!(tame_chance(master, easy) <= 95, "never a sure thing");
    }

    #[test]
    fn pet_skills_unlock_on_the_ladder() {
        assert_eq!(pet_skills_at(1).count(), 0, "no skills before level 3");
        assert_eq!(pet_skills_at(3).count(), 1, "savage bite at 3");
        assert_eq!(pet_skills_at(8).count(), 2, "rend at 8");
        assert_eq!(pet_skills_at(15).count(), 3, "roar at 15");
        assert_eq!(pet_skills_at(22).count(), 4, "guard at 22");
        assert_eq!(pet_skills_at(30).count(), PET_SKILLS.len(), "pounce at 30");
        // Unlock levels are strictly increasing.
        for w in PET_SKILLS.windows(2) {
            assert!(w[1].level > w[0].level, "pet skill unlocks climb");
        }
    }
}
