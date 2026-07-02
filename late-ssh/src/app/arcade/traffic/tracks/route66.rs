//! Route 66 — Chicago, IL to Santa Monica, CA.
//!
//! The Mother Road. ~3,940 km across America's heartland. This track captures
//! the iconic journey from Chicago skyscrapers through endless prairies, Texas
//! flatlands, New Mexico mesas, and Arizona/Mojave desert to the Pacific Ocean.

use super::presets::*;
use crate::app::arcade::traffic::theme;
use crate::app::arcade::traffic::track::{
    Lane, Lanes, Obstacle, ObstacleEffect, Road, RoadAspect, Sceneries, Shoulder, Shoulders, Stage,
    Theme, Track,
};

// ─── Lane variants ───────────────────────────────────────────────────────────

const CHICAGO_LANE_OUT: Lane = Lane {
    traffic_density: 0.3,
    traffic_cars: CITY_CAR_MIX,
    ..CITY_LANE
};
const CHICAGO_LANE_IN: Lane = Lane {
    traffic_density: 0.2,
    traffic_cars: CITY_CAR_MIX,
    ..CITY_LANE
};

// LA traffic: denser than Chicago, you arrive through sprawl
const LA_LANE_OUT: Lane = Lane {
    traffic_density: 0.4,
    traffic_cars: CITY_CAR_MIX,
    ..CITY_LANE
};
const LA_LANE_IN: Lane = Lane {
    traffic_density: 0.3,
    traffic_cars: CITY_CAR_MIX,
    ..CITY_LANE
};

const TOWN_LANE: Lane = Lane {
    own_max_speed: 110.0,
    traffic_density: 0.3,
    traffic_cars: AMERICAN_CAR_MIX,
    obstacles: &[Obstacle {
        style: theme::OBSTACLE_SPEED_BUMP,
        frequency: 0.025,
        effects: &[ObstacleEffect::SpeedChange { affect: -0.70 }],
    }],
    ..CITY_LANE
};

const INTERSTATE_OUT: Lane = Lane {
    traffic_density: 0.15,
    ..INTERSTATE_LANE
};
const INTERSTATE_IN: Lane = Lane {
    own_max_speed: 250.0, // wrong-side speed bonus: 200 → 230
    traffic_density: 0.1,
    ..INTERSTATE_LANE
};

const PLAINS_OUT: Lane = Lane {
    traffic_density: 0.1,
    ..PLAINS_LANE
};
const PLAINS_IN: Lane = Lane {
    own_max_speed: 240.0, // wrong-side speed bonus
    traffic_density: 0.1,
    ..PLAINS_LANE
};

// Historic two-lane blacktop — narrow, patchy, low speed
const R66_BLACKTOP: Lane = Lane {
    style: theme::LANE_ASPHALT_PATCHY,
    own_max_speed: 130.0,
    traffic_density: 0.1,
    traffic_min_speed: 60.0,
    traffic_max_speed: 95.0,
    traffic_cars: AMERICAN_CAR_MIX,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_SMALL,
            frequency: 0.03,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.25 }],
        },
        Obstacle {
            style: theme::OBSTACLE_SPEED_BUMP,
            frequency: 0.01,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.60 }],
        },
    ],
    ..PLAINS_LANE
};

const DESERT_OUT: Lane = Lane {
    traffic_density: 0.1,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_SAND_DRIFT,
            frequency: 0.040,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.40 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_SMALL,
            frequency: 0.025,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.20 }],
        },
        Obstacle {
            style: theme::OBSTACLE_OIL_SPILL,
            frequency: 0.008,
            effects: &[
                ObstacleEffect::BlockWheels { cooldown_ms: 400 },
                ObstacleEffect::SpeedChange { affect: -0.15 },
            ],
        },
    ],
    ..DESERT_LANE
};
const DESERT_IN: Lane = Lane {
    own_max_speed: 205.0, // wrong-side speed bonus
    traffic_density: 0.1,
    ..DESERT_OUT
};

