// Items, equipment, inventory, and shop NPCs for Lateania.
//
// Items are static data with stat modifiers. A character carries an inventory of
// item ids and equips one item per slot; equipping recomputes derived stats.
// Consumables apply an effect when used. Shops are NPC-run storefronts in the
// town of Embergate, each NPC keyed to a room and selling a themed catalog.

use std::sync::OnceLock;

use super::classes::Class;

/// Where an item can be worn. Consumables and valuables have no slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Weapon,
    Head,
    Chest,
    Legs,
    Hands,
    Feet,
    Ring,
    Trinket,
}

impl Slot {
    pub fn label(self) -> &'static str {
        match self {
            Self::Weapon => "weapon",
            Self::Head => "head",
            Self::Chest => "chest",
            Self::Legs => "legs",
            Self::Hands => "hands",
            Self::Feet => "feet",
            Self::Ring => "ring",
            Self::Trinket => "trinket",
        }
    }

    pub const WEARABLE: [Slot; 8] = [
        Slot::Weapon,
        Slot::Head,
        Slot::Chest,
        Slot::Legs,
        Slot::Hands,
        Slot::Feet,
        Slot::Ring,
        Slot::Trinket,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Epic,
    Legendary,
}

impl Rarity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Common => "common",
            Self::Uncommon => "uncommon",
            Self::Rare => "rare",
            Self::Epic => "epic",
            Self::Legendary => "legendary",
        }
    }
}

/// What kind of thing an item is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    /// Worn in a slot; contributes stat mods.
    Equipment(Slot),
    /// Used from inventory; heals or restores resource.
    Consumable { heal: i32, restore: i32 },
    /// Sold for gold; no other use.
    Valuable,
}

/// Flat stat bonuses an equipped item grants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StatMods {
    pub attack: i32,
    pub max_hp: i32,
    pub armor: i32,
}

/// A static item definition.
#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub id: u32,
    pub name: &'static str,
    pub desc: &'static str,
    pub kind: ItemKind,
    pub rarity: Rarity,
    pub mods: StatMods,
    /// Buy price in gold; sells back at roughly half.
    pub price: i64,
    /// If set, this gear is tuned for one class (a hint, not a hard restriction).
    pub class_hint: Option<Class>,
}

impl Item {
    pub fn slot(&self) -> Option<Slot> {
        match self.kind {
            ItemKind::Equipment(slot) => Some(slot),
            _ => None,
        }
    }

    pub fn sell_price(&self) -> i64 {
        (self.price / 2).max(1)
    }

    /// A single "how good is this gear" score, for comparing two items in the
    /// same slot. Attack and armor weigh full; HP is cheaper per point. Non-gear
    /// scores 0. Used for upgrade highlighting and the "sell non-upgrades" batch.
    pub fn power(&self) -> i32 {
        match self.kind {
            ItemKind::Equipment(_) => self.mods.attack * 3 + self.mods.armor * 3 + self.mods.max_hp,
            _ => 0,
        }
    }

    /// A compact one-line summary of what the item does, for the inventory and
    /// shop panels: e.g. "+8 atk", "+10 hp +2 arm", "heal 30 / +20 res", or a
    /// sell-value hint for valuables.
    pub fn stat_summary(&self) -> String {
        match self.kind {
            ItemKind::Equipment(_) => {
                let mut parts = Vec::new();
                if self.mods.attack != 0 {
                    parts.push(format!("{:+} atk", self.mods.attack));
                }
                if self.mods.max_hp != 0 {
                    parts.push(format!("{:+} hp", self.mods.max_hp));
                }
                if self.mods.armor != 0 {
                    parts.push(format!("{:+} arm", self.mods.armor));
                }
                parts.join(" ")
            }
            ItemKind::Consumable { heal, restore } => {
                let mut parts = Vec::new();
                if heal != 0 {
                    parts.push(format!("heal {heal}"));
                }
                if restore != 0 {
                    parts.push(format!("+{restore} res"));
                }
                parts.join(" / ")
            }
            ItemKind::Valuable => format!("valuable / sell {}g", self.sell_price()),
        }
    }
}

#[allow(clippy::too_many_arguments)]
const fn eq(
    id: u32,
    name: &'static str,
    desc: &'static str,
    slot: Slot,
    rarity: Rarity,
    attack: i32,
    max_hp: i32,
    armor: i32,
    price: i64,
    class_hint: Option<Class>,
) -> Item {
    Item {
        id,
        name,
        desc,
        kind: ItemKind::Equipment(slot),
        rarity,
        mods: StatMods {
            attack,
            max_hp,
            armor,
        },
        price,
        class_hint,
    }
}

const fn consumable(
    id: u32,
    name: &'static str,
    desc: &'static str,
    rarity: Rarity,
    heal: i32,
    restore: i32,
    price: i64,
) -> Item {
    Item {
        id,
        name,
        desc,
        kind: ItemKind::Consumable { heal, restore },
        rarity,
        mods: StatMods {
            attack: 0,
            max_hp: 0,
            armor: 0,
        },
        price,
        class_hint: None,
    }
}

const fn valuable(
    id: u32,
    name: &'static str,
    desc: &'static str,
    rarity: Rarity,
    price: i64,
) -> Item {
    Item {
        id,
        name,
        desc,
        kind: ItemKind::Valuable,
        rarity,
        mods: StatMods {
            attack: 0,
            max_hp: 0,
            armor: 0,
        },
        price,
        class_hint: None,
    }
}

/// The full item catalog.
pub const BONEWRIGHT_SCEPTER_ID: u32 = 1011;
pub const HEARTWOOD_THORNBLADE_ID: u32 = 1012;
pub const ABYSSAL_HARPOON_ID: u32 = 1013;
pub const CRYPT_SAINT_COIF_ID: u32 = 1123;
pub const THORNHIDE_GRIPS_ID: u32 = 1124;
pub const TIDEBLACK_CARAPACE_ID: u32 = 1125;
pub const RELIQUARY_SIGIL_ID: u32 = 1208;
pub const HEART_TREE_CHARM_ID: u32 = 1209;
pub const DEEPCURRENT_BAND_ID: u32 = 1210;
pub const CATACOMBS_RELIC_ID: u32 = 1402;
pub const THORNWOOD_RELIC_ID: u32 = 1403;
pub const CAVERNS_RELIC_ID: u32 = 1404;

