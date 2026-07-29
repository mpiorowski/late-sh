/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * This Source Code Form is "Incompatible With Secondary Licenses", as
 * defined by the Mozilla Public License, v. 2.0.
 *
 * Derived from A Dark Room by Michael Townsend / Doublespeak Games
 * (https://github.com/doublespeakgames/adarkroom), MPL-2.0. Every scene, loot
 * table and line below is transcribed from `script/events/setpieces.js`. See
 * LICENSING.md and NOTICE. */

//! The landmarks. Stepping onto one of these squares starts its setpiece, and
//! playing it out is what turns the map into an economy: the mines feed the
//! village, the city arms the wanderer, and the crashed ship is the way off
//! this rock.

use super::data::{Building, Perk, Resource};
use super::event::{Button, Combat, Cost, Effect, Event, Loot, Next, Scene};

/// A setpiece by table key. The world names them by the landmark's `scene`.
pub fn by_key(key: &str) -> Option<&'static Event> {
    SETPIECES.iter().find(|event| event.key == key)
}

pub static SETPIECES: [Event; 12] = [
    OUTPOST,
    HOUSE,
    CAVE,
    TOWN,
    CITY,
    SWAMP,
    BOREHOLE,
    BATTLEFIELD,
    SHIP,
    IRON_MINE,
    COAL_MINE,
    SULPHUR_MINE,
];

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

/// A "go on" button that costs a torch.
const fn torchlit(text: &'static str, next: Next) -> Button {
    Button {
        text,
        cost: &[Cost::Store(Resource::Torch, 1)],
        next,
        ..Button::EMPTY
    }
}

/// An enemy's stat line, exactly the fields upstream hangs off a combat
/// scene. Bundled so a fight scene reads as "who" plus "what happens around
/// the fight".
struct Foe {
    enemy: &'static str,
    chara: char,
    health: i64,
    damage: i64,
    hit: f64,
    attack_delay: f64,
    ranged: bool,
}

const fn foe(
    enemy: &'static str,
    chara: char,
    health: i64,
    damage: i64,
    hit: f64,
    attack_delay: f64,
    ranged: bool,
) -> Foe {
    Foe {
        enemy,
        chara,
        health,
        damage,
        hit,
        attack_delay,
        ranged,
    }
}

/// A fight scene. Upstream's setpiece fights carry no death line of their
/// own; the buttons underneath say what happens next.
const fn brawl(
    key: &'static str,
    notification: &'static str,
    foe: Foe,
    spoils: &'static [Loot],
    buttons: &'static [Button],
) -> Scene {
    Scene {
        key,
        notification: Some(notification),
        combat: Some(Combat {
            enemy: foe.enemy,
            chara: foe.chara,
            health: foe.health,
            damage: foe.damage,
            hit: foe.hit,
            attack_delay: foe.attack_delay,
            ranged: foe.ranged,
            death_message: "",
            loot: spoils,
            next: Next::End,
        }),
        buttons,
        ..Scene::EMPTY
    }
}

// ---------------------------------------------------------------------------
// The outpost
// ---------------------------------------------------------------------------

static OUTPOST: Event = Event {
    key: "outpost",
    title: "An Outpost",
    available: &[],
    scenes: &[Scene {
        key: "start",
        text: &["a safe place in the wilds."],
        notification: Some("a safe place in the wilds."),
        on_load: &[Effect::UseOutpost],
        loot: &[loot(Resource::CuredMeat, 1.0, 5, 10)],
        buttons: &[Button::leave("leave")],
        ..Scene::EMPTY
    }],
};

// ---------------------------------------------------------------------------
// The old house
// ---------------------------------------------------------------------------

static HOUSE: Event = Event {
    key: "house",
    title: "An Old House",
    available: &[],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "an old house remains here, once white siding yellowed and peeling.",
                "the door hangs open.",
            ],
            notification: Some("the remains of an old house stand as a monument to simpler times"),
            buttons: &[
                goes(
                    "go inside",
                    Next::Weighted(&[(0.25, "medicine"), (0.5, "supplies"), (1.0, "occupied")]),
                ),
                Button::leave("leave"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "supplies",
            text: &[
                "the house is abandoned, but not yet picked over.",
                "still a few drops of water in the old well.",
            ],
            on_load: &[Effect::MarkVisited, Effect::RefillWater],
            loot: &[
                loot(Resource::CuredMeat, 0.8, 1, 10),
                loot(Resource::Leather, 0.2, 1, 10),
                loot(Resource::Cloth, 0.5, 1, 10),
            ],
            buttons: &[Button::leave("leave")],
            ..Scene::EMPTY
        },
        Scene {
            key: "medicine",
            text: &[
                "the house has been ransacked.",
                "but there is a cache of medicine under the floorboards.",
            ],
            on_load: &[Effect::MarkVisited],
            loot: &[loot(Resource::Medicine, 1.0, 2, 5)],
            buttons: &[Button::leave("leave")],
            ..Scene::EMPTY
        },
        Scene {
            on_load: &[Effect::MarkVisited],
            ..brawl(
                "occupied",
                "a man charges down the hall, a rusty blade in his hand",
                foe("squatter", 'E', 10, 3, 0.8, 2.0, false),
                &[
                    loot(Resource::CuredMeat, 0.8, 1, 10),
                    loot(Resource::Leather, 0.2, 1, 10),
                    loot(Resource::Cloth, 0.5, 1, 10),
                ],
                &[Button::leave("leave")],
            )
        },
    ],
};

// ---------------------------------------------------------------------------
// The cave
// ---------------------------------------------------------------------------

