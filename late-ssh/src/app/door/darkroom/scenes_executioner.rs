/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * This Source Code Form is "Incompatible With Secondary Licenses", as
 * defined by the Mozilla Public License, v. 2.0.
 *
 * Derived from A Dark Room by Michael Townsend / Doublespeak Games
 * (https://github.com/doublespeakgames/adarkroom), MPL-2.0. Every scene, stat
 * line, loot table and sentence below is transcribed from
 * `script/events/executioner.js`. See LICENSING.md and NOTICE. */

//! The ravaged battleship: the wreck of the Executioner, its three decks, and
//! the immortal wanderer sitting on the command bridge.
//!
//! This is the one landmark that is not a single visit. The first arrival runs
//! `executioner-intro`, which ends by powering the ship up and taking the
//! strange device; every arrival after that opens on `executioner-antechamber`,
//! a bank of elevators whose buttons fall away as their decks are picked
//! clean. Clearing all three unlocks the command deck, and the fight at the
//! end of it hands over the fleet beacon, which is what makes the ascent end
//! somewhere other than the dark.

use super::data::Resource;
use super::event::{
    Button, Combat, Condition, Cost, Effect, Event, Loot, Next, Scene, Special, SpecialAction,
    Status,
};
use super::model::Deck;

/// One of the battleship's events by table key. The world names the landmark
/// `executioner`; which of these it opens depends on how far in the wanderer
/// already is, and that decision lives in `world::battleship_scene`.
pub fn by_key(key: &str) -> Option<&'static Event> {
    EXECUTIONER.iter().find(|event| event.key == key)
}

pub static EXECUTIONER: [Event; 6] = [INTRO, ANTECHAMBER, ENGINEERING, MARTIAL, MEDICAL, COMMAND];

// ---------------------------------------------------------------------------
// Helpers, the same shapes `scenes_setpieces` uses
// ---------------------------------------------------------------------------

const fn loot(item: Resource, chance: f64, min: i64, max: i64) -> Loot {
    Loot {
        item,
        chance,
        min,
        max,
    }
}

/// A plain "go on" button.
const fn goes(text: &'static str, next: Next) -> Button {
    Button {
        text,
        next,
        ..Button::EMPTY
    }
}

/// The "leave" row every scene in here carries, which ends the visit outright.
const LEAVE: Button = Button::leave("leave");

/// A scene that is only words and whatever is lying around.
const fn room(
    key: &'static str,
    text: &'static [&'static str],
    spoils: &'static [Loot],
    buttons: &'static [Button],
) -> Scene {
    Scene {
        key,
        text,
        loot: spoils,
        buttons,
        ..Scene::EMPTY
    }
}

/// A fight scene: the enemy's whole stat line plus the rows that come after
/// it. Upstream spreads a shared `Enemies.Executioner.*` entry into the scene
/// and then overrides `notification` where the line differs, so the line is
/// passed in at every site rather than defaulted.
const fn fight(
    key: &'static str,
    notification: &'static str,
    combat: Combat,
    buttons: &'static [Button],
) -> Scene {
    Scene {
        key,
        notification: Some(notification),
        combat: Some(combat),
        buttons,
        ..Scene::EMPTY
    }
}

// ---------------------------------------------------------------------------
// The recurring defenders (upstream `Enemies.Executioner`)
// ---------------------------------------------------------------------------

const GUARD_LINE: &str = "tripped a motion sensor.";
const GUARD: Combat = Combat {
    enemy: "mechanical guard",
    chara: 'G',
    health: 60,
    damage: 10,
    hit: 0.8,
    attack_delay: 2.0,
    ranged: true,
    loot: &[
        loot(Resource::EnergyCell, 0.8, 1, 5),
        loot(Resource::LaserRifle, 0.8, 1, 1),
        loot(Resource::AlienAlloy, 0.2, 1, 1),
    ],
    ..Combat::PLAIN
};

const QUADRUPED_LINE: &str = "a mobile defence platform trundles around the corner.";
const QUADRUPED: Combat = Combat {
    enemy: "mechanical quadruped",
    chara: 'Q',
    health: 70,
    damage: 8,
    hit: 0.8,
    attack_delay: 1.0,
    ranged: false,
    // Upstream's table lists `alien alloy` twice; a JavaScript object literal
    // keeps the last of a repeated key, so the guaranteed single alloy never
    // existed and the quadruped really does drop 2-4 at one in five. Kept as
    // the game behaves, not as the table reads.
    loot: &[loot(Resource::AlienAlloy, 0.2, 2, 4)],
    ..Combat::PLAIN
};

const MEDIC_LINE: &str = "a medical drone wheels out of control.";
const MEDIC: Combat = Combat {
    enemy: "broken medic",
    chara: 'M',
    health: 80,
    damage: 15,
    hit: 0.8,
    attack_delay: 3.0,
    ranged: false,
    // Half dead, it starts spitting something that keeps working after it
    // lands.
    at_health: &[(40, Status::Venomous)],
    loot: &[
        loot(Resource::AlienAlloy, 1.0, 1, 2),
        loot(Resource::Hypo, 0.2, 1, 4),
    ],
    ..Combat::PLAIN
};

const TURRET_LINE: &str = "one of the defence turrets still works.";
const TURRET: Combat = Combat {
    enemy: "defence turret",
    chara: 'T',
    health: 50,
    damage: 25,
    hit: 0.8,
    attack_delay: 4.0,
    ranged: true,
    loot: &[
        loot(Resource::EnergyCell, 0.8, 1, 5),
        loot(Resource::AlienAlloy, 0.8, 1, 1),
        loot(Resource::LaserRifle, 0.2, 1, 1),
    ],
    ..Combat::PLAIN
};