pub const ITEMS: &[Item] = &[
    // ---- Weapons (the Smithy) -------------------------------------------
    eq(
        1000,
        "Rusty Shortsword",
        "A pitted blade, but it holds an edge.",
        Slot::Weapon,
        Rarity::Common,
        4,
        0,
        0,
        25,
        None,
    ),
    eq(
        1001,
        "Iron Longsword",
        "Honest steel, balanced and keen.",
        Slot::Weapon,
        Rarity::Common,
        8,
        0,
        0,
        80,
        Some(Class::Warrior),
    ),
    eq(
        1002,
        "Oak Hunting Bow",
        "A supple bow strung with waxed gut.",
        Slot::Weapon,
        Rarity::Common,
        8,
        0,
        0,
        80,
        Some(Class::Ranger),
    ),
    eq(
        1003,
        "Apprentice Staff",
        "Carved with channels for raw mana.",
        Slot::Weapon,
        Rarity::Common,
        7,
        0,
        0,
        75,
        Some(Class::Mage),
    ),
    eq(
        1004,
        "Twin Daggers",
        "A matched pair, light and wickedly quick.",
        Slot::Weapon,
        Rarity::Uncommon,
        9,
        0,
        0,
        110,
        Some(Class::Rogue),
    ),
    eq(
        1005,
        "Blessed Mace",
        "Its head is graven with the rising sun.",
        Slot::Weapon,
        Rarity::Uncommon,
        8,
        6,
        0,
        120,
        Some(Class::Cleric),
    ),
    eq(
        1006,
        "Steel Greatsword",
        "A two-handed brute that bites through mail.",
        Slot::Weapon,
        Rarity::Rare,
        16,
        0,
        0,
        320,
        Some(Class::Warrior),
    ),
    eq(
        1007,
        "Yew Warbow",
        "Tall as a man and twice as unforgiving.",
        Slot::Weapon,
        Rarity::Rare,
        15,
        0,
        0,
        300,
        Some(Class::Ranger),
    ),
    eq(
        1008,
        "Runed Battlestaff",
        "Old runes wake and glow when you hold it.",
        Slot::Weapon,
        Rarity::Rare,
        15,
        0,
        0,
        300,
        Some(Class::Mage),
    ),
    eq(
        1009,
        "Embergate Falchion",
        "Forged in the town's own furnace; ever warm.",
        Slot::Weapon,
        Rarity::Epic,
        24,
        8,
        0,
        900,
        None,
    ),
    eq(
        1010,
        "Mythril Arming Sword",
        "A masterwork blade commissioned for adventurers with more gold than caution.",
        Slot::Weapon,
        Rarity::Legendary,
        34,
        16,
        0,
        2600,
        None,
    ),
    eq(
        BONEWRIGHT_SCEPTER_ID,
        "Bonewright Scepter",
        "A black-bone rod still warm with stolen grave-lamp fire.",
        Slot::Weapon,
        Rarity::Epic,
        28,
        12,
        0,
        1400,
        None,
    ),
    eq(
        HEARTWOOD_THORNBLADE_ID,
        "Heartwood Thornblade",
        "A living blade of heartwood and hooked green-black thorn.",
        Slot::Weapon,
        Rarity::Epic,
        30,
        18,
        0,
        1550,
        None,
    ),
    eq(
        ABYSSAL_HARPOON_ID,
        "Abyssal Harpoon",
        "A barbed spear that hums with pressure from a lightless deep.",
        Slot::Weapon,
        Rarity::Legendary,
        32,
        20,
        0,
        1750,
        None,
    ),
    // ---- Armor (the Outfitter) ------------------------------------------
    eq(
        1100,
        "Padded Cap",
        "Quilted cloth, better than a bare head.",
        Slot::Head,
        Rarity::Common,
        0,
        6,
        1,
        20,
        None,
    ),
    eq(
        1101,
        "Leather Jerkin",
        "Boiled hide, scarred from a previous owner.",
        Slot::Chest,
        Rarity::Common,
        0,
        12,
        2,
        45,
        None,
    ),
    eq(
        1102,
        "Leather Leggings",
        "Supple and quiet on the road.",
        Slot::Legs,
        Rarity::Common,
        0,
        9,
        2,
        40,
        None,
    ),
    eq(
        1103,
        "Worn Gloves",
        "The fingers are reinforced with hide.",
        Slot::Hands,
        Rarity::Common,
        0,
        4,
        1,
        18,
        None,
    ),
    eq(
        1104,
        "Traveler's Boots",
        "Broken in across a hundred leagues.",
        Slot::Feet,
        Rarity::Common,
        0,
        5,
        1,
        22,
        None,
    ),
    eq(
        1105,
        "Iron Helm",
        "A plain bucket of a helm, but it works.",
        Slot::Head,
        Rarity::Uncommon,
        0,
        14,
        3,
        90,
        Some(Class::Warrior),
    ),
    eq(
        1106,
        "Chainmail Hauberk",
        "Riveted links that turn a blade.",
        Slot::Chest,
        Rarity::Uncommon,
        0,
        26,
        5,
        180,
        Some(Class::Warrior),
    ),
    eq(
        1107,
        "Mage's Robe",
        "Woven with silver thread that hums faintly.",
        Slot::Chest,
        Rarity::Uncommon,
        4,
        16,
        1,
        170,
        Some(Class::Mage),
    ),
    eq(
        1108,
        "Shadowweave Vest",
        "Drinks the light; you are hard to look at.",
        Slot::Chest,
        Rarity::Rare,
        6,
        22,
        3,
        340,
        Some(Class::Rogue),
    ),
    eq(
        1109,
        "Dawnplate Cuirass",
        "Holy steel that gleams even in the dark.",
        Slot::Chest,
        Rarity::Epic,
        4,
        40,
        8,
        880,
        Some(Class::Cleric),
    ),
    eq(
        1110,
        "Scout's Hood",
        "Weatherproof cloth with a narrow shadowing brim.",
        Slot::Head,
        Rarity::Uncommon,
        2,
        10,
        1,
        115,
        Some(Class::Ranger),
    ),
    eq(
        1111,
        "Reinforced Gauntlets",
        "Layered leather and steel plates over the knuckles.",
        Slot::Hands,
        Rarity::Uncommon,
        2,
        9,
        2,
        125,
        Some(Class::Warrior),
    ),
    eq(
        1112,
        "Steel Sallet",
        "A close helm with a narrow, practical visor.",
        Slot::Head,
        Rarity::Rare,
        1,
        24,
        5,
        310,
        None,
    ),
    eq(
        1113,
        "Spellwoven Gloves",
        "Fine gloves stitched with conductive silver thread.",
        Slot::Hands,
        Rarity::Rare,
        5,
        12,
        2,
        320,
        Some(Class::Mage),
    ),
    eq(
        1114,
        "Barrow Crown",
        "A tarnished war-crown taken from a king who refused the grave.",
        Slot::Head,
        Rarity::Rare,
        3,
        28,
        5,
        420,
        None,
    ),
    eq(
        1115,
        "Tidecaller's Grips",
        "Brine-dark gloves that never quite dry.",
        Slot::Hands,
        Rarity::Rare,
        6,
        16,
        2,
        430,
        None,
    ),
    eq(
        1116,
        "Emberguard Helm",
        "Blackened plate with a coal-red glow behind the visor.",
        Slot::Head,
        Rarity::Epic,
        4,
        36,
        7,
        780,
        None,
    ),
    eq(
        1117,
        "Rimeforged Gloves",
        "Gauntlets rimed with frost that hardens around every blow.",
        Slot::Hands,
        Rarity::Epic,
        7,
        22,
        4,
        760,
        None,
    ),
    eq(
        1118,
        "Saintguard Visor",
        "A citadel helm engraved with prayers almost worn smooth.",
        Slot::Head,
        Rarity::Epic,
        5,
        42,
        8,
        920,
        Some(Class::Cleric),
    ),
    eq(
        1119,
        "Abyssal Talons",
        "Demon-forged clawed gauntlets that drink torchlight.",
        Slot::Hands,
        Rarity::Legendary,
        10,
        28,
        5,
        1300,
        None,
    ),
    eq(
        1120,
        "Masterwork Greathelm",
        "A custom-fitted helm from Tomas's locked display case.",
        Slot::Head,
        Rarity::Legendary,
        6,
        52,
        10,
        2400,
        None,
    ),
    eq(
        1121,
        "Masterwork Gauntlets",
        "Perfectly weighted steel, lined with grip-leather and quiet runes.",
        Slot::Hands,
        Rarity::Legendary,
        11,
        30,
        6,
        2400,
        None,
    ),
    eq(
        1122,
        "Runic Warplate",
        "Expensive plate reinforced with every ward the outfitter trusts.",
        Slot::Chest,
        Rarity::Legendary,
        7,
        66,
        13,
        3400,
        None,
    ),
    eq(
        CRYPT_SAINT_COIF_ID,
        "Crypt-Saint Coif",
        "A silvered mail coif sewn with funerary prayers.",
        Slot::Head,
        Rarity::Epic,
        4,
        44,
        8,
        1450,
        None,
    ),
    eq(
        THORNHIDE_GRIPS_ID,
        "Thornhide Grips",
        "Living bark and hide wrapped into cruel hooked gloves.",
        Slot::Hands,
        Rarity::Epic,
        9,
        30,
        5,
        1550,
        None,
    ),
    eq(
        TIDEBLACK_CARAPACE_ID,
        "Tideblack Carapace",
        "A shell cuirass lacquered black by the drowned abyss.",
        Slot::Chest,
        Rarity::Legendary,
        7,
        64,
        13,
        1900,
        None,
    ),
    // ---- Trinkets and rings (the Curio Cart) ----------------------------
    eq(
        1200,
        "Copper Band",
        "A simple ring, faintly lucky.",
        Slot::Ring,
        Rarity::Common,
        1,
        4,
        0,
        30,
        None,
    ),
    eq(
        1201,
        "Garnet Ring",
        "The stone catches firelight and holds it.",
        Slot::Ring,
        Rarity::Uncommon,
        3,
        8,
        0,
        130,
        None,
    ),
    eq(
        1202,
        "Signet of Embergate",
        "Marks the bearer as a friend of the town.",
        Slot::Ring,
        Rarity::Rare,
        5,
        14,
        2,
        360,
        None,
    ),
    eq(
        1203,
        "Hare's-Foot Charm",
        "For luck, and the speed to use it.",
        Slot::Trinket,
        Rarity::Common,
        2,
        3,
        0,
        35,
        None,
    ),
    eq(
        1204,
        "Vial of Saint's Tears",
        "Warm to the touch; it wards off despair.",
        Slot::Trinket,
        Rarity::Uncommon,
        0,
        18,
        2,
        150,
        None,
    ),
    eq(
        1205,
        "Wyrmscale Talisman",
        "A single frost-dragon scale, cold forever.",
        Slot::Trinket,
        Rarity::Epic,
        8,
        20,
        4,
        820,
        None,
    ),
    eq(
        1206,
        "Vaultkeeper's Band",
        "A heavy ring sold only to adventurers who can afford to lose it.",
        Slot::Ring,
        Rarity::Epic,
        8,
        26,
        3,
        1750,
        None,
    ),
    eq(
        1207,
        "Dragonbone Reliquary",
        "A polished dragonbone charm set in a frame of soft gold.",
        Slot::Trinket,
        Rarity::Legendary,
        11,
        34,
        5,
        2700,
        None,
    ),
    eq(
        RELIQUARY_SIGIL_ID,
        "Reliquary Sigil",
        "A saint's seal recast from silver stolen back from the dead.",
        Slot::Ring,
        Rarity::Epic,
        8,
        28,
        3,
        1350,
        None,
    ),
    eq(
        HEART_TREE_CHARM_ID,
        "Heart-Tree Charm",
        "A humming splinter of old heartwood bound in copper wire.",
        Slot::Trinket,
        Rarity::Epic,
        9,
        30,
        4,
        1500,
        None,
    ),
    eq(
        DEEPCURRENT_BAND_ID,
        "Deepcurrent Band",
        "A cold ring that tightens when deep water is near.",
        Slot::Ring,
        Rarity::Legendary,
        10,
        34,
        4,
        1700,
        None,
    ),
    // ---- Consumables (the Apothecary) -----------------------------------
    consumable(
        1300,
        "Minor Healing Draught",
        "A bitter red tonic that closes small wounds.",
        Rarity::Common,
        40,
        0,
        25,
    ),
    consumable(
        1301,
        "Healing Potion",
        "The reliable choice of every sensible adventurer.",
        Rarity::Uncommon,
        90,
        0,
        75,
    ),
    consumable(
        1302,
        "Greater Healing Elixir",
        "Mends even grievous hurts in moments.",
        Rarity::Rare,
        210,
        0,
        165,
    ),
    consumable(
        1303,
        "Draught of Vigor",
        "Restores the fire that fuels your craft.",
        Rarity::Uncommon,
        0,
        80,
        65,
    ),
    consumable(
        1304,
        "Elixir of Renewal",
        "Restores both flesh and will at once.",
        Rarity::Epic,
        180,
        120,
        280,
    ),
    consumable(
        1305,
        "Phoenix Tonic",
        "A bright, expensive cordial for adventurers deep past prudence.",
        Rarity::Legendary,
        420,
        220,
        1500,
    ),
    // ---- Valuables (sold to any merchant) -------------------------------
    Item {
        id: 1400,
        name: "Gold Ingot",
        desc: "A solid bar, good anywhere coin is taken.",
        kind: ItemKind::Valuable,
        rarity: Rarity::Uncommon,
        mods: StatMods {
            attack: 0,
            max_hp: 0,
            armor: 0,
        },
        price: 200,
        class_hint: None,
    },
    Item {
        id: 1401,
        name: "Cut Ruby",
        desc: "A merchant's eyes will light at the sight of it.",
        kind: ItemKind::Valuable,
        rarity: Rarity::Rare,
        mods: StatMods {
            attack: 0,
            max_hp: 0,
            armor: 0,
        },
        price: 500,
        class_hint: None,
    },
    Item {
        id: CATACOMBS_RELIC_ID,
        name: "Catacomb Reliquary",
        desc: "A chapel reliquary recovered from the old crypts below Tasmania.",
        kind: ItemKind::Valuable,
        rarity: Rarity::Rare,
        mods: StatMods {
            attack: 0,
            max_hp: 0,
            armor: 0,
        },
        price: 220,
        class_hint: None,
    },
    Item {
        id: THORNWOOD_RELIC_ID,
        name: "Heartwood Fetish",
        desc: "A knotted charm carved from ancient Thornwood heartwood.",
        kind: ItemKind::Valuable,
        rarity: Rarity::Rare,
        mods: StatMods {
            attack: 0,
            max_hp: 0,
            armor: 0,
        },
        price: 240,
        class_hint: None,
    },
    Item {
        id: CAVERNS_RELIC_ID,
        name: "Abyssal Salvage",
        desc: "A barnacle-crusted keepsake dredged from the Drowned Caverns.",
        kind: ItemKind::Valuable,
        rarity: Rarity::Rare,
        mods: StatMods {
            attack: 0,
            max_hp: 0,
            armor: 0,
        },
        price: 260,
        class_hint: None,
    },
];

