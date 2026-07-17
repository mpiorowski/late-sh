//! Cosmic Highway — a road through the Solar System.
//!
//! From a terrestrial launch pad into low orbit, past the Moon and across
//! the red dust of Mars, through the lethal Asteroid Belt, around Jupiter,
//! through Saturn's rings, and into the lonely deep of space to Pluto's edge.
//! ~85 km displayed, ~10 min.

use super::presets::*;
use crate::app::arcade::traffic::theme;
use crate::app::arcade::traffic::track::{
    Lane, Lanes, Object, Obstacle, ObstacleEffect, Road, RoadAspect, Sceneries, Scenery, Shoulders,
    Stage, Theme, Track,
};

// ─── Lane variants ────────────────────────────────────────────────────────────

// Climbing into orbit — faster than a highway, traffic thinning fast
const ORBIT_LANE: Lane = Lane {
    own_max_speed: 240.0,
    traffic_density: 0.1,
    traffic_cars: SPACE_SHIP_MIX,
    ..SPACE_LANE
};

// Lunar surface — rough, slow, craters everywhere
const MOON_LANE: Lane = Lane {
    style: theme::LANE_GRAVEL,
    own_max_speed: 140.0,
    passive_decel: 2.5,
    traffic_density: 0.1,
    traffic_cars: SPACE_SHIP_MIX,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_CRATER,
            frequency: 0.045,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.80 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_BIG,
            frequency: 0.025,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.50 }],
        },
    ],
    ..MOUNTAIN_LANE
};

// Mars — cracked dust road, violent sand storms
const MARS_LANE: Lane = Lane {
    own_max_speed: 160.0,
    passive_decel: 1.0,
    traffic_density: 0.1,
    traffic_cars: SPACE_SHIP_MIX,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_SAND_DRIFT,
            frequency: 0.048,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.55 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_CRATER,
            frequency: 0.018,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.78 }],
        },
    ],
    ..DESERT_LANE
};

// Asteroid Belt — dodge or die
const ASTEROID_BELT_LANE: Lane = Lane {
    own_max_speed: 160.0,
    traffic_density: 0.1,
    traffic_cars: SPACE_SHIP_MIX,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_ASTEROID_CHUNK,
            frequency: 0.058,
            effects: &[
                ObstacleEffect::BlockWheels { cooldown_ms: 400 },
                ObstacleEffect::SpeedChange { affect: -0.62 },
            ],
        },
        Obstacle {
            style: theme::OBSTACLE_METEOR,
            frequency: 0.014,
            effects: &[ObstacleEffect::Crash],
        },
    ],
    ..SPACE_LANE
};

// Jupiter Approach — gravity assist, fast, light turbulence
const JUPITER_LANE: Lane = Lane {
    own_max_speed: 320.0,
    traffic_density: 0.1,
    traffic_cars: SPACE_SHIP_MIX,
    obstacles: &[Obstacle {
        style: theme::OBSTACLE_ASTEROID_CHUNK,
        frequency: 0.016,
        effects: &[ObstacleEffect::SpeedChange { affect: -0.30 }],
    }],
    ..SPACE_LANE
};

// Saturn's Rings — ring particles like a soft asteroid field
const RING_LANE: Lane = Lane {
    own_max_speed: 240.0,
    traffic_density: 0.1,
    traffic_cars: SPACE_SHIP_MIX,
    obstacles: &[Obstacle {
        style: theme::OBSTACLE_ASTEROID_CHUNK,
        frequency: 0.038,
        effects: &[
            ObstacleEffect::BlockWheels { cooldown_ms: 250 },
            ObstacleEffect::SpeedChange { affect: -0.45 },
        ],
    }],
    ..SPACE_LANE
};

// Deep Space — no obstacles, pure velocity
const DEEP_SPACE_LANE: Lane = Lane {
    own_max_speed: 500.0,
    traffic_density: 0.1,
    traffic_cars: SPACE_SHIP_MIX,
    obstacles: &[],
    ..SPACE_LANE
};

