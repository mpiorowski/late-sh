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
//   * The pet **auto-skills** - abilities keyed to a pet's level, firing
//     automatically in the combat round (see `svc.rs`). Their raw power scales
//     with the pet's own attack, so a bigger beast hits harder on the same rung.
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
    // ---- Wildbound: the rideable beasts (five wild, five mythical) -------
    //
    // These sit above the fifty classic beasts on the taming ladder, so their
    // stats have to *start* above the best tame-50 beast (the Green Wyrm of the
    // World-Oak at attack 38, the Scion at hp 700) and climb from there. They
    // originally began at attack 22 / hp 420, which meant taming 51..=79 earned
    // you a strictly worse companion than the one you already had - twenty-five
    // levels of the trade spent going backwards. Pinned by
    // `every_taming_tier_offers_a_better_companion_than_the_one_below`.
    beast(
        "wb_palfrey",
        "Duskmane Palfrey",
        "\u{1F40E}",
        55,
        720,
        40,
        "a calm-eyed forest horse, dusk-grey down the mane; steady under a saddle",
    ),
    beast(
        "wb_elk",
        "Greatantler Elk",
        "\u{1F98C}",
        60,
        760,
        42,
        "a bull elk whose antlers scrape the low boughs; strong enough to carry two",
    ),
    beast(
        "wb_ram",
        "Snowcrest Ram",
        "\u{1F411}",
        65,
        800,
        44,
        "a mountain ram, sure-footed on ledges no horse would dare",
    ),
    beast(
        "wb_strider",
        "Fenland Strider",
        "\u{1F9B6}",
        70,
        840,
        46,
        "a long-legged marsh runner that skims the soft ground like a skipped stone",
    ),
    beast(
        "wb_direstag",
        "Direhorn Stag",
        "\u{1F98C}",
        75,
        880,
        48,
        "a stag grown vast and wary in the deep wood; it suffers only a worthy rider",
    ),
    beast(
        "wb_unicorn",
        "Moonlit Unicorn",
        "\u{1F984}",
        80,
        920,
        50,
        "a unicorn seen only where moonlight pools; its stride bends the miles",
    ),
    beast(
        "wb_hippogriff",
        "Stormfeather Hippogriff",
        "\u{1F985}",
        85,
        950,
        52,
        "half hawk, half horse, all weather; it lands where the storm was heading",
    ),
    beast(
        "wb_griffin",
        "Emberwing Griffin",
        "\u{1F981}",
        90,
        970,
        54,
        "a griffin whose wingbeats shed sparks; the sky shortens beneath it",
    ),
    beast(
        "wb_wyvern",
        "Verdant Wyvern",
        "\u{1F409}",
        95,
        985,
        55,
        "a green-scaled wyvern of the canopy roads; it knows every gap in the world",
    ),
    beast(
        "wb_worldserpent",
        "Aurora Worldserpent",
        "\u{1F30C}",
        100,
        1000,
        56,
        "the horizon-swimmer of the old sagas; to ride it is to arrive before you left",
    ),
];

/// Number of tameable beasts (the design target is fifty).
pub const TAMEABLE_COUNT: usize = TAMEABLE.len();

/// The rideable species and how far they carry you: one keypress while mounted
/// strides this many rooms. The wild mounts walk 2-3; the mythicals at the top
/// of the taming ladder stride 4, and the very best skip 5 rooms at a time.
pub const RIDEABLE: &[(&str, u8)] = &[
    ("wb_palfrey", 2),
    ("wb_elk", 2),
    ("wb_ram", 3),
    ("wb_strider", 3),
    ("wb_direstag", 3),
    ("wb_unicorn", 4),
    ("wb_hippogriff", 4),
    ("wb_griffin", 4),
    ("wb_wyvern", 5),
    ("wb_worldserpent", 5),
];

/// How many rooms one mounted step covers for a species, if it can be ridden.
pub fn mount_stride(species_key: &str) -> Option<u8> {
    RIDEABLE
        .iter()
        .find(|(key, _)| *key == species_key)
        .map(|&(_, stride)| stride)
}

