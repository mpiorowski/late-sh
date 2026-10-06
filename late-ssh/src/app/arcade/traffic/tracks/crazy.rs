//! Chaos Highway — the track with no rules.
//!
//! Stages: 5+4 lane Tokyo mega-interchange → 600 km/h neon turbo strip →
//! monster truck alley → gridlock hell → the obstacle garden →
//! micro car cobblestone city → 6+5 lane impossible highway →
//! 1000 km/h final sprint.  ~69 km, ~9 min.

use super::presets::{
    CAR_HATCHBACK, CAR_MICRO, CAR_MONSTER, CAR_RV, CAR_SEMI, CAR_TRUCK, CITY_LANE, CITY_SCENERY,
    COBBLE_LANE, CRASH_BARRIER_SHOULDERS, CRAZY_MIX, EURO_VILLAGE_SCENERY, GRIDLOCK_LANE,
    GUARDRAIL_SHOULDERS, HIGHWAY_DIVIDERS, HIGHWAY_LANE, HIGHWAY_SHOULDERS, NEON_BARRIER_SHOULDERS,
    PLAINS_LANE, PLAINS_SCENERY, RURAL_DIVIDERS, RURAL_SCENERY, SIDEWALK_SHOULDERS,
    STARFIELD_SCENERY, TOWN_SCENERY, TURBO_LANE, URBAN_DIVIDERS,
};
use crate::app::arcade::traffic::theme;
use crate::app::arcade::traffic::track::{
    Car, Lane, Lanes, Obstacle, ObstacleEffect, Road, RoadAspect, Sceneries, Scenery, Shoulders,
    Stage, Theme, Track,
};

// ─── Local car mixes ─────────────────────────────────────────────────────────

const MONSTER_MIX: &[Car] = &[CAR_SEMI, CAR_TRUCK, CAR_RV, CAR_MONSTER];
const MICRO_MIX: &[Car] = &[CAR_MICRO, CAR_HATCHBACK];

// ─── Lane variants ────────────────────────────────────────────────────────────

// Tokyo: dense city at speed
const TOKYO_OUT: Lane = Lane {
    own_max_speed: 110.0,
    traffic_density: 0.4,
    traffic_cars: CRAZY_MIX,
    ..CITY_LANE
};
const TOKYO_IN: Lane = Lane {
    own_max_speed: 110.0,
    traffic_density: 0.35,
    traffic_cars: CRAZY_MIX,
    ..CITY_LANE
};

// Gridlock: legally a road, practically a car park
const GRIDLOCK: Lane = Lane {
    traffic_density: 0.8,
    ..GRIDLOCK_LANE
};

// Monster Truck Alley: wide lanes, everything oversized
const MONSTER_LANE: Lane = Lane {
    own_max_speed: 130.0,
    traffic_density: 0.35,
    traffic_cars: MONSTER_MIX,
    ..HIGHWAY_LANE
};

// Obstacle Garden: every hazard type at maximum density
const OBSTACLE_GARDEN_LANE: Lane = Lane {
    own_max_speed: 130.0,
    traffic_density: 0.22,
    traffic_cars: CRAZY_MIX,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_BIG,
            frequency: 0.042,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.40 }],
        },
        Obstacle {
            style: theme::OBSTACLE_SPEED_BUMP,
            frequency: 0.032,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.65 }],
        },
        Obstacle {
            style: theme::OBSTACLE_OIL_SPILL,
            frequency: 0.028,
            effects: &[
                ObstacleEffect::BlockWheels { cooldown_ms: 400 },
                ObstacleEffect::SpeedChange { affect: -0.20 },
            ],
        },
        Obstacle {
            style: theme::OBSTACLE_FALLEN_TREE,
            frequency: 0.014,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.90 }],
        },
        Obstacle {
            style: theme::OBSTACLE_SPIKES,
            frequency: 0.010,
            effects: &[ObstacleEffect::Crash],
        },
    ],
    ..PLAINS_LANE
};

// Micro Lane: cobblestones, bumper cars edition
const MICRO_LANE: Lane = Lane {
    own_max_speed: 32.0,
    traffic_density: 0.5,
    traffic_cars: MICRO_MIX,
    ..COBBLE_LANE
};

