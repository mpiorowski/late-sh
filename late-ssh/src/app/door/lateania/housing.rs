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
    // ---- Trophies & the wider world ----
    f(
        "dragon_skull",
        "a dragon's skull",
        900,
        "A wyrm's bleached skull the size of a cart, jaws propped open, a story you only half believe mounted over the door.",
    ),
    f(
        "kraken_tankard",
        "a kraken-ivory tankard",
        70,
        "A drinking horn carved from a kraken's beak, said to keep ale cold and boasts tall.",
    ),
    f(
        "trophy_rack",
        "a weapon trophy rack",
        130,
        "A wall rack of retired blades and a notched axe, each with a name and a grudge behind it.",
    ),
    f(
        "map_table",
        "a campaign map table",
        240,
        "A great table sunk with a relief map of Lateania, little carved tokens still mid-march across it.",
    ),
    f(
        "ships_wheel",
        "a ship's wheel",
        110,
        "A salt-silvered wheel from some lost caravel, mounted on the wall to be spun on stormy nights.",
    ),
    f(
        "reaches_pearl",
        "an abyssal pearl",
        480,
        "A pearl the size of a fist that holds a slow drift of light, dredged from the Sundered Reaches at great cost.",
    ),
    // ---- Garden & green ----
    f(
        "herb_planter",
        "a window planter",
        28,
        "A long clay planter of basil and bright nasturtiums leaning toward what light there is.",
    ),
    f(
        "orange_tree",
        "a potted orange tree",
        130,
        "A dwarf orange tree in a glazed tub, three real fruit on it, the whole room the better for its scent.",
    ),
    f(
        "koi_basin",
        "a koi basin",
        220,
        "A wide ceramic basin where fat orange koi turn slow circles and rise hopefully at every footstep.",
    ),
    f(
        "bonsai",
        "a bonsai pine",
        95,
        "A gnarled little pine in a shallow dish, a whole windswept mountainside shrunk to the span of a hand.",
    ),
    f(
        "topiary_stag",
        "a topiary stag",
        160,
        "A box-hedge clipped into a leaping stag, kept indoors in a half-barrel and trimmed with quiet pride.",
    ),
    // ---- Light & wonder ----
    f(
        "witchlight_lamp",
        "a witchlight lamp",
        150,
        "A lamp of cold green marshfire sealed in glass, throwing shadows that lean the wrong way.",
    ),
    f(
        "sunstone",
        "a glowing sunstone",
        260,
        "A fist of amber stone that drinks the day and gives it slowly back through the night, warm to the touch.",
    ),
    f(
        "paper_lanterns",
        "a string of paper lanterns",
        40,
        "A looping string of painted paper lanterns that turns any rafter into a festival.",
    ),
    f(
        "hearthfire_glass",
        "a hearthfire stained-glass",
        300,
        "A leaded window of ruby and gold that, lit from behind, sets a permanent sunset on the far wall.",
    ),
    // ---- Sound & study ----
    f(
        "lute_stand",
        "a lute on a stand",
        85,
        "A pear-bellied lute resting on a carved stand, one string always a hair flat, forever waiting.",
    ),
    f(
        "music_box",
        "a rosewood music box",
        130,
        "A rosewood box that, wound, picks out a small sad waltz on steel teeth before running slowly down.",
    ),
    f(
        "celestial_globe",
        "a celestial globe",
        200,
        "A globe of the night sky, constellations picked out in tiny brass studs you can read by touch.",
    ),
    f(
        "reading_nook",
        "a cushioned reading nook",
        175,
        "A window seat heaped with cushions and a forgotten blanket, the single best chair you will ever own.",
    ),
    f(
        "quill_set",
        "an inkstand and quills",
        45,
        "A brass inkstand with a fan of trimmed swan quills and a blotter ruined by better letters than yours.",
    ),
    // ---- Comfort & hearth ----
    f(
        "pelt_pile",
        "a pile of furs",
        60,
        "A heaped tumble of soft pelts by the fire, claimed nightly by whoever - or whatever - reaches it first.",
    ),
    f(
        "steaming_bath",
        "a copper bath",
        280,
        "A deep copper tub you fill from the kettle-crane, the one true luxury after a season in the Reaches.",
    ),
    f(
        "cat_basket",
        "a cat's basket",
        20,
        "A round wicker basket with a worn cushion, empty just now, but the indentation says otherwise.",
    ),
    f(
        "welcome_rug",
        "a woven welcome mat",
        16,
        "A coarse coir mat at the threshold, the word WELCOME picked out in faded red, meaning every letter.",
    ),
    // ---- Adventurer's hall: hard-won trophies & prestige pieces ----
    f(
        "trophy_wall",
        "a mounted trophy wall",
        520,
        "A long oak board hung with the spoils of a life on the road: a cracked boss-fang, a notched blade, a scorched banner. Every piece a story you lived to tell.",
    ),
    f(
        "war_table",
        "a campaigner's war table",
        640,
        "A great table under a map of all the known world, its every region weighted down with carved markers - the taken cities, the drowned isles, the ashen reach - moved by a hand that has walked them.",
    ),
    f(
        "wyrm_skull_mount",
        "a mounted wyrm skull",
        900,
        "A vast reptilian skull bolted above the hearth, sockets big enough to put your fist in. Guests go quiet when they notice it is real.",
    ),
    f(
        "training_dummy",
        "a padded training dummy",
        140,
        "A straw-stuffed pell on a stout post, its canvas hide split and mended a hundred times. It never hits back, and it never complains.",
    ),
    f(
        "relic_cabinet",
        "a glass relic cabinet",
        480,
        "A locked cabinet of leaded glass displaying the odd treasures no merchant would price: a tide-king's pearl, a heartwood fetish, a shard of the sundering wound.",
    ),
    f(
        "champions_banner",
        "a champion's banner",
        300,
        "A tall standard on a gilded pole, its device your own, hung from a home that a nobody could never have afforded. You are not a nobody.",
    ),
    f(
        "hearthstone_forge",
        "a small hearthstone forge",
        420,
        "A compact home forge with bellows and a cooling-trough, just the thing for a smith who would rather work by their own fire than the public one.",
    ),
    f(
        "star_orrery",
        "a brass star-orrery",
        760,
        "A delicate clockwork of the heavens, its planets creeping on brass arms. Wind it and the whole slow dance of the sky turns in your parlour.",
    ),
    f(
        "memorial_candle",
        "an everburning memorial candle",
        160,
        "A fat beeswax candle in a shrine-niche that the draughts never seem to reach, lit for the friends the road took. It has not gone out yet.",
    ),
    f(
        "grand_hearth",
        "a great roaring hearth",
        700,
        "A vast fieldstone fireplace big enough to stand in, throwing heat and amber light into every corner. A home is only ever as warm as its fire, and this one is an inferno of welcome.",
    ),
];

/// Look up a furnishing by its stable key.
pub fn furniture_by_key(key: &str) -> Option<&'static Furniture> {
    FURNITURE.iter().find(|x| x.key == key)
}

#[cfg(test)]
#[path = "housing_test.rs"]
mod housing_test;

