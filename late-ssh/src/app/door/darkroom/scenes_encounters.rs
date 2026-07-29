/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * This Source Code Form is "Incompatible With Secondary Licenses", as
 * defined by the Mozilla Public License, v. 2.0.
 *
 * Derived from A Dark Room by Michael Townsend / Doublespeak Games
 * (https://github.com/doublespeakgames/adarkroom), MPL-2.0. Every fight below
 * is transcribed from `script/events/encounters.js`. See LICENSING.md and
 * NOTICE. */

//! What jumps out at you in the wasteland. Three tiers by distance from the
//! village, and within a tier the terrain decides what is out there.

use super::data::Resource;
use super::event::{Combat, Condition, Event, Loot, Next, Scene};
use super::world_data::Tile;

/// Every wandering fight, tier by tier.
pub static ENCOUNTERS: [Event; 11] = [
    SNARLING_BEAST,
    GAUNT_MAN,
    STRANGE_BIRD,
    TWO_HEADED_CREATURE,
    SHIVERING_MAN,
    MAN_EATER,
    SCAVENGER,
    HUGE_LIZARD,
    FERAL_TERROR,
    SOLDIER,
    SNIPER,
];

// ---------------------------------------------------------------------------
// Tier 1: within ten squares of home
// ---------------------------------------------------------------------------

static SNARLING_BEAST: Event = Event {
    key: "snarling beast",
    title: "A Snarling Beast",
    available: &[
        Condition::DistanceAtMost(10),
        Condition::TerrainIs(Tile::Forest),
    ],
    scenes: &[Scene {
        key: "start",
        notification: Some("a snarling beast leaps out of the underbrush"),
        combat: Some(Combat {
            enemy: "snarling beast",
            chara: 'R',
            health: 5,
            damage: 1,
            hit: 0.8,
            attack_delay: 1.0,
            ranged: false,
            death_message: "the snarling beast is dead",
            loot: &[
                Loot {
                    item: Resource::Fur,
                    chance: 1.0,
                    min: 1,
                    max: 3,
                },
                Loot {
                    item: Resource::Meat,
                    chance: 1.0,
                    min: 1,
                    max: 3,
                },
                Loot {
                    item: Resource::Teeth,
                    chance: 0.8,
                    min: 1,
                    max: 3,
                },
            ],
            next: Next::End,
        }),
        ..Scene::EMPTY
    }],
};

static GAUNT_MAN: Event = Event {
    key: "gaunt man",
    title: "A Gaunt Man",
    available: &[
        Condition::DistanceAtMost(10),
        Condition::TerrainIs(Tile::Barrens),
    ],
    scenes: &[Scene {
        key: "start",
        notification: Some("a gaunt man approaches, a crazed look in his eye"),
        combat: Some(Combat {
            enemy: "gaunt man",
            chara: 'E',
            health: 6,
            damage: 2,
            hit: 0.8,
            attack_delay: 2.0,
            ranged: false,
            death_message: "the gaunt man is dead",
            loot: &[
                Loot {
                    item: Resource::Cloth,
                    chance: 0.8,
                    min: 1,
                    max: 3,
                },
                Loot {
                    item: Resource::Teeth,
                    chance: 0.8,
                    min: 1,
                    max: 2,
                },
                Loot {
                    item: Resource::Leather,
                    chance: 0.5,
                    min: 1,
                    max: 2,
                },
            ],
            next: Next::End,
        }),
        ..Scene::EMPTY
    }],
};

static STRANGE_BIRD: Event = Event {
    key: "strange bird",
    title: "A Strange Bird",
    available: &[
        Condition::DistanceAtMost(10),
        Condition::TerrainIs(Tile::Field),
    ],
    scenes: &[Scene {
        key: "start",
        notification: Some("a strange looking bird speeds across the plains"),
        combat: Some(Combat {
            enemy: "strange bird",
            chara: 'R',
            health: 4,
            damage: 3,
            hit: 0.8,
            attack_delay: 2.0,
            ranged: false,
            death_message: "the strange bird is dead",
            loot: &[
                Loot {
                    item: Resource::Scales,
                    chance: 0.8,
                    min: 1,
                    max: 3,
                },
                Loot {
                    item: Resource::Teeth,
                    chance: 0.5,
                    min: 1,
                    max: 2,
                },
                Loot {
                    item: Resource::Meat,
                    chance: 0.8,
                    min: 1,
                    max: 3,
                },
            ],
            next: Next::End,
        }),
        ..Scene::EMPTY
    }],
};

static TWO_HEADED_CREATURE: Event = Event {
    key: "two-headed creature",
    title: "A Two-Headed Creature",
    available: &[
        Condition::DistanceAtMost(10),
        Condition::TerrainIs(Tile::Field),
    ],
    scenes: &[Scene {
        key: "start",
        notification: Some("a two-headed creature appears, the smaller head trembling"),
        combat: Some(Combat {
            enemy: "two-headed creature",
            chara: 'K',
            health: 10,
            damage: 2,
            hit: 0.5,
            attack_delay: 3.0,
            ranged: false,
            death_message: "the two creatures are dead",
            loot: &[
                Loot {
                    item: Resource::Fur,
                    chance: 1.0,
                    min: 2,
                    max: 4,
                },
                Loot {
                    item: Resource::Teeth,
                    chance: 0.8,
                    min: 2,
                    max: 3,
                },
                Loot {
                    item: Resource::Meat,
                    chance: 0.8,
                    min: 2,
                    max: 3,
                },
            ],
            next: Next::End,
        }),
        ..Scene::EMPTY
    }],
};

// ---------------------------------------------------------------------------
// Tier 2: ten to twenty out
// ---------------------------------------------------------------------------

static SHIVERING_MAN: Event = Event {
    key: "shivering man",
    title: "A Shivering Man",
    available: &[
        Condition::DistanceOver(10),
        Condition::DistanceAtMost(20),
        Condition::TerrainIs(Tile::Barrens),
    ],
    scenes: &[Scene {
        key: "start",
        notification: Some("a shivering man approaches and attacks with surprising strength"),
        combat: Some(Combat {
            enemy: "shivering man",
            chara: 'E',
            health: 20,
            damage: 5,
            hit: 0.5,
            attack_delay: 1.0,
            ranged: false,
            death_message: "the shivering man is dead",
            loot: &[
                Loot {
                    item: Resource::Cloth,
                    chance: 0.2,
                    min: 1,
                    max: 1,
                },
                Loot {
                    item: Resource::Teeth,
                    chance: 0.8,
                    min: 1,
                    max: 2,
                },
                Loot {
                    item: Resource::Leather,
                    chance: 0.2,
                    min: 1,
                    max: 1,
                },
                Loot {
                    item: Resource::Medicine,
                    chance: 0.7,
                    min: 1,
                    max: 3,
                },
            ],
            next: Next::End,
        }),
        ..Scene::EMPTY
    }],
};

static MAN_EATER: Event = Event {
    key: "man-eater",
    title: "A Man-Eater",
    available: &[
        Condition::DistanceOver(10),
        Condition::DistanceAtMost(20),
        Condition::TerrainIs(Tile::Forest),
    ],
    scenes: &[Scene {
        key: "start",
        notification: Some("a large creature attacks, claws freshly bloodied"),
        combat: Some(Combat {
            enemy: "man-eater",
            chara: 'T',
            health: 25,
            damage: 3,
            hit: 0.8,
            attack_delay: 1.0,
            ranged: false,
            death_message: "the man-eater is dead",
            loot: &[
                Loot {
                    item: Resource::Fur,
                    chance: 1.0,
                    min: 5,
                    max: 10,
                },
                Loot {
                    item: Resource::Meat,
                    chance: 1.0,
                    min: 5,
                    max: 10,
                },
                Loot {
                    item: Resource::Teeth,
                    chance: 0.8,
                    min: 5,
                    max: 10,
                },
            ],
            next: Next::End,
        }),
        ..Scene::EMPTY
    }],
};

static SCAVENGER: Event = Event {
    key: "scavenger",
    title: "A Scavenger",
    available: &[
        Condition::DistanceOver(10),
        Condition::DistanceAtMost(20),
        Condition::TerrainIs(Tile::Barrens),
    ],
    scenes: &[Scene {
        key: "start",
        notification: Some("a scavenger draws close, hoping for an easy score"),
        combat: Some(Combat {
            enemy: "scavenger",
            chara: 'E',
            health: 30,
            damage: 4,
            hit: 0.8,
            attack_delay: 2.0,
            ranged: false,
            death_message: "the scavenger is dead",
            loot: &[
                Loot {
                    item: Resource::Cloth,
                    chance: 0.8,
                    min: 5,
                    max: 10,
                },
                Loot {
                    item: Resource::Leather,
                    chance: 0.8,
                    min: 5,
                    max: 10,
                },
                Loot {
                    item: Resource::Iron,
                    chance: 0.5,
                    min: 1,
                    max: 5,
                },
                Loot {
                    item: Resource::Medicine,
                    chance: 0.1,
                    min: 1,
                    max: 2,
                },
            ],
            next: Next::End,
        }),
        ..Scene::EMPTY
    }],
};

static HUGE_LIZARD: Event = Event {
    key: "lizard",
    title: "A Huge Lizard",
    available: &[
        Condition::DistanceOver(10),
        Condition::DistanceAtMost(20),
        Condition::TerrainIs(Tile::Field),
    ],
    scenes: &[Scene {
        key: "start",
        notification: Some("the grass thrashes wildly as a huge lizard pushes through"),
        combat: Some(Combat {
            enemy: "lizard",
            chara: 'T',
            health: 20,
            damage: 5,
            hit: 0.8,
            attack_delay: 2.0,
            ranged: false,
            death_message: "the lizard is dead",
            loot: &[
                Loot {
                    item: Resource::Scales,
                    chance: 0.8,
                    min: 5,
                    max: 10,
                },
                Loot {
                    item: Resource::Teeth,
                    chance: 0.5,
                    min: 5,
                    max: 10,
                },
                Loot {
                    item: Resource::Meat,
                    chance: 0.8,
                    min: 5,
                    max: 10,
                },
            ],
            next: Next::End,
        }),
        ..Scene::EMPTY
    }],
};

// ---------------------------------------------------------------------------
// Tier 3: past twenty
// ---------------------------------------------------------------------------

static FERAL_TERROR: Event = Event {
    key: "feral terror",
    title: "A Feral Terror",
    available: &[
        Condition::DistanceOver(20),
        Condition::TerrainIs(Tile::Forest),
    ],
    scenes: &[Scene {
        key: "start",
        notification: Some("a beast, wilder than imagining, erupts out of the foliage"),
        combat: Some(Combat {
            enemy: "feral terror",
            chara: 'T',
            health: 45,
            damage: 6,
            hit: 0.8,
            attack_delay: 1.0,
            ranged: false,
            death_message: "the feral terror is dead",
            loot: &[
                Loot {
                    item: Resource::Fur,
                    chance: 1.0,
                    min: 5,
                    max: 10,
                },
                Loot {
                    item: Resource::Meat,
                    chance: 1.0,
                    min: 5,
                    max: 10,
                },
                Loot {
                    item: Resource::Teeth,
                    chance: 0.8,
                    min: 5,
                    max: 10,
                },
            ],
            next: Next::End,
        }),
        ..Scene::EMPTY
    }],
};

static SOLDIER: Event = Event {
    key: "soldier",
    title: "A Soldier",
    available: &[
        Condition::DistanceOver(20),
        Condition::TerrainIs(Tile::Barrens),
    ],
    scenes: &[Scene {
        key: "start",
        notification: Some("a soldier opens fire from across the desert"),
        combat: Some(Combat {
            enemy: "soldier",
            chara: 'D',
            health: 50,
            damage: 8,
            hit: 0.8,
            attack_delay: 2.0,
            ranged: true,
            death_message: "the soldier is dead",
            loot: &[
                Loot {
                    item: Resource::Cloth,
                    chance: 0.8,
                    min: 5,
                    max: 10,
                },
                Loot {
                    item: Resource::Bullets,
                    chance: 0.5,
                    min: 1,
                    max: 5,
                },
                Loot {
                    item: Resource::Rifle,
                    chance: 0.2,
                    min: 1,
                    max: 1,
                },
                Loot {
                    item: Resource::Medicine,
                    chance: 0.1,
                    min: 1,
                    max: 2,
                },
            ],
            next: Next::End,
        }),
        ..Scene::EMPTY
    }],
};

static SNIPER: Event = Event {
    key: "sniper",
    title: "A Sniper",
    available: &[
        Condition::DistanceOver(20),
        Condition::TerrainIs(Tile::Field),
    ],
    scenes: &[Scene {
        key: "start",
        notification: Some("a shot rings out, from somewhere in the long grass"),
        combat: Some(Combat {
            enemy: "sniper",
            chara: 'D',
            health: 30,
            damage: 15,
            hit: 0.8,
            attack_delay: 4.0,
            ranged: true,
            death_message: "the sniper is dead",
            loot: &[
                Loot {
                    item: Resource::Cloth,
                    chance: 0.8,
                    min: 5,
                    max: 10,
                },
                Loot {
                    item: Resource::Bullets,
                    chance: 0.5,
                    min: 1,
                    max: 5,
                },
                Loot {
                    item: Resource::Rifle,
                    chance: 0.2,
                    min: 1,
                    max: 1,
                },
                Loot {
                    item: Resource::Medicine,
                    chance: 0.1,
                    min: 1,
                    max: 2,
                },
            ],
            next: Next::End,
        }),
        ..Scene::EMPTY
    }],
};

/// An encounter by table key, for resuming a parked fight.
pub fn by_key(key: &str) -> Option<&'static Event> {
    ENCOUNTERS.iter().find(|event| event.key == key)
}