const MOJAVE_LANE: Lane = Lane {
    own_max_speed: 140.0,
    traffic_density: 0.1,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_SAND_DRIFT,
            frequency: 0.060,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.50 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_CRATER,
            frequency: 0.018,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.85 }],
        },
        Obstacle {
            style: theme::OBSTACLE_ANIMAL,
            frequency: 0.008,
            effects: &[ObstacleEffect::Crash],
        },
    ],
    ..DESERT_LANE
};

const MOJAVE_LANE_IN: Lane = Lane {
    own_max_speed: 180.0, // wrong-side speed bonus
    ..MOJAVE_LANE
};

// ─── Shoulder strips ─────────────────────────────────────────────────────────

const PLAINS_SHOULDERS_L: &[Shoulder] = &[
    Shoulder {
        style: theme::SHOULDER_SOFT_EDGE,
        repeat: 0,
    },
    Shoulder {
        style: theme::SHOULDER_EMPTY,
        repeat: 0,
    },
    Shoulder {
        style: theme::SHOULDER_WIRE_FENCE,
        repeat: 8,
    },
    Shoulder {
        style: theme::SHOULDER_EMPTY,
        repeat: 0,
    },
];
const PLAINS_SHOULDERS_R: &[Shoulder] = &[
    Shoulder {
        style: theme::SHOULDER_SOFT_EDGE,
        repeat: 0,
    },
    Shoulder {
        style: theme::SHOULDER_EMPTY,
        repeat: 0,
    },
    Shoulder {
        style: theme::SHOULDER_POLES,
        repeat: 12,
    },
    Shoulder {
        style: theme::SHOULDER_EMPTY,
        repeat: 0,
    },
];
const DESERT_SHOULDERS_BOTH: &[Shoulder] = &[
    Shoulder {
        style: theme::SHOULDER_SAND_EDGE,
        repeat: 0,
    },
    Shoulder {
        style: theme::SHOULDER_EMPTY,
        repeat: 0,
    },
];

// ─── Stages ──────────────────────────────────────────────────────────────────

const S01_CHICAGO: Stage = Stage {
    name: "Chicago",
    description: "Start of Route 66, 1966. Grant Park behind you, Pacific Ocean 3,940 km ahead. City traffic is thick.",
    icon: theme::STAGE_METROPOLIS,
    theme: Theme::Standard,
    distance_km: 80.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[CHICAGO_LANE_OUT, CHICAGO_LANE_OUT],
            incoming: &[CHICAGO_LANE_IN, CHICAGO_LANE_IN],
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

const S02_ILLINOIS: Stage = Stage {
    name: "Illinois Plains",
    description: "Flat farmland as far as the eye can see. Corn, sky, and the occasional grain silo.",
    icon: theme::STAGE_WILD_PLAINS,
    theme: Theme::Standard,
    distance_km: 480.0,
    road: Road {
        aspect: RoadAspect {
            dividers: HIGHWAY_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[INTERSTATE_OUT, INTERSTATE_OUT],
            incoming: &[INTERSTATE_IN, INTERSTATE_IN],
        },
        sceneries: Sceneries {
            left: PLAINS_SCENERY,
            right: PLAINS_SCENERY,
        },
        shoulders: Shoulders {
            left: PLAINS_SHOULDERS_L,
            right: PLAINS_SHOULDERS_R,
        },
    },
};

const S03_ST_LOUIS: Stage = Stage {
    name: "St. Louis",
    description: "Gateway to the West. The arch glints on the horizon. Traffic clumps at every light.",
    icon: theme::STAGE_CITY_OUTSKIRTS,
    theme: Theme::Standard,
    distance_km: 60.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[CHICAGO_LANE_OUT],
            incoming: &[CHICAGO_LANE_IN],
        },
        sceneries: Sceneries {
            left: TOWN_SCENERY,
            right: CITY_SCENERY,
        },
        shoulders: Shoulders {
            left: SIDEWALK_SHOULDERS,
            right: SIDEWALK_SHOULDERS,
        },
    },
};

