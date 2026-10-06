/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * This Source Code Form is "Incompatible With Secondary Licenses", as
 * defined by the Mozilla Public License, v. 2.0.
 *
 * Derived from A Dark Room by Michael Townsend / Doublespeak Games
 * (https://github.com/doublespeakgames/adarkroom), MPL-2.0. Every scene and
 * every line below is transcribed from `script/events/global.js`,
 * `script/events/room.js` and `script/events/outside.js`. See LICENSING.md
 * and NOTICE. */

//! What wanders into the village. The pool the session rolls against every
//! few minutes while the player is in the door.

use super::data::{Perk, Resource};
use super::event::{Button, Condition, Cost, Effect, Event, Kill, Next, Scene};

/// Every village event, in upstream's pool order (global, room, outside).
pub static POOL: [Event; 16] = [
    THIEF,
    NOMAD,
    NOISES_OUTSIDE,
    NOISES_INSIDE,
    BEGGAR,
    SHADY_BUILDER,
    WANDERER_WOOD,
    WANDERER_FUR,
    SCOUT,
    MASTER,
    SICK_MAN,
    RUINED_TRAP,
    HUT_FIRE,
    SICKNESS,
    PLAGUE,
    BEAST_ATTACK,
];

/// The military raid needs a cleared city, so it lives beside the pool and is
/// appended when that has happened (upstream keeps it in the same list, with
/// the condition doing the work).
pub static MILITARY_RAID: Event = Event {
    key: "military raid",
    title: "A Military Raid",
    available: &[
        Condition::InOutside,
        Condition::PopulationOver(0),
        Condition::CityCleared,
    ],
    scenes: &[Scene {
        key: "start",
        text: &[
            "a gunshot rings through the trees.",
            "well armed men charge out of the forest, firing into the crowd.",
            "after a skirmish they are driven away, but not without losses.",
        ],
        notification: Some("troops storm the village"),
        on_load: &[Effect::KillVillagers(Kill::Range(1, 41))],
        reward: &[(Resource::Bullets, 10), (Resource::CuredMeat, 50)],
        buttons: &[Button {
            text: "go home",
            notification: Some("warfare is bloodthirsty"),
            ..Button::EMPTY
        }],
        ..Scene::EMPTY
    }],
};

// ---------------------------------------------------------------------------
// Global
// ---------------------------------------------------------------------------

static THIEF: Event = Event {
    key: "thief",
    title: "The Thief",
    available: &[Condition::InVillage, Condition::ThievesActive],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "the villagers haul a filthy man out of the store room.",
                "say his folk have been skimming the supplies.",
                "say he should be strung up as an example.",
            ],
            notification: Some("a thief is caught"),
            buttons: &[
                Button {
                    text: "hang him",
                    next: Next::Scene("hang"),
                    ..Button::EMPTY
                },
                Button {
                    text: "spare him",
                    next: Next::Scene("spare"),
                    ..Button::EMPTY
                },
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "hang",
            text: &[
                "the villagers hang the thief high in front of the store room.",
                "the point is made. in the next few days, the missing supplies are returned.",
            ],
            on_load: &[Effect::SettleThieves {
                pay_back: true,
                perk: None,
            }],
            buttons: &[Button::leave("leave")],
            ..Scene::EMPTY
        },
        Scene {
            key: "spare",
            text: &[
                "the man says he's grateful. says he won't come around any more.",
                "shares what he knows about sneaking before he goes.",
            ],
            on_load: &[Effect::SettleThieves {
                pay_back: false,
                perk: Some(Perk::Stealthy),
            }],
            buttons: &[Button::leave("leave")],
            ..Scene::EMPTY
        },
    ],
};

// ---------------------------------------------------------------------------
// The room
// ---------------------------------------------------------------------------

