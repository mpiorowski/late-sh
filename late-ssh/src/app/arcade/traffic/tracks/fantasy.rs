//! The Realm — a fantasy journey through magical lands.
//!
//! Cobblestone villages → enchanted forests → crystal caves → dragon's lava pass
//! → a bridge over pure void → ancient ruins → the Dark Realm → elven highway
//! → the obsidian Citadel → Realm's End.  ~78 km, ~14 min.

use super::presets::*;
use crate::app::arcade::traffic::theme;
use crate::app::arcade::traffic::track::{
    Lane, Lanes, Obstacle, ObstacleEffect, Road, RoadAspect, Sceneries, Scenery, Shoulders, Stage,
    Theme, Track,
};

// ─── Lane variants ────────────────────────────────────────────────────────────

const VILLAGE_LANE: Lane = Lane {
    own_max_speed: 100.0,
    traffic_density: 0.35,
    traffic_cars: FANTASY_MIX,
    obstacles: &[Obstacle {
        style: theme::OBSTACLE_SPEED_BUMP,
        frequency: 0.022,
        effects: &[ObstacleEffect::SpeedChange { affect: -0.65 }],
    }],
    ..COBBLE_LANE
};

const ENCHANTED_LANE: Lane = Lane {
    own_max_speed: 140.0,
    passive_decel: 2.0,
    traffic_density: 0.1,
    traffic_cars: FANTASY_MIX,
    obstacles: &[Obstacle {
        style: theme::OBSTACLE_POTHOLE_SMALL,
        frequency: 0.022,
        effects: &[ObstacleEffect::SpeedChange { affect: -0.25 }],
    }],
    ..FOREST_LANE
};

const CRYSTAL_CAVE_LANE: Lane = Lane {
    own_max_speed: 120.0,
    traffic_density: 0.1,
    traffic_cars: FANTASY_MIX,
    obstacles: &[Obstacle {
        style: theme::OBSTACLE_MAGIC_TRAP,
        frequency: 0.024,
        effects: &[
            ObstacleEffect::BlockWheels { cooldown_ms: 450 },
            ObstacleEffect::SpeedChange { affect: -0.45 },
        ],
    }],
    ..CRYSTAL_LANE
};

const DRAGONS_PASS_LANE: Lane = Lane {
    own_max_speed: 100.0,
    traffic_density: 0.1,
    traffic_cars: FANTASY_MIX,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_DRAGON_FIRE,
            frequency: 0.035,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.72 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_CRATER,
            frequency: 0.012,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.85 }],
        },
    ],
    ..MOUNTAIN_LANE
};

const RUINS_LANE: Lane = Lane {
    own_max_speed: 90.0,
    traffic_density: 0.1,
    traffic_cars: FANTASY_MIX,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_FALLEN_TREE,
            frequency: 0.032,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.80 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_CRATER,
            frequency: 0.014,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.70 }],
        },
    ],
    ..MOUNTAIN_LANE
};

const DARK_REALM_LANE: Lane = Lane {
    own_max_speed: 110.0,
    traffic_density: 0.12,
    traffic_cars: FANTASY_MIX,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_MAGIC_TRAP,
            frequency: 0.042,
            effects: &[
                ObstacleEffect::BlockWheels { cooldown_ms: 600 },
                ObstacleEffect::SpeedChange { affect: -0.55 },
            ],
        },
        Obstacle {
            style: theme::OBSTACLE_SPIKES,
            frequency: 0.014,
            effects: &[ObstacleEffect::Crash],
        },
    ],
    ..CRYSTAL_LANE
};

const ELVEN_LANE: Lane = Lane {
    own_max_speed: 200.0,
    traffic_density: 0.10,
    traffic_cars: FANTASY_MIX,
    ..HIGHWAY_LANE
};

const CITADEL_LANE: Lane = Lane {
    own_max_speed: 80.0,
    traffic_density: 0.4,
    traffic_cars: FANTASY_MIX,
    ..COBBLE_LANE
};

const REALM_END_LANE: Lane = Lane {
    own_max_speed: 180.0,
    traffic_density: 0.1,
    traffic_cars: FANTASY_MIX,
    obstacles: &[Obstacle {
        style: theme::OBSTACLE_MAGIC_TRAP,
        frequency: 0.018,
        effects: &[
            ObstacleEffect::BlockWheels { cooldown_ms: 300 },
            ObstacleEffect::SpeedChange { affect: -0.30 },
        ],
    }],
    ..CRYSTAL_LANE
};