static CAVE: Event = Event {
    key: "cave",
    title: "A Damp Cave",
    available: &[],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "the mouth of the cave is wide and dark.",
                "can't see what's inside.",
            ],
            notification: Some("the earth here is split, as if bearing an ancient wound"),
            buttons: &[
                torchlit(
                    "go inside",
                    Next::Weighted(&[(0.3, "a1"), (0.6, "a2"), (1.0, "a3")]),
                ),
                Button::leave("leave"),
            ],
            ..Scene::EMPTY
        },
        brawl(
            "a1",
            "a startled beast defends its home",
            foe("beast", 'R', 5, 1, 0.8, 1.0, false),
            &[
                loot(Resource::Fur, 1.0, 1, 10),
                loot(Resource::Teeth, 0.8, 1, 5),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "b1"), (1.0, "b2")])),
                Button::leave("leave cave"),
            ],
        ),
        Scene {
            key: "a2",
            text: &[
                "the cave narrows a few feet in.",
                "the walls are moist and moss-covered",
            ],
            buttons: &[
                goes("squeeze", Next::Weighted(&[(0.5, "b2"), (1.0, "b3")])),
                Button::leave("leave cave"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "a3",
            text: &[
                "the remains of an old camp sits just inside the cave.",
                "bedrolls, torn and blackened, lay beneath a thin layer of dust.",
            ],
            loot: &[
                loot(Resource::CuredMeat, 1.0, 1, 5),
                loot(Resource::Torch, 0.5, 1, 5),
                loot(Resource::Leather, 0.3, 1, 5),
            ],
            buttons: &[
                goes("continue", Next::Weighted(&[(0.5, "b3"), (1.0, "b4")])),
                Button::leave("leave cave"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "b1",
            text: &[
                "the body of a wanderer lies in a small cavern.",
                "rot's been to work on it, and some of the pieces are missing.",
                "can't tell what left it here.",
            ],
            loot: &[
                loot(Resource::IronSword, 1.0, 1, 1),
                loot(Resource::CuredMeat, 0.8, 1, 5),
                loot(Resource::Torch, 0.5, 1, 3),
                loot(Resource::Medicine, 0.1, 1, 2),
            ],
            buttons: &[
                goes("continue", Next::Scene("c1")),
                Button::leave("leave cave"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "b2",
            text: &[
                "the torch sputters and dies in the damp air",
                "the darkness is absolute",
            ],
            notification: Some("the torch goes out"),
            buttons: &[
                torchlit("continue", Next::Scene("c1")),
                Button::leave("leave cave"),
            ],
            ..Scene::EMPTY
        },
        brawl(
            "b3",
            "a startled beast defends its home",
            foe("beast", 'R', 5, 1, 0.8, 1.0, false),
            &[
                loot(Resource::Fur, 1.0, 1, 3),
                loot(Resource::Teeth, 0.8, 1, 2),
            ],
            &[
                goes("continue", Next::Scene("c2")),
                Button::leave("leave cave"),
            ],
        ),
        brawl(
            "b4",
            "a cave lizard attacks",
            foe("cave lizard", 'R', 6, 3, 0.8, 2.0, false),
            &[
                loot(Resource::Scales, 1.0, 1, 3),
                loot(Resource::Teeth, 0.8, 1, 2),
            ],
            &[
                goes("continue", Next::Scene("c2")),
                Button::leave("leave cave"),
            ],
        ),
        brawl(
            "c1",
            "a large beast charges out of the dark",
            foe("beast", 'R', 10, 3, 0.8, 2.0, false),
            &[
                loot(Resource::Fur, 1.0, 1, 3),
                loot(Resource::Teeth, 1.0, 1, 3),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "end1"), (1.0, "end2")])),
                Button::leave("leave cave"),
            ],
        ),
        brawl(
            "c2",
            "a giant lizard shambles forward",
            foe("lizard", 'T', 10, 4, 0.8, 2.0, false),
            &[
                loot(Resource::Scales, 1.0, 1, 3),
                loot(Resource::Teeth, 1.0, 1, 3),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.7, "end2"), (1.0, "end3")])),
                Button::leave("leave cave"),
            ],
        ),
        Scene {
            key: "end1",
            text: &["the nest of a large animal lies at the back of the cave."],
            on_load: &[Effect::ClearDungeon],
            loot: &[
                loot(Resource::Meat, 1.0, 5, 10),
                loot(Resource::Fur, 1.0, 5, 10),
                loot(Resource::Scales, 1.0, 5, 10),
                loot(Resource::Teeth, 1.0, 5, 10),
                loot(Resource::Cloth, 0.5, 5, 10),
            ],
            buttons: &[Button::leave("leave cave")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end2",
            text: &["a small supply cache is hidden at the back of the cave."],
            on_load: &[Effect::ClearDungeon],
            loot: &[
                loot(Resource::Cloth, 1.0, 5, 10),
                loot(Resource::Leather, 1.0, 5, 10),
                loot(Resource::Iron, 1.0, 5, 10),
                loot(Resource::CuredMeat, 1.0, 5, 10),
                loot(Resource::Steel, 0.5, 5, 10),
                loot(Resource::Bolas, 0.3, 1, 3),
                loot(Resource::Medicine, 0.15, 1, 4),
            ],
            buttons: &[Button::leave("leave cave")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end3",
            text: &["an old case is wedged behind a rock, covered in a thick layer of dust."],
            on_load: &[Effect::ClearDungeon],
            loot: &[
                loot(Resource::SteelSword, 1.0, 1, 1),
                loot(Resource::Bolas, 0.5, 1, 3),
                loot(Resource::Medicine, 0.3, 1, 3),
            ],
            buttons: &[Button::leave("leave cave")],
            ..Scene::EMPTY
        },
    ],
};

// ---------------------------------------------------------------------------
// The town
// ---------------------------------------------------------------------------