// ---- Raw gathering materials --------------------------------------------
//
// Trees, ore veins, fishing spots and herb/skinning patches (see world::NODES)
// drop these raw materials when harvested (see svc gather). They are Valuables
// for now - immediately sellable to any merchant, which is what "tradeable"
// means today - and become crafting inputs in the crafting update. IDs live in
// 4000..4100 (skill index * 20 + tier), clear of the authored (<1500) and
// generated Frontier/Reaches (3000..3400) ranges.

/// Base id for the raw-material catalog.
pub const MATERIAL_BASE: u32 = 4000;
/// Tiers per gathering skill (levels of material, low to high).
pub const MATERIAL_TIERS: u32 = 5;

/// The item id of the raw material a skill drops at a given tier (0-based). The
/// `skill_index` is `skills::GatherSkill::index`.
pub const fn material_id(skill_index: u32, tier: u32) -> u32 {
    MATERIAL_BASE + skill_index * 20 + tier
}

/// Names per skill (rows follow `GatherSkill::index`) and tier (columns low->high).
const MATERIAL_NAMES: [[&str; 5]; 5] = [
    // Woodcutting
    ["Birch Log", "Oak Log", "Ash Log", "Yew Log", "Ironbark Log"],
    // Mining
    [
        "Copper Ore",
        "Tin Ore",
        "Iron Ore",
        "Silver Ore",
        "Mithril Ore",
    ],
    // Fishing
    [
        "River Bream",
        "Silver Trout",
        "Grey Pike",
        "Deep Sturgeon",
        "Moonscale Fish",
    ],
    // Foraging
    [
        "Marsh Sage",
        "Redleaf",
        "Bloodthistle",
        "Frostbloom",
        "Sunmoss",
    ],
    // Skinning
    [
        "Rough Hide",
        "Thick Hide",
        "Boar Hide",
        "Bear Pelt",
        "Direhide",
    ],
];