// Impossible Highway: turbo + obstacles + chaos traffic
const IMPOSSIBLE_LANE: Lane = Lane {
    own_max_speed: 500.0,
    traffic_density: 0.30,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_SPEED_BUMP,
            frequency: 0.022,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.60 }],
        },
        Obstacle {
            style: theme::OBSTACLE_OIL_SPILL,
            frequency: 0.016,
            effects: &[
                ObstacleEffect::BlockWheels { cooldown_ms: 300 },
                ObstacleEffect::SpeedChange { affect: -0.15 },
            ],
        },
    ],
    ..TURBO_LANE
};

// Final Sprint: 1000 km/h. Empty road. Go.
const SPRINT_LANE: Lane = Lane {
    own_max_speed: 1000.0,
    traffic_density: 0.01,
    obstacles: &[],
    ..TURBO_LANE
};

// ─── Local sceneries ─────────────────────────────────────────────────────────

const NEON_VOID: Scenery = Scenery {
    width: 6,
    background: theme::SCENERY_VOID,
    objects: &[],
};

// ─── Stages ──────────────────────────────────────────────────────────────────

const S01_TOKYO: Stage = Stage {
    name: "Tokyo Interchange",
    description: "Nine lanes. Everyone going everywhere at once. The world's most complex road junction in rush hour. Somehow still moving.",
    icon: theme::STAGE_METROPOLIS,
    theme: Theme::Standard,
    distance_km: 10.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[TOKYO_OUT, TOKYO_OUT, TOKYO_OUT, TOKYO_OUT, TOKYO_OUT],
            incoming: &[TOKYO_IN, TOKYO_IN, TOKYO_IN, TOKYO_IN],
        },
        sceneries: Sceneries {
            left: CITY_SCENERY,
            right: CITY_SCENERY,
        },
        shoulders: Shoulders {
            left: SIDEWALK_SHOULDERS,
            right: SIDEWALK_SHOULDERS,
        },
    },
};

const S02_TURBO_STRIP: Stage = Stage {
    name: "Turbo Strip",
    description: "Private raceway. No limit. No traffic. Just asphalt, neon barriers, and 600 km/h of pure velocity. This is what cars are for.",
    icon: theme::STAGE_HIGHWAY,
    theme: Theme::Standard,
    distance_km: 15.0,
    road: Road {
        aspect: RoadAspect {
            dividers: HIGHWAY_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[TURBO_LANE, TURBO_LANE, TURBO_LANE],
            incoming: &[TURBO_LANE, TURBO_LANE],
        },
        sceneries: Sceneries {
            left: NEON_VOID,
            right: NEON_VOID,
        },
        shoulders: Shoulders {
            left: NEON_BARRIER_SHOULDERS,
            right: NEON_BARRIER_SHOULDERS,
        },
    },
};

const S03_MONSTER_ALLEY: Stage = Stage {
    name: "Monster Truck Alley",
    description: "Every vehicle is enormous. Semis, monster trucks, and 18-wheelers wall to wall. Weave between rolling cliffs of steel at moderate speed.",
    icon: theme::STAGE_WILD_PLAINS,
    theme: Theme::Standard,
    distance_km: 8.0,
    road: Road {
        aspect: RoadAspect {
            dividers: HIGHWAY_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[MONSTER_LANE, MONSTER_LANE],
            incoming: &[MONSTER_LANE, MONSTER_LANE],
        },
        sceneries: Sceneries {
            left: PLAINS_SCENERY,
            right: PLAINS_SCENERY,
        },
        shoulders: Shoulders {
            left: HIGHWAY_SHOULDERS,
            right: HIGHWAY_SHOULDERS,
        },
    },
};

const S04_GRIDLOCK: Stage = Stage {
    name: "Gridlock City",
    description: "Monday morning. Every road. Zero movement. Maximum density. Welcome to the commute — everybody stop, nobody go, all horns optional.",
    icon: theme::STAGE_CHAOS,
    theme: Theme::Standard,
    distance_km: 3.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[GRIDLOCK],
            incoming: &[GRIDLOCK],
        },
        sceneries: Sceneries {
            left: TOWN_SCENERY,
            right: TOWN_SCENERY,
        },
        shoulders: Shoulders {
            left: SIDEWALK_SHOULDERS,
            right: SIDEWALK_SHOULDERS,
        },
    },
};