// ─── Local sceneries ─────────────────────────────────────────────────────────

const VOID_SIDES: Scenery = Scenery {
    width: 6,
    background: theme::SCENERY_VOID,
    objects: &[],
};

// ─── Stages ──────────────────────────────────────────────────────────────────

const S01_BRAMBLEWOOD: Stage = Stage {
    name: "Village of Bramblewood",
    description: "Cobblestone streets, lantern light, smoke from chimney-tops. A cheerful start to an extraordinary journey.",
    icon: theme::STAGE_VILLAGE,
    theme: Theme::Standard,
    distance_km: 8.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[VILLAGE_LANE, VILLAGE_LANE],
            incoming: &[VILLAGE_LANE],
        },
        sceneries: Sceneries {
            left: EURO_VILLAGE_SCENERY,
            right: EURO_VILLAGE_SCENERY,
        },
        shoulders: Shoulders {
            left: MAGIC_RUNE_SHOULDERS,
            right: MAGIC_RUNE_SHOULDERS,
        },
    },
};

const S02_ENCHANTED_FOREST: Stage = Stage {
    name: "Enchanted Forest",
    description: "Bioluminescent mushrooms line the path. Giant oaks whisper ancient secrets. Something watches from the undergrowth.",
    icon: theme::STAGE_WILD_FOREST,
    theme: Theme::Standard,
    distance_km: 12.0,
    road: Road {
        aspect: RoadAspect {
            dividers: FOREST_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[ENCHANTED_LANE],
            incoming: &[ENCHANTED_LANE],
        },
        sceneries: Sceneries {
            left: MAGIC_FOREST_SCENERY,
            right: MAGIC_FOREST_SCENERY,
        },
        shoulders: Shoulders {
            left: MAGIC_RUNE_SHOULDERS,
            right: MAGIC_RUNE_SHOULDERS,
        },
    },
};

const S03_CRYSTAL_CAVES: Stage = Stage {
    name: "Glimmering Caves",
    description: "Veins of pure crystal pulse with inner light. The road cuts through the mountain's heart. Magic traps shimmer ahead — don't be lured.",
    icon: theme::STAGE_TUNNEL,
    theme: Theme::Winter,
    distance_km: 6.0,
    road: Road {
        aspect: RoadAspect {
            dividers: FOREST_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[CRYSTAL_CAVE_LANE],
            incoming: &[CRYSTAL_CAVE_LANE],
        },
        sceneries: Sceneries {
            left: CRYSTAL_CAVE_SCENERY,
            right: CRYSTAL_CAVE_SCENERY,
        },
        shoulders: Shoulders {
            left: MAGIC_RUNE_SHOULDERS,
            right: MAGIC_RUNE_SHOULDERS,
        },
    },
};

const S04_DRAGONS_PASS: Stage = Stage {
    name: "Dragon's Pass",
    description: "Lava rivers glow on both sides. A great dragon banks overhead and breathes fire across the road. Keep moving.",
    icon: theme::STAGE_DRAGON,
    theme: Theme::Desert,
    distance_km: 10.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[DRAGONS_PASS_LANE],
            incoming: &[DRAGONS_PASS_LANE],
        },
        sceneries: Sceneries {
            left: LAVA_SCENERY,
            right: LAVA_SCENERY,
        },
        shoulders: Shoulders {
            left: GUARDRAIL_SHOULDERS,
            right: GUARDRAIL_SHOULDERS,
        },
    },
};

const S05_FLOATING_BRIDGE: Stage = Stage {
    name: "The Floating Bridge",
    description: "Crystal causeway suspended above pure void. No railings. No ground below. Just the road and the abyss.",
    icon: theme::STAGE_BRIDGE,
    theme: Theme::Standard,
    distance_km: 5.0,
    road: Road {
        aspect: RoadAspect {
            dividers: FOREST_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[CRYSTAL_CAVE_LANE],
            incoming: &[CRYSTAL_CAVE_LANE],
        },
        sceneries: Sceneries {
            left: VOID_SIDES,
            right: VOID_SIDES,
        },
        shoulders: NO_SHOULDERS,
    },
};

const S06_ANCIENT_RUINS: Stage = Stage {
    name: "Ancient Ruins",
    description: "Collapsed pillars block the road. Roots burst through stone. Something old and forgotten stirs beneath the rubble.",
    icon: theme::STAGE_WILD_HILLS,
    theme: Theme::Standard,
    distance_km: 8.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[RUINS_LANE],
            incoming: &[RUINS_LANE],
        },
        sceneries: Sceneries {
            left: DARK_REALM_SCENERY,
            right: MAGIC_FOREST_SCENERY,
        },
        shoulders: Shoulders {
            left: GUARDRAIL_SHOULDERS,
            right: MAGIC_RUNE_SHOULDERS,
        },
    },
};