static NOMAD: Event = Event {
    key: "nomad",
    title: "The Nomad",
    available: &[Condition::InRoom, Condition::HasStore(Resource::Fur)],
    scenes: &[Scene {
        key: "start",
        text: &[
            "a nomad shuffles into view, laden with makeshift bags bound with rough twine.",
            "won't say from where he came, but it's clear that he's not staying.",
        ],
        notification: Some("a nomad arrives, looking to trade"),
        buttons: &[
            Button {
                text: "buy scales",
                cost: &[Cost::Store(Resource::Fur, 100)],
                reward: &[(Resource::Scales, 1)],
                next: Next::Stay,
                ..Button::EMPTY
            },
            Button {
                text: "buy teeth",
                cost: &[Cost::Store(Resource::Fur, 200)],
                reward: &[(Resource::Teeth, 1)],
                next: Next::Stay,
                ..Button::EMPTY
            },
            Button {
                text: "buy bait",
                cost: &[Cost::Store(Resource::Fur, 5)],
                reward: &[(Resource::Bait, 1)],
                notification: Some("traps are more effective with bait."),
                next: Next::Stay,
                ..Button::EMPTY
            },
            Button {
                text: "buy compass",
                cost: &[
                    Cost::Store(Resource::Fur, 300),
                    Cost::Store(Resource::Scales, 15),
                    Cost::Store(Resource::Teeth, 5),
                ],
                available: &[Condition::StoreBelow(Resource::Compass, 1)],
                reward: &[(Resource::Compass, 1)],
                notification: Some("the old compass is dented and dusty, but it looks to work."),
                next: Next::Stay,
                ..Button::EMPTY
            },
            Button::leave("say goodbye"),
        ],
        ..Scene::EMPTY
    }],
};

static NOISES_OUTSIDE: Event = Event {
    key: "noises outside",
    title: "Noises",
    available: &[Condition::InRoom, Condition::HasStore(Resource::Wood)],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "through the walls, shuffling noises can be heard.",
                "can't tell what they're up to.",
            ],
            notification: Some("strange noises can be heard through the walls"),
            buttons: &[
                Button {
                    text: "investigate",
                    next: Next::Weighted(&[(0.3, "stuff"), (1.0, "nothing")]),
                    ..Button::EMPTY
                },
                Button::leave("ignore them"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "nothing",
            text: &["vague shapes move, just out of sight.", "the sounds stop."],
            buttons: &[Button::leave("go back inside")],
            ..Scene::EMPTY
        },
        Scene {
            key: "stuff",
            text: &[
                "a bundle of sticks lies just beyond the threshold, wrapped in coarse furs.",
                "the night is silent.",
            ],
            reward: &[(Resource::Wood, 100), (Resource::Fur, 10)],
            buttons: &[Button::leave("go back inside")],
            ..Scene::EMPTY
        },
    ],
};

static NOISES_INSIDE: Event = Event {
    key: "noises inside",
    title: "Noises",
    available: &[Condition::InRoom, Condition::HasStore(Resource::Wood)],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "scratching noises can be heard from the store room.",
                "something's in there.",
            ],
            notification: Some("something's in the store room"),
            buttons: &[
                Button {
                    text: "investigate",
                    next: Next::Weighted(&[(0.5, "scales"), (0.8, "teeth"), (1.0, "cloth")]),
                    ..Button::EMPTY
                },
                Button::leave("ignore them"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "scales",
            text: &[
                "some wood is missing.",
                "the ground is littered with small scales",
            ],
            on_load: &[Effect::TradeWoodFor(Resource::Scales)],
            buttons: &[Button::leave("leave")],
            ..Scene::EMPTY
        },
        Scene {
            key: "teeth",
            text: &[
                "some wood is missing.",
                "the ground is littered with small teeth",
            ],
            on_load: &[Effect::TradeWoodFor(Resource::Teeth)],
            buttons: &[Button::leave("leave")],
            ..Scene::EMPTY
        },
        Scene {
            key: "cloth",
            text: &[
                "some wood is missing.",
                "the ground is littered with scraps of cloth",
            ],
            on_load: &[Effect::TradeWoodFor(Resource::Cloth)],
            buttons: &[Button::leave("leave")],
            ..Scene::EMPTY
        },
    ],
};