/// One flavour line per skill (rows follow `GatherSkill::index`).
const MATERIAL_FLAVOR: [&str; 5] = [
    "A length of cut timber, ready for the sawbench.",
    "Raw ore, still cold from the rock; a smith can smelt it down.",
    "A fresh-landed fish, good eating or good bait.",
    "A bundle of cut herbs, pungent and green.",
    "A cleaned hide, ready for the tanner's rack.",
];

fn build_materials() -> Vec<Item> {
    let mut out = Vec::with_capacity(25);
    for (s, names) in MATERIAL_NAMES.iter().enumerate() {
        for (t, name) in names.iter().enumerate() {
            let tier = t as i64;
            // 6, 24, 54, 96, 150 gold: a modest trickle, so gathering feeds
            // crafting rather than replacing combat as a gold source.
            let price = 6 * (tier + 1) * (tier + 1);
            let rarity = match t {
                0 | 1 => Rarity::Common,
                2 | 3 => Rarity::Uncommon,
                _ => Rarity::Rare,
            };
            out.push(Item {
                id: material_id(s as u32, t as u32),
                name,
                desc: MATERIAL_FLAVOR[s],
                kind: ItemKind::Valuable,
                rarity,
                mods: StatMods {
                    attack: 0,
                    max_hp: 0,
                    armor: 0,
                },
                price,
                class_hint: None,
            });
        }
    }
    out
}

/// The raw-material catalog, built once and reused for the `item` lookup.
pub fn materials() -> &'static [Item] {
    static CATALOG: OnceLock<Vec<Item>> = OnceLock::new();
    CATALOG.get_or_init(build_materials)
}

// ---- Crafted goods -------------------------------------------------------
//
// Crafting turns raw materials into refined intermediates (ingots, planks,
// leather) and finished goods (weapons, armor, potions, poisons, food). IDs live
// in 4200..4500, clear of the raw materials (4000..4100). Recipes in
// `crafting.rs` reference these ids by the const helpers below; `item` resolves
// them like any other. Five tiers each, mirroring the material tiers.

pub const CRAFTED_BASE: u32 = 4200;

pub const fn ingot_id(tier: u32) -> u32 {
    CRAFTED_BASE + tier
}
pub const fn plank_id(tier: u32) -> u32 {
    CRAFTED_BASE + 20 + tier
}
pub const fn leather_id(tier: u32) -> u32 {
    CRAFTED_BASE + 40 + tier
}
pub const fn smith_weapon_id(tier: u32) -> u32 {
    CRAFTED_BASE + 100 + tier
}
pub const fn smith_armor_id(tier: u32) -> u32 {
    CRAFTED_BASE + 120 + tier
}
pub const fn wood_weapon_id(tier: u32) -> u32 {
    CRAFTED_BASE + 140 + tier
}
pub const fn leather_armor_id(tier: u32) -> u32 {
    CRAFTED_BASE + 160 + tier
}
pub const fn potion_id(tier: u32) -> u32 {
    CRAFTED_BASE + 200 + tier
}
pub const fn poison_id(tier: u32) -> u32 {
    CRAFTED_BASE + 220 + tier
}
pub const fn food_id(tier: u32) -> u32 {
    CRAFTED_BASE + 240 + tier
}
/// Masterwork gear (the endgame recipe sinks); `n` is 0 (blade) or 1 (plate).
pub const fn masterwork_id(n: u32) -> u32 {
    CRAFTED_BASE + 180 + n
}

/// The tier of a poison item id, if `id` is one (used to route it to the
/// weapon-coating action instead of the normal consumable path).
pub fn poison_tier(id: u32) -> Option<u32> {
    (0..5).find(|&t| poison_id(t) == id)
}

/// The tier of a cooked-food item id, if `id` is one (food grants a well-fed
/// regen buff on top of its heal).
pub fn food_tier(id: u32) -> Option<u32> {
    (0..5).find(|&t| food_id(t) == id)
}

const INGOT_NAMES: [&str; 5] = [
    "Copper Ingot",
    "Tin Ingot",
    "Iron Ingot",
    "Silver Ingot",
    "Mithril Ingot",
];
const PLANK_NAMES: [&str; 5] = [
    "Birch Plank",
    "Oak Plank",
    "Ash Plank",
    "Yew Plank",
    "Ironbark Plank",
];
const LEATHER_NAMES: [&str; 5] = [
    "Rough Leather",
    "Thick Leather",
    "Boar Leather",
    "Bear Leather",
    "Dire Leather",
];
const SMITH_WEAPON_NAMES: [&str; 5] = [
    "Copper Sword",
    "Tin Sabre",
    "Iron Sword",
    "Silver Sword",
    "Mithril Sword",
];
const SMITH_ARMOR_NAMES: [&str; 5] = [
    "Copper Cuirass",
    "Tin Cuirass",
    "Iron Cuirass",
    "Silver Cuirass",
    "Mithril Cuirass",
];
const WOOD_WEAPON_NAMES: [&str; 5] = [
    "Birch Shortbow",
    "Oak Longbow",
    "Ash Recurve",
    "Yew Warbow",
    "Ironbark Greatbow",
];
const LEATHER_ARMOR_NAMES: [&str; 5] = [
    "Rough Jerkin",
    "Thick Jerkin",
    "Boarhide Vest",
    "Bearhide Coat",
    "Direhide Cuirass",
];
const POTION_NAMES: [&str; 5] = [
    "Minor Healing Draught",
    "Lesser Healing Draught",
    "Greater Healing Draught",
    "Superior Healing Draught",
    "Master Healing Draught",
];
const POISON_NAMES: [&str; 5] = [
    "Weak Toxin",
    "Numbing Poison",
    "Virulent Bile",
    "Deadly Venom",
    "Wyrm Venom",
];
const FOOD_NAMES: [&str; 5] = [
    "Grilled Bream",
    "Pan-Seared Trout",
    "Smoked Pike",
    "Sturgeon Steak",
    "Moonscale Feast",
];

const INTER_RARITY: [Rarity; 5] = [
    Rarity::Common,
    Rarity::Common,
    Rarity::Uncommon,
    Rarity::Uncommon,
    Rarity::Rare,
];
const FINAL_RARITY: [Rarity; 5] = [
    Rarity::Common,
    Rarity::Uncommon,
    Rarity::Uncommon,
    Rarity::Rare,
    Rarity::Epic,
];

