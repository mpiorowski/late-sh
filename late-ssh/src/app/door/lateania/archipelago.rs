// The Shattered Archipelago - a portal-linked expansion for Lateania.
//
// Two things live out past the known world, both reached by waystone portals
// rather than by walking:
//
//   * A handful of safe VILLAGES - small havens with their own flavour, each
//     holding a portal so you can hop between them and back to the mainland.
//   * A thousand rooms of ISLANDS - twenty of them, each carved as a braided
//     maze or an organic cavern (never a grid), each with its own scenery and a
//     named boss, and each reached by its own portal.
//
// This module holds only the data and the address arithmetic. The room
// generation lives in `world.rs` (`extend_villages` / `extend_archipelago`), and
// the portal fast-travel action lives in `svc.rs`.

use super::world::RoomId;

/// First room id of the villages block. One room per village.
pub const VILLAGE_BASE: RoomId = 8_000;

/// The safe havens, each `(name, blurb)`. Index is the room offset from
/// `VILLAGE_BASE`. Order is stable (used as the portal-menu order).
pub const VILLAGES: &[(&str, &str)] = &[
    (
        "Lantern Cove",
        "A sheltered fishing village of blue-shuttered cottages ringed around a still tide-pool harbour, its jetties strung with paper lanterns that never seem to go out. Gulls wheel over drying nets and the whole place smells of salt, woodsmoke, and someone's slow-simmering chowder.",
    ),
    (
        "Emberfall Rest",
        "A caravan waystation built into the lee of a red mesa, all canvas awnings and cook-fires, where travellers between the far isles trade rumours over cardamom coffee. The heat shimmers off the stone by day; by night the sky is an ocean of cold stars.",
    ),
    (
        "Hollowmere",
        "A quiet hamlet on stilts above a mirror-black fen, its walkways of grey boarding threading between willow and reed. Frogs keep up a companionable racket, will-o'-wisps drift among the trees, and the folk here speak softly and mean it.",
    ),
    (
        "Skyreach Landing",
        "A cliff-top village clinging to the shoulder of a mountain, its houses stacked like swallows' nests above a sea of cloud. The wind is constant and clean, prayer-flags snap on every rail, and from the terrace you can watch the weather being born below you.",
    ),
];

/// First room id of the archipelago block. Each island reserves a
/// `WIDTH * HEIGHT` cell field.
pub const ARCH_BASE: RoomId = 20_000;
pub const ARCH_W: usize = 10;
pub const ARCH_H: usize = 5;
/// Room ids reserved per island (the cell field), whether or not every cell
/// becomes a room (caverns are sparse).
pub const ARCH_STRIDE: RoomId = (ARCH_W * ARCH_H) as RoomId;
/// Fixed seed base for deterministic island generation.
pub const ARCH_SEED: u64 = 0x15_1A9D_5EED_u64;

/// The number of islands.
pub const ISLAND_COUNT: usize = ISLANDS.len();