// ---------------------------------------------------------------------------
// Exploring a ravaged battleship (the first visit)
// ---------------------------------------------------------------------------

static INTRO: Event = Event {
    key: "executioner-intro",
    title: "A Ravaged Battleship",
    available: &[],
    scenes: &[
        Scene {
            key: "start",
            notification: Some("the remains of a huge ship are embedded in the earth."),
            text: &[
                "the remains of a massive battleship lie here, like a silent sealed city.",
                "it lists to the side in a deep crevasse, cut when it fell from the sky.",
                "the hatches are all sealed, but the hull is blown out just above the dirt, providing an entrance.",
            ],
            buttons: &[
                Button {
                    text: "enter",
                    cost: &[Cost::Store(Resource::Torch, 1)],
                    next: Next::Scene("1"),
                    ..Button::EMPTY
                },
                LEAVE,
            ],
            ..Scene::EMPTY
        },
        room(
            "1",
            &[
                "the interior of the ship is cold and dark. what little light there is only accentuates its harsh angles.",
                "the walls hum faintly.",
            ],
            &[],
            &[
                goes(
                    "continue",
                    Next::Weighted(&[(0.4, "2-1"), (0.8, "2-2"), (1.0, "2-3")]),
                ),
                LEAVE,
            ],
        ),
        // ---- the webbed corridor ----
        room(
            "2-1",
            &[
                "thick, sticky webbing covers the walls of the corridor.",
                "deeper into the ship, the darkness seems almost to writhe.",
                "a small knapsack hangs from a cluster of webs, a few feet from the floor.",
            ],
            &[
                loot(Resource::CuredMeat, 0.8, 1, 5),
                loot(Resource::Bullets, 0.5, 1, 5),
                loot(Resource::EnergyCell, 0.2, 1, 5),
            ],
            &[goes("continue", Next::Scene("3-1")), LEAVE],
        ),
        fight(
            "3-1",
            "a huge arthropod lunges from the shadows, its mandibles thrashing.",
            Combat {
                enemy: "chitinous horror",
                chara: 'H',
                health: 60,
                damage: 1,
                hit: 0.7,
                attack_delay: 0.25,
                ranged: false,
                loot: &[
                    loot(Resource::Meat, 0.8, 5, 10),
                    loot(Resource::Scales, 0.5, 5, 10),
                ],
                ..Combat::PLAIN
            },
            &[goes("continue", Next::Scene("4-1")), LEAVE],
        ),
        fight(
            "4-1",
            "the webs part, and a grotesque insect lurches forward.",
            Combat {
                enemy: "chitinous queen",
                chara: 'Q',
                health: 70,
                damage: 1,
                hit: 0.7,
                attack_delay: 0.25,
                ranged: false,
                loot: &[
                    loot(Resource::Meat, 0.8, 8, 12),
                    loot(Resource::Scales, 0.5, 8, 12),
                ],
                ..Combat::PLAIN
            },
            &[goes("continue", Next::Scene("5")), LEAVE],
        ),
        // ---- the military camp ----
        fight(
            "2-2",
            "an operative waits in ambush around the corner.",
            Combat {
                enemy: "operative",
                chara: 'O',
                health: 60,
                damage: 8,
                hit: 0.8,
                attack_delay: 2.0,
                ranged: false,
                loot: &[
                    loot(Resource::Bayonet, 0.5, 1, 1),
                    loot(Resource::Bullets, 0.8, 1, 5),
                    loot(Resource::CuredMeat, 0.8, 1, 5),
                ],
                ..Combat::PLAIN
            },
            &[goes("continue", Next::Scene("3-2")), LEAVE],
        ),
        room(
            "3-2",
            &[
                "the military has set up a small camp just inside the ship.",
                "crude attempts have been made to cut into the walls.",
                "scraps of copper wire litter the floor.",
                "two bedrolls are wedged into a corner.",
            ],
            &[
                loot(Resource::CuredMeat, 1.0, 1, 5),
                loot(Resource::Torch, 0.8, 1, 3),
                loot(Resource::Bullets, 0.5, 1, 5),
                loot(Resource::AlienAlloy, 0.2, 1, 2),
            ],
            &[goes("continue", Next::Scene("4-2")), LEAVE],
        ),
        fight(
            "4-2",
            "a dusty researcher clumsily hides in the shadows.",
            Combat {
                enemy: "researcher",
                chara: 'R',
                health: 20,
                damage: 1,
                hit: 0.8,
                attack_delay: 2.0,
                ranged: false,
                loot: &[
                    loot(Resource::Torch, 0.8, 1, 3),
                    loot(Resource::Cloth, 0.8, 1, 5),
                    loot(Resource::CuredMeat, 0.8, 1, 5),
                ],
                ..Combat::PLAIN
            },
            &[goes("continue", Next::Scene("5")), LEAVE],
        ),
        // ---- the barricade ----
        room(
            "2-3",
            &[
                "debris is stacked in the corridor, forming a low barricade.",
                "the walls are scorched and melted.",
                "behind the barricade, a few weapons lay abandoned.",
            ],
            &[
                loot(Resource::LaserRifle, 1.0, 1, 3),
                loot(Resource::EnergyCell, 0.8, 1, 5),
                loot(Resource::PlasmaRifle, 0.2, 1, 1),
            ],
            &[goes("continue", Next::Scene("3-3")), LEAVE],
        ),
        room(
            "3-3",
            &[
                "the partially devoured remains of several wanderers are piled before a dark corridor.",
                "shuffling noises can be heard from within.",
            ],
            &[
                loot(Resource::EnergyCell, 0.5, 1, 5),
                loot(Resource::Cloth, 0.8, 1, 5),
            ],
            &[goes("continue", Next::Scene("4-3")), LEAVE],
        ),
        fight(
            "4-3",
            "an ancient beast has made these ruins its home.",
            Combat {
                enemy: "ancient beast",
                chara: 'A',
                health: 60,
                damage: 6,
                hit: 0.8,
                attack_delay: 1.0,
                ranged: false,
                loot: &[
                    loot(Resource::Fur, 1.0, 5, 10),
                    loot(Resource::Meat, 1.0, 5, 10),
                    loot(Resource::Teeth, 0.8, 5, 10),
                ],
                ..Combat::PLAIN
            },
            &[goes("continue", Next::Scene("5")), LEAVE],
        ),
        // ---- the sealed door, and what waking the ship costs ----
        room(
            "5",
            &[
                "a maintenance panel is embedded in the wall next to a large sealed door.",
                "perhaps the ship's systems are still operational.",
            ],
            &[],
            &[goes("power cycle", Next::Scene("6")), LEAVE],
        ),
        fight(
            "6",
            "as the lights come online, so too do the defence systems.",
            Combat {
                enemy: "automated turret",
                chara: 'T',
                health: 60,
                damage: 10,
                hit: 0.8,
                attack_delay: 2.5,
                ranged: true,
                loot: &[
                    loot(Resource::EnergyCell, 0.8, 1, 5),
                    loot(Resource::LaserRifle, 0.2, 1, 1),
                ],
                ..Combat::PLAIN
            },
            &[goes("continue", Next::Scene("7")), LEAVE],
        ),
        Scene {
            key: "7",
            text: &[
                "beyond the bulkhead is a small antechamber, seemingly untouched by scavengers.",
                "a large hatch grinds open, and the wind rushes in.",
                "a strange device sits on the floor. looks important.",
            ],
            // The square is deliberately *not* marked visited: the battleship
            // is the one landmark worth coming back to, and the antechamber is
            // what a later visit opens on.
            on_load: &[Effect::DrawRoad, Effect::EnterBattleship],
            buttons: &[Button::leave("take device and leave")],
            ..Scene::EMPTY
        },
    ],
};