fn build_crafted() -> Vec<Item> {
    let mut out = Vec::new();
    // Per-tier stat/price tables (index 0..5, low to high).
    const INGOT_PRICE: [i64; 5] = [24, 54, 96, 150, 220];
    const PLANK_PRICE: [i64; 5] = [20, 46, 84, 130, 190];
    const LEATHER_PRICE: [i64; 5] = [22, 50, 90, 140, 205];
    const WEAPON_ATK: [i32; 5] = [6, 11, 16, 21, 26];
    const WEAPON_PRICE: [i64; 5] = [60, 140, 260, 440, 700];
    const BOW_ATK: [i32; 5] = [5, 10, 15, 20, 25];
    const BOW_PRICE: [i64; 5] = [55, 130, 250, 430, 690];
    const PLATE_HP: [i32; 5] = [8, 16, 26, 40, 60];
    const PLATE_ARM: [i32; 5] = [1, 2, 3, 4, 6];
    const PLATE_PRICE: [i64; 5] = [70, 150, 280, 460, 720];
    const JERKIN_HP: [i32; 5] = [6, 12, 20, 30, 44];
    const JERKIN_ARM: [i32; 5] = [1, 1, 2, 3, 4];
    const JERKIN_PRICE: [i64; 5] = [50, 120, 230, 400, 640];
    const POTION_HEAL: [i32; 5] = [25, 45, 75, 120, 180];
    const POTION_PRICE: [i64; 5] = [20, 45, 90, 160, 260];
    const POISON_PRICE: [i64; 5] = [15, 40, 80, 140, 220];
    const FOOD_HEAL: [i32; 5] = [20, 35, 55, 85, 130];
    const FOOD_REST: [i32; 5] = [10, 20, 35, 55, 85];
    const FOOD_PRICE: [i64; 5] = [15, 35, 70, 120, 190];

    for t in 0..5usize {
        let tu = t as u32;
        // Intermediates (sellable valuables and recipe inputs).
        out.push(valuable(
            ingot_id(tu),
            INGOT_NAMES[t],
            "A refined metal bar, ready for the forge.",
            INTER_RARITY[t],
            INGOT_PRICE[t],
        ));
        out.push(valuable(
            plank_id(tu),
            PLANK_NAMES[t],
            "A planed board, true and square for the workbench.",
            INTER_RARITY[t],
            PLANK_PRICE[t],
        ));
        out.push(valuable(
            leather_id(tu),
            LEATHER_NAMES[t],
            "Supple tanned leather, ready to be worked.",
            INTER_RARITY[t],
            LEATHER_PRICE[t],
        ));
        // Finished goods.
        out.push(eq(
            smith_weapon_id(tu),
            SMITH_WEAPON_NAMES[t],
            "Forged steel with a keen, hammered edge.",
            Slot::Weapon,
            FINAL_RARITY[t],
            WEAPON_ATK[t],
            0,
            0,
            WEAPON_PRICE[t],
            None,
        ));
        out.push(eq(
            smith_armor_id(tu),
            SMITH_ARMOR_NAMES[t],
            "A forged breastplate, proof against a hard blow.",
            Slot::Chest,
            FINAL_RARITY[t],
            0,
            PLATE_HP[t],
            PLATE_ARM[t],
            PLATE_PRICE[t],
            None,
        ));
        out.push(eq(
            wood_weapon_id(tu),
            WOOD_WEAPON_NAMES[t],
            "A supple bow of seasoned wood, strung and true.",
            Slot::Weapon,
            FINAL_RARITY[t],
            BOW_ATK[t],
            0,
            0,
            BOW_PRICE[t],
            Some(Class::Ranger),
        ));
        out.push(eq(
            leather_armor_id(tu),
            LEATHER_ARMOR_NAMES[t],
            "Light leather armor that never slows a step.",
            Slot::Chest,
            FINAL_RARITY[t],
            0,
            JERKIN_HP[t],
            JERKIN_ARM[t],
            JERKIN_PRICE[t],
            None,
        ));
        out.push(consumable(
            potion_id(tu),
            POTION_NAMES[t],
            "A brewed cordial that knits wounds closed.",
            FINAL_RARITY[t],
            POTION_HEAL[t],
            0,
            POTION_PRICE[t],
        ));
        // Poisons are sellable for now; the depth update makes them applyable.
        out.push(valuable(
            poison_id(tu),
            POISON_NAMES[t],
            "A stoppered vial of poison, meant to coat a blade.",
            FINAL_RARITY[t],
            POISON_PRICE[t],
        ));
        out.push(consumable(
            food_id(tu),
            FOOD_NAMES[t],
            "A hot cooked meal that restores body and focus.",
            FINAL_RARITY[t],
            FOOD_HEAL[t],
            FOOD_REST[t],
            FOOD_PRICE[t],
        ));
    }
    // Masterwork gear: the endgame smithing sinks, made from many top-tier
    // materials at high skill. A clear step above the tier-4 craftables.
    out.push(eq(
        masterwork_id(0),
        "Masterwork Greatblade",
        "A flawless blade of folded mithril, the work of a master's whole art.",
        Slot::Weapon,
        Rarity::Legendary,
        34,
        0,
        0,
        1600,
        None,
    ));
    out.push(eq(
        masterwork_id(1),
        "Masterwork Plate",
        "A suit of mirror-bright mithril plate, proof against nearly anything.",
        Slot::Chest,
        Rarity::Legendary,
        0,
        80,
        8,
        1700,
        None,
    ));
    out
}

/// The crafted-goods catalog, built once and reused for the `item` lookup.
pub fn crafted() -> &'static [Item] {
    static CATALOG: OnceLock<Vec<Item>> = OnceLock::new();
    CATALOG.get_or_init(build_crafted)
}

// ---- The Sunderlakes fish catalog (ids 4600..4700) -----------------------
//
// Forty distinct fish species netted, angled, and speared across the lake
// country of the Sunderlakes (see world::extend_lakes). They sit in a fresh
// 4600..4700 band, clear of the raw materials (4000..4100), crafted goods
// (4200..4500), and the generated Frontier/Reaches/Kaelmyr loot (3000..3600).
//
// Each fish is a resource-node yield with a *varying* sell price - a wide
// spread from a few-gold minnow to a prized several-hundred-gold catch, so a
// deep-water haul is a real reward. Roughly a third are edible: those are
// `Consumable`s that heal and/or restore resource, scaling with the fish's
// prestige, and a handful of the rarest carry a "special" - a well-fed
// `HealOverTime` (see `fish_well_fed` / `use_item`) that makes a legendary
// fish genuinely worth eating rather than only selling. The rest are pure
// `Valuable` sell loot. Everything resolves through `item(id)`.

/// Base id for the Sunderlakes fish catalog.
pub const FISH_BASE: u32 = 4600;
/// Number of distinct fish species.
pub const FISH_COUNT: u32 = 40;

/// A fish species definition, compiled into the catalog. `heal`/`restore` of 0
/// means a pure `Valuable` (sell-only); non-zero makes it an edible
/// `Consumable`. `well_fed` (if set) is the per-tick well-fed regen a special
/// fish grants when eaten (see `fish_well_fed`).
struct FishDef {
    /// Offset from `FISH_BASE`; also the species' place in the catalog.
    slot: u32,
    name: &'static str,
    desc: &'static str,
    rarity: Rarity,
    price: i64,
    heal: i32,
    restore: i32,
    well_fed: i32,
}

#[allow(clippy::too_many_arguments)]
const fn fishdef(
    slot: u32,
    name: &'static str,
    desc: &'static str,
    rarity: Rarity,
    price: i64,
    heal: i32,
    restore: i32,
    well_fed: i32,
) -> FishDef {
    FishDef {
        slot,
        name,
        desc,
        rarity,
        price,
        heal,
        restore,
        well_fed,
    }
}

