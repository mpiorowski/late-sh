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

    /// A compact one-line summary of what the item does, for the inventory and
    /// shop panels: e.g. "+8 atk", "+10 hp +2 arm", "heal 30 / +20 res", or empty
    /// for plain valuables.
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
            ItemKind::Valuable => String::new(),
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

/// The full item catalog.
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
];

pub fn item(id: u32) -> Option<&'static Item> {
    ITEMS
        .iter()
        .find(|i| i.id == id)
        .or_else(|| frontier_items().iter().find(|i| i.id == id))
}

// ---- Frontier catalog (procedural) --------------------------------------
//
// The frontier expansion (see world::extend_frontier) is too large to author
// item-by-item, so its loot is generated: one tier per zone - twenty tiers x ten
// slots = 200 items, scaling with depth so each of the twenty zones drops its own
// progressively stronger gear. Built once and leaked to 'static so it slots into
// the same `item(id)` lookup as the hand-authored `ITEMS`. IDs live in 3000..3200.

/// Number of frontier loot tiers - one per zone (see world::FRONTIER_ZONES_DATA).
pub const FRONTIER_TIERS: usize = 20;

/// The full generated frontier item catalog (200 items).
pub fn frontier_items() -> &'static [Item] {
    static CATALOG: OnceLock<Vec<Item>> = OnceLock::new();
    CATALOG.get_or_init(build_frontier_items)
}