// Pluto's Edge — frigid, icy, nearly empty
const PLUTO_LANE: Lane = Lane {
    style: theme::LANE_GRAVEL,
    own_max_speed: 110.0,
    passive_decel: 2.0,
    traffic_density: 0.1,
    traffic_cars: SPACE_SHIP_MIX,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_ICE_PATCH,
            frequency: 0.062,
            effects: &[
                ObstacleEffect::BlockWheels { cooldown_ms: 700 },
                ObstacleEffect::SpeedChange { affect: -0.40 },
            ],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_CRATER,
            frequency: 0.020,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.65 }],
        },
    ],
    ..MOUNTAIN_LANE
};

// ─── Local sceneries ─────────────────────────────────────────────────────────

const MOON_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_ROCK,
        incidence: 0.75,
    },
    Object {
        style: theme::OBJ_STAR,
        incidence: 0.20,
    },
    Object {
        style: theme::OBJ_CRYSTAL_SPIKE,
        incidence: 0.05,
    },
];

const MOON_SCENERY: Scenery = Scenery {
    width: 12,
    background: theme::SCENERY_SNOW,
    objects: MOON_OBJECTS,
};

const ASTEROID_SCENERY_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_ROCK,
        incidence: 0.50,
    },
    Object {
        style: theme::OBJ_STAR,
        incidence: 0.30,
    },
    Object {
        style: theme::OBJ_PLANET_SMALL,
        incidence: 0.10,
    },
    Object {
        style: theme::OBJ_CRYSTAL_SPIKE,
        incidence: 0.10,
    },
];

const ASTEROID_SCENERY: Scenery = Scenery {
    width: 14,
    background: theme::SCENERY_STARFIELD,
    objects: ASTEROID_SCENERY_OBJECTS,
};

// ─── Stages ──────────────────────────────────────────────────────────────────

const S01_LAUNCH_PAD: Stage = Stage {
    name: "Launch Pad",
    description: "T-minus ten seconds. The road leads straight out of the atmosphere. Fuel trucks and support vehicles crowd the launch facility.",
    icon: theme::STAGE_CITY,
    theme: Theme::Standard,
    distance_km: 6.0,
    road: Road {
        lanes: Lanes {
            outgoing: &[CITY_LANE, CITY_LANE],
            incoming: &[CITY_LANE, CITY_LANE],
        },
        sceneries: Sceneries {
            left: CITY_SCENERY,
            right: PLAINS_SCENERY,
        },
        ..ROAD_CITY_2X2
    },
};

const S02_LOW_EARTH_ORBIT: Stage = Stage {
    name: "Low Earth Orbit",
    description: "The atmosphere thins. The sky deepens from blue to black. Stars appear. Traffic evaporates to a few brave ships heading the same direction.",
    icon: theme::STAGE_HIGHWAY,
    theme: Theme::Standard,
    distance_km: 8.0,
    road: Road {
        lanes: Lanes {
            outgoing: &[ORBIT_LANE, ORBIT_LANE],
            incoming: &[ORBIT_LANE, ORBIT_LANE],
        },
        sceneries: Sceneries {
            left: STARFIELD_SCENERY,
            right: STARFIELD_SCENERY,
        },
        ..ROAD_SPACE_2X2
    },
};

const S03_THE_MOON: Stage = Stage {
    name: "The Moon",
    description: "Grey regolith and ancient craters. The lunar surface is unmerciful — deep craters every hundred metres, no atmosphere to slow the fall.",
    icon: theme::STAGE_MOON,
    theme: Theme::Standard,
    distance_km: 10.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[MOON_LANE],
            incoming: &[MOON_LANE],
        },
        sceneries: Sceneries {
            left: MOON_SCENERY,
            right: MOON_SCENERY,
        },
        shoulders: Shoulders {
            left: SPACE_BEACON_SHOULDERS,
            right: SPACE_BEACON_SHOULDERS,
        },
    },
};

const S04_MARS: Stage = Stage {
    name: "Martian Highway",
    description: "The red planet in all its glory — and fury. Dust storms sweep across the cracked asphalt without warning. The atmosphere is thin and the craters are deep.",
    icon: theme::STAGE_PLANET,
    theme: Theme::Desert,
    distance_km: 12.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[MARS_LANE],
            incoming: &[MARS_LANE],
        },
        sceneries: Sceneries {
            left: SOUTHWESTERN_SCENERY,
            right: SOUTHWESTERN_SCENERY,
        },
        shoulders: Shoulders {
            left: SPACE_BEACON_SHOULDERS,
            right: SPACE_BEACON_SHOULDERS,
        },
    },
};