/// The forty fish of the Sunderlakes, ordered roughly by the Fishing level and
/// zone depth at which they are caught: small shallow-water fish first, prized
/// deep-water and drowned-valley catches last. Slots are contiguous 0..40.
#[rustfmt::skip]
const FISH_DEFS: [FishDef; 40] = [
    // --- Shallow reed-water: cheap, plentiful, a few humble edibles ---------
    fishdef(0,  "Silver Minnow",       "A palmful of quicksilver, barely worth the hook - but they shoal in their thousands.", Rarity::Common, 8,   0,  0,  0),
    fishdef(1,  "Reed Perch",          "A striped little perch that hangs in the reed-shadows. Bony, but honest eating.",        Rarity::Common, 14,  10, 0,  0),
    fishdef(2,  "Mudsnout Carp",       "A whiskered bottom-feeder the colour of the mire it grubs in.",                          Rarity::Common, 18,  0,  0,  0),
    fishdef(3,  "Copperscale Roach",   "A common roach that flashes copper when it turns in the shallows.",                      Rarity::Common, 22,  0,  0,  0),
    fishdef(4,  "Marsh Bream",         "A broad, slab-sided bream that fights well above its weight in the weed.",               Rarity::Common, 30,  16, 0,  0),
    fishdef(5,  "Bristle Loach",       "A spiny loach that clings to the stones; the meres are thick with them.",                Rarity::Common, 26,  0,  0,  0),
    fishdef(6,  "Fenwater Tench",      "A stubborn olive tench, slick with the healing slime the fen-folk prize.",               Rarity::Uncommon, 44, 24, 8,  0),
    fishdef(7,  "Islet Rudd",          "A red-finned rudd that patrols the island shallows in bright, wary schools.",            Rarity::Common, 34,  0,  0,  0),
    // --- Open meres & flooded caverns: mid-value, sturdier fish -------------
    fishdef(8,  "Blue Mere Trout",     "A cold-water trout gone deep blue in the still meres. A fine table fish.",               Rarity::Uncommon, 60, 34, 0,  0),
    fishdef(9,  "Ghost Grayling",      "A pale, half-translucent grayling that seems to swim through the water like smoke.",     Rarity::Uncommon, 72, 0,  0,  0),
    fishdef(10, "Cavern Blindfish",    "An eyeless white fish of the flooded caves, feeling its way through the dark.",          Rarity::Uncommon, 88, 0,  0,  0),
    fishdef(11, "Reedmace Pike",       "A lean ambush-pike that lies like a green log among the reeds.",                         Rarity::Uncommon, 96, 40, 0,  0),
    fishdef(12, "Sunken Char",         "A deep-dwelling char, its belly banded rose and gold from the cold dark.",               Rarity::Uncommon, 110, 46, 12, 0),
    fishdef(13, "Drowned Valley Eel",  "A long muscular eel that threads the flooded orchards of the drowned valleys.",          Rarity::Uncommon, 84, 0,  0,  0),
    fishdef(14, "Lanternjaw",          "A cave-fish that dangles a wisp of cold blue light before its own gaping mouth.",        Rarity::Rare, 140, 0,  0,  0),
    fishdef(15, "Silt-Gilded Barbel",  "A big golden barbel that roots the deep silt, its scales edged like beaten coin.",       Rarity::Rare, 165, 0,  0,  0),
    // --- Deep water & mere-hearts: rarer, richer, restorative catches -------
    fishdef(16, "Moonpale Salmon",     "A salmon that runs the deep channels only by moonlight, its flesh rich and pink.",       Rarity::Rare, 185, 60, 18, 0),
    fishdef(17, "Glasswater Sturgeon", "An armoured sturgeon of the clearest deeps, old as the meres themselves.",               Rarity::Rare, 210, 0,  0,  0),
    fishdef(18, "Meregleam Tench",     "A tench whose scales hold a faint inner glow, drawn up from lightless water.",           Rarity::Rare, 175, 55, 20, 0),
    fishdef(19, "Stormfin Bass",       "A powerful bass that feeds hardest under a breaking storm, thick and fighting-fit.",     Rarity::Rare, 155, 0,  0,  0),
    fishdef(20, "Hollow-Cavern Ray",   "A pale freshwater ray that glides the drowned cavern-halls like a slow ghost.",          Rarity::Rare, 230, 0,  0,  0),
    fishdef(21, "Bittern's Bane",      "A vicious spined predator the marsh-birds have learned to leave well alone.",            Rarity::Rare, 195, 0,  0,  0),
    fishdef(22, "Amberweed Golden",    "A goldfish grown huge and lordly in the amber weed-beds, worth a merchant's smile.",     Rarity::Rare, 260, 0,  0,  0),
    fishdef(23, "Frostmere Whitefish", "A silver whitefish of the highest, coldest meres, its meat firm and clean.",             Rarity::Rare, 205, 70, 24, 0),
    // --- The prized deeps: the trophy fish anglers boast of ------------------
    fishdef(24, "Kingfisher's Prize",  "The great striped perch every angler swears is a myth until the line goes taut.",        Rarity::Epic, 320, 0,  0,  0),
    fishdef(25, "Deep Meregold",       "A slab of living gold from the mere-hearts; a single scale would buy supper.",           Rarity::Epic, 380, 0,  0,  0),
    fishdef(26, "Silverback Salmon",   "A monster salmon, silver-backed and heavy as a hound, that runs the sunken falls.",      Rarity::Epic, 340, 95, 30, 0),
    fishdef(27, "Drowned-God Carp",    "A vast, slow, ancient carp the fen-shrines were raised to honour. Uncanny to hold.",     Rarity::Epic, 420, 0,  0,  0),
    fishdef(28, "Voidmere Sturgeon",   "A black sturgeon from the deepest drowned trench, scaled like old iron.",                Rarity::Epic, 460, 0,  0,  0),
    fishdef(29, "Ghostlight Pike",     "A pike lit from within by a drowned corpse-glow; the old anglers make a warding sign.",  Rarity::Epic, 390, 0,  0,  0),
    fishdef(30, "Tempest Marlin",      "A freshwater marlin that leaps the storm-swells, its bill sharp as a boarding-pike.",    Rarity::Epic, 440, 110, 34, 0),
    fishdef(31, "Abyss Anglerfish",    "A horror of the lightless deep, all teeth and a single cold luring lamp.",               Rarity::Epic, 405, 0,  0,  0),
    // --- Legends of the Sunderlakes: the specials worth eating --------------
    fishdef(32, "Sunderlake Leviathan","A young leviathan of the drowned deeps; men have retired on a single one.",              Rarity::Legendary, 540, 0,   0,  0),
    fishdef(33, "The Mere-Mother",     "A carp so old and so vast the fen-folk name her a minor goddess. To land her is a saga.", Rarity::Legendary, 620, 150, 50, 5),
    fishdef(34, "Moonscale Royal",     "The true moonscale, silver-white and shining; a bite of it mends flesh and spirit both.", Rarity::Legendary, 580, 140, 45, 4),
    fishdef(35, "Drowned Crown Bass",  "A bass that wears a crown-crest of gold spines, king of some sunken lake-court.",         Rarity::Legendary, 560, 0,   0,  0),
    fishdef(36, "Heartglow Trout",     "A trout that burns a warm gold from within; eaten fresh it fills you with lasting vigour.",Rarity::Legendary, 600, 135, 55, 5),
    fishdef(37, "The Fathom-King",     "A titan eel of the deepest trench, black and endless; a trophy beyond price.",           Rarity::Legendary, 660, 0,   0,  0),
    fishdef(38, "Weeping Silverfin",   "A shimmering fish the drowned-valley shrines wept over; its flesh is said to be blessed.", Rarity::Legendary, 590, 160, 60, 6),
    fishdef(39, "The First Fish",      "Grey and eyeless and older than the meres, from water that has never seen the sky. Sacred.",Rarity::Legendary, 700, 0,   0,  0),
];

fn build_fish() -> Vec<Item> {
    FISH_DEFS
        .iter()
        .map(|f| {
            let id = FISH_BASE + f.slot;
            let kind = if f.heal != 0 || f.restore != 0 {
                ItemKind::Consumable {
                    heal: f.heal,
                    restore: f.restore,
                }
            } else {
                ItemKind::Valuable
            };
            Item {
                id,
                name: f.name,
                desc: f.desc,
                kind,
                rarity: f.rarity,
                mods: StatMods {
                    attack: 0,
                    max_hp: 0,
                    armor: 0,
                },
                price: f.price,
                class_hint: None,
            }
        })
        .collect()
}

/// The Sunderlakes fish catalog, built once and reused for `item` lookups.
pub fn fish() -> &'static [Item] {
    static CATALOG: OnceLock<Vec<Item>> = OnceLock::new();
    CATALOG.get_or_init(build_fish)
}

/// The per-tick well-fed regen a special (legendary) fish grants when eaten, if
/// it carries one - reuses the same `HealOverTime` self-effect as cooked food
/// (see `use_item`). `None` for ordinary fish.
pub fn fish_well_fed(id: u32) -> Option<i32> {
    if !(FISH_BASE..FISH_BASE + FISH_COUNT).contains(&id) {
        return None;
    }
    FISH_DEFS
        .iter()
        .find(|f| FISH_BASE + f.slot == id)
        .filter(|f| f.well_fed > 0)
        .map(|f| f.well_fed)
}

