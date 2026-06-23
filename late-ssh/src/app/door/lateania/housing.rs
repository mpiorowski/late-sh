// Player housing for Lateania - true-Ultima-Online-style shared-world homes.
//
// Houses are NOT dynamic rooms. They are pre-built, always-present rooms in a
// public district (Hearthward Close, off Embergate's Market Row), generated in
// `world.rs::extend_housing`. What is dynamic - and lives as side-state on the
// service - is *ownership* (who holds the deed to a plot) and *furnishings*
// (what each owner has placed in their rooms). This is the same "static rooms +
// side-map" pattern that made mazes, mob behaviours, and wildlife tractable: the
// whole movement/snapshot/visiting machinery works unchanged, and because the
// street and every door are public, anyone can walk up and visit a home.
//
// This module holds only the data and the address arithmetic. The world wiring
// (room generation), the runtime (claiming deeds, buying/placing furniture), the
// views, and persistence live in `world.rs`, `svc.rs`, and `persist.rs`.

use super::world::RoomId;

/// First room id of the housing district. The close itself is `HOUSING_BASE`;
/// each tier's plot occupies the ten ids starting at `plot_base(tier_index)`.
pub const HOUSING_BASE: RoomId = 9000;

/// One class of home, cheapest (smallest) first. A plot's interior is `ground`
/// rooms on the entry floor plus `upper` rooms reached by a stair.
#[derive(Clone, Copy, Debug)]
pub struct TierDef {
    /// Stable persistence key (never reorder/rename).
    pub key: &'static str,
    pub label: &'static str,
    /// Price of the deed, in gold.
    pub price: i64,
    pub ground: usize,
    pub upper: usize,
    /// Flavour shown at the deed clerk.
    pub blurb: &'static str,
}

impl TierDef {
    pub fn rooms(&self) -> usize {
        self.ground + self.upper
    }
}

/// The five homes, hut to tower. Index is the plot id (one plot per tier).
pub const TIERS: &[TierDef] = &[
    TierDef {
        key: "hut",
        label: "Wattle Hut",
        price: 500,
        ground: 1,
        upper: 0,
        blurb: "A single round room of wattle and turf with a smoke-hole roof - humble, snug, and yours.",
    },
    TierDef {
        key: "cottage",
        label: "Thatched Cottage",
        price: 1_500,
        ground: 2,
        upper: 0,
        blurb: "A two-room cottage under good thatch, with a hearth-room and a back room for sleeping or stores.",
    },
    TierDef {
        key: "longhouse",
        label: "Timber Longhouse",
        price: 4_000,
        ground: 3,
        upper: 0,
        blurb: "A broad three-room longhouse of oak and pitch, room enough for a family, a forge-corner, and a feast.",
    },
    TierDef {
        key: "manor",
        label: "Stone Manor",
        price: 9_000,
        ground: 3,
        upper: 1,
        blurb: "A walled manor of dressed stone: three grand rooms below and a private solar up the stair.",
    },
    TierDef {
        key: "tower",
        label: "Wizard's Tower",
        price: 20_000,
        ground: 3,
        upper: 2,
        blurb: "A five-room tower crowned with a star-roofed sanctum - the grandest hearth coin can raise.",
    },
];

/// First room id of tier `i`'s plot (the entrance). Plots are spaced ten ids
/// apart so a tier could grow without colliding with its neighbour.
pub fn plot_base(i: usize) -> RoomId {
    HOUSING_BASE + 10 * (i as RoomId + 1)
}

/// Which tier/plot (0-based) a room belongs to, if it is a house interior.
pub fn plot_of_room(room: RoomId) -> Option<usize> {
    (0..TIERS.len()).find(|&i| {
        let base = plot_base(i);
        room >= base && room < base + TIERS[i].rooms() as RoomId
    })
}

/// Whether a room is part of the housing district (the close or any interior).
pub fn is_housing_room(room: RoomId) -> bool {
    room == HOUSING_BASE || plot_of_room(room).is_some()
}