static BEGGAR: Event = Event {
    key: "beggar",
    title: "The Beggar",
    available: &[Condition::InRoom, Condition::HasStore(Resource::Fur)],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "a beggar arrives.",
                "asks for any spare furs to keep him warm at night.",
            ],
            notification: Some("a beggar arrives"),
            buttons: &[
                Button {
                    text: "give 50",
                    cost: &[Cost::Store(Resource::Fur, 50)],
                    next: Next::Weighted(&[(0.5, "scales"), (0.8, "teeth"), (1.0, "cloth")]),
                    ..Button::EMPTY
                },
                Button {
                    text: "give 100",
                    cost: &[Cost::Store(Resource::Fur, 100)],
                    next: Next::Weighted(&[(0.5, "teeth"), (0.8, "scales"), (1.0, "cloth")]),
                    ..Button::EMPTY
                },
                Button::leave("turn him away"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "scales",
            text: &[
                "the beggar expresses his thanks.",
                "leaves a pile of small scales behind.",
            ],
            reward: &[(Resource::Scales, 20)],
            buttons: &[Button::leave("say goodbye")],
            ..Scene::EMPTY
        },
        Scene {
            key: "teeth",
            text: &[
                "the beggar expresses his thanks.",
                "leaves a pile of small teeth behind.",
            ],
            reward: &[(Resource::Teeth, 20)],
            buttons: &[Button::leave("say goodbye")],
            ..Scene::EMPTY
        },
        Scene {
            key: "cloth",
            text: &[
                "the beggar expresses his thanks.",
                "leaves some scraps of cloth behind.",
            ],
            reward: &[(Resource::Cloth, 20)],
            buttons: &[Button::leave("say goodbye")],
            ..Scene::EMPTY
        },
    ],
};

static SHADY_BUILDER: Event = Event {
    key: "shady builder",
    title: "The Shady Builder",
    available: &[
        Condition::InRoom,
        Condition::HutsAtLeast(5),
        Condition::HutsBelow(20),
    ],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "a shady builder passes through",
                "says he can build you a hut for less wood",
            ],
            notification: Some("a shady builder passes through"),
            buttons: &[
                Button {
                    text: "300 wood",
                    cost: &[Cost::Store(Resource::Wood, 300)],
                    next: Next::Weighted(&[(0.6, "steal"), (1.0, "build")]),
                    ..Button::EMPTY
                },
                Button::leave("say goodbye"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "steal",
            text: &["the shady builder has made off with your wood"],
            notification: Some("the shady builder has made off with your wood"),
            buttons: &[Button::leave("go home")],
            ..Scene::EMPTY
        },
        Scene {
            key: "build",
            text: &["the shady builder builds a hut"],
            notification: Some("the shady builder builds a hut"),
            on_load: &[Effect::BuildHut],
            buttons: &[Button::leave("go home")],
            ..Scene::EMPTY
        },
    ],
};

/// The wanderer's return is upstream's one delayed payoff: rolled when he
/// leaves, paid a minute later whether or not the player is still watching.
static WANDERER_WOOD: Event = Event {
    key: "wanderer wood",
    title: "The Mysterious Wanderer",
    available: &[Condition::InRoom, Condition::HasStore(Resource::Wood)],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "a wanderer arrives with an empty cart. says if he leaves with wood, he'll be back with more.",
                "builder's not sure he's to be trusted.",
            ],
            notification: Some("a mysterious wanderer arrives"),
            buttons: &[
                Button {
                    text: "give 100",
                    cost: &[Cost::Store(Resource::Wood, 100)],
                    next: Next::Scene("wood100"),
                    ..Button::EMPTY
                },
                Button {
                    text: "give 500",
                    cost: &[Cost::Store(Resource::Wood, 500)],
                    next: Next::Scene("wood500"),
                    ..Button::EMPTY
                },
                Button::leave("turn him away"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "wood100",
            text: &["the wanderer leaves, cart loaded with wood"],
            on_load: &[Effect::ScheduleReward {
                chance: 0.5,
                resource: Resource::Wood,
                amount: 300,
                delay_secs: 60,
                message: "the mysterious wanderer returns, cart piled high with wood.",
            }],
            buttons: &[Button::leave("say goodbye")],
            ..Scene::EMPTY
        },
        Scene {
            key: "wood500",
            text: &["the wanderer leaves, cart loaded with wood"],
            on_load: &[Effect::ScheduleReward {
                chance: 0.3,
                resource: Resource::Wood,
                amount: 1500,
                delay_secs: 60,
                message: "the mysterious wanderer returns, cart piled high with wood.",
            }],
            buttons: &[Button::leave("say goodbye")],
            ..Scene::EMPTY
        },
    ],
};