// ---------------------------------------------------------------------------
// Deeper into a ravaged battleship (every visit after the first)
// ---------------------------------------------------------------------------

static ANTECHAMBER: Event = Event {
    key: "executioner-antechamber",
    title: "A Ravaged Battleship",
    available: &[],
    scenes: &[Scene {
        key: "start",
        text: &[
            "a large hatch opens into a wide corridor.",
            "the corridor leads to a bank of elevators, which appear to be functional.",
        ],
        // Each deck's button names the deck once; what it says and where it
        // goes both come off `Deck`, so a wing cannot end up labelled one
        // thing and opening another.
        buttons: &[
            Button {
                text: Deck::Engineering.label(),
                available: &[Condition::DeckPending(Deck::Engineering)],
                next: Next::Event(Deck::Engineering.scene()),
                ..Button::EMPTY
            },
            Button {
                text: Deck::Medical.label(),
                available: &[Condition::DeckPending(Deck::Medical)],
                next: Next::Event(Deck::Medical.scene()),
                ..Button::EMPTY
            },
            Button {
                text: Deck::Martial.label(),
                available: &[Condition::DeckPending(Deck::Martial)],
                next: Next::Event(Deck::Martial.scene()),
                ..Button::EMPTY
            },
            Button {
                text: "command deck",
                available: &[Condition::DecksClear],
                next: Next::Event("executioner-command"),
                ..Button::EMPTY
            },
            LEAVE,
        ],
        ..Scene::EMPTY
    }],
};

// ---------------------------------------------------------------------------
// Engineering wing
// ---------------------------------------------------------------------------