pub fn item(id: u32) -> Option<&'static Item> {
    ITEMS
        .iter()
        .find(|i| i.id == id)
        .or_else(|| frontier_items().iter().find(|i| i.id == id))
        .or_else(|| reaches_items().iter().find(|i| i.id == id))
        .or_else(|| kaelmyr_items().iter().find(|i| i.id == id))
        .or_else(|| materials().iter().find(|i| i.id == id))
        .or_else(|| crafted().iter().find(|i| i.id == id))
        .or_else(|| fish().iter().find(|i| i.id == id))
}

// ---- Generated catalogs (Frontier and Sundered Reaches) ------------------
//
// The frontier expansion (see world::extend_frontier) is too large to author
// item-by-item, so its loot is generated: one tier per zone - twenty tiers x ten
// slots = 200 items, scaling with depth so each of the twenty zones drops its own
// progressively stronger gear. Built once and leaked to 'static so it slots into
// the same `item(id)` lookup as the hand-authored `ITEMS`. Frontier IDs live in
// 3000..3200; the Sundered Reaches continue the same curve in 3200..3400, with
// Reaches tier 0 picking up just above Frontier tier 19 so the new continent
// is a real gear step past the King.

/// Number of frontier loot tiers - one per zone (see world::FRONTIER_ZONES_DATA).
pub const FRONTIER_TIERS: usize = 20;

/// Number of Sundered Reaches loot tiers - one per zone (see world::REACHES_ZONES_DATA).
pub const REACHES_TIERS: usize = 20;

/// Number of Kaelmyr loot tiers - one per zone (see world::KAELMYR_ZONES_DATA).
pub const KAELMYR_TIERS: usize = 20;

const FRONTIER_ITEM_BASE: u32 = 3000;
const REACHES_ITEM_BASE: u32 = 3200;
/// Kaelmyr, the Ashen Reach: a third generated continent, its gear one clear
/// step past the drowned Reaches. IDs live in the free 3400..3600 band (authored
/// items top out well below 3000; materials start at 4000).
pub const KAELMYR_ITEM_BASE: u32 = 3400;
/// The Cinderfall Shore relic (Kaelmyr tier-0 relic), dropped on the ashen shore
/// and collected for the ash-cairn board's opening bounty.
pub const KAELMYR_SHORE_RELIC_ID: u32 = KAELMYR_ITEM_BASE + 9;

/// The full generated frontier item catalog (200 items).
pub fn frontier_items() -> &'static [Item] {
    static CATALOG: OnceLock<Vec<Item>> = OnceLock::new();
    CATALOG.get_or_init(build_frontier_items)
}

/// The full generated Sundered Reaches item catalog (200 items).
pub fn reaches_items() -> &'static [Item] {
    static CATALOG: OnceLock<Vec<Item>> = OnceLock::new();
    CATALOG.get_or_init(build_reaches_items)
}

/// The full generated Kaelmyr item catalog (200 items).
pub fn kaelmyr_items() -> &'static [Item] {
    static CATALOG: OnceLock<Vec<Item>> = OnceLock::new();
    CATALOG.get_or_init(build_kaelmyr_items)
}

/// The drop table for a frontier zone (tier 0..FRONTIER_TIERS): representative
/// weapon, head, chest, hands, ring, draught, and relic entries from that tier.
/// Tiers past the last clamp to the deepest table.
pub fn frontier_loot(tier: usize) -> &'static [u32] {
    static TABLES: OnceLock<Vec<Vec<u32>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| generated_loot_tables(FRONTIER_ITEM_BASE, FRONTIER_TIERS));
    tables[tier.min(FRONTIER_TIERS - 1)].as_slice()
}

/// The drop table for a Sundered Reaches zone (tier 0..REACHES_TIERS), same
/// shape as `frontier_loot` but drawn from the Reaches catalog.
pub fn reaches_loot(tier: usize) -> &'static [u32] {
    static TABLES: OnceLock<Vec<Vec<u32>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| generated_loot_tables(REACHES_ITEM_BASE, REACHES_TIERS));
    tables[tier.min(REACHES_TIERS - 1)].as_slice()
}

/// The drop table for a Kaelmyr zone (tier 0..KAELMYR_TIERS), same shape as
/// `reaches_loot` but drawn from the Kaelmyr catalog.
pub fn kaelmyr_loot(tier: usize) -> &'static [u32] {
    static TABLES: OnceLock<Vec<Vec<u32>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| generated_loot_tables(KAELMYR_ITEM_BASE, KAELMYR_TIERS));
    tables[tier.min(KAELMYR_TIERS - 1)].as_slice()
}

fn generated_loot_tables(base_id: u32, tiers: usize) -> Vec<Vec<u32>> {
    (0..tiers as u32)
        .map(|t| {
            let base = base_id + t * 10;
            vec![
                base,
                base + 1,
                base + 2,
                base + 4,
                base + 6,
                base + 8,
                base + 9,
            ]
        })
        .collect()
}

fn build_frontier_items() -> Vec<Item> {
    // One material per zone, low to high - matched to the twenty FRONTIER_ZONES.
    const MATERIALS: [&str; FRONTIER_TIERS] = [
        "Cindersteel",
        "Bogiron",
        "Glimmerwood",
        "Stormglass",
        "Bonewrought",
        "Tideforged",
        "Verdigris",
        "Emberforged",
        "Frostbitten",
        "Saltglass",
        "Sporeweave",
        "Clockwork",
        "Bloodforged",
        "Resonant",
        "Rimebound",
        "Obsidian",
        "Driftbone",
        "Magmacore",
        "Starless",
        "Voidtouched",
    ];
    // Rarity climbs in even bands across the twenty tiers.
    const TIER_RARITY: [Rarity; FRONTIER_TIERS] = [
        Rarity::Common,
        Rarity::Common,
        Rarity::Common,
        Rarity::Common,
        Rarity::Uncommon,
        Rarity::Uncommon,
        Rarity::Uncommon,
        Rarity::Uncommon,
        Rarity::Rare,
        Rarity::Rare,
        Rarity::Rare,
        Rarity::Rare,
        Rarity::Epic,
        Rarity::Epic,
        Rarity::Epic,
        Rarity::Epic,
        Rarity::Legendary,
        Rarity::Legendary,
        Rarity::Legendary,
        Rarity::Legendary,
    ];
    build_generated_items(GeneratedRealm {
        base_id: FRONTIER_ITEM_BASE,
        power_offset: 0,
        materials: &MATERIALS,
        rarities: &TIER_RARITY,
        gear_desc: |type_name| {
            format!(
                "Frontier-forged {type_name}, scarred by the deep wilds and all the keener for it."
            )
        },
        draught_desc: "A restorative brew distilled from frontier herbs.",
        relic_desc: "A frontier curio with no combat use; merchants buy these for good gold.",
    })
}

fn build_reaches_items() -> Vec<Item> {
    // One material per zone, low to high - matched to the twenty REACHES_ZONES.
    const MATERIALS: [&str; REACHES_TIERS] = [
        "Saltwrought",
        "Wrecksteel",
        "Weepstone",
        "Kelpbound",
        "Sirenscale",
        "Drownwood",
        "Galewrought",
        "Brineglass",
        "Valmaric",
        "Pearlbound",
        "Coralwrought",
        "Tideglass",
        "Leviathanbone",
        "Mourningsilver",
        "Tempestcore",
        "Mawbone",
        "Drownedgold",
        "Stormheart",
        "Abyssglass",
        "Sundersteel",
    ];
    // The whole continent sits past the Frontier's top tier, so every Reaches
    // tier reads as endgame gear.
    const TIER_RARITY: [Rarity; REACHES_TIERS] = [Rarity::Legendary; REACHES_TIERS];
    build_generated_items(GeneratedRealm {
        base_id: REACHES_ITEM_BASE,
        // Continue the Frontier's power curve: Reaches tier 0 lands just above
        // Frontier tier 19.
        power_offset: FRONTIER_TIERS as i32,
        materials: &MATERIALS,
        rarities: &TIER_RARITY,
        gear_desc: |type_name| {
            format!(
                "Drowned-realm {type_name}, raised from the Sundered Reaches and cold with the weight of the deep."
            )
        },
        draught_desc: "A briny restorative pressed from abyssal kelp and pearl-dust.",
        relic_desc: "A relic of the drowned realm with no combat use; merchants pay dearly for these.",
    })
}