const S07_DARK_REALM: Stage = Stage {
    name: "The Dark Realm",
    description: "Crystal paths warp through a space where light bends wrong. Magic traps pulse like heartbeats. Spiked barriers wait for the unwary.",
    icon: theme::STAGE_SPECIAL,
    theme: Theme::Standard,
    distance_km: 8.0,
    road: Road {
        aspect: RoadAspect {
            dividers: FOREST_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[DARK_REALM_LANE, DARK_REALM_LANE],
            incoming: &[DARK_REALM_LANE],
        },
        sceneries: Sceneries {
            left: DARK_REALM_SCENERY,
            right: DARK_REALM_SCENERY,
        },
        shoulders: Shoulders {
            left: MAGIC_RUNE_SHOULDERS,
            right: MAGIC_RUNE_SHOULDERS,
        },
    },
};

const S08_ELVEN_CAUSEWAY: Stage = Stage {
    name: "Elven Causeway",
    description: "Impossibly smooth magic-woven road through an ancient enchanted forest. Elven runes carved every hundred metres accelerate you onward.",
    icon: theme::STAGE_HIGHWAY,
    theme: Theme::Standard,
    distance_km: 10.0,
    road: Road {
        aspect: RoadAspect {
            dividers: HIGHWAY_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[ELVEN_LANE, ELVEN_LANE],
            incoming: &[ELVEN_LANE, ELVEN_LANE],
        },
        sceneries: Sceneries {
            left: MAGIC_FOREST_SCENERY,
            right: MAGIC_FOREST_SCENERY,
        },
        shoulders: Shoulders {
            left: MAGIC_RUNE_SHOULDERS,
            right: MAGIC_RUNE_SHOULDERS,
        },
    },
};

const S09_THE_CITADEL: Stage = Stage {
    name: "The Citadel",
    description: "Towers of obsidian rise above cobblestone avenues. Dense crowds, dark spires, and the smell of forgotten wars.",
    icon: theme::STAGE_CASTLE,
    theme: Theme::Standard,
    distance_km: 6.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[CITADEL_LANE, CITADEL_LANE],
            incoming: &[CITADEL_LANE],
        },
        sceneries: Sceneries {
            left: DARK_REALM_SCENERY,
            right: DARK_REALM_SCENERY,
        },
        shoulders: Shoulders {
            left: SIDEWALK_SHOULDERS,
            right: SIDEWALK_SHOULDERS,
        },
    },
};

const S10_REALMS_END: Stage = Stage {
    name: "Realm's End",
    description: "The road dissolves into crystal light. Magic traps — or blessings? — flicker across the final stretch. Beyond this point lies only legend.",
    icon: theme::STAGE_SPECIAL,
    theme: Theme::Winter,
    distance_km: 5.0,
    road: Road {
        aspect: RoadAspect {
            dividers: FOREST_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[REALM_END_LANE, REALM_END_LANE],
            incoming: &[REALM_END_LANE],
        },
        sceneries: Sceneries {
            left: CRYSTAL_CAVE_SCENERY,
            right: MAGIC_FOREST_SCENERY,
        },
        shoulders: Shoulders {
            left: MAGIC_RUNE_SHOULDERS,
            right: MAGIC_RUNE_SHOULDERS,
        },
    },
};

// ─── Track ───────────────────────────────────────────────────────────────────

pub const TRACK: Track = Track {
    name: "The Realm",
    author: "claude",
    description: "A fantasy journey through enchanted lands: cobblestone villages, luminous crystal caves, a dragon-guarded lava pass, a bridge over pure void, ancient ruins, the terrifying Dark Realm, the legendary Elven Causeway, and the obsidian Citadel at the edge of all things.",
    stages: &[
        S01_BRAMBLEWOOD,
        S02_ENCHANTED_FOREST,
        S03_CRYSTAL_CAVES,
        S04_DRAGONS_PASS,
        S05_FLOATING_BRIDGE,
        S06_ANCIENT_RUINS,
        S07_DARK_REALM,
        S08_ELVEN_CAUSEWAY,
        S09_THE_CITADEL,
        S10_REALMS_END,
    ],
    distance_scale: 0.18,
    speed_scale: 2.0,
    lives: 3,
};