static WANDERER_FUR: Event = Event {
    key: "wanderer fur",
    title: "The Mysterious Wanderer",
    available: &[Condition::InRoom, Condition::HasStore(Resource::Fur)],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "a wanderer arrives with an empty cart. says if she leaves with furs, she'll be back with more.",
                "builder's not sure she's to be trusted.",
            ],
            notification: Some("a mysterious wanderer arrives"),
            buttons: &[
                Button {
                    text: "give 100",
                    cost: &[Cost::Store(Resource::Fur, 100)],
                    next: Next::Scene("fur100"),
                    ..Button::EMPTY
                },
                Button {
                    text: "give 500",
                    cost: &[Cost::Store(Resource::Fur, 500)],
                    next: Next::Scene("fur500"),
                    ..Button::EMPTY
                },
                Button::leave("turn her away"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "fur100",
            text: &["the wanderer leaves, cart loaded with furs"],
            on_load: &[Effect::ScheduleReward {
                chance: 0.5,
                resource: Resource::Fur,
                amount: 300,
                delay_secs: 60,
                message: "the mysterious wanderer returns, cart piled high with furs.",
            }],
            buttons: &[Button::leave("say goodbye")],
            ..Scene::EMPTY
        },
        Scene {
            key: "fur500",
            text: &["the wanderer leaves, cart loaded with furs"],
            on_load: &[Effect::ScheduleReward {
                chance: 0.3,
                resource: Resource::Fur,
                amount: 1500,
                delay_secs: 60,
                message: "the mysterious wanderer returns, cart piled high with furs.",
            }],
            buttons: &[Button::leave("say goodbye")],
            ..Scene::EMPTY
        },
    ],
};

static SCOUT: Event = Event {
    key: "scout",
    title: "The Scout",
    available: &[Condition::InRoom, Condition::WorldUnlocked],
    scenes: &[Scene {
        key: "start",
        text: &[
            "the scout says she's been all over.",
            "willing to talk about it, for a price.",
        ],
        notification: Some("a scout stops for the night"),
        buttons: &[
            Button {
                text: "buy map",
                cost: &[
                    Cost::Store(Resource::Fur, 200),
                    Cost::Store(Resource::Scales, 10),
                ],
                available: &[Condition::MapNotFull],
                notification: Some("the map uncovers a bit of the world"),
                effects: &[Effect::UncoverMap],
                next: Next::Stay,
                ..Button::EMPTY
            },
            Button {
                text: "learn scouting",
                cost: &[
                    Cost::Store(Resource::Fur, 1000),
                    Cost::Store(Resource::Scales, 50),
                    Cost::Store(Resource::Teeth, 20),
                ],
                available: &[Condition::LacksPerk(Perk::Scout)],
                effects: &[Effect::GrantPerk(Perk::Scout)],
                next: Next::Stay,
                ..Button::EMPTY
            },
            Button::leave("say goodbye"),
        ],
        ..Scene::EMPTY
    }],
};

static MASTER: Event = Event {
    key: "master",
    title: "The Master",
    available: &[Condition::InRoom, Condition::WorldUnlocked],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "an old wanderer arrives.",
                "he smiles warmly and asks for lodgings for the night.",
            ],
            notification: Some("an old wanderer arrives"),
            buttons: &[
                Button {
                    text: "agree",
                    cost: &[
                        Cost::Store(Resource::CuredMeat, 100),
                        Cost::Store(Resource::Fur, 100),
                        Cost::Store(Resource::Torch, 1),
                    ],
                    next: Next::Scene("agree"),
                    ..Button::EMPTY
                },
                Button::leave("turn him away"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "agree",
            text: &["in exchange, the wanderer offers his wisdom."],
            buttons: &[
                Button {
                    text: "evasion",
                    available: &[Condition::LacksPerk(Perk::Evasive)],
                    effects: &[Effect::GrantPerk(Perk::Evasive)],
                    ..Button::EMPTY
                },
                Button {
                    text: "precision",
                    available: &[Condition::LacksPerk(Perk::Precise)],
                    effects: &[Effect::GrantPerk(Perk::Precise)],
                    ..Button::EMPTY
                },
                Button {
                    text: "force",
                    available: &[Condition::LacksPerk(Perk::Barbarian)],
                    effects: &[Effect::GrantPerk(Perk::Barbarian)],
                    ..Button::EMPTY
                },
                Button::leave("nothing"),
            ],
            ..Scene::EMPTY
        },
    ],
};