static ENGINEERING: Event = Event {
    key: "executioner-engineering",
    title: "Engineering Wing",
    available: &[],
    scenes: &[
        room(
            "start",
            &[
                "elevator doors open to a blasted corridor. debris covers the floor, piled into makeshift defences.",
                "emergency lighting flickers.",
            ],
            &[],
            &[
                goes(
                    "continue",
                    Next::Weighted(&[(0.3, "1-1"), (0.7, "1-2"), (1.0, "1-3")]),
                ),
                LEAVE,
            ],
        ),
        // ---- the assembly line ----
        room(
            "1-1",
            &[
                "an automated assembly line performs its empty routines, long since deprived of materials.",
                "its final works lie forgotten, covered by a thin layer of dust.",
            ],
            &[
                loot(Resource::EnergyCell, 0.8, 1, 5),
                loot(Resource::LaserRifle, 0.2, 1, 1),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "2-1a"), (1.0, "2-1b")])),
                LEAVE,
            ],
        ),
        fight(
            "2-1a",
            "assembly arms spin wildly out of control.",
            Combat {
                enemy: "unruly welder",
                chara: 'W',
                health: 50,
                damage: 13,
                hit: 0.8,
                attack_delay: 2.0,
                ranged: false,
                loot: &[
                    loot(Resource::EnergyCell, 0.8, 1, 5),
                    loot(Resource::AlienAlloy, 0.2, 1, 1),
                ],
                ..Combat::PLAIN
            },
            &[goes("continue", Next::Scene("3-1")), LEAVE],
        ),
        room(
            "2-1b",
            &[
                "assembly arms spark and jitter.",
                "a cacophony of decrepit machinery fills the room.",
            ],
            &[],
            &[goes("continue", Next::Scene("3-1")), LEAVE],
        ),
        fight(
            "3-1",
            GUARD_LINE,
            GUARD,
            &[goes("continue", Next::Scene("4")), LEAVE],
        ),
        // ---- the engine room ----
        fight(
            "1-2",
            TURRET_LINE,
            TURRET,
            &[goes("continue", Next::Scene("2-2")), LEAVE],
        ),
        room(
            "2-2",
            &[
                "must have been the engine room, once. the massive machines now stand inert, twisted and scorched by explosions.",
                "the destruction is uniform and precise.",
                "bits of them can be scavenged.",
            ],
            &[loot(Resource::AlienAlloy, 1.0, 2, 5)],
            &[
                goes("continue", Next::Weighted(&[(0.5, "3-2a"), (1.0, "3-2b")])),
                LEAVE,
            ],
        ),
        fight(
            "3-2a",
            GUARD_LINE,
            GUARD,
            &[goes("continue", Next::Scene("4")), LEAVE],
        ),
        room(
            "3-2b",
            &[
                "none of the ship's engines escaped the destruction.",
                "it's no mystery why she no longer flies.",
            ],
            &[],
            &[goes("continue", Next::Scene("4")), LEAVE],
        ),
        // ---- the burning junction ----
        Scene {
            key: "1-3",
            text: &[
                "sparks cascade from a reactivated power junction, and catch.",
                "the flames fill the corridor.",
            ],
            // No way out of this one but through: upstream offers no leave.
            // Both buttons carry a cost, so `Active::rows` falls back to a
            // leave row for a wanderer who can pay neither, rather than
            // holding the SSH session hostage.
            buttons: &[
                Button {
                    text: "extinguish",
                    cost: &[Cost::Water(5)],
                    next: Next::Weighted(&[(0.5, "2-3a"), (1.0, "2-3b")]),
                    ..Button::EMPTY
                },
                Button {
                    text: "rush through",
                    cost: &[Cost::Hp(10)],
                    next: Next::Weighted(&[(0.5, "2-3a"), (1.0, "2-3b")]),
                    ..Button::EMPTY
                },
            ],
            ..Scene::EMPTY
        },
        fight(
            "2-3a",
            GUARD_LINE,
            GUARD,
            &[goes("continue", Next::Scene("3-3")), LEAVE],
        ),
        room(
            "2-3b",
            &[
                "rows of inert security robots hang suspended from the ceiling.",
                "wires run overhead, corroded and useless.",
            ],
            &[],
            &[goes("continue", Next::Scene("3-3")), LEAVE],
        ),
        room(
            "3-3",
            &[
                "more signs of past combat down the hall. guard post is ransacked.",
                "still, some things can be found.",
            ],
            &[
                loot(Resource::EnergyCell, 0.8, 1, 5),
                loot(Resource::LaserRifle, 0.7, 1, 1),
                loot(Resource::Grenade, 0.6, 1, 3),
                loot(Resource::PlasmaRifle, 0.2, 1, 1),
            ],
            &[goes("continue", Next::Scene("4")), LEAVE],
        ),
        // ---- research and development ----
        Scene {
            key: "4",
            text: &[
                "marks on the door read 'research and development.' everything seems mostly untouched, but dead.",
                "one machine thrums with power, and might still work.",
            ],
            buttons: &[
                Button {
                    text: "use machine",
                    cost: &[Cost::Store(Resource::AlienAlloy, 1)],
                    effects: &[Effect::HealFull],
                    next: Next::Scene("4-heal"),
                    ..Button::EMPTY
                },
                goes("continue", Next::Weighted(&[(0.5, "5-1"), (1.0, "5-2")])),
                LEAVE,
            ],
            ..Scene::EMPTY
        },
        room(
            "4-heal",
            &["step inside, and the machine whirs. muscle and bone reknit. good as new."],
            &[],
            &[
                goes("continue", Next::Weighted(&[(0.5, "5-1"), (1.0, "5-2")])),
                LEAVE,
            ],
        ),
        fight(
            "5-1",
            TURRET_LINE,
            TURRET,
            &[goes("continue", Next::Scene("6")), LEAVE],
        ),
        room(
            "5-2",
            &[
                "the machines here look unfinished, abandoned by their creator. wires and other scrap are scattered about the work benches.",
            ],
            &[],
            &[goes("continue", Next::Scene("6")), LEAVE],
        ),
        room(
            "6",
            &[
                "experimental plans cover one wall, held by an unseen force.",
                "this one looks useful.",
            ],
            &[loot(Resource::HypoBlueprint, 1.0, 1, 1)],
            &[goes("continue", Next::Scene("7-intro")), LEAVE],
        ),
        Scene {
            key: "7-intro",
            text: &["clattering metal and old servos. something is coming..."],
            // No way past this one either.
            buttons: &[goes("fight", Next::Scene("7"))],
            ..Scene::EMPTY
        },
        fight(
            "7",
            "an unfinished automaton whirs to life.",
            Combat {
                enemy: "unstable prototype",
                chara: 'P',
                health: 150,
                damage: 5,
                hit: 0.8,
                attack_delay: 2.0,
                ranged: false,
                // Every five seconds it throws a shield up, and the next hit
                // heals it instead of hurting it.
                specials: &[Special {
                    delay: 5.0,
                    action: SpecialAction::Take(Status::Shield),
                }],
                loot: &[
                    loot(Resource::AlienAlloy, 1.0, 1, 3),
                    loot(Resource::KineticArmourBlueprint, 1.0, 1, 1),
                ],
                ..Combat::PLAIN
            },
            &[goes("continue", Next::Scene("8")), LEAVE],
        ),
        Scene {
            key: "8",
            text: &[
                "at the back of the workshop, elevator doors twitch and buzz.",
                "looks like a way out of here.",
            ],
            on_load: &[Effect::ClearDeck(Deck::Engineering)],
            buttons: &[LEAVE],
            ..Scene::EMPTY
        },
    ],
};