/// A placeable furnishing sold by the housing clerk and set down in a home.
#[derive(Clone, Copy, Debug)]
pub struct Furniture {
    /// Stable persistence key (never reorder/rename).
    pub key: &'static str,
    pub name: &'static str,
    pub price: i64,
    pub desc: &'static str,
}

const fn f(key: &'static str, name: &'static str, price: i64, desc: &'static str) -> Furniture {
    Furniture {
        key,
        name,
        price,
        desc,
    }
}

/// The furnishing catalogue. Over fifty pieces, each with its own flavour, sold
/// at the housing clerk and placed in a home you own.
pub const FURNITURE: &[Furniture] = &[
    // ---- Seating ----
    f(
        "oak_stool",
        "an oak milking stool",
        12,
        "A three-legged stool worn glassy-smooth by generations of sitting.",
    ),
    f(
        "rush_chair",
        "a rush-seated chair",
        24,
        "A plain ladder-back chair, its seat woven from river rushes that creak companionably.",
    ),
    f(
        "carved_armchair",
        "a carved armchair",
        90,
        "A deep armchair of dark walnut, its arms ending in carved wolf-heads polished by resting hands.",
    ),
    f(
        "velvet_settle",
        "a velvet settle",
        180,
        "A long high-backed settle cushioned in wine-red velvet, made for two by the fire.",
    ),
    f(
        "lords_throne",
        "a lord's throne",
        600,
        "A throne of black oak and gilt, far too grand for any honest cottage - which is rather the point.",
    ),
    // ---- Tables ----
    f(
        "trestle_table",
        "a trestle table",
        40,
        "A long plank table on folding trestles, scarred by knives and ringed by a thousand cups.",
    ),
    f(
        "round_table",
        "a round table",
        70,
        "A sturdy round table of waxed elm where no one sits at the head.",
    ),
    f(
        "writing_desk",
        "a writing desk",
        130,
        "A slope-topped desk with inkwell, sand-shaker, and a drawer that sticks just so.",
    ),
    f(
        "chess_table",
        "a chess table",
        150,
        "A little inlaid table set with a board of bone and ebony, the pieces mid-game and waiting.",
    ),
    f(
        "banquet_board",
        "a banquet board",
        260,
        "A vast banquet board that seats a dozen, its centre carved with a running hunt.",
    ),
    // ---- Beds ----
    f(
        "straw_pallet",
        "a straw pallet",
        15,
        "A ticking sack stuffed with fresh straw - it rustles, it prickles, and after a long road it is heaven.",
    ),
    f(
        "rope_bed",
        "a rope-strung bed",
        60,
        "A simple frame strung with rope and topped with a wool mattress; you tighten the ropes when they sag.",
    ),
    f(
        "feather_bed",
        "a feather bed",
        200,
        "A deep goose-down bed you sink into past your ears, piled with quilts and bolsters.",
    ),
    f(
        "canopy_bed",
        "a canopy bed",
        420,
        "A four-poster hung with embroidered curtains that close out the cold and the world alike.",
    ),
    // ---- Storage ----
    f(
        "oak_chest",
        "an oak chest",
        35,
        "A banded oak chest with a stiff iron hasp, the inside sweet with cedar.",
    ),
    f(
        "linen_press",
        "a linen press",
        110,
        "A tall press for folded linens, its doors carved with twining ivy.",
    ),
    f(
        "apothecary_cabinet",
        "an apothecary cabinet",
        170,
        "A cabinet of forty tiny drawers, each labelled in a cramped hand with herbs half of which you cannot name.",
    ),
    f(
        "strongbox",
        "an iron strongbox",
        240,
        "A squat iron box with three locks and a reputation; thieves have wept over it.",
    ),
    f(
        "bookshelf",
        "a bookshelf",
        130,
        "A leaning shelf crammed with cracked-spine books, ledgers, and one suspiciously hollow volume.",
    ),
    f(
        "scroll_rack",
        "a scroll rack",
        95,
        "A honeycomb rack of pigeonholes stuffed with rolled maps and yellowing scrolls.",
    ),
    // ---- Lighting ----
    f(
        "tallow_candle",
        "a tallow candle",
        5,
        "A fat tallow candle on a pricket, guttering and smoky and somehow always the last light burning.",
    ),
    f(
        "iron_candelabra",
        "an iron candelabra",
        55,
        "A branched iron candelabra that throws a warm, shifting web of light up the walls.",
    ),
    f(
        "hanging_lantern",
        "a hanging lantern",
        45,
        "A pierced-tin lantern on a chain that scatters stars of light across the ceiling.",
    ),
    f(
        "crystal_chandelier",
        "a crystal chandelier",
        380,
        "A chandelier dripping with cut crystal that turns candleflame into a hundred trembling rainbows.",
    ),
    f(
        "everflame_brazier",
        "an everflame brazier",
        320,
        "A brass brazier of cold blue fire that needs no fuel and never gutters, a small wonder bought dear.",
    ),
    // ---- Floor coverings ----
    f(
        "woven_rug",
        "a woven rug",
        30,
        "A bright rag rug braided from old clothes, the kind every grandmother makes and no one throws away.",
    ),
    f(
        "bearskin_rug",
        "a bearskin rug",
        140,
        "A great brown bearskin sprawled before the hearth, head and all, glaring at every guest.",
    ),
    f(
        "woolen_runner",
        "a woollen runner",
        60,
        "A long striped runner that softens cold flagstones and muffles a midnight tread.",
    ),
    f(
        "eastern_carpet",
        "an eastern carpet",
        300,
        "A deep-piled carpet from the far caravan-roads, its pattern a maze you could lose an afternoon in.",
    ),
    // ---- Hearth & warmth ----
    f(
        "fire_irons",
        "a set of fire irons",
        25,
        "Poker, tongs, and a little brass shovel on a stand, blackened and indispensable.",
    ),
    f(
        "kettle_crane",
        "a kettle crane",
        40,
        "An iron crane that swings a kettle over the fire and back, for tea at the lift of a hand.",
    ),
    f(
        "carved_mantel",
        "a carved mantelpiece",
        160,
        "A mantel of pale stone carved with leaping hares, just right for a row of treasures.",
    ),
    f(
        "stone_hearth",
        "a great stone hearth",
        280,
        "A walk-in hearth of fieldstone that could roast an ox and warm a hall, the heart of any home.",
    ),
    // ---- Kitchen & sundries ----
    f(
        "water_barrel",
        "a water barrel",
        18,
        "A standing oak barrel, its lid a little askew, the water within cold and faintly green.",
    ),
    f(
        "hanging_herbs",
        "hanging herbs",
        14,
        "Bunches of sage, thyme, and lavender hung to dry from the rafters, scenting the whole room.",
    ),
    f(
        "cheese_safe",
        "a cheese safe",
        50,
        "A little louvred cupboard on legs that keeps the cheese cool and the mice furious.",
    ),
    f(
        "copper_pots",
        "a rack of copper pots",
        85,
        "A rack of beaten copper pots and pans, each dent a story, all of them gleaming.",
    ),
    f(
        "baker_oven",
        "a beehive oven",
        150,
        "A domed clay oven built into the wall, still warm, the air around it always smelling of bread.",
    ),
    // ---- Wall & decoration ----
    f(
        "wall_tapestry",
        "a wall tapestry",
        120,
        "A faded tapestry of a unicorn hunt, moth-nibbled at one corner and worth more than the house.",
    ),
    f(
        "hunting_trophy",
        "a hunting trophy",
        100,
        "A stag's head mounted on a shield, antlers spread, regarding the room with glassy reproach.",
    ),
    f(
        "framed_map",
        "a framed map",
        75,
        "A framed map of Lateania, the ink browning, a careful X over somewhere you have never been.",
    ),
    f(
        "oval_mirror",
        "an oval looking-glass",
        110,
        "A silvered oval mirror in a gilt frame that flatters in candlelight and tells the truth at dawn.",
    ),
    f(
        "ancestor_portrait",
        "an ancestor portrait",
        130,
        "A stern painted forebear in a heavy frame, eyes that follow you, jaw set against all your decisions.",
    ),
    f(
        "ship_in_bottle",
        "a ship in a bottle",
        65,
        "A full-rigged caravel impossibly inside a green glass bottle, forever an inch from the open sea.",
    ),
    // ---- Comfort & curios ----
    f(
        "globe_lamp",
        "a star-globe lamp",
        210,
        "A glass globe of slow-drifting motes of light, a captured fragment of night sky on a brass stand.",
    ),
    f(
        "singing_bird",
        "a clockwork songbird",
        230,
        "A jewelled clockwork bird that whirs, preens, and pipes three real tunes before it must be wound again.",
    ),
    f(
        "hookah_lounge",
        "a cushioned lounge",
        190,
        "A low divan heaped with tasselled cushions, made for long evenings and longer stories.",
    ),
    f(
        "prayer_shrine",
        "a hearth shrine",
        70,
        "A small niche shrine to the Dawn, a stub of candle always lit, a place to leave a worry.",
    ),
    f(
        "potted_fern",
        "a potted fern",
        22,
        "A green fern in a glazed pot that somehow thrives where you swear no light reaches.",
    ),
    f(
        "songbird_cage",
        "a finch cage",
        48,
        "A domed wicker cage of chittering finches, all bustle and seed-husks and small joy.",
    ),
    // ---- Workspace ----
    f(
        "spinning_wheel",
        "a spinning wheel",
        90,
        "A treadle wheel with a half-spun bobbin, the floor around it furred with stray wool.",
    ),
    f(
        "workbench",
        "a workbench",
        80,
        "A heavy bench vice-fitted and tool-scarred, the air above it bright with sawdust and intent.",
    ),
    f(
        "alchemy_bench",
        "an alchemy bench",
        260,
        "A scorched bench of retorts and bubbling glass, the faint reek of sulphur a permanent tenant.",
    ),
    f(
        "loom",
        "a standing loom",
        200,
        "A tall floor loom warped with bright thread, a half-made blanket growing row by patient row.",
    ),
    // ---- Grand ----
    f(
        "pipe_organ",
        "a small pipe organ",
        700,
        "A chapel organ of gleaming pipes that fills the whole tower with sound and the neighbours with opinions.",
    ),
    f(
        "indoor_fountain",
        "an indoor fountain",
        540,
        "A marble basin where a stone fish spouts a thread of water that whispers all night long.",
    ),
    f(
        "starlit_orrery",
        "a brass orrery",
        760,
        "A clockwork orrery of sun, moons, and worlds that turn at the truth's own pace beneath a domed ceiling.",
    ),
];

/// Look up a furnishing by its stable key.
pub fn furniture_by_key(key: &str) -> Option<&'static Furniture> {
    FURNITURE.iter().find(|x| x.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_has_fifty_plus_unique_pieces() {
        assert!(FURNITURE.len() >= 50, "at least fifty furnishings");
        let mut keys: Vec<&str> = FURNITURE.iter().map(|x| x.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), FURNITURE.len(), "furniture keys are unique");
        for x in FURNITURE {
            assert!(x.desc.len() > 30, "{} has a real description", x.key);
            assert!(furniture_by_key(x.key).is_some());
        }
    }

    #[test]
    fn plots_do_not_overlap_and_map_back() {
        for (i, t) in TIERS.iter().enumerate() {
            let base = plot_base(i);
            for r in base..base + t.rooms() as RoomId {
                assert_eq!(plot_of_room(r), Some(i), "room {r} maps to plot {i}");
                assert!(is_housing_room(r));
            }
        }
        assert!(is_housing_room(HOUSING_BASE), "the close is a housing room");
        assert_eq!(plot_of_room(HOUSING_BASE), None, "the close is not a plot");
    }
}
