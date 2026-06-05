// Items, equipment, inventory, and shop NPCs for Lateania.
//
// Items are static data with stat modifiers. A character carries an inventory of
// item ids and equips one item per slot; equipping recomputes derived stats.
// Consumables apply an effect when used. Shops are NPC-run storefronts in the
// town of Embergate, each NPC keyed to a room and selling a themed catalog.

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
    // ---- Consumables (the Apothecary) -----------------------------------
    consumable(
        1300,
        "Minor Healing Draught",
        "A bitter red tonic that closes small wounds.",
        Rarity::Common,
        30,
        0,
        20,
    ),
    consumable(
        1301,
        "Healing Potion",
        "The reliable choice of every sensible adventurer.",
        Rarity::Uncommon,
        70,
        0,
        55,
    ),
    consumable(
        1302,
        "Greater Healing Elixir",
        "Mends even grievous hurts in moments.",
        Rarity::Rare,
        150,
        0,
        140,
    ),
    consumable(
        1303,
        "Draught of Vigor",
        "Restores the fire that fuels your craft.",
        Rarity::Uncommon,
        0,
        60,
        50,
    ),
    consumable(
        1304,
        "Elixir of Renewal",
        "Restores both flesh and will at once.",
        Rarity::Epic,
        120,
        80,
        220,
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
    ITEMS.iter().find(|i| i.id == id)
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
        stock: &[1000, 1001, 1002, 1003, 1004, 1005, 1006, 1007, 1008, 1009],
    },
    Shop {
        room: 201,
        npc_name: "Tomas Threadneedle",
        shop_name: "The Outfitter's Stall",
        greeting: "A wiry man peers over a counter heaped with hide and mail. \"Armor keeps a body breathing. Browse, browse.\"",
        stock: &[1100, 1101, 1102, 1103, 1104, 1105, 1106, 1107, 1108, 1109],
    },
    Shop {
        room: 202,
        npc_name: "Old Mirela",
        shop_name: "The Apothecary",
        greeting: "Shelves of bottles glint behind a stooped woman who smells of crushed herbs. \"Hurt, are you? I have just the thing.\"",
        stock: &[1300, 1301, 1302, 1303, 1304],
    },
    Shop {
        room: 203,
        npc_name: "Pell the Magpie",
        shop_name: "The Curio Cart",
        greeting: "A grinning fellow guards a cart of glittering oddments. \"Rings, charms, lucky bits and bobs! All genuine, mostly.\"",
        stock: &[1200, 1201, 1202, 1203, 1204, 1205],
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