static SICK_MAN: Event = Event {
    key: "sick man",
    title: "The Sick Man",
    available: &[Condition::InRoom, Condition::HasStore(Resource::Medicine)],
    scenes: &[
        Scene {
            key: "start",
            text: &["a man hobbles up, coughing.", "he begs for medicine."],
            notification: Some("a sick man hobbles up"),
            buttons: &[
                Button {
                    text: "give 1 medicine",
                    cost: &[Cost::Store(Resource::Medicine, 1)],
                    notification: Some("the man swallows the medicine eagerly"),
                    next: Next::Weighted(&[
                        (0.1, "alloy"),
                        (0.3, "cells"),
                        (0.5, "scales"),
                        (1.0, "nothing"),
                    ]),
                    ..Button::EMPTY
                },
                Button::leave("tell him to leave"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "alloy",
            text: &[
                "the man is thankful.",
                "he leaves a reward.",
                "some weird metal he picked up on his travels.",
            ],
            reward: &[(Resource::AlienAlloy, 1)],
            buttons: &[Button::leave("say goodbye")],
            ..Scene::EMPTY
        },
        Scene {
            key: "cells",
            text: &[
                "the man is thankful.",
                "he leaves a reward.",
                "some weird glowing boxes he picked up on his travels.",
            ],
            reward: &[(Resource::EnergyCell, 3)],
            buttons: &[Button::leave("say goodbye")],
            ..Scene::EMPTY
        },
        Scene {
            key: "scales",
            text: &[
                "the man is thankful.",
                "he leaves a reward.",
                "all he has are some scales.",
            ],
            reward: &[(Resource::Scales, 5)],
            buttons: &[Button::leave("say goodbye")],
            ..Scene::EMPTY
        },
        Scene {
            key: "nothing",
            text: &["the man expresses his thanks and hobbles off."],
            buttons: &[Button::leave("say goodbye")],
            ..Scene::EMPTY
        },
    ],
};

// ---------------------------------------------------------------------------
// Outside
// ---------------------------------------------------------------------------

static RUINED_TRAP: Event = Event {
    key: "ruined trap",
    title: "A Ruined Trap",
    available: &[Condition::InOutside, Condition::HasTrap],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "some of the traps have been torn apart.",
                "large prints lead away, into the forest.",
            ],
            notification: Some("some traps have been destroyed"),
            on_load: &[Effect::WreckTraps],
            buttons: &[
                Button {
                    text: "track them",
                    next: Next::Weighted(&[(0.5, "nothing"), (1.0, "catch")]),
                    ..Button::EMPTY
                },
                Button::leave("ignore them"),
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "nothing",
            text: &[
                "the tracks disappear after just a few minutes.",
                "the forest is silent.",
            ],
            notification: Some("nothing was found"),
            buttons: &[Button::leave("go home")],
            ..Scene::EMPTY
        },
        Scene {
            key: "catch",
            text: &[
                "not far from the village lies a large beast, its fur matted with blood.",
                "it puts up little resistance before the knife.",
            ],
            notification: Some("there was a beast. it's dead now"),
            reward: &[
                (Resource::Fur, 100),
                (Resource::Meat, 100),
                (Resource::Teeth, 10),
            ],
            buttons: &[Button::leave("go home")],
            ..Scene::EMPTY
        },
    ],
};

static HUT_FIRE: Event = Event {
    key: "hut fire",
    title: "Fire",
    available: &[
        Condition::InOutside,
        Condition::HutsAtLeast(1),
        Condition::PopulationOver(50),
    ],
    scenes: &[Scene {
        key: "start",
        text: &[
            "a fire rampages through one of the huts, destroying it.",
            "all residents in the hut perished in the fire.",
        ],
        notification: Some("a fire has started"),
        on_load: &[Effect::DestroyHuts(1)],
        buttons: &[Button {
            text: "mourn",
            notification: Some("some villagers have died"),
            ..Button::EMPTY
        }],
        ..Scene::EMPTY
    }],
};