const S04_MISSOURI: Stage = Stage {
    name: "Route 66 / Missouri",
    description: "Historic two-lane blacktop through the Ozarks. This is real Route 66 — narrow, patchy, and perfect.",
    icon: theme::STAGE_HIGHWAY,
    theme: Theme::Standard,
    distance_km: 250.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[R66_BLACKTOP],
            incoming: &[R66_BLACKTOP],
        },
        sceneries: Sceneries {
            left: RURAL_SCENERY,
            right: FOREST_SCENERY,
        },
        shoulders: Shoulders {
            left: RURAL_SHOULDERS,
            right: RURAL_SHOULDERS,
        },
    },
};

const S05_OKLAHOMA: Stage = Stage {
    name: "Oklahoma Prairie",
    description: "Sky as wide as an ocean. Just you, the road, telephone poles, and the occasional tumbleweed. Wrong-side has more room.",
    icon: theme::STAGE_WILD_PLAINS,
    theme: Theme::Standard,
    distance_km: 540.0,
    road: Road {
        aspect: RoadAspect {
            dividers: HIGHWAY_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[PLAINS_OUT],
            incoming: &[PLAINS_IN],
        },
        sceneries: Sceneries {
            left: PLAINS_SCENERY,
            right: PLAINS_SCENERY,
        },
        shoulders: Shoulders {
            left: PLAINS_SHOULDERS_L,
            right: PLAINS_SHOULDERS_R,
        },
    },
};

const S06_TEXAS: Stage = Stage {
    name: "Texas Panhandle",
    description: "Amarillo by morning. Flat as a pool table, twice as green. Wind turbines spin forever.",
    icon: theme::STAGE_WILD_PLAINS,
    theme: Theme::Standard,
    distance_km: 320.0,
    road: Road {
        aspect: RoadAspect {
            dividers: HIGHWAY_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[INTERSTATE_OUT, INTERSTATE_OUT],
            incoming: &[INTERSTATE_IN, INTERSTATE_IN],
        },
        sceneries: Sceneries {
            left: PLAINS_SCENERY,
            right: PLAINS_SCENERY,
        },
        shoulders: Shoulders {
            left: PLAINS_SHOULDERS_L,
            right: PLAINS_SHOULDERS_R,
        },
    },
};

const S07_AMARILLO: Stage = Stage {
    name: "Amarillo, TX",
    description: "Big steak country. Speed bumps through every intersection. Watch the cattle trucks.",
    icon: theme::STAGE_CITY_OUTSKIRTS,
    theme: Theme::Standard,
    distance_km: 60.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[TOWN_LANE, TOWN_LANE],
            incoming: &[TOWN_LANE],
        },
        sceneries: Sceneries {
            left: TOWN_SCENERY,
            right: RURAL_SCENERY,
        },
        shoulders: Shoulders {
            left: SIDEWALK_SHOULDERS,
            right: RURAL_SHOULDERS,
        },
    },
};

const S08_NEW_MEXICO: Stage = Stage {
    name: "New Mexico Mesas",
    description: "Land of Enchantment. Terracotta mesas rise from the desert floor. Sand drifts across the road. The other lane looks emptier.",
    icon: theme::STAGE_WILD_HILLS,
    theme: Theme::Desert,
    distance_km: 510.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[DESERT_OUT],
            incoming: &[DESERT_IN],
        },
        sceneries: Sceneries {
            left: SOUTHWESTERN_SCENERY,
            right: SOUTHWESTERN_SCENERY,
        },
        shoulders: Shoulders {
            left: DESERT_SHOULDERS_BOTH,
            right: DESERT_SHOULDERS_BOTH,
        },
    },
};