const S05_OBSTACLE_GARDEN: Stage = Stage {
    name: "The Obstacle Garden",
    description: "Road designers were fired after this. Potholes, speed bumps, oil slicks, fallen trees, and spike strips — all on the same road, at high density.",
    icon: theme::STAGE_WILD_HILLS,
    theme: Theme::Standard,
    distance_km: 8.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[OBSTACLE_GARDEN_LANE, OBSTACLE_GARDEN_LANE],
            incoming: &[OBSTACLE_GARDEN_LANE, OBSTACLE_GARDEN_LANE],
        },
        sceneries: Sceneries {
            left: RURAL_SCENERY,
            right: RURAL_SCENERY,
        },
        shoulders: Shoulders {
            left: GUARDRAIL_SHOULDERS,
            right: GUARDRAIL_SHOULDERS,
        },
    },
};

const S06_MICRO_CITY: Stage = Stage {
    name: "Micro City",
    description: "Tiny cars. Maximum density. Cobblestones. 30 km/h speed limit strictly enforced by physics. Like bumper cars, but everyone is actually trying.",
    icon: theme::STAGE_VILLAGE,
    theme: Theme::Standard,
    distance_km: 5.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[MICRO_LANE],
            incoming: &[MICRO_LANE],
        },
        sceneries: Sceneries {
            left: EURO_VILLAGE_SCENERY,
            right: EURO_VILLAGE_SCENERY,
        },
        shoulders: Shoulders {
            left: SIDEWALK_SHOULDERS,
            right: SIDEWALK_SHOULDERS,
        },
    },
};

const S07_IMPOSSIBLE_HIGHWAY: Stage = Stage {
    name: "Impossible Highway",
    description: "Eleven lanes. Mixed traffic from microcars to monster trucks. 500 km/h limit. Speed bumps and oil slicks for no reason. Perfectly normal road.",
    icon: theme::STAGE_HIGHWAY,
    theme: Theme::Standard,
    distance_km: 12.0,
    road: Road {
        aspect: RoadAspect {
            dividers: HIGHWAY_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[
                IMPOSSIBLE_LANE,
                IMPOSSIBLE_LANE,
                IMPOSSIBLE_LANE,
                IMPOSSIBLE_LANE,
                IMPOSSIBLE_LANE,
                IMPOSSIBLE_LANE,
            ],
            incoming: &[
                IMPOSSIBLE_LANE,
                IMPOSSIBLE_LANE,
                IMPOSSIBLE_LANE,
                IMPOSSIBLE_LANE,
                IMPOSSIBLE_LANE,
            ],
        },
        sceneries: Sceneries {
            left: STARFIELD_SCENERY,
            right: STARFIELD_SCENERY,
        },
        shoulders: Shoulders {
            left: NEON_BARRIER_SHOULDERS,
            right: NEON_BARRIER_SHOULDERS,
        },
    },
};

const S08_FINAL_SPRINT: Stage = Stage {
    name: "Final Sprint",
    description: "One lane. No traffic. No obstacles. One thousand kilometres per hour. Eight kilometres. Seven seconds. Go.",
    icon: theme::STAGE_SPECIAL,
    theme: Theme::Standard,
    distance_km: 8.0,
    road: Road {
        aspect: RoadAspect {
            dividers: HIGHWAY_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[SPRINT_LANE],
            incoming: &[SPRINT_LANE],
        },
        sceneries: Sceneries {
            left: NEON_VOID,
            right: NEON_VOID,
        },
        shoulders: Shoulders {
            left: CRASH_BARRIER_SHOULDERS,
            right: CRASH_BARRIER_SHOULDERS,
        },
    },
};

// ─── Track ───────────────────────────────────────────────────────────────────

pub const TRACK: Track = Track {
    name: "Chaos Highway",
    author: "claude",
    description: "A road with no rules and every extreme: 9-lane Tokyo interchange, 600 km/h neon strip, wall-to-wall monster trucks, rush-hour gridlock, the Obstacle Garden, micro-car bumper city, 11-lane impossible highway, and a 1000 km/h final sprint.",
    stages: &[
        S01_TOKYO,
        S02_TURBO_STRIP,
        S03_MONSTER_ALLEY,
        S04_GRIDLOCK,
        S05_OBSTACLE_GARDEN,
        S06_MICRO_CITY,
        S07_IMPOSSIBLE_HIGHWAY,
        S08_FINAL_SPRINT,
    ],
    distance_scale: 0.32,
    speed_scale: 3.0,
    lives: 5,
};