// ---------------------------------------------------------------------------
// Martial wing
// ---------------------------------------------------------------------------

static MARTIAL: Event = Event {
    key: "executioner-martial",
    title: "Martial Wing",
    available: &[],
    scenes: &[
        room(
            "start",
            &[
                "metal grinds, and the elevator doors open halfway. beyond is a brightly lit battlefield. remains litter the corridor, undisturbed by scavengers.",
                "looks like they tried to barricade the elevators.",
            ],
            &[],
            &[goes("continue", Next::Scene("1")), LEAVE],
        ),
        Scene {
            key: "1",
            text: &[
                "further along, the corridor branches.",
                "the door to the left is sealed and refuses to open.",
            ],
            buttons: &[
                Button {
                    text: "blow it down",
                    cost: &[Cost::Store(Resource::Grenade, 1)],
                    next: Next::Scene("2-1"),
                    ..Button::EMPTY
                },
                goes(
                    "continue right",
                    Next::Weighted(&[(0.5, "2-2"), (1.0, "2-3")]),
                ),
                LEAVE,
            ],
            ..Scene::EMPTY
        },
        // ---- behind the sealed door ----
        room(
            "2-1",
            &[
                "the blast throws the door inwards.",
                "through the bulkhead is a large room, walls lined with weapon racks. fighting seems to have passed it by.",
            ],
            &[
                loot(Resource::EnergyBlade, 1.0, 2, 5),
                loot(Resource::LaserRifle, 1.0, 2, 5),
                loot(Resource::EnergyCell, 1.0, 5, 20),
                loot(Resource::Grenade, 0.8, 1, 5),
                loot(Resource::PlasmaRifle, 0.2, 1, 1),
            ],
            &[goes("continue", Next::Scene("3-1")), LEAVE],
        ),
        fight(
            "3-1",
            TURRET_LINE,
            TURRET,
            &[goes("continue", Next::Scene("4-1")), LEAVE],
        ),
        room(
            "4-1",
            &[
                "another door at the end of the hall, sealed from this side.",
                "should be able to open it.",
            ],
            &[],
            &[goes("continue", Next::Scene("5")), LEAVE],
        ),
        // ---- the crew cabins ----
        fight(
            "2-2",
            TURRET_LINE,
            TURRET,
            &[
                goes("continue", Next::Weighted(&[(0.5, "3-2a"), (1.0, "3-2b")])),
                LEAVE,
            ],
        ),
        fight(
            "3-2a",
            QUADRUPED_LINE,
            QUADRUPED,
            &[goes("continue", Next::Scene("4-2")), LEAVE],
        ),
        room(
            "3-2b",
            &["the corridor is eerily silent."],
            &[],
            &[goes("continue", Next::Scene("4-2")), LEAVE],
        ),
        room(
            "4-2",
            &[
                "crew cabins flank the hall, devoid of life.",
                "a few useful items can be scavenged.",
            ],
            &[
                loot(Resource::EnergyCell, 1.0, 1, 5),
                loot(Resource::EnergyBlade, 0.2, 1, 1),
            ],
            &[goes("continue", Next::Scene("5")), LEAVE],
        ),
        // ---- the ruined turrets ----
        room(
            "2-3",
            &[
                "ruined defence turrets flank the corridor.",
                "could put the scrap to good use.",
            ],
            &[loot(Resource::AlienAlloy, 1.0, 1, 3)],
            &[
                goes("continue", Next::Weighted(&[(0.5, "3-3a"), (1.0, "3-3b")])),
                LEAVE,
            ],
        ),
        fight(
            "3-3a",
            GUARD_LINE,
            GUARD,
            &[goes("continue", Next::Scene("4-3")), LEAVE],
        ),
        room(
            "3-3b",
            &[
                "small sensors in the walls still look to be operational.",
                "easily avoided.",
            ],
            &[],
            &[goes("continue", Next::Scene("4-3")), LEAVE],
        ),
        fight(
            "4-3",
            QUADRUPED_LINE,
            QUADRUPED,
            &[goes("continue", Next::Scene("5")), LEAVE],
        ),
        // ---- the barricades and the plans ----
        room(
            "5",
            &[
                "large barricades bisect the corridor, scorched by weapons fire.",
                "bodies litter the ground on either side.",
            ],
            &[],
            &[goes("continue", Next::Scene("6")), LEAVE],
        ),
        room(
            "6",
            &[
                "documents are scattered down the hall, most charred and curled.",
                "this one looks interesting.",
            ],
            &[loot(Resource::PlasmaRifleBlueprint, 1.0, 1, 1)],
            &[
                goes("continue", Next::Weighted(&[(0.5, "7-1"), (1.0, "7-2")])),
                LEAVE,
            ],
        ),
        Scene {
            key: "7-1",
            text: &[
                "the next door leads to a ransacked planning room.",
                "maps of the surface can still be found amongst the debris.",
            ],
            buttons: &[
                Button {
                    text: "scavenge maps",
                    // Upstream calls `World.applyMap()` three times over.
                    effects: &[Effect::UncoverMap, Effect::UncoverMap, Effect::UncoverMap],
                    next: Next::Scene("8-1a"),
                    ..Button::EMPTY
                },
                goes("continue", Next::Scene("8-1b")),
                LEAVE,
            ],
            ..Scene::EMPTY
        },
        fight(
            "8-1a",
            "drew some attention with all that noise.",
            GUARD,
            &[goes("continue", Next::Scene("9-1")), LEAVE],
        ),
        room(
            "8-1b",
            &[
                "slipped past an automated sentry.",
                "if only they'd been destroyed along with everything else.",
            ],
            &[],
            &[goes("continue", Next::Scene("9-1")), LEAVE],
        ),
        fight(
            "9-1",
            "ran straight into another one.",
            GUARD,
            &[goes("continue", Next::Scene("10")), LEAVE],
        ),
        // ---- the containment cells ----
        room(
            "7-2",
            &[
                "the corridor passes through a security checkpoint. the defences are blown apart, ragged edges scorched by laser fire.",
                "past the checkpoint, banks of containment cells can be seen.",
            ],
            &[],
            &[
                goes("continue", Next::Weighted(&[(0.5, "8-2a"), (1.0, "8-2b")])),
                LEAVE,
            ],
        ),
        room(
            "8-2a",
            &[
                "the cells are all empty.",
                "power cables running across the ceiling are split in several places, sparking occasionally.",
            ],
            &[],
            &[goes("continue", Next::Scene("9-2")), LEAVE],
        ),
        room(
            "8-2b",
            &[
                "the guards died at their posts, shot through with superheated plasma.",
                "their weapons lie on the floor beside them.",
            ],
            &[
                loot(Resource::LaserRifle, 1.0, 2, 2),
                loot(Resource::EnergyCell, 1.0, 5, 10),
            ],
            &[goes("continue", Next::Scene("9-2")), LEAVE],
        ),
        fight(
            "9-2",
            QUADRUPED_LINE,
            QUADRUPED,
            &[goes("continue", Next::Scene("10")), LEAVE],
        ),
        // ---- the training complex ----
        Scene {
            key: "10",
            text: &[
                "the corridor opens onto a vast training complex, obstacles and features blackened by real combat.",
                "a regenerative machine hums uncannily by one of the courses.",
            ],
            buttons: &[
                Button {
                    text: "use machine",
                    cost: &[Cost::Store(Resource::AlienAlloy, 1)],
                    effects: &[Effect::HealFull],
                    next: Next::Scene("11"),
                    ..Button::EMPTY
                },
                goes("continue", Next::Scene("11")),
                LEAVE,
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "11",
            text: &[
                "motion from the centre of the yard.",
                "a sparring automaton, still fully function and crusted with timeworn blood, lunges forward.",
            ],
            buttons: &[goes("engage", Next::Scene("12"))],
            ..Scene::EMPTY
        },
        fight(
            "12",
            "the machine attacks, blades whirling.",
            Combat {
                enemy: "murderous robot",
                chara: 'M',
                health: 250,
                damage: 10,
                hit: 0.8,
                attack_delay: 3.0,
                ranged: false,
                // Thirteen seconds in it energises, and every swing after that
                // hits four times as hard.
                specials: &[Special {
                    delay: 13.0,
                    action: SpecialAction::Take(Status::Energised),
                }],
                loot: &[
                    loot(Resource::AlienAlloy, 1.0, 1, 3),
                    loot(Resource::DisruptorBlueprint, 1.0, 1, 1),
                ],
                ..Combat::PLAIN
            },
            &[goes("continue", Next::Scene("13"))],
        ),
        Scene {
            key: "13",
            text: &[
                "the ruins of the sparring machine clatter to the ground.",
                "picked this deck clean.",
            ],
            on_load: &[Effect::ClearDeck(Deck::Martial)],
            buttons: &[LEAVE],
            ..Scene::EMPTY
        },
    ],
};

// ---------------------------------------------------------------------------
// Medical wing
// ---------------------------------------------------------------------------

static MEDICAL: Event = Event {
    key: "executioner-medical",
    title: "Medical Wing",
    available: &[],
    scenes: &[
        room(
            "start",
            &[
                "elevator doors open to an empty corridor.",
                "a few dusty corpses can be seen further down, but this deck appears to have been spared most of the combat.",
            ],
            &[],
            &[goes("continue", Next::Scene("1")), LEAVE],
        ),
        fight(
            "1",
            TURRET_LINE,
            TURRET,
            &[goes("continue", Next::Scene("2")), LEAVE],
        ),
        room(
            "2",
            &[
                "past the checkpoint, the corridor is undamaged save for sporadic graffiti.",
                "there was no fighting here.",
            ],
            &[],
            &[
                goes("continue", Next::Weighted(&[(0.5, "3a"), (1.0, "3b")])),
                LEAVE,
            ],
        ),
        fight(
            "3a",
            QUADRUPED_LINE,
            QUADRUPED,
            &[goes("continue", Next::Scene("4")), LEAVE],
        ),
        room(
            "3b",
            &[
                "automated guardians still stalk the halls, unaware that their masters have long gone.",
                "clumsy machines, and easily avoided.",
            ],
            &[],
            &[goes("continue", Next::Scene("4")), LEAVE],
        ),
        room(
            "4",
            &[
                "medical gurneys are fixed to grooves running down the corridor walls.",
                "the automated patient transport system now sits motionless.",
            ],
            &[],
            &[
                goes("continue", Next::Weighted(&[(0.5, "5-1"), (1.0, "5-2")])),
                LEAVE,
            ],
        ),
        // ---- the dispatch bay ----
        fight(
            "5-1",
            MEDIC_LINE,
            MEDIC,
            &[
                goes("continue", Next::Weighted(&[(0.5, "6-1a"), (1.0, "6-1b")])),
                LEAVE,
            ],
        ),
        fight(
            "6-1a",
            "it had friends.",
            MEDIC,
            &[goes("continue", Next::Scene("7-1")), LEAVE],
        ),
        room(
            "6-1b",
            &[
                "more medical robots stand frozen, attached by a network of wires.",
                "they take no notice of the intrusion.",
            ],
            &[],
            &[goes("continue", Next::Scene("7-1")), LEAVE],
        ),
        room(
            "7-1",
            &[
                "weapons are strewn about the medical dispatch bay. must have been used as a muster point.",
                "more strange graffiti adorns the walls.",
            ],
            &[
                loot(Resource::LaserRifle, 1.0, 1, 1),
                loot(Resource::EnergyCell, 1.0, 3, 10),
            ],
            &[goes("continue", Next::Scene("8")), LEAVE],
        ),
        // ---- the strategy room ----
        Scene {
            key: "5-2",
            text: &[
                "this ward has been converted to a makeshift strategy room, maps scrawled hastily on any flat surface.",
                "a secure locker is set into one wall.",
            ],
            buttons: &[
                goes("force locker", Next::Scene("6-2a-intro")),
                goes("continue", Next::Scene("6-2b")),
                LEAVE,
            ],
            ..Scene::EMPTY
        },
        room(
            "6-2a-intro",
            &["hinges rusted through. no challenge."],
            &[
                loot(Resource::EnergyCell, 1.0, 5, 10),
                loot(Resource::Hypo, 1.0, 1, 3),
            ],
            &[goes("continue", Next::Scene("6-2a")), LEAVE],
        ),
        fight(
            "6-2a",
            "the noise draws attention.",
            MEDIC,
            &[goes("continue", Next::Scene("7-2")), LEAVE],
        ),
        room(
            "6-2b",
            &[
                "better to move without drawing attention.",
                "noises can be heard from the corridor outside.",
            ],
            &[],
            &[goes("continue", Next::Scene("7-2")), LEAVE],
        ),
        fight(
            "7-2",
            QUADRUPED_LINE,
            QUADRUPED,
            &[goes("continue", Next::Scene("8")), LEAVE],
        ),
        // ---- the thing that goes off ----
        fight(
            "8",
            "something's wrong with this robot.",
            Combat {
                enemy: "unstable automaton",
                chara: 'A',
                health: 100,
                damage: 10,
                hit: 0.7,
                attack_delay: 2.0,
                ranged: false,
                // It does not fall over; it goes off, and it takes thirty with
                // it whatever the wanderer does.
                explosion: Some(30),
                loot: &[loot(Resource::GlowstoneBlueprint, 1.0, 1, 1)],
                ..Combat::PLAIN
            },
            &[goes("continue", Next::Scene("9")), LEAVE],
        ),
        room(
            "9",
            &[
                "another checkpoint ahead, fitted with heavy doors.",
                "security is even tighter here.",
            ],
            &[],
            &[
                goes("continue", Next::Weighted(&[(0.5, "10a"), (1.0, "10b")])),
                LEAVE,
            ],
        ),
        fight(
            "10a",
            GUARD_LINE,
            GUARD,
            &[goes("continue", Next::Scene("11")), LEAVE],
        ),
        room(
            "10b",
            &[
                "slipped through unnoticed.",
                "air whistles as the doors open. this section must have lower pressure than the rest of the ship.",
            ],
            &[],
            &[goes("continue", Next::Scene("11")), LEAVE],
        ),
        fight(
            "11",
            MEDIC_LINE,
            MEDIC,
            &[
                goes("continue", Next::Weighted(&[(0.5, "12-1"), (1.0, "12-2")])),
                LEAVE,
            ],
        ),
        // ---- the cold store ----
        room(
            "12-1",
            &[
                "the air is cooler here. low cabinets ring the room, doors dusted with frost.",
                "samples of something biological inside.",
            ],
            &[loot(Resource::CuredMeat, 1.0, 5, 10)],
            &[
                goes(
                    "continue",
                    Next::Weighted(&[(0.5, "13-1a"), (1.0, "13-1b")]),
                ),
                LEAVE,
            ],
        ),
        fight(
            "13-1a",
            GUARD_LINE,
            GUARD,
            &[goes("continue", Next::Scene("14-1")), LEAVE],
        ),
        room(
            "13-1b",
            &[
                "security drones still patrol the hallways.",
                "predictable paths.",
            ],
            &[],
            &[goes("continue", Next::Scene("14-1")), LEAVE],
        ),
        fight(
            "14-1",
            MEDIC_LINE,
            MEDIC,
            &[goes("continue", Next::Scene("15")), LEAVE],
        ),
        // ---- the surgery ----
        room(
            "12-2",
            &[
                "surgical tools are scattered on the floor, near what appears the be the remains of a fire.",
                "strange.",
            ],
            &[],
            &[
                goes(
                    "continue",
                    Next::Weighted(&[(0.5, "13-2a"), (1.0, "13-2b")]),
                ),
                LEAVE,
            ],
        ),
        fight(
            "13-2a",
            MEDIC_LINE,
            MEDIC,
            &[goes("continue", Next::Scene("14-2")), LEAVE],
        ),
        room(
            "13-2b",
            &[
                "the air in this room has a metallic tinge. floor is covered in dark powder.",
                "some completed explosives in the corner.",
            ],
            &[loot(Resource::Grenade, 1.0, 3, 8)],
            &[goes("continue", Next::Scene("14-2")), LEAVE],
        ),
        fight(
            "14-2",
            MEDIC_LINE,
            MEDIC,
            &[goes("continue", Next::Scene("15")), LEAVE],
        ),
        // ---- what was kept in the cells ----
        room(
            "15",
            &[
                "containment cells arranged at the back of the room, all open.",
                "something moving up ahead.",
            ],
            &[],
            &[goes("continue", Next::Scene("16")), LEAVE],
        ),
        fight(
            "16",
            "a mutated beast leaps from its cell.",
            Combat {
                enemy: "malformed experiment",
                chara: 'E',
                health: 200,
                damage: 5,
                hit: 0.8,
                attack_delay: 2.0,
                ranged: false,
                // Sixteen seconds in it goes into a frenzy, and keeps going
                // back into one.
                specials: &[Special {
                    delay: 16.0,
                    action: SpecialAction::Take(Status::Enraged),
                }],
                loot: &[loot(Resource::StimBlueprint, 1.0, 1, 1)],
                ..Combat::PLAIN
            },
            &[goes("continue", Next::Scene("17"))],
        ),
        Scene {
            key: "17",
            text: &[
                "the creature's tortured breathing ceases.",
                "nothing more here.",
            ],
            on_load: &[Effect::ClearDeck(Deck::Medical)],
            buttons: &[LEAVE],
            ..Scene::EMPTY
        },
    ],
};

// ---------------------------------------------------------------------------
// Command deck
// ---------------------------------------------------------------------------

static COMMAND: Event = Event {
    key: "executioner-command",
    title: "Command Deck",
    available: &[],
    scenes: &[
        room(
            "start",
            &[
                "the path to the command bridge is wide, walls adorned with decorative shields.",
                "fighting hadn't reached here, it seems.",
            ],
            &[],
            &[goes("continue", Next::Scene("1")), LEAVE],
        ),
        fight(
            "1",
            GUARD_LINE,
            GUARD,
            &[goes("continue", Next::Scene("2")), LEAVE],
        ),
        room(
            "2",
            &[
                "detour through the officer's lounge.",
                "might be something useful here.",
            ],
            &[],
            &[
                goes("continue", Next::Weighted(&[(0.5, "3a"), (1.0, "3b")])),
                LEAVE,
            ],
        ),
        room(
            "3a",
            &["small weapons cache in a cabinet.", "lucky."],
            &[
                loot(Resource::EnergyCell, 1.0, 3, 10),
                loot(Resource::Grenade, 0.8, 1, 5),
            ],
            &[goes("continue", Next::Scene("4")), LEAVE],
        ),
        room(
            "3b",
            &["found some medical supplies in a discarded bag."],
            &[loot(Resource::Hypo, 1.0, 1, 3)],
            &[goes("continue", Next::Scene("4")), LEAVE],
        ),
        room(
            "4",
            &[
                "the command deck is empty, save for a squat figure sitting motionless in the centre of the room.",
                "in a flash, the figure is standing.",
            ],
            &[],
            &[goes("approach", Next::Scene("5")), LEAVE],
        ),
        Scene {
            key: "5",
            text: &[
                "wanderer form, but not quite flesh. not quite metal either. a crystal set into its chest pulses with light.",
                "it says it saw the rebellion coming. said it made arrangements.",
                "says it can't die.",
            ],
            // There is no walking away from this one.
            buttons: &[goes("observe", Next::Scene("6"))],
            ..Scene::EMPTY
        },
        fight(
            "6",
            "the immortal wanderer attacks.",
            Combat {
                enemy: "immortal wanderer",
                chara: '@',
                health: 500,
                damage: 12,
                hit: 0.8,
                attack_delay: 2.0,
                ranged: false,
                // Every seven seconds it picks a new trick, never the one it
                // just used: shield the next hit, work itself into a rage, or
                // sit and bank everything thrown at it to give back at once.
                specials: &[Special {
                    delay: 7.0,
                    action: SpecialAction::RotateCommand,
                }],
                loot: &[loot(Resource::FleetBeacon, 1.0, 1, 1)],
                ..Combat::PLAIN
            },
            &[goes("continue", Next::Scene("7"))],
        ),
        Scene {
            key: "7",
            text: &[
                "the crystal pulses brightly, then goes dark. the assailant shimmers as its shape becomes less defined.",
                "then it is gone.",
                "time to get out of here.",
            ],
            // The wreck becomes an outpost, which is also what stops the
            // square from ever offering the antechamber again. That, and the
            // beacon now sitting in the pack, is the whole record that this
            // happened: upstream sets no flag here either.
            on_load: &[Effect::ClearDungeon],
            buttons: &[LEAVE],
            ..Scene::EMPTY
        },
    ],
};