static SICKNESS: Event = Event {
    key: "sickness",
    title: "Sickness",
    available: &[
        Condition::InOutside,
        Condition::PopulationOver(10),
        Condition::PopulationBelow(50),
        Condition::HasStore(Resource::Medicine),
    ],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "a sickness is spreading through the village.",
                "medicine is needed immediately.",
            ],
            notification: Some("some villagers are ill"),
            buttons: &[
                Button {
                    text: "1 medicine",
                    cost: &[Cost::Store(Resource::Medicine, 1)],
                    next: Next::Scene("healed"),
                    ..Button::EMPTY
                },
                Button {
                    text: "ignore it",
                    next: Next::Scene("death"),
                    ..Button::EMPTY
                },
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "healed",
            text: &["the sickness is cured in time."],
            notification: Some("sufferers are healed"),
            buttons: &[Button::leave("go home")],
            ..Scene::EMPTY
        },
        Scene {
            key: "death",
            text: &[
                "the sickness spreads through the village.",
                "the days are spent with burials.",
                "the nights are rent with screams.",
            ],
            notification: Some("sufferers are left to die"),
            on_load: &[Effect::KillVillagers(Kill::HalfPopulation)],
            buttons: &[Button::leave("go home")],
            ..Scene::EMPTY
        },
    ],
};

static PLAGUE: Event = Event {
    key: "plague",
    title: "Plague",
    available: &[
        Condition::InOutside,
        Condition::PopulationOver(50),
        Condition::HasStore(Resource::Medicine),
    ],
    scenes: &[
        Scene {
            key: "start",
            text: &[
                "a terrible plague is fast spreading through the village.",
                "medicine is needed immediately.",
            ],
            notification: Some("a plague afflicts the village"),
            buttons: &[
                // The need is desperate, so the price is not kind.
                Button {
                    text: "buy medicine",
                    cost: &[
                        Cost::Store(Resource::Scales, 70),
                        Cost::Store(Resource::Teeth, 50),
                    ],
                    reward: &[(Resource::Medicine, 1)],
                    next: Next::Stay,
                    ..Button::EMPTY
                },
                Button {
                    text: "5 medicine",
                    cost: &[Cost::Store(Resource::Medicine, 5)],
                    next: Next::Scene("healed"),
                    ..Button::EMPTY
                },
                Button {
                    text: "do nothing",
                    next: Next::Scene("death"),
                    ..Button::EMPTY
                },
            ],
            ..Scene::EMPTY
        },
        Scene {
            key: "healed",
            text: &[
                "the plague is kept from spreading.",
                "only a few die.",
                "the rest bury them.",
            ],
            notification: Some("epidemic is eradicated eventually"),
            on_load: &[Effect::KillVillagers(Kill::Range(2, 7))],
            buttons: &[Button::leave("go home")],
            ..Scene::EMPTY
        },
        Scene {
            key: "death",
            text: &[
                "the plague rips through the village.",
                "the nights are rent with screams.",
                "the only hope is a quick death.",
            ],
            notification: Some("population is almost exterminated"),
            on_load: &[Effect::KillVillagers(Kill::Range(10, 90))],
            buttons: &[Button::leave("go home")],
            ..Scene::EMPTY
        },
    ],
};

static BEAST_ATTACK: Event = Event {
    key: "beast attack",
    title: "A Beast Attack",
    available: &[Condition::InOutside, Condition::PopulationOver(0)],
    scenes: &[Scene {
        key: "start",
        text: &[
            "a pack of snarling beasts pours out of the trees.",
            "the fight is short and bloody, but the beasts are repelled.",
            "the villagers retreat to mourn the dead.",
        ],
        notification: Some("wild beasts attack the villagers"),
        on_load: &[Effect::KillVillagers(Kill::Range(1, 11))],
        reward: &[
            (Resource::Fur, 100),
            (Resource::Meat, 100),
            (Resource::Teeth, 10),
        ],
        buttons: &[Button {
            text: "go home",
            notification: Some("predators become prey. price is unfair"),
            ..Button::EMPTY
        }],
        ..Scene::EMPTY
    }],
};
