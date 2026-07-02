//! Sample track — exercises every stage feature so the rendering and physics
//! code can be smoke-tested with realistic input.

use super::presets::*;
use crate::app::arcade::traffic::theme;
use crate::app::arcade::traffic::track::{
    Lane, Lanes, Obstacle, ObstacleEffect, Road, RoadAspect, Sceneries, Shoulders, Stage, Theme,
    Track,
};

// ─── Per-stage road geometries ───────────────────────────────────────────────

const CITY_ROAD: Road = Road {
    aspect: RoadAspect {
        dividers: URBAN_DIVIDERS,
    },
    lanes: Lanes {
        incoming: &[CITY_LANE, CITY_LANE],
        outgoing: &[CITY_LANE, CITY_LANE],
    },
    sceneries: Sceneries {
        left: CITY_SCENERY,
        right: CITY_SCENERY,
    },
    shoulders: Shoulders {
        left: SIDEWALK_SHOULDERS,
        right: SIDEWALK_SHOULDERS,
    },
};

const HIGHWAY_ROAD: Road = Road {
    aspect: RoadAspect {
        dividers: HIGHWAY_DIVIDERS,
    },
    lanes: Lanes {
        incoming: &[HIGHWAY_LANE, HIGHWAY_LANE],
        outgoing: &[HIGHWAY_LANE, HIGHWAY_LANE, HIGHWAY_LANE],
    },
    sceneries: Sceneries {
        left: HIGHWAY_SCENERY,
        right: HIGHWAY_SCENERY,
    },
    shoulders: Shoulders {
        left: HIGHWAY_SHOULDERS,
        right: HIGHWAY_SHOULDERS,
    },
};

const RURAL_LANE_BUMPY: Lane = Lane {
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_SMALL,
            frequency: 0.04,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.10 }],
        },
        Obstacle {
            style: theme::OBSTACLE_SPEED_BUMP,
            frequency: 0.02,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.30 }],
        },
    ],
    ..RURAL_LANE
};

const RURAL_ROAD: Road = Road {
    aspect: RoadAspect {
        dividers: RURAL_DIVIDERS,
    },
    lanes: Lanes {
        incoming: &[RURAL_LANE_BUMPY],
        outgoing: &[RURAL_LANE_BUMPY],
    },
    sceneries: Sceneries {
        left: RURAL_SCENERY,
        right: RURAL_SCENERY,
    },
    shoulders: Shoulders {
        left: RURAL_SHOULDERS,
        right: RURAL_SHOULDERS,
    },
};

const FOREST_LANE_HAZARD: Lane = Lane {
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_FALLEN_TREE,
            frequency: 0.015,
            effects: &[ObstacleEffect::Crash],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_BIG,
            frequency: 0.05,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.40 }],
        },
    ],
    ..FOREST_LANE
};

const FOREST_ROAD: Road = Road {
    aspect: RoadAspect {
        dividers: FOREST_DIVIDERS,
    },
    lanes: Lanes {
        incoming: &[FOREST_LANE_HAZARD],
        outgoing: &[FOREST_LANE_HAZARD],
    },
    sceneries: Sceneries {
        left: FOREST_SCENERY,
        right: FOREST_SCENERY,
    },
    shoulders: Shoulders {
        left: FOREST_SHOULDERS,
        right: FOREST_SHOULDERS,
    },
};

// ─── Track ───────────────────────────────────────────────────────────────────

pub const TRACK: Track = Track {
    name: "Sample",
    author: "odd",
    description: "A 60-km drive through city, highway, country and snowy forest. \
         Touches every stage feature — good for testing.",
    stages: &[
        Stage {
            name: "City outskirts",
            description: "Leaving town — keep an eye out for traffic lights",
            icon: "🏙",
            theme: Theme::Standard,
            distance_km: 8.0,
            road: CITY_ROAD,
        },
        Stage {
            name: "Open highway",
            description: "Smooth asphalt — open it up but watch the trucks",
            icon: "🛣",
            theme: Theme::Standard,
            distance_km: 30.0,
            road: HIGHWAY_ROAD,
        },
        Stage {
            name: "Country backroad",
            description: "Patchy tarmac — potholes and bumps slow you down",
            icon: "🌾",
            theme: Theme::Standard,
            distance_km: 12.0,
            road: RURAL_ROAD,
        },
        Stage {
            name: "Snowy forest pass",
            description: "Slippery dirt and fallen trees — easy does it",
            icon: "🌲",
            theme: Theme::Winter,
            distance_km: 10.0,
            road: FOREST_ROAD,
        },
    ],
    distance_scale: 0.2,
    speed_scale: 2.0,
    lives: 3,
};