/// A `const` constructor for a tameable species (keeps the table readable).
/// Every classic/Wildbound beast walks the shared `PET_SKILLS` ladder.
const fn beast(
    key: &'static str,
    name: &'static str,
    glyph: &'static str,
    tame_level: i32,
    base_hp: i32,
    base_attack: i32,
    desc: &'static str,
) -> PetSpecies {
    beast_with_skills(
        key,
        name,
        glyph,
        tame_level,
        base_hp,
        base_attack,
        desc,
        PET_SKILLS,
    )
}

/// Like [`beast`], but with its own auto-skill ladder instead of the shared
/// one - what makes the five Aelunor companions genuinely play differently
/// from each other and from the classic fifty.
#[allow(clippy::too_many_arguments)]
const fn beast_with_skills(
    key: &'static str,
    name: &'static str,
    glyph: &'static str,
    tame_level: i32,
    base_hp: i32,
    base_attack: i32,
    desc: &'static str,
    skills: &'static [PetSkill],
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
        skills,
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
    /// Index into the *combined* `TAMEABLE` then `AELUNOR_TAMEABLE` pool -
    /// resolve with `beast_species`, never by indexing `TAMEABLE` directly.
    pub species: usize,
}

/// Every place a tameable beast roams: the classic fifty at a Broceliande
/// forest gate, plus the five Aelunor companions at an Aelunor wood-gate
/// (`world::aelunor_entrances` - never offset 0, since every Aelunor zone is
/// cavern-carved and offset 0 is always solid rock there; Broceliande's own
/// maze zones are the one place offset 0 genuinely is the entrance, since
/// `carve_maze`'s DFS always starts at cell 0). Built once and cached: each
/// beast is placed in the zone whose depth suits its taming difficulty, at
/// that zone's entrance, which is always real and safe. Several beasts can
/// share a gate, reading as a menagerie at each woodward-holt.
pub fn wild_beasts() -> &'static [WildBeast] {
    use std::sync::OnceLock;
    static BEASTS: OnceLock<Vec<WildBeast>> = OnceLock::new();
    BEASTS
        .get_or_init(|| {
            // Map each beast onto a zone by its rank (0..50 -> zone 0..N), and
            // home it at that zone's entrance gate (offset 0, always real for
            // Broceliande's maze-carved DFS - see the fn doc comment).
            let zones = BROCELIANDE_ZONE_COUNT.max(1);
            let mut beasts: Vec<WildBeast> = TAMEABLE
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    let zone = (i * zones / TAMEABLE_COUNT).min(zones - 1);
                    let home = BROCELIANDE_BASE + zone as u32 * BROCELIANDE_ZONE_STRIDE;
                    WildBeast { home, species: i }
                })
                .collect();

            let entrances = super::world::aelunor_entrances();
            let ae_zones = entrances.len().max(1);
            beasts.extend(AELUNOR_TAMEABLE.iter().enumerate().filter_map(|(i, _)| {
                let zone = (i * ae_zones / AELUNOR_TAMEABLE.len()).min(ae_zones - 1);
                entrances.get(zone).map(|&home| WildBeast {
                    home,
                    // Continues past TAMEABLE's own index range - see
                    // `beast_species`.
                    species: TAMEABLE.len() + i,
                })
            }));
            beasts
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
/// (returns 0). `cha_pct` is the tamer's Charisma (`AbilityScores::tame_pct`),
/// percent points on top. Capped below certainty so even a master can be
/// thrown.
pub fn tame_chance(taming_xp: i64, beast: &PetSpecies, cha_pct: i32) -> u32 {
    let level = skill_level_for_xp(taming_xp);
    if level < beast.tame_level {
        return 0;
    }
    let surplus = level - beast.tame_level;
    // 40% at exactly the required level, +9% per level of surplus, plus
    // Charisma, capped at 95.
    (40 + surplus * 9 + cha_pct).clamp(0, 95) as u32
}

/// Xp awarded for a *successful* tame: scales with the beast's difficulty, so
/// taming a great wyrm is worth far more than a hare. Kept generous enough that
/// working up the beasts is a real, rewarding progression on the shared
/// skill curve.
pub fn tame_xp(beast: &PetSpecies) -> i32 {
    30 + beast.tame_level * beast.tame_level / 2
}