/// The drop table for a frontier zone (tier 0..FRONTIER_TIERS): representative
/// weapon, head, chest, hands, ring, draught, and relic entries from that tier.
/// Tiers past the last clamp to the deepest table.
pub fn frontier_loot(tier: usize) -> &'static [u32] {
    static TABLES: OnceLock<Vec<Vec<u32>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| {
        (0..FRONTIER_TIERS as u32)
            .map(|t| {
                let base = 3000 + t * 10;
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
    });
    tables[tier.min(FRONTIER_TIERS - 1)].as_slice()
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

    let mut out = Vec::with_capacity(FRONTIER_TIERS * 10);
    for tier in 0..FRONTIER_TIERS {
        let t = (tier + 1) as i32;
        let rarity = TIER_RARITY[tier];
        let mat = MATERIALS[tier];
        for (i, (slot, type_name)) in SLOTS.iter().enumerate() {
            let id = 3000 + (tier as u32) * 10 + i as u32;
            let name: &'static str = Box::leak(format!("{mat} {type_name}").into_boxed_str());
            let desc: &'static str = Box::leak(
                format!(
                    "Frontier-forged {}, scarred by the deep wilds and all the keener for it.",
                    type_name.to_ascii_lowercase()
                )
                .into_boxed_str(),
            );
            let (attack, max_hp, armor) = match slot {
                Slot::Weapon => (6 + t * 2, 0, 0),
                Slot::Head => (0, 8 + t * 3, 1 + t / 2),
                Slot::Chest => (0, 16 + t * 5, 2 + t),
                Slot::Legs => (0, 10 + t * 4, 1 + t),
                Slot::Hands => (1 + t / 2, 4 + t * 2, t / 2),
                Slot::Feet => (0, 6 + t * 2, t / 2),
                Slot::Ring => (t, 4 + t * 2, 0),
                Slot::Trinket => (t / 2, 6 + t * 3, t / 2),
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
                price: (50 + t * 40) as i64,
                class_hint: None,
            });
        }
        // A restorative draught and a sellable relic round out each tier.
        let draught: &'static str = Box::leak(format!("{mat} Draught").into_boxed_str());
        out.push(Item {
            id: 3000 + (tier as u32) * 10 + 8,
            name: draught,
            desc: "A restorative brew distilled from frontier herbs.",
            kind: ItemKind::Consumable {
                heal: 20 + t * 10,
                restore: 10 + t * 5,
            },
            rarity: Rarity::Common,
            mods: StatMods::default(),
            price: (15 + t * 8) as i64,
            class_hint: None,
        });
        let relic: &'static str = Box::leak(format!("{mat} Relic").into_boxed_str());
        out.push(Item {
            id: 3000 + (tier as u32) * 10 + 9,
            name: relic,
            desc: "A frontier curio, prized by collectors back in the capitals.",
            kind: ItemKind::Valuable,
            rarity,
            mods: StatMods::default(),
            price: (40 + t * 30) as i64,
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
            1100, 1101, 1102, 1103, 1104, 1105, 1106, 1107, 1108, 1109, 1110, 1111, 1112,
            1113, 1120, 1121, 1122,
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
mod tests {
    use super::*;

    #[test]
    fn item_ids_are_unique() {
        let mut ids: Vec<u32> = ITEMS.iter().map(|i| i.id).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(n, ids.len(), "duplicate item id");
    }

    #[test]
    fn every_shop_sells_real_items() {
        for shop in SHOPS {
            assert!(!shop.stock.is_empty(), "{} has no stock", shop.shop_name);
            for id in shop.stock {
                assert!(item(*id).is_some(), "shop sells missing item {id}");
            }
        }
    }

    #[test]
    fn shops_offer_late_gold_sinks() {
        let costly: Vec<_> = SHOPS
            .iter()
            .flat_map(|shop| shop.stock.iter().filter_map(|id| item(*id)))
            .filter(|it| it.price >= 1_500)
            .collect();
        assert!(
            costly.len() >= 6,
            "shops should offer enough expensive late-game stock"
        );
        assert!(
            costly.iter().any(|it| matches!(it.kind, ItemKind::Consumable { .. })),
            "shops should include a repeatable expensive consumable"
        );
    }

    #[test]
    fn apothecary_consumables_scale_into_late_recovery() {
        let minor = item(1300).expect("minor draught exists");
        let potion = item(1301).expect("healing potion exists");
        let greater = item(1302).expect("greater elixir exists");
        let renewal = item(1304).expect("renewal elixir exists");
        let phoenix = item(1305).expect("phoenix tonic exists");

        let healing = |it: &Item| match it.kind {
            ItemKind::Consumable { heal, restore } => (heal, restore),
            _ => panic!("expected consumable"),
        };

        assert!(healing(minor).0 < healing(potion).0);
        assert!(healing(potion).0 < healing(greater).0);
        assert!(healing(renewal).0 >= 180 && healing(renewal).1 >= 120);
        assert!(healing(phoenix).0 >= 400 && healing(phoenix).1 >= 200);
    }

    #[test]
    fn outfitter_sells_real_head_and_hand_upgrades() {
        let outfitter = SHOPS
            .iter()
            .find(|shop| shop.shop_name == "The Outfitter's Stall")
            .expect("outfitter shop exists");
        let stock: Vec<_> = outfitter
            .stock
            .iter()
            .filter_map(|id| item(*id))
            .collect();

        assert!(
            stock
                .iter()
                .any(|it| it.slot() == Some(Slot::Head) && it.price >= 2_000),
            "outfitter should sell a late-game helm"
        );
        assert!(
            stock
                .iter()
                .any(|it| it.slot() == Some(Slot::Hands) && it.price >= 2_000),
            "outfitter should sell late-game gloves"
        );
    }

    #[test]
    fn frontier_loot_includes_head_and_hands() {
        let slots: Vec<_> = frontier_loot(0)
            .iter()
            .filter_map(|id| item(*id).and_then(Item::slot))
            .collect();
        assert!(slots.contains(&Slot::Head), "frontier should drop helms");
        assert!(
            slots.contains(&Slot::Hands),
            "frontier should drop gauntlets"
        );
    }

    #[test]
    fn equipment_reports_its_slot() {
        for it in ITEMS {
            if let ItemKind::Equipment(slot) = it.kind {
                assert_eq!(it.slot(), Some(slot));
            } else {
                assert_eq!(it.slot(), None);
            }
        }
    }

    #[test]
    fn sell_price_is_never_zero() {
        for it in ITEMS {
            assert!(it.sell_price() >= 1, "{} sells for nothing", it.name);
        }
    }
}