const S05_ASTEROID_BELT: Stage = Stage {
    name: "Asteroid Belt",
    description: "Between Mars and Jupiter: hundreds of millions of rocks from pebbles to mountains. The chunks hit without warning. The meteors don't miss.",
    icon: theme::STAGE_WILD_HILLS,
    theme: Theme::Standard,
    distance_km: 8.0,
    road: Road {
        aspect: RoadAspect {
            dividers: FOREST_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[ASTEROID_BELT_LANE],
            incoming: &[ASTEROID_BELT_LANE],
        },
        sceneries: Sceneries {
            left: ASTEROID_SCENERY,
            right: ASTEROID_SCENERY,
        },
        shoulders: Shoulders {
            left: SPACE_BEACON_SHOULDERS,
            right: SPACE_BEACON_SHOULDERS,
        },
    },
};

const S06_JUPITER_APPROACH: Stage = Stage {
    name: "Jupiter Approach",
    description: "The largest planet in the Solar System looms ahead. Its gravity pulls you faster than any engine could. Light turbulence — occasional ring debris.",
    icon: theme::STAGE_PLANET,
    theme: Theme::Standard,
    distance_km: 10.0,
    road: Road {
        lanes: Lanes {
            outgoing: &[JUPITER_LANE, JUPITER_LANE, JUPITER_LANE],
            incoming: &[JUPITER_LANE, JUPITER_LANE],
        },
        ..ROAD_SPACE_3X2
    },
};

const S07_SATURNS_RINGS: Stage = Stage {
    name: "Saturn's Rings",
    description: "A billion chunks of ice and rock, each the size of a car. You're driving through them. The view is spectacular. The physics is not forgiving.",
    icon: theme::STAGE_GALAXY,
    theme: Theme::Winter,
    distance_km: 8.0,
    road: Road {
        lanes: Lanes {
            outgoing: &[RING_LANE, RING_LANE],
            incoming: &[RING_LANE, RING_LANE],
        },
        ..ROAD_SPACE_2X2
    },
};

const S08_DEEP_SPACE: Stage = Stage {
    name: "Deep Space",
    description: "Past Saturn, nothing. Absolute silence. Zero obstacles. The Cosmic Highway stretches into darkness at half the speed of a thought. Just drive.",
    icon: theme::STAGE_GALAXY,
    theme: Theme::Standard,
    distance_km: 15.0,
    road: Road {
        lanes: Lanes {
            outgoing: &[DEEP_SPACE_LANE, DEEP_SPACE_LANE, DEEP_SPACE_LANE],
            incoming: &[DEEP_SPACE_LANE, DEEP_SPACE_LANE],
        },
        ..ROAD_SPACE_3X2
    },
};

const S09_PLUTOS_EDGE: Stage = Stage {
    name: "Pluto's Edge",
    description: "The last outpost of the Solar System. Surface temperature: -233°C. Ice patches freeze your tyres solid. Beyond this: only stars.",
    icon: theme::STAGE_SPECIAL,
    theme: Theme::Winter,
    distance_km: 8.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[PLUTO_LANE],
            incoming: &[PLUTO_LANE],
        },
        sceneries: Sceneries {
            left: ALPINE_SCENERY,
            right: STARFIELD_SCENERY,
        },
        shoulders: Shoulders {
            left: SPACE_BEACON_SHOULDERS,
            right: SPACE_BEACON_SHOULDERS,
        },
    },
};

// ─── Track ───────────────────────────────────────────────────────────────────

pub const TRACK: Track = Track {
    name: "Cosmic Highway",
    author: "claude",
    description: "From Earth's launch pad to Pluto's frozen edge. The road goes through low orbit, the lunar surface, Mars, the Asteroid Belt, Jupiter, Saturn's rings, and deep space. Watch for meteors.",
    stages: &[
        S01_LAUNCH_PAD,
        S02_LOW_EARTH_ORBIT,
        S03_THE_MOON,
        S04_MARS,
        S05_ASTEROID_BELT,
        S06_JUPITER_APPROACH,
        S07_SATURNS_RINGS,
        S08_DEEP_SPACE,
        S09_PLUTOS_EDGE,
    ],
    distance_scale: 0.20,
    speed_scale: 2.5,
    lives: 4,
};