// ---- Pet auto-skills ------------------------------------------------------
//
// A companion (bought or tamed) unlocks abilities as it gains levels, and they
// fire automatically in the combat round on their own cooldowns. Every pet walks
// the same unlock ladder, keyed to its **level** (L2 / L4 / L6 / L8 / L10,
// surfaced in the pet view so the player sees what is coming). The skills aren't
// varied by species; instead each one's power scales with the pet's own attack
// in `svc.rs`, so a wyrm's Savage Bite lands far harder than a hare's on the
// same rung.

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
    /// A mend: heals the owner directly (fae/druidic pets only).
    Mend,
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

/// The unlock ladder shared by every companion. The five rungs unlock at L2, L4,
/// L6, L8 and L10, so a well-fed, loyal companion reaches all five within the
/// `PET_MAX_LEVEL` (10) loyalty cap. (They previously unlocked at 3/8/15/22/30,
/// which left the top three rungs dead content behind the cap.)
pub const PET_SKILLS: &[PetSkill] = &[
    PetSkill {
        level: 2,
        name: "Savage Bite",
        effect: PetSkillEffect::SavageBite,
        cooldown: 3,
        power: 6,
    },
    PetSkill {
        level: 4,
        name: "Rend",
        effect: PetSkillEffect::Rend,
        cooldown: 4,
        power: 4,
    },
    PetSkill {
        level: 6,
        name: "Intimidating Roar",
        effect: PetSkillEffect::Roar,
        cooldown: 6,
        power: 5,
    },
    PetSkill {
        level: 8,
        name: "Loyal Guard",
        effect: PetSkillEffect::Guard,
        cooldown: 6,
        power: 12,
    },
    PetSkill {
        level: 10,
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

// ---- Aelunor companions: five tameable beasts, each with its own spells ---
//
// Unlike the fifty classic beasts (all sharing `PET_SKILLS`), each of these
// carries its own auto-skill ladder, so they play differently from each
// other, not just look different. Home of the region's own species: a
// support healer, a tank, a glass-cannon, a bleed hybrid, and an apex
// all-rounder. Placed one per gate at five of Aelunor's twelve zones (see
// `aelunor_wild_beasts`), so each is genuinely exclusive to Aelunor.
//
// KEYS ARE PERSISTED - never reorder or rename.

const AELUNOR_FAERIE_SKILLS: &[PetSkill] = &[
    PetSkill {
        level: 2,
        name: "Fae Mending",
        effect: PetSkillEffect::Mend,
        cooldown: 4,
        power: 10,
    },
    PetSkill {
        level: 5,
        name: "Glamour",
        effect: PetSkillEffect::Roar,
        cooldown: 6,
        power: 5,
    },
    PetSkill {
        level: 8,
        name: "Thorn Rend",
        effect: PetSkillEffect::Rend,
        cooldown: 4,
        power: 5,
    },
];

const AELUNOR_SAPLING_SKILLS: &[PetSkill] = &[
    PetSkill {
        level: 2,
        name: "Root Guard",
        effect: PetSkillEffect::Guard,
        cooldown: 5,
        power: 10,
    },
    PetSkill {
        level: 6,
        name: "Sap Mending",
        effect: PetSkillEffect::Mend,
        cooldown: 6,
        power: 14,
    },
    PetSkill {
        level: 10,
        name: "Heartwood Slam",
        effect: PetSkillEffect::SavageBite,
        cooldown: 5,
        power: 10,
    },
];

const AELUNOR_OWL_SKILLS: &[PetSkill] = &[
    PetSkill {
        level: 2,
        name: "Talon Strike",
        effect: PetSkillEffect::SavageBite,
        cooldown: 3,
        power: 8,
    },
    PetSkill {
        level: 6,
        name: "Silent Stoop",
        effect: PetSkillEffect::Pounce,
        cooldown: 5,
        power: 14,
    },
    PetSkill {
        level: 10,
        name: "Moonlit Dive",
        effect: PetSkillEffect::Pounce,
        cooldown: 6,
        power: 22,
    },
];

const AELUNOR_FOX_SKILLS: &[PetSkill] = &[
    PetSkill {
        level: 3,
        name: "Briar Rend",
        effect: PetSkillEffect::Rend,
        cooldown: 4,
        power: 5,
    },
    PetSkill {
        level: 7,
        name: "Druid's Blessing",
        effect: PetSkillEffect::Roar,
        cooldown: 6,
        power: 6,
    },
    PetSkill {
        level: 10,
        name: "Pounce from the Bracken",
        effect: PetSkillEffect::Pounce,
        cooldown: 7,
        power: 18,
    },
];

const AELUNOR_HOUND_SKILLS: &[PetSkill] = &[
    PetSkill {
        level: 2,
        name: "Wild Hunt Bite",
        effect: PetSkillEffect::SavageBite,
        cooldown: 3,
        power: 7,
    },
    PetSkill {
        level: 5,
        name: "Baying Rend",
        effect: PetSkillEffect::Rend,
        cooldown: 4,
        power: 5,
    },
    PetSkill {
        level: 7,
        name: "Huntmaster's Guard",
        effect: PetSkillEffect::Guard,
        cooldown: 6,
        power: 12,
    },
    PetSkill {
        level: 9,
        name: "Second Wind",
        effect: PetSkillEffect::Mend,
        cooldown: 7,
        power: 16,
    },
    PetSkill {
        level: 10,
        name: "the Wild Hunt's Kill",
        effect: PetSkillEffect::Pounce,
        cooldown: 7,
        power: 20,
    },
];

/// The five tameable companions of Aelunor, ordered easy to hard. Stats climb
/// with `tame_level` under the same "no beast Pareto-dominated by an easier
/// one" rule as the classic fifty: same-tier beasts may trade attack for hp,
/// but nothing higher may lose on both axes to something cheaper. The rule
/// spans both pools, since a player grinds one Animal Taming level and takes
/// the best beast it opens wherever it roams - `taming_test`'s
/// `no_beast_is_out_classed_by_an_easier_one` walks the combined list, so a
/// stat edit here is checked against every Broceliande beast too.
pub const AELUNOR_TAMEABLE: &[PetSpecies] = &[
    beast_with_skills(
        "ae_faerie",
        "Moonlit Faerie",
        "\u{1F9DA}",
        8,
        58,
        9,
        "a small fae creature trailing cold moonlight, quick to heal what it loves",
        AELUNOR_FAERIE_SKILLS,
    ),
    beast_with_skills(
        "ae_sapling",
        "Woodkin Sapling",
        "\u{1F331}",
        18,
        130,
        11,
        "a walking sapling of the deep wood, slow, patient, and hard to fell",
        AELUNOR_SAPLING_SKILLS,
    ),
    beast_with_skills(
        "ae_owl",
        "High Elf Owl",
        "\u{1F989}",
        26,
        70,
        22,
        "a silver-eyed owl bonded to the high elves, all speed and talon",
        AELUNOR_OWL_SKILLS,
    ),
    beast_with_skills(
        "ae_fox",
        "Druid's Fox",
        "\u{1F98A}",
        34,
        110,
        21,
        "a russet fox that runs at a druid's heel and fights with the wood's own cunning",
        AELUNOR_FOX_SKILLS,
    ),
    beast_with_skills(
        "ae_hound",
        "Wild Hunt Hound",
        "\u{1F415}",
        44,
        200,
        28,
        "a spectral hound of the Wild Hunt, the rarest and deadliest companion Aelunor offers",
        AELUNOR_HOUND_SKILLS,
    ),
];

/// Resolve a `WildBeast.species` index to its `PetSpecies`, across **both**
/// pools `wild_beasts()` can place: the classic fifty (`TAMEABLE`, indices
/// `0..TAMEABLE.len()`) and the five Aelunor companions (`AELUNOR_TAMEABLE`,
/// indices continuing on from there). `wild_beasts()` is the one thing that
/// actually builds `WildBeast.species` values, so this is its match - every
/// external consumer of a `WildBeast` (the tame panel, the tame action, the
/// map POI index) should resolve through this, never index `TAMEABLE`
/// directly, or an Aelunor placement panics on an out-of-range index.
pub fn beast_species(index: usize) -> &'static PetSpecies {
    TAMEABLE
        .get(index)
        .or_else(|| AELUNOR_TAMEABLE.get(index - TAMEABLE.len()))
        .expect("WildBeast.species always indexes TAMEABLE then AELUNOR_TAMEABLE")
}

/// The Animal Taming trade's stable key (for persistence/display parity).
pub fn taming_key() -> &'static str {
    TamingSkill::key()
}

#[cfg(test)]
#[path = "taming_test.rs"]
mod taming_test;