/// One island: (name, adjective, ground, landmark, creatures, three mob names,
/// boss). Prose is composed by `world.rs`. Each has unique scenery.
#[allow(clippy::type_complexity)]
pub const ISLANDS: &[(&str, &str, &str, &str, &str, [&str; 3], &str)] = &[
    (
        "Isle of Glass Sands",
        "sun-blind",
        "singing glass sand",
        "a shore of fused green glass",
        "glass-scuttlers",
        [
            "a glass-shell crab",
            "a mirage-stalker",
            "a sunstruck marauder",
        ],
        "Vitreon, the Sunfused",
    ),
    (
        "Coral Crown Atoll",
        "reef-bright",
        "bleached coral rubble",
        "a ring-reef of blood-red coral",
        "reef-wardens",
        [
            "a coral revenant",
            "a stinging drifter",
            "a pearl-mad diver",
        ],
        "The Reefcrowned Queen",
    ),
    (
        "Ashfall Cinderisle",
        "smoke-choked",
        "warm black cinders",
        "a cone of slow-breathing fire",
        "cinder-born",
        ["a cinder hound", "an ashen wraith", "a magma-slick brute"],
        "Pyrexis of the Cinder Cone",
    ),
    (
        "Whispering Fenholm",
        "mist-drowned",
        "sucking peat",
        "a drowned bell-tower in the reeds",
        "fen-lurkers",
        [
            "a bog-drowned thrall",
            "a marsh-lamp wisp",
            "a reed-strangler",
        ],
        "The Fenholm Drowned One",
    ),
    (
        "Frostspar Skerries",
        "ice-locked",
        "blue glacier ice",
        "a cathedral of frozen spray",
        "frost-things",
        [
            "a rime-clad stalker",
            "a glacier lurker",
            "a frostbitten reaver",
        ],
        "Hrimmaw the Everfrozen",
    ),
    (
        "Thornweald Isle",
        "bramble-choked",
        "root-knotted loam",
        "a wall of ten-foot black thorns",
        "thorn-beasts",
        ["a bramble-lynx", "a barb-hide boar", "a strangling vine"],
        "The Thornweald Antlered King",
    ),
    (
        "Sunken Vault Isle",
        "gold-glinting",
        "silted vault-tile",
        "a treasury swallowed by the sea",
        "vault-guardians",
        [
            "a gilded sentinel",
            "a coin-hoard wraith",
            "a drowned reeve",
        ],
        "The Vaultkeeper Undying",
    ),
    (
        "Stormglass Reach",
        "thunder-lit",
        "wave-polished stone",
        "a lightning-fused glass spire",
        "storm-callers",
        [
            "a stormbound corsair",
            "a spark-lured horror",
            "a gale-wraith",
        ],
        "Astridax, Voice of the Gale",
    ),
    (
        "Bonewhite Atoll",
        "bleached",
        "ground bone-meal",
        "a beach of whale ribs and skulls",
        "bone-pickers",
        ["a marrow-crawler", "a skull-piper", "a bone-plated brute"],
        "The Ossuary Leviathan",
    ),
    (
        "Verdant Ruin Isle",
        "green-drowned",
        "moss-eaten flagstone",
        "a jungle-swallowed dead city",
        "ruin-haunts",
        [
            "a vine-bound sentinel",
            "a temple revenant",
            "a canopy stalker",
        ],
        "The Verdant God-Below",
    ),
    (
        "Mirrorlake Isle",
        "glass-still",
        "quicksilver shallows",
        "a lake that shows the wrong sky",
        "mirror-drowned",
        [
            "a glass-skinned hunter",
            "a reflection-wraith",
            "a silvered drowner",
        ],
        "Your Own Reflection, Wrong",
    ),
    (
        "Saltspire Pillars",
        "salt-crusted",
        "crunching salt flat",
        "a forest of white salt spires",
        "salt-things",
        [
            "a brine crawler",
            "a salt-blind marauder",
            "a crystalline horror",
        ],
        "The Salt Cathedral's Heart",
    ),
    (
        "Duskmoth Grove",
        "twilight-dim",
        "spore-soft mulch",
        "a grove of moth-winged trees",
        "moth-kin",
        [
            "a giant dusk-moth",
            "a spore-drunk lurker",
            "a luminous stalker",
        ],
        "The Duskmoth Empress",
    ),
    (
        "Rustwrack Shoals",
        "iron-stained",
        "rust-red shingle",
        "a graveyard of iron hulls",
        "wrack-scavengers",
        [
            "a rust-clad ghoul",
            "a barnacled reaver",
            "an iron-boned wight",
        ],
        "The Rustwrack Dredger",
    ),
    (
        "Windsong Cliffs",
        "wind-scoured",
        "wind-bared chalk",
        "cliffs that sing in the gale",
        "cliff-harpies",
        ["a singing harpy", "a chalk-clinger", "a windborne wraith"],
        "Aeolith of the Singing Cliffs",
    ),
    (
        "Gloomtide Trench",
        "lightless",
        "abyssal trench-silt",
        "a chasm that swallows the tide",
        "trench-maws",
        ["a gulper horror", "an anglerfiend", "a pressure-wraith"],
        "That Which the Trench Fed",
    ),
    (
        "Amberglow Isle",
        "honey-lit",
        "fossil-amber grit",
        "a cliff of insects trapped in amber",
        "amber-things",
        [
            "an amber-shelled crawler",
            "a fossil-wraith",
            "a resin-drowned horror",
        ],
        "The Amber-Sealed Ancient",
    ),
    (
        "Starfall Crater",
        "meteor-scarred",
        "black star-glass",
        "a crater still faintly humming",
        "star-touched",
        [
            "a starfall stalker",
            "a fallen-light horror",
            "a crater-wraith",
        ],
        "The Thing That Fell From the Sky",
    ),
    (
        "Tempest Eye Isle",
        "cyclone-wracked",
        "spinning wrack",
        "the calm at a storm's heart",
        "eye-born",
        [
            "a maelstrom revenant",
            "a churning horror",
            "a vortex-wraith",
        ],
        "The Eye That Never Closes",
    ),
    (
        "Worldwound Isle",
        "reality-frayed",
        "the floor of the world",
        "the wound where the map ends",
        "the unmade",
        [
            "a herald of the unmade",
            "an unwritten terror",
            "a fray-walker",
        ],
        "The Sundering, Made Flesh",
    ),
];

/// The safe entrance (waystone) room of island `i`.
pub fn island_entrance(i: usize) -> RoomId {
    ARCH_BASE + i as RoomId * ARCH_STRIDE
}

/// A village's room id.
pub fn village_room(i: usize) -> RoomId {
    VILLAGE_BASE + i as RoomId
}

/// Whether a room is one of the safe villages.
pub fn is_village_room(room: RoomId) -> bool {
    (VILLAGE_BASE..VILLAGE_BASE + VILLAGES.len() as RoomId).contains(&room)
}

/// Whether a room belongs to the archipelago islands.
pub fn is_archipelago_room(room: RoomId) -> bool {
    (ARCH_BASE..ARCH_BASE + ISLAND_COUNT as RoomId * ARCH_STRIDE).contains(&room)
}

/// Whether a room hosts a fast-travel waystone (a village or an island entrance),
/// used both to place the `Portal` feature and to make the network reachable.
pub fn has_waystone(room: RoomId) -> bool {
    is_village_room(room) || (0..ISLAND_COUNT).any(|i| island_entrance(i) == room)
}

/// Every portal destination: `(label, room)` - the villages first, then each
/// island's landing. This is the fast-travel menu, shown at any waystone.
pub fn portal_destinations() -> Vec<(&'static str, RoomId)> {
    let mut out: Vec<(&'static str, RoomId)> = VILLAGES
        .iter()
        .enumerate()
        .map(|(i, (name, _))| (*name, village_room(i)))
        .collect();
    for (i, isle) in ISLANDS.iter().enumerate() {
        out.push((isle.0, island_entrance(i)));
    }
    out
}