fn build_kaelmyr_items() -> Vec<Item> {
    // One ashland material per zone, low to high - matched to KAELMYR_ZONES_DATA.
    const MATERIALS: [&str; KAELMYR_TIERS] = [
        "Ashglass",
        "Cinderbound",
        "Emberforged",
        "Slagsteel",
        "Pyrewrought",
        "Charbone",
        "Glowstone",
        "Sootglass",
        "Magmawrought",
        "Basaltbound",
        "Stormglass",
        "Skyforged",
        "Voidcinder",
        "Wrathsteel",
        "Hollowbone",
        "Choirglass",
        "Sunderash",
        "Godsforged",
        "Cataclysm",
        "Worldwound",
    ];
    // Kaelmyr is the deepest continent yet, so every tier reads as endgame gear.
    const TIER_RARITY: [Rarity; KAELMYR_TIERS] = [Rarity::Legendary; KAELMYR_TIERS];
    build_generated_items(GeneratedRealm {
        base_id: KAELMYR_ITEM_BASE,
        // Continue the power curve one full continent past the Reaches: Kaelmyr
        // tier 0 lands just above Reaches tier 19.
        power_offset: (FRONTIER_TIERS + REACHES_TIERS) as i32,
        materials: &MATERIALS,
        rarities: &TIER_RARITY,
        gear_desc: |type_name| {
            format!(
                "Ash-forged {type_name}, hammered on the burning anvils of Kaelmyr and never once cooled."
            )
        },
        draught_desc: "A scalding tonic brewed from ash-lichen and cinder-salt.",
        relic_desc: "A relic of the Ashen Reach with no combat use; collectors pay a fortune for these.",
    })
}

struct GeneratedRealm {
    base_id: u32,
    /// Added to the 1-based tier before computing stats, so a later realm's
    /// tiers continue an earlier realm's power curve instead of restarting it.
    power_offset: i32,
    materials: &'static [&'static str; 20],
    rarities: &'static [Rarity; 20],
    gear_desc: fn(&str) -> String,
    draught_desc: &'static str,
    relic_desc: &'static str,
}

fn build_generated_items(realm: GeneratedRealm) -> Vec<Item> {
    const SLOTS: [(Slot, &str); 8] = [
        (Slot::Weapon, "Blade"),
        (Slot::Head, "Helm"),
        (Slot::Chest, "Cuirass"),
        (Slot::Legs, "Greaves"),
        (Slot::Hands, "Gauntlets"),
        (Slot::Feet, "Boots"),
        (Slot::Ring, "Band"),
        (Slot::Trinket, "Charm"),
    ];

    let tiers = realm.materials.len();
    let mut out = Vec::with_capacity(tiers * 10);
    for tier in 0..tiers {
        let t = realm.power_offset + (tier + 1) as i32;
        let rarity = realm.rarities[tier];
        let mat = realm.materials[tier];
        for (i, (slot, type_name)) in SLOTS.iter().enumerate() {
            let id = realm.base_id + (tier as u32) * 10 + i as u32;
            let name: &'static str = Box::leak(format!("{mat} {type_name}").into_boxed_str());
            let desc: &'static str =
                Box::leak((realm.gear_desc)(&type_name.to_ascii_lowercase()).into_boxed_str());
            let (attack, max_hp, armor) = match slot {
                Slot::Weapon => (30 + t * 3, 0, 0),
                Slot::Head => (2 + t / 2, 32 + t * 5, 5 + t / 2),
                Slot::Chest => (1 + t / 3, 58 + t * 8, 8 + t),
                Slot::Legs => (t / 2, 38 + t * 6, 6 + t),
                Slot::Hands => (6 + t, 20 + t * 3, 3 + t / 2),
                Slot::Feet => (t / 2, 24 + t * 3, 3 + t / 2),
                Slot::Ring => (6 + t, 20 + t * 3, t / 2),
                Slot::Trinket => (4 + t / 2, 28 + t * 4, 2 + t / 2),
            };
            out.push(Item {
                id,
                name,
                desc,
                kind: ItemKind::Equipment(*slot),
                rarity,
                mods: StatMods {
                    attack,
                    max_hp,
                    armor,
                },
                price: (220 + t * 85) as i64,
                class_hint: None,
            });
        }
        // A restorative draught and a sellable relic round out each tier.
        let draught: &'static str = Box::leak(format!("{mat} Draught").into_boxed_str());
        out.push(Item {
            id: realm.base_id + (tier as u32) * 10 + 8,
            name: draught,
            desc: realm.draught_desc,
            kind: ItemKind::Consumable {
                heal: 120 + t * 20,
                restore: 60 + t * 10,
            },
            rarity: Rarity::Common,
            mods: StatMods::default(),
            price: (90 + t * 20) as i64,
            class_hint: None,
        });
        let relic: &'static str = Box::leak(format!("{mat} Relic").into_boxed_str());
        out.push(Item {
            id: realm.base_id + (tier as u32) * 10 + 9,
            name: relic,
            desc: realm.relic_desc,
            kind: ItemKind::Valuable,
            rarity,
            mods: StatMods::default(),
            price: (180 + t * 60) as i64,
            class_hint: None,
        });
    }
    out
}

/// A shop run by an NPC in a specific town room.
#[derive(Clone, Copy, Debug)]
pub struct Shop {
    pub room: super::world::RoomId,
    pub npc_name: &'static str,
    pub shop_name: &'static str,
    /// The line the NPC greets shoppers with.
    pub greeting: &'static str,
    pub stock: &'static [u32],
}

/// Every storefront in Embergate, keyed to the room its NPC stands in.
pub const SHOPS: &[Shop] = &[
    Shop {
        room: 3, // Market Row -> the smithy
        npc_name: "Bruna Ironhand",
        shop_name: "The Ember Forge",
        greeting: "Bruna looks up from the anvil, soot on her brow. \"Steel for steel's work. What'll it be?\"",
        stock: &[
            1000, 1001, 1002, 1003, 1004, 1005, 1006, 1007, 1008, 1009, 1010,
        ],
    },
    Shop {
        room: 201,
        npc_name: "Tomas Threadneedle",
        shop_name: "The Outfitter's Stall",
        greeting: "A wiry man peers over a counter heaped with hide and mail. \"Armor keeps a body breathing. Browse, browse.\"",
        stock: &[
            1100, 1101, 1102, 1103, 1104, 1105, 1106, 1107, 1108, 1109, 1110, 1111, 1112, 1113,
            1120, 1121, 1122,
        ],
    },
    Shop {
        room: 202,
        npc_name: "Old Mirela",
        shop_name: "The Apothecary",
        greeting: "Shelves of bottles glint behind a stooped woman who smells of crushed herbs. \"Hurt, are you? I have just the thing.\"",
        stock: &[1300, 1301, 1302, 1303, 1304, 1305],
    },
    Shop {
        room: 203,
        npc_name: "Pell the Magpie",
        shop_name: "The Curio Cart",
        greeting: "A grinning fellow guards a cart of glittering oddments. \"Rings, charms, lucky bits and bobs! All genuine, mostly.\"",
        stock: &[1200, 1201, 1202, 1203, 1204, 1205, 1206, 1207],
    },
];

pub fn shop_at(room: super::world::RoomId) -> Option<&'static Shop> {
    SHOPS.iter().find(|s| s.room == room)
}

#[cfg(test)]
#[path = "items_test.rs"]
mod items_test;