const S09_ALBUQUERQUE: Stage = Stage {
    name: "Albuquerque",
    description: "New Mexico's biggest city. Hot air balloons above, green chile on every menu.",
    icon: theme::STAGE_CITY_OUTSKIRTS,
    theme: Theme::Desert,
    distance_km: 60.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[TOWN_LANE],
            incoming: &[TOWN_LANE],
        },
        sceneries: Sceneries {
            left: TOWN_SCENERY,
            right: SOUTHWESTERN_SCENERY,
        },
        shoulders: Shoulders {
            left: SIDEWALK_SHOULDERS,
            right: DESERT_SHOULDERS_BOTH,
        },
    },
};

const S10_ARIZONA: Stage = Stage {
    name: "Arizona Desert",
    description: "Saguaro cactus and 45°C heat. Sand drifts across the road endlessly. The oncoming lane looks clear — and faster.",
    icon: theme::STAGE_DESERT,
    theme: Theme::Desert,
    distance_km: 530.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[DESERT_OUT],
            incoming: &[DESERT_IN],
        },
        sceneries: Sceneries {
            left: SOUTHWESTERN_SCENERY,
            right: SOUTHWESTERN_SCENERY,
        },
        shoulders: Shoulders {
            left: DESERT_SHOULDERS_BOTH,
            right: DESERT_SHOULDERS_BOTH,
        },
    },
};

const S11_MOJAVE: Stage = Stage {
    name: "Mojave Desert",
    description: "Road buckles in 50°C heat. Craters from washed-out asphalt. Watch for desert animals. Almost there.",
    icon: theme::STAGE_DESERT,
    theme: Theme::Desert,
    distance_km: 270.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[MOJAVE_LANE],
            incoming: &[MOJAVE_LANE_IN],
        },
        sceneries: Sceneries {
            left: DESERT_SCENERY,
            right: SOUTHWESTERN_SCENERY,
        },
        shoulders: Shoulders {
            left: DESERT_SHOULDERS_BOTH,
            right: DESERT_SHOULDERS_BOTH,
        },
    },
};

const S12_SAN_BERNARDINO: Stage = Stage {
    name: "San Bernardino",
    description: "LA sprawl begins. Traffic thickens suddenly. You can faintly smell the Pacific.",
    icon: theme::STAGE_CITY_OUTSKIRTS,
    theme: Theme::Standard,
    distance_km: 80.0,
    road: Road {
        lanes: Lanes {
            outgoing: &[LA_LANE_OUT, LA_LANE_OUT],
            incoming: &[LA_LANE_IN, LA_LANE_IN],
        },
        sceneries: Sceneries {
            left: TOWN_SCENERY,
            right: TOWN_SCENERY,
        },
        ..ROAD_CITY_2X2
    },
};

const S13_SANTA_MONICA: Stage = Stage {
    name: "Santa Monica",
    description: "End of the line. Pacific Ocean dead ahead. 3,940 km from Chicago. Route 66 ends here.",
    icon: theme::STAGE_SPECIAL,
    theme: Theme::Standard,
    distance_km: 80.0,
    road: Road {
        lanes: Lanes {
            outgoing: &[LA_LANE_OUT, LA_LANE_OUT],
            incoming: &[LA_LANE_IN, LA_LANE_IN],
        },
        sceneries: Sceneries {
            left: COASTAL_SCENERY,
            right: CITY_SCENERY,
        },
        ..ROAD_CITY_2X2
    },
};

// ─── Track ───────────────────────────────────────────────────────────────────

pub const TRACK: Track = Track {
    name: "Route 66",
    author: "claude",
    description: "The Mother Road. Chicago to Santa Monica — coast to coast across America's heartland. Starts in a city, fades to endless plains, burns through desert, ends at the Pacific.",
    stages: &[
        S01_CHICAGO,
        S02_ILLINOIS,
        S03_ST_LOUIS,
        S04_MISSOURI,
        S05_OKLAHOMA,
        S06_TEXAS,
        S07_AMARILLO,
        S08_NEW_MEXICO,
        S09_ALBUQUERQUE,
        S10_ARIZONA,
        S11_MOJAVE,
        S12_SAN_BERNARDINO,
        S13_SANTA_MONICA,
    ],
    distance_scale: 0.017,
    speed_scale: 2.5,
    lives: 3,
};