static TOWN: Event = Event {
    key: "town",
    title: "A Deserted Town",
    available: &[],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "a small suburb lays ahead, empty houses scorched and peeling.",
                "broken streetlights stand, rusting. light hasn't graced this place in a long time.",
            ],
            notification: Some("the town lies abandoned, its citizens long dead"),
            buttons: &[
                goes(
                    "explore",
                    Next::Weighted(&[(0.3, "a1"), (0.7, "a3"), (1.0, "a2")]),
                ),
                Button::leave("leave"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "a1",
            text: &[
                "where the windows of the schoolhouse aren't shattered, they're blackened with soot.",
                "the double doors creak endlessly in the wind.",
            ],
            buttons: &[
                torchlit("enter", Next::Weighted(&[(0.5, "b1"), (1.0, "b2")])),
                Button::leave("leave town"),
            ],
            ..Scene::EMPTY
        },
        brawl(
            "a2",
            "ambushed on the street.",
            foe("thug", 'E', 30, 4, 0.8, 2.0, false),
            &[
                loot(Resource::Cloth, 0.8, 5, 10),
                loot(Resource::Leather, 0.8, 5, 10),
                loot(Resource::CuredMeat, 0.5, 1, 5),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "b3"), (1.0, "b4")])),
                Button::leave("leave town"),
            ],
        ),
        Scene {
            key: "a3",
            text: &[
                "a squat building up ahead.",
                "a green cross barely visible behind grimy windows.",
            ],
            buttons: &[
                torchlit("enter", Next::Weighted(&[(0.5, "b5"), (1.0, "end5")])),
                Button::leave("leave town"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "b1",
            text: &["a small cache of supplies is tucked inside a rusting locker."],
            loot: &[
                loot(Resource::CuredMeat, 1.0, 1, 5),
                loot(Resource::Torch, 0.8, 1, 3),
                loot(Resource::Bullets, 0.3, 1, 5),
                loot(Resource::Medicine, 0.05, 1, 3),
            ],
            buttons: &[
                goes("continue", Next::Weighted(&[(0.5, "c1"), (1.0, "c2")])),
                Button::leave("leave town"),
            ],
            ..Scene::EMPTY
        },
        brawl(
            "b2",
            "a scavenger waits just inside the door.",
            foe("scavenger", 'E', 30, 4, 0.8, 2.0, false),
            &[
                loot(Resource::Cloth, 0.8, 5, 10),
                loot(Resource::Leather, 0.8, 5, 10),
                loot(Resource::CuredMeat, 0.5, 1, 5),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "c2"), (1.0, "c3")])),
                Button::leave("leave town"),
            ],
        ),
        brawl(
            "b3",
            "a beast stands alone in an overgrown park.",
            foe("beast", 'R', 25, 3, 0.8, 1.0, false),
            &[
                loot(Resource::Teeth, 1.0, 1, 5),
                loot(Resource::Fur, 1.0, 5, 10),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "c4"), (1.0, "c5")])),
                Button::leave("leave town"),
            ],
        ),
        Scene {
            key: "b4",
            text: &[
                "an overturned caravan is spread across the pockmarked street.",
                "it's been picked over by scavengers, but there's still some things worth taking.",
            ],
            loot: &[
                loot(Resource::CuredMeat, 0.8, 1, 5),
                loot(Resource::Torch, 0.5, 1, 3),
                loot(Resource::Bullets, 0.3, 1, 5),
                loot(Resource::Medicine, 0.1, 1, 3),
            ],
            buttons: &[
                goes("continue", Next::Weighted(&[(0.5, "c5"), (1.0, "c6")])),
                Button::leave("leave town"),
            ],
            ..Scene::EMPTY
        },
        brawl(
            "b5",
            "a madman attacks, screeching.",
            foe("madman", 'E', 10, 6, 0.3, 1.0, false),
            &[
                loot(Resource::Cloth, 0.3, 2, 4),
                loot(Resource::CuredMeat, 0.9, 1, 5),
                loot(Resource::Medicine, 0.4, 1, 2),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.3, "end5"), (1.0, "end6")])),
                Button::leave("leave town"),
            ],
        ),
        brawl(
            "c1",
            "a thug moves out of the shadows.",
            foe("thug", 'E', 30, 4, 0.8, 2.0, false),
            &[
                loot(Resource::Cloth, 0.8, 5, 10),
                loot(Resource::Leather, 0.8, 5, 10),
                loot(Resource::CuredMeat, 0.5, 1, 5),
            ],
            &[
                goes("continue", Next::Scene("d1")),
                Button::leave("leave town"),
            ],
        ),
        brawl(
            "c2",
            "a beast charges out of a ransacked classroom.",
            foe("beast", 'R', 25, 3, 0.8, 1.0, false),
            &[
                loot(Resource::Teeth, 1.0, 1, 5),
                loot(Resource::Fur, 1.0, 5, 10),
            ],
            &[
                goes("continue", Next::Scene("d1")),
                Button::leave("leave town"),
            ],
        ),
        Scene {
            key: "c3",
            text: &[
                "through the large gymnasium doors, footsteps can be heard.",
                "the torchlight casts a flickering glow down the hallway.",
                "the footsteps stop.",
            ],
            buttons: &[
                goes("enter", Next::Scene("d1")),
                Button::leave("leave town"),
            ],
            ..Scene::EMPTY
        },
        brawl(
            "c4",
            "another beast, draw by the noise, leaps out of a copse of trees.",
            foe("beast", 'R', 25, 4, 0.8, 1.0, false),
            &[
                loot(Resource::Teeth, 1.0, 1, 5),
                loot(Resource::Fur, 1.0, 5, 10),
            ],
            &[
                goes("continue", Next::Scene("d2")),
                Button::leave("leave town"),
            ],
        ),
        Scene {
            key: "c5",
            text: &[
                "something's causing a commotion a ways down the road.",
                "a fight, maybe.",
            ],
            buttons: &[
                goes("continue", Next::Scene("d2")),
                Button::leave("leave town"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "c6",
            text: &[
                "a small basket of food is hidden under a park bench, with a note attached.",
                "can't read the words.",
            ],
            loot: &[loot(Resource::CuredMeat, 1.0, 1, 5)],
            buttons: &[
                goes("continue", Next::Scene("d2")),
                Button::leave("leave town"),
            ],
            ..Scene::EMPTY
        },
        brawl(
            "d1",
            "a panicked scavenger bursts through the door, screaming.",
            foe("scavenger", 'E', 30, 5, 0.8, 2.0, false),
            &[
                loot(Resource::CuredMeat, 1.0, 1, 5),
                loot(Resource::Leather, 0.8, 5, 10),
                loot(Resource::SteelSword, 0.5, 1, 1),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "end1"), (1.0, "end2")])),
                Button::leave("leave town"),
            ],
        ),
        brawl(
            "d2",
            "a man stands over a dead wanderer. notices he's not alone.",
            foe("vigilante", 'D', 30, 6, 0.8, 2.0, false),
            &[
                loot(Resource::CuredMeat, 1.0, 1, 5),
                loot(Resource::Leather, 0.8, 5, 10),
                loot(Resource::SteelSword, 0.5, 1, 1),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "end3"), (1.0, "end4")])),
                Button::leave("leave town"),
            ],
        ),
        Scene {
            key: "end1",
            text: &[
                "scavenger had a small camp in the school.",
                "collected scraps spread across the floor like they fell from heaven.",
            ],
            on_load: &[Effect::ClearDungeon],
            loot: &[
                loot(Resource::SteelSword, 1.0, 1, 1),
                loot(Resource::Steel, 1.0, 5, 10),
                loot(Resource::CuredMeat, 1.0, 5, 10),
                loot(Resource::Bolas, 0.5, 1, 5),
                loot(Resource::Medicine, 0.3, 1, 2),
            ],
            buttons: &[Button::leave("leave town")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end2",
            text: &[
                "scavenger'd been looking for supplies in here, it seems.",
                "a shame to let what he'd found go to waste.",
            ],
            on_load: &[Effect::ClearDungeon],
            loot: &[
                loot(Resource::Coal, 1.0, 5, 10),
                loot(Resource::CuredMeat, 1.0, 5, 10),
                loot(Resource::Leather, 1.0, 5, 10),
            ],
            buttons: &[Button::leave("leave town")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end3",
            text: &[
                "beneath the wanderer's rags, clutched in one of its many hands, a glint of steel.",
                "worth killing for, it seems.",
            ],
            on_load: &[Effect::ClearDungeon],
            loot: &[
                loot(Resource::Rifle, 1.0, 1, 1),
                loot(Resource::Bullets, 1.0, 1, 5),
            ],
            buttons: &[Button::leave("leave town")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end4",
            text: &[
                "eye for an eye seems fair.",
                "always worked before, at least.",
                "picking the bones finds some useful trinkets.",
            ],
            on_load: &[Effect::ClearDungeon],
            loot: &[
                loot(Resource::CuredMeat, 1.0, 5, 10),
                loot(Resource::Iron, 1.0, 5, 10),
                loot(Resource::Torch, 1.0, 1, 5),
                loot(Resource::Bolas, 0.5, 1, 5),
                loot(Resource::Medicine, 0.1, 1, 2),
            ],
            buttons: &[Button::leave("leave town")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end5",
            text: &["some medicine abandoned in the drawers."],
            on_load: &[Effect::ClearDungeon],
            loot: &[loot(Resource::Medicine, 1.0, 2, 5)],
            buttons: &[Button::leave("leave town")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end6",
            text: &[
                "the clinic has been ransacked.",
                "only dust and stains remain.",
            ],
            on_load: &[Effect::ClearDungeon],
            buttons: &[Button::leave("leave town")],
            ..Scene::EMPTY
        },
    ],
};

// ---------------------------------------------------------------------------
// The city
// ---------------------------------------------------------------------------

/// Everything that finishes the city also brings the soldiers down on the
/// village, which is upstream's `game.cityCleared`.
const CITY_CLEARED: &[Effect] = &[Effect::ClearDungeon, Effect::ClearCity];

static CITY: Event = Event {
    key: "city",
    title: "A Ruined City",
    available: &[],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "a battered highway sign stands guard at the entrance to this once-great city.",
                "the towers that haven't crumbled jut from the landscape like the ribcage of some ancient beast.",
                "might be things worth having still inside.",
            ],
            notification: Some("the towers of a decaying city dominate the skyline"),
            buttons: &[
                goes(
                    "explore",
                    Next::Weighted(&[(0.2, "a1"), (0.5, "a2"), (0.8, "a3"), (1.0, "a4")]),
                ),
                Button::leave("leave"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "a1",
            text: &[
                "the streets are empty.",
                "the air is filled with dust, driven relentlessly by the hard winds.",
            ],
            buttons: &[
                goes("continue", Next::Weighted(&[(0.5, "b1"), (1.0, "b2")])),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "a2",
            text: &[
                "orange traffic cones are set across the street, faded and cracked.",
                "lights flash through the alleys between buildings.",
            ],
            buttons: &[
                goes("continue", Next::Weighted(&[(0.5, "b3"), (1.0, "b4")])),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "a3",
            text: &[
                "a large shanty town sprawls across the streets.",
                "faces, darkened by soot and blood, stare out from crooked huts.",
            ],
            buttons: &[
                goes("continue", Next::Weighted(&[(0.5, "b5"), (1.0, "b6")])),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "a4",
            text: &["the shell of an abandoned hospital looms ahead."],
            buttons: &[
                torchlit("enter", Next::Weighted(&[(0.5, "b7"), (1.0, "b8")])),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "b1",
            text: &[
                "the old tower seems mostly intact.",
                "the shell of a burned out car blocks the entrance.",
                "most of the windows at ground level are busted anyway.",
            ],
            buttons: &[
                goes("enter", Next::Weighted(&[(0.5, "c1"), (1.0, "c2")])),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        brawl(
            "b2",
            "a huge lizard scrambles up out of the darkness of an old metro station.",
            foe("lizard", 'R', 20, 5, 0.8, 2.0, false),
            &[
                loot(Resource::Scales, 0.8, 5, 10),
                loot(Resource::Teeth, 0.5, 5, 10),
                loot(Resource::Meat, 0.8, 5, 10),
            ],
            &[
                goes("descend", Next::Weighted(&[(0.5, "c2"), (1.0, "c3")])),
                Button::leave("leave city"),
            ],
        ),
        brawl(
            "b3",
            "the shot echoes in the empty street.",
            foe("sniper", 'D', 30, 15, 0.8, 4.0, true),
            &[
                loot(Resource::CuredMeat, 0.8, 1, 5),
                loot(Resource::Bullets, 0.5, 1, 5),
                loot(Resource::Rifle, 0.2, 1, 1),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "c4"), (1.0, "c5")])),
                Button::leave("leave city"),
            ],
        ),
        brawl(
            "b4",
            "the soldier steps out from between the buildings, rifle raised.",
            foe("soldier", 'D', 50, 8, 0.8, 2.0, true),
            &[
                loot(Resource::CuredMeat, 0.8, 1, 5),
                loot(Resource::Bullets, 0.5, 1, 5),
                loot(Resource::Rifle, 0.2, 1, 1),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "c5"), (1.0, "c6")])),
                Button::leave("leave city"),
            ],
        ),
        brawl(
            "b5",
            "a frail man stands defiantly, blocking the path.",
            foe("frail man", 'E', 10, 1, 0.8, 2.0, false),
            &[
                loot(Resource::CuredMeat, 0.8, 1, 5),
                loot(Resource::Cloth, 0.5, 1, 5),
                loot(Resource::Leather, 0.2, 1, 1),
                loot(Resource::Medicine, 0.05, 1, 3),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "c7"), (1.0, "c8")])),
                Button::leave("leave city"),
            ],
        ),
        Scene {
            key: "b6",
            text: &[
                "nothing but downcast eyes.",
                "the people here were broken a long time ago.",
            ],
            buttons: &[
                goes("continue", Next::Weighted(&[(0.5, "c8"), (1.0, "c9")])),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "b7",
            text: &[
                "empty corridors.",
                "the place has been swept clean by scavengers.",
            ],
            buttons: &[
                goes(
                    "continue",
                    Next::Weighted(&[(0.3, "c12"), (0.7, "c10"), (1.0, "c11")]),
                ),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        brawl(
            "b8",
            "an old man bursts through a door, wielding a scalpel.",
            foe("old man", 'E', 10, 3, 0.5, 2.0, false),
            &[
                loot(Resource::CuredMeat, 0.5, 1, 3),
                loot(Resource::Cloth, 0.8, 1, 5),
                loot(Resource::Medicine, 0.5, 1, 2),
            ],
            &[
                goes(
                    "continue",
                    Next::Weighted(&[(0.3, "c13"), (0.7, "c11"), (1.0, "end15")]),
                ),
                Button::leave("leave city"),
            ],
        ),
        brawl(
            "c1",
            "a thug is waiting on the other side of the wall.",
            foe("thug", 'E', 30, 3, 0.8, 2.0, false),
            &[
                loot(Resource::SteelSword, 0.5, 1, 1),
                loot(Resource::CuredMeat, 0.5, 1, 3),
                loot(Resource::Cloth, 0.8, 1, 5),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "d1"), (1.0, "d2")])),
                Button::leave("leave city"),
            ],
        ),
        brawl(
            "c2",
            "a snarling beast jumps out from behind a car.",
            foe("beast", 'R', 30, 2, 0.8, 1.0, false),
            &[
                loot(Resource::Meat, 0.8, 1, 5),
                loot(Resource::Fur, 0.8, 1, 5),
                loot(Resource::Teeth, 0.5, 1, 5),
            ],
            &[
                goes("continue", Next::Scene("d2")),
                Button::leave("leave city"),
            ],
        ),
        Scene {
            key: "c3",
            text: &[
                "street above the subway platform is blown away.",
                "lets some light down into the dusty haze.",
                "a sound comes from the tunnel, just ahead.",
            ],
            buttons: &[
                torchlit("investigate", Next::Weighted(&[(0.5, "d2"), (1.0, "d3")])),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "c4",
            text: &[
                "looks like a camp of sorts up ahead.",
                "rusted chainlink is pulled across an alleyway.",
                "fires burn in the courtyard beyond.",
            ],
            buttons: &[
                goes("continue", Next::Weighted(&[(0.5, "d4"), (1.0, "d5")])),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "c5",
            text: &[
                "more voices can be heard ahead.",
                "they must be here for a reason.",
            ],
            buttons: &[
                goes("continue", Next::Scene("d5")),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "c6",
            text: &[
                "the sound of gunfire carries on the wind.",
                "the street ahead glows with firelight.",
            ],
            buttons: &[
                goes("continue", Next::Weighted(&[(0.5, "d5"), (1.0, "d6")])),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "c7",
            text: &[
                "more squatters are crowding around now.",
                "someone throws a stone.",
            ],
            buttons: &[
                goes("continue", Next::Weighted(&[(0.5, "d7"), (1.0, "d8")])),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "c8",
            text: &[
                "an improvised shop is set up on the sidewalk.",
                "the owner stands by, stoic.",
            ],
            loot: &[
                loot(Resource::SteelSword, 0.8, 1, 1),
                loot(Resource::Rifle, 0.5, 1, 1),
                loot(Resource::Bullets, 0.25, 1, 8),
                loot(Resource::AlienAlloy, 0.01, 1, 1),
                loot(Resource::Medicine, 0.5, 1, 4),
            ],
            buttons: &[
                goes("continue", Next::Scene("d8")),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "c9",
            text: &[
                "strips of meat hang drying by the side of the street.",
                "the people back away, avoiding eye contact.",
            ],
            loot: &[loot(Resource::CuredMeat, 1.0, 5, 10)],
            buttons: &[
                goes("continue", Next::Weighted(&[(0.5, "d8"), (1.0, "d9")])),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "c10",
            text: &["someone has locked and barricaded the door to this operating theatre."],
            buttons: &[
                goes(
                    "continue",
                    Next::Weighted(&[(0.2, "end12"), (0.6, "d10"), (1.0, "d11")]),
                ),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        brawl(
            "c11",
            "a tribe of elderly squatters is camped out in this ward.",
            foe("squatters", 'E', 40, 2, 0.7, 0.5, false),
            &[
                loot(Resource::CuredMeat, 0.5, 1, 3),
                loot(Resource::Cloth, 0.8, 3, 8),
                loot(Resource::Medicine, 0.3, 1, 3),
            ],
            &[
                goes("continue", Next::Scene("end10")),
                Button::leave("leave city"),
            ],
        ),
        brawl(
            "c12",
            "a pack of lizards rounds the corner.",
            foe("lizards", 'R', 30, 4, 0.7, 0.7, false),
            &[
                loot(Resource::Meat, 1.0, 3, 8),
                loot(Resource::Teeth, 1.0, 2, 4),
                loot(Resource::Scales, 1.0, 3, 5),
            ],
            &[
                goes("continue", Next::Scene("end10")),
                Button::leave("leave city"),
            ],
        ),
        Scene {
            key: "c13",
            text: &["strips of meat are hung up to dry in this ward."],
            loot: &[loot(Resource::CuredMeat, 1.0, 3, 10)],
            buttons: &[
                goes(
                    "continue",
                    Next::Weighted(&[(0.5, "end10"), (1.0, "end11")]),
                ),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        brawl(
            "d1",
            "a large bird nests at the top of the stairs.",
            foe("bird", 'R', 45, 5, 0.7, 1.0, false),
            &[loot(Resource::Meat, 0.8, 5, 10)],
            &[
                goes("continue", Next::Weighted(&[(0.5, "end1"), (1.0, "end2")])),
                Button::leave("leave city"),
            ],
        ),
        Scene {
            key: "d2",
            text: &[
                "the debris is denser here.",
                "maybe some useful stuff in the rubble.",
            ],
            loot: &[
                loot(Resource::Bullets, 0.5, 1, 5),
                loot(Resource::Steel, 0.8, 1, 10),
                loot(Resource::AlienAlloy, 0.01, 1, 1),
                loot(Resource::Cloth, 1.0, 1, 10),
            ],
            buttons: &[
                goes("continue", Next::Scene("end2")),
                Button::leave("leave city"),
            ],
            ..Scene::EMPTY
        },
        brawl(
            "d3",
            "a swarm of rats rushes up the tunnel.",
            foe("rats", 'R', 60, 1, 0.8, 0.25, false),
            &[
                loot(Resource::Fur, 0.8, 5, 10),
                loot(Resource::Teeth, 0.5, 5, 10),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "end2"), (1.0, "end3")])),
                Button::leave("leave city"),
            ],
        ),
        brawl(
            "d4",
            "a large man attacks, waving a bayonet.",
            foe("veteran", 'D', 45, 6, 0.8, 2.0, false),
            &[
                loot(Resource::Bayonet, 0.5, 1, 1),
                loot(Resource::CuredMeat, 0.8, 1, 5),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "end4"), (1.0, "end5")])),
                Button::leave("leave city"),
            ],
        ),
        brawl(
            "d5",
            "a second soldier opens fire.",
            foe("soldier", 'D', 50, 8, 0.8, 2.0, true),
            &[
                loot(Resource::CuredMeat, 0.8, 1, 5),
                loot(Resource::Bullets, 0.5, 1, 5),
                loot(Resource::Rifle, 0.2, 1, 1),
            ],
            &[
                goes("continue", Next::Scene("end5")),
                Button::leave("leave city"),
            ],
        ),
        brawl(
            "d6",
            "a masked soldier rounds the corner, gun drawn",
            foe("commando", 'D', 55, 3, 0.9, 2.0, true),
            &[
                loot(Resource::Rifle, 0.5, 1, 1),
                loot(Resource::Bullets, 0.8, 1, 5),
                loot(Resource::CuredMeat, 0.8, 1, 5),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "end5"), (1.0, "end6")])),
                Button::leave("leave city"),
            ],
        ),
        brawl(
            "d7",
            "the crowd surges forward.",
            foe("squatters", 'E', 40, 2, 0.7, 0.5, false),
            &[
                loot(Resource::Cloth, 0.8, 1, 5),
                loot(Resource::Teeth, 0.5, 1, 5),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "end7"), (1.0, "end8")])),
                Button::leave("leave city"),
            ],
        ),
        brawl(
            "d8",
            "a youth lashes out with a tree branch.",
            foe("youth", 'E', 45, 2, 0.7, 1.0, false),
            &[
                loot(Resource::Cloth, 0.8, 1, 5),
                loot(Resource::Teeth, 0.5, 1, 5),
            ],
            &[
                goes("continue", Next::Scene("end8")),
                Button::leave("leave city"),
            ],
        ),
        brawl(
            "d9",
            "a squatter stands firmly in the doorway of a small hut.",
            foe("squatter", 'E', 20, 3, 0.8, 2.0, false),
            &[
                loot(Resource::Cloth, 0.8, 1, 5),
                loot(Resource::Teeth, 0.5, 1, 5),
            ],
            &[
                goes("continue", Next::Weighted(&[(0.5, "end8"), (1.0, "end9")])),
                Button::leave("leave city"),
            ],
        ),
        brawl(
            "d10",
            "behind the door, a deformed figure awakes and attacks.",
            foe("deformed", 'T', 40, 8, 0.6, 2.0, false),
            &[
                loot(Resource::Cloth, 0.8, 1, 5),
                loot(Resource::Teeth, 1.0, 2, 2),
                loot(Resource::Steel, 0.6, 1, 3),
                loot(Resource::Scales, 0.1, 2, 3),
            ],
            &[goes("continue", Next::Scene("end14"))],
        ),
        brawl(
            "d11",
            "as soon as the door is open a little bit, hundreds of tentacles erupt.",
            foe("tentacles", 'T', 60, 2, 0.6, 0.5, false),
            &[loot(Resource::Meat, 1.0, 10, 20)],
            &[goes("continue", Next::Scene("end13"))],
        ),
        Scene {
            key: "end1",
            text: &[
                "bird must have liked shiney things.",
                "some good stuff woven into its nest.",
            ],
            on_load: CITY_CLEARED,
            loot: &[
                loot(Resource::Bullets, 0.8, 5, 10),
                loot(Resource::Bolas, 0.5, 1, 5),
                loot(Resource::AlienAlloy, 0.5, 1, 1),
            ],
            buttons: &[Button::leave("leave city")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end2",
            text: &[
                "not much here.",
                "scavengers must have gotten to this place already.",
            ],
            on_load: CITY_CLEARED,
            loot: &[
                loot(Resource::Torch, 0.8, 1, 5),
                loot(Resource::CuredMeat, 0.5, 1, 5),
            ],
            buttons: &[Button::leave("leave city")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end3",
            text: &[
                "the tunnel opens up at another platform.",
                "the walls are scorched from an old battle.",
                "bodies and supplies from both sides litter the ground.",
            ],
            on_load: CITY_CLEARED,
            loot: &[
                loot(Resource::Rifle, 0.8, 1, 1),
                loot(Resource::Bullets, 0.8, 1, 5),
                loot(Resource::LaserRifle, 0.3, 1, 1),
                loot(Resource::EnergyCell, 0.3, 1, 5),
                loot(Resource::AlienAlloy, 0.3, 1, 1),
            ],
            buttons: &[Button::leave("leave city")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end4",
            text: &[
                "the small military outpost is well supplied.",
                "arms and munitions, relics from the war, are neatly arranged on the store-room floor.",
                "just as deadly now as they were then.",
            ],
            on_load: CITY_CLEARED,
            loot: &[
                loot(Resource::Rifle, 1.0, 1, 1),
                loot(Resource::Bullets, 1.0, 1, 10),
                loot(Resource::Grenade, 0.8, 1, 5),
            ],
            buttons: &[Button::leave("leave city")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end5",
            text: &[
                "searching the bodies yields a few supplies.",
                "more soldiers will be on their way.",
                "time to move on.",
            ],
            on_load: CITY_CLEARED,
            loot: &[
                loot(Resource::Rifle, 1.0, 1, 1),
                loot(Resource::Bullets, 1.0, 1, 10),
                loot(Resource::CuredMeat, 0.8, 1, 5),
                loot(Resource::Medicine, 0.1, 1, 4),
            ],
            buttons: &[Button::leave("leave city")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end6",
            text: &[
                "the small settlement has clearly been burning a while.",
                "the bodies of the wanderers that lived here are still visible in the flames.",
            ],
            on_load: CITY_CLEARED,
            loot: &[
                loot(Resource::LaserRifle, 0.5, 1, 1),
                loot(Resource::EnergyCell, 0.5, 1, 5),
                loot(Resource::CuredMeat, 1.0, 1, 10),
            ],
            buttons: &[Button::leave("leave city")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end7",
            text: &["the remaining settlers flee from the violence, their belongings forgotten."],
            on_load: CITY_CLEARED,
            loot: &[
                loot(Resource::SteelSword, 0.8, 1, 1),
                loot(Resource::EnergyCell, 0.5, 1, 5),
                loot(Resource::CuredMeat, 1.0, 1, 10),
            ],
            buttons: &[Button::leave("leave city")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end8",
            text: &["the young settler was carrying a canvas sack."],
            on_load: CITY_CLEARED,
            loot: &[
                loot(Resource::SteelSword, 0.8, 1, 1),
                loot(Resource::Bolas, 0.5, 1, 5),
                loot(Resource::CuredMeat, 1.0, 1, 10),
            ],
            buttons: &[Button::leave("leave city")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end9",
            text: &["inside the hut, a child cries."],
            on_load: CITY_CLEARED,
            loot: &[
                loot(Resource::Rifle, 0.8, 1, 1),
                loot(Resource::Bullets, 0.8, 1, 5),
                loot(Resource::Bolas, 0.5, 1, 5),
                loot(Resource::AlienAlloy, 0.2, 1, 1),
            ],
            buttons: &[Button::leave("leave city")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end10",
            text: &[
                "the stench of rot and death fills the operating theatres.",
                "a few items are scattered on the ground.",
                "there is nothing else here.",
            ],
            on_load: CITY_CLEARED,
            loot: &[
                loot(Resource::EnergyCell, 0.3, 1, 1),
                loot(Resource::Medicine, 0.3, 1, 5),
                loot(Resource::Teeth, 1.0, 3, 8),
                loot(Resource::Scales, 0.9, 4, 7),
            ],
            buttons: &[Button::leave("leave city")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end11",
            text: &[
                "a pristine medicine cabinet at the end of a hallway.",
                "the rest of the hospital is empty.",
            ],
            on_load: CITY_CLEARED,
            loot: &[
                loot(Resource::EnergyCell, 0.2, 1, 1),
                loot(Resource::Medicine, 1.0, 3, 10),
                loot(Resource::Teeth, 0.2, 1, 2),
            ],
            buttons: &[Button::leave("leave city")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end12",
            text: &["someone had been stockpiling loot here."],
            on_load: CITY_CLEARED,
            loot: &[
                loot(Resource::EnergyCell, 0.2, 1, 3),
                loot(Resource::Medicine, 0.5, 3, 10),
                loot(Resource::Bullets, 1.0, 2, 8),
                loot(Resource::Torch, 0.5, 1, 3),
                loot(Resource::Grenade, 0.5, 1, 1),
                loot(Resource::AlienAlloy, 0.8, 1, 2),
            ],
            buttons: &[Button::leave("leave city")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end13",
            text: &[
                "the tentacular horror is defeated.",
                "inside, the remains of its victims are everywhere.",
            ],
            on_load: CITY_CLEARED,
            loot: &[
                loot(Resource::SteelSword, 0.5, 1, 3),
                loot(Resource::Rifle, 0.3, 1, 2),
                loot(Resource::Teeth, 1.0, 2, 8),
                loot(Resource::Cloth, 0.5, 3, 6),
                loot(Resource::AlienAlloy, 0.1, 1, 1),
            ],
            buttons: &[Button::leave("leave city")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end14",
            text: &[
                "the warped man lies dead.",
                "the operating theatre has a lot of curious equipment.",
            ],
            on_load: CITY_CLEARED,
            loot: &[
                loot(Resource::EnergyCell, 0.8, 2, 5),
                loot(Resource::Medicine, 1.0, 3, 12),
                loot(Resource::Cloth, 0.5, 1, 3),
                loot(Resource::Steel, 0.3, 2, 3),
                loot(Resource::AlienAlloy, 0.3, 1, 1),
            ],
            buttons: &[Button::leave("leave city")],
            ..Scene::EMPTY
        },
        Scene {
            key: "end15",
            text: &["the old man had a small cache of interesting items."],
            on_load: CITY_CLEARED,
            loot: &[
                loot(Resource::AlienAlloy, 0.8, 1, 1),
                loot(Resource::Medicine, 1.0, 1, 4),
                loot(Resource::CuredMeat, 1.0, 3, 7),
                loot(Resource::Bolas, 0.5, 1, 3),
                loot(Resource::Fur, 0.8, 1, 5),
            ],
            buttons: &[Button::leave("leave city")],
            ..Scene::EMPTY
        },
    ],
};

// ---------------------------------------------------------------------------
// The quiet ones
// ---------------------------------------------------------------------------

static SWAMP: Event = Event {
    key: "swamp",
    title: "A Murky Swamp",
    available: &[],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "rotting reeds rise out of the swampy earth.",
                "a lone frog sits in the muck, silently.",
            ],
            notification: Some("a swamp festers in the stagnant air."),
            buttons: &[goes("enter", Next::Scene("cabin")), Button::leave("leave")],
            ..Scene::EMPTY
        },
        Scene {
            key: "cabin",
            text: &[
                "deep in the swamp is a moss-covered cabin.",
                "an old wanderer sits inside, in a seeming trance.",
            ],
            buttons: &[
                Button {
                    text: "talk",
                    cost: &[Cost::Store(Resource::Charm, 1)],
                    next: Next::Scene("talk"),
                    ..Button::EMPTY
                },
                Button::leave("leave"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "talk",
            text: &[
                "the wanderer takes the charm and nods slowly.",
                "he speaks of once leading the great fleets to fresh worlds.",
                "unfathomable destruction to fuel wanderer hungers.",
                "his time here, now, is his penance.",
            ],
            on_load: &[Effect::GrantPerk(Perk::Gastronome), Effect::MarkVisited],
            buttons: &[Button::leave("leave")],
            ..Scene::EMPTY
        },
    ],
};

static BOREHOLE: Event = Event {
    key: "borehole",
    title: "A Huge Borehole",
    available: &[],
    scenes: &[Scene {
        key: "start",
        text: &[
            "a huge hole is cut deep into the earth, evidence of the past harvest.",
            "they took what they came for, and left.",
            "castoff from the mammoth drills can still be found by the edges of the precipice.",
        ],
        on_load: &[Effect::MarkVisited],
        loot: &[loot(Resource::AlienAlloy, 1.0, 1, 3)],
        buttons: &[Button::leave("leave")],
        ..Scene::EMPTY
    }],
};

static BATTLEFIELD: Event = Event {
    key: "battlefield",
    title: "A Forgotten Battlefield",
    available: &[],
    scenes: &[Scene {
        key: "start",
        text: &[
            "a battle was fought here, long ago.",
            "battered technology from both sides lays dormant on the blasted landscape.",
        ],
        on_load: &[Effect::MarkVisited],
        loot: &[
            loot(Resource::Rifle, 0.5, 1, 3),
            loot(Resource::Bullets, 0.8, 5, 20),
            loot(Resource::LaserRifle, 0.3, 1, 3),
            loot(Resource::EnergyCell, 0.5, 5, 10),
            loot(Resource::Grenade, 0.5, 1, 5),
            loot(Resource::AlienAlloy, 0.3, 1, 1),
        ],
        buttons: &[Button::leave("leave")],
        ..Scene::EMPTY
    }],
};

static SHIP: Event = Event {
    key: "ship",
    title: "A Crashed Ship",
    available: &[],
    scenes: &[Scene {
        key: "start",
        text: &[
            "the familiar curves of a wanderer vessel rise up out of the dust and ash.",
            "lucky that the natives can't work the mechanisms.",
            "with a little effort, it might fly again.",
        ],
        on_load: &[Effect::MarkVisited, Effect::DrawRoad, Effect::FoundShip],
        buttons: &[Button::leave("salvage")],
        ..Scene::EMPTY
    }],
};

// ---------------------------------------------------------------------------
// The mines, which is what the village is really out here for
// ---------------------------------------------------------------------------

static IRON_MINE: Event = Event {
    key: "ironmine",
    title: "The Iron Mine",
    available: &[],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "an old iron mine sits here, tools abandoned and left to rust.",
                "bleached bones are strewn about the entrance. many, deeply scored with jagged grooves.",
                "feral howls echo out of the darkness.",
            ],
            notification: Some("the path leads to an abandoned mine"),
            buttons: &[
                torchlit("go inside", Next::Scene("enter")),
                Button::leave("leave"),
            ],
            ..Scene::EMPTY
        },
        brawl(
            "enter",
            "a large creature lunges, muscles rippling in the torchlight",
            foe("beastly matriarch", 'T', 10, 4, 0.8, 2.0, false),
            &[
                loot(Resource::Teeth, 1.0, 5, 10),
                loot(Resource::Scales, 0.8, 5, 10),
                loot(Resource::Cloth, 0.5, 5, 10),
            ],
            &[goes("leave", Next::Scene("cleared"))],
        ),
        Scene {
            key: "cleared",
            text: &["the beast is dead.", "the mine is now safe for workers."],
            notification: Some("the iron mine is clear of dangers"),
            on_load: &[
                Effect::DrawRoad,
                Effect::GrantBuilding(Building::IronMine),
                Effect::MarkVisited,
            ],
            buttons: &[Button::leave("leave")],
            ..Scene::EMPTY
        },
    ],
};

static COAL_MINE: Event = Event {
    key: "coalmine",
    title: "The Coal Mine",
    available: &[],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "camp fires burn by the entrance to the mine.",
                "men mill about, weapons at the ready.",
            ],
            notification: Some("this old mine is not abandoned"),
            buttons: &[goes("attack", Next::Scene("a1")), Button::leave("leave")],
            ..Scene::EMPTY
        },
        brawl(
            "a1",
            "a man joins the fight",
            foe("man", 'E', 10, 3, 0.8, 2.0, false),
            &[
                loot(Resource::CuredMeat, 0.8, 1, 5),
                loot(Resource::Cloth, 0.8, 1, 5),
            ],
            &[goes("continue", Next::Scene("a2")), Button::leave("run")],
        ),
        brawl(
            "a2",
            "a man joins the fight",
            foe("man", 'E', 10, 3, 0.8, 2.0, false),
            &[
                loot(Resource::CuredMeat, 0.8, 1, 5),
                loot(Resource::Cloth, 0.8, 1, 5),
            ],
            &[goes("continue", Next::Scene("a3")), Button::leave("run")],
        ),
        brawl(
            "a3",
            "only the chief remains.",
            foe("chief", 'D', 20, 5, 0.8, 2.0, false),
            &[
                loot(Resource::CuredMeat, 1.0, 5, 10),
                loot(Resource::Cloth, 0.8, 5, 10),
                loot(Resource::Iron, 0.8, 1, 5),
            ],
            &[goes("continue", Next::Scene("cleared"))],
        ),
        Scene {
            key: "cleared",
            text: &[
                "the camp is still, save for the crackling of the fires.",
                "the mine is now safe for workers.",
            ],
            notification: Some("the coal mine is clear of dangers"),
            on_load: &[
                Effect::DrawRoad,
                Effect::GrantBuilding(Building::CoalMine),
                Effect::MarkVisited,
            ],
            buttons: &[Button::leave("leave")],
            ..Scene::EMPTY
        },
    ],
};

static SULPHUR_MINE: Event = Event {
    key: "sulphurmine",
    title: "The Sulphur Mine",
    available: &[],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "the military is already set up at the mine's entrance.",
                "soldiers patrol the perimeter, rifles slung over their shoulders.",
            ],
            notification: Some("a military perimeter is set up around the mine."),
            buttons: &[goes("attack", Next::Scene("a1")), Button::leave("leave")],
            ..Scene::EMPTY
        },
        brawl(
            "a1",
            "a soldier, alerted, opens fire.",
            foe("soldier", 'D', 50, 8, 0.8, 2.0, true),
            &[
                loot(Resource::CuredMeat, 0.8, 1, 5),
                loot(Resource::Bullets, 0.5, 1, 5),
                loot(Resource::Rifle, 0.2, 1, 1),
            ],
            &[goes("continue", Next::Scene("a2")), Button::leave("run")],
        ),
        brawl(
            "a2",
            "a second soldier joins the fight.",
            foe("soldier", 'D', 50, 8, 0.8, 2.0, true),
            &[
                loot(Resource::CuredMeat, 0.8, 1, 5),
                loot(Resource::Bullets, 0.5, 1, 5),
                loot(Resource::Rifle, 0.2, 1, 1),
            ],
            &[goes("continue", Next::Scene("a3")), Button::leave("run")],
        ),
        brawl(
            "a3",
            "a grizzled soldier attacks, waving a bayonet.",
            foe("veteran", 'D', 65, 10, 0.8, 2.0, false),
            &[
                loot(Resource::Bayonet, 0.5, 1, 1),
                loot(Resource::CuredMeat, 0.8, 1, 5),
            ],
            &[goes("continue", Next::Scene("cleared"))],
        ),
        Scene {
            key: "cleared",
            text: &[
                "the military presence has been cleared.",
                "the mine is now safe for workers.",
            ],
            notification: Some("the sulphur mine is clear of dangers"),
            on_load: &[
                Effect::DrawRoad,
                Effect::GrantBuilding(Building::SulphurMine),
                Effect::MarkVisited,
            ],
            buttons: &[Button::leave("leave")],
            ..Scene::EMPTY
        },
    ],
};
