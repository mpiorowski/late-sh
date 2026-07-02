//! Eurotrip — London to Barcelona.
//!
//! Across the English Channel, through France, Belgium, the German Autobahn
//! (no speed limit!), Austrian motorways, the Swiss/Austrian Alps, northern
//! Italy, the French Riviera, Monaco's cobblestones, and into Barcelona.

use super::presets::*;
use crate::app::arcade::traffic::theme;
use crate::app::arcade::traffic::track::{
    Lane, Lanes, Obstacle, ObstacleEffect, Road, RoadAspect, Sceneries, Shoulder, Shoulders, Stage,
    Theme, Track,
};

// ─── Lane variants ───────────────────────────────────────────────────────────

const LONDON_OUT: Lane = Lane {
    traffic_density: 0.35,
    traffic_cars: EURO_CITY_MIX,
    ..CITY_LANE
};
const LONDON_IN: Lane = Lane {
    traffic_density: 0.30,
    traffic_cars: EURO_CITY_MIX,
    ..CITY_LANE
};

const EURO_CITY_OUT: Lane = Lane {
    traffic_density: 0.4,
    traffic_cars: EURO_CITY_MIX,
    ..CITY_LANE
};
const EURO_CITY_IN: Lane = Lane {
    traffic_density: 0.3,
    traffic_cars: EURO_CITY_MIX,
    ..CITY_LANE
};

const MOTORWAY_OUT: Lane = Lane {
    traffic_density: 0.2,
    ..MOTORWAY_LANE
};
const MOTORWAY_IN: Lane = Lane {
    own_max_speed: 230.0, // wrong-side speed bonus: 200 → 230
    traffic_density: 0.15,
    ..MOTORWAY_LANE
};

// Channel Tunnel: controlled, slower, enclosed
const TUNNEL_LANE: Lane = Lane {
    own_max_speed: 140.0,
    traffic_min_speed: 80.0,
    traffic_max_speed: 120.0,
    traffic_density: 0.1,
    traffic_cars: EURO_HIGHWAY_MIX,
    obstacles: &[],
    ..MOTORWAY_LANE
};

// German Autobahn: no speed limit on unrestricted sections
const AUTOBAHN_FAST_OUT: Lane = Lane {
    own_min_speed: 0.0,
    own_max_speed: 350.0,
    traffic_min_speed: 100.0,
    traffic_max_speed: 220.0,
    traffic_density: 0.12,
    traffic_cars: EURO_HIGHWAY_MIX,
    obstacles: &[],
    ..AUTOBAHN_LANE
};
const AUTOBAHN_SLOW_IN: Lane = Lane {
    own_max_speed: 200.0,
    traffic_min_speed: 80.0,
    traffic_max_speed: 150.0,
    traffic_density: 0.20,
    ..AUTOBAHN_FAST_OUT
};

// Austrian/Italian motorway: slightly slower, more trucks
const ALPINE_MOTORWAY_OUT: Lane = Lane {
    traffic_density: 0.18,
    traffic_cars: EURO_HIGHWAY_MIX,
    ..MOTORWAY_LANE
};
const ALPINE_MOTORWAY_IN: Lane = Lane {
    own_max_speed: 225.0, // wrong-side speed bonus: 200 → 225
    traffic_density: 0.14,
    ..ALPINE_MOTORWAY_OUT
};

// Mountain pass: single narrow lane, guardrails, ice risk
const ALPS_LANE: Lane = Lane {
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_ICE_PATCH,
            frequency: 0.030,
            effects: &[ObstacleEffect::BlockBrakes { cooldown_ms: 650 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_SMALL,
            frequency: 0.020,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.25 }],
        },
        Obstacle {
            style: theme::OBSTACLE_FALLEN_TREE,
            frequency: 0.005,
            effects: &[ObstacleEffect::Crash],
        },
    ],
    ..MOUNTAIN_LANE
};

// Brenner Pass descent: road works common, ice possible
const BRENNER_LANE: Lane = Lane {
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_ROADWORK,
            frequency: 0.020,
            effects: &[
                ObstacleEffect::SpeedChange { affect: -0.55 },
                ObstacleEffect::BlockGas { cooldown_ms: 700 },
            ],
        },
        Obstacle {
            style: theme::OBSTACLE_ICE_PATCH,
            frequency: 0.015,
            effects: &[ObstacleEffect::BlockBrakes { cooldown_ms: 500 }],
        },
    ],
    ..MOUNTAIN_LANE
};

// Monaco cobblestones: very tight, dense luxury traffic
const MONACO_LANE: Lane = Lane {
    own_max_speed: 55.0,
    traffic_density: 0.50,
    traffic_cars: EURO_CITY_MIX,
    obstacles: &[Obstacle {
        style: theme::OBSTACLE_POTHOLE_SMALL,
        frequency: 0.025,
        effects: &[ObstacleEffect::SpeedChange { affect: -0.20 }],
    }],
    ..COBBLE_LANE
};

// ─── Shoulder strips ─────────────────────────────────────────────────────────

const ALPS_SHOULDERS: &[Shoulder] = &[
    Shoulder {
        style: theme::SHOULDER_GUARDRAIL,
        repeat: 0,
    },
    Shoulder {
        style: theme::SHOULDER_EMPTY,
        repeat: 0,
    },
    Shoulder {
        style: theme::SHOULDER_TREE_PINE,
        repeat: 6,
    },
];
const TUNNEL_SHOULDERS_BOTH: &[Shoulder] = &[Shoulder {
    style: theme::SHOULDER_CRASH_BARRIER,
    repeat: 0,
}];
const MOTORWAY_POLES_R: &[Shoulder] = &[
    Shoulder {
        style: theme::SHOULDER_HARD_EDGE,
        repeat: 0,
    },
    Shoulder {
        style: theme::SHOULDER_POLES,
        repeat: 8,
    },
    Shoulder {
        style: theme::SHOULDER_EMPTY,
        repeat: 0,
    },
];
const MONACO_SHOULDERS: &[Shoulder] = &[
    Shoulder {
        style: theme::SHOULDER_HARD_EDGE,
        repeat: 0,
    },
    Shoulder {
        style: theme::SHOULDER_PARKED_CAR,
        repeat: 3,
    },
];

// ─── Stages ──────────────────────────────────────────────────────────────────

const S01_LONDON: Stage = Stage {
    name: "London",
    description: "Start in the heart of London. Congestion charge zone. Dense traffic, narrow streets, everyone in a hurry.",
    icon: theme::STAGE_METROPOLIS,
    theme: Theme::Standard,
    distance_km: 80.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[LONDON_OUT, LONDON_OUT],
            incoming: &[LONDON_IN, LONDON_IN],
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

const S02_M20: Stage = Stage {
    name: "M20 Motorway",
    description: "British motorway south toward Dover. Three lanes, moderate speed, clouds overhead as always.",
    icon: theme::STAGE_HIGHWAY,
    theme: Theme::Standard,
    distance_km: 120.0,
    road: Road {
        lanes: Lanes {
            outgoing: &[MOTORWAY_OUT, MOTORWAY_OUT],
            incoming: &[MOTORWAY_IN, MOTORWAY_IN],
        },
        sceneries: Sceneries {
            left: HIGHWAY_SCENERY,
            right: RURAL_SCENERY,
        },
        ..ROAD_MOTORWAY_2X2
    },
};

const S03_CHANNEL_TUNNEL: Stage = Stage {
    name: "Channel Tunnel",
    description: "Undersea tunnel beneath the English Channel. 50 km of darkness, 140 km/h max, controlled entry queues.",
    icon: theme::STAGE_TUNNEL,
    theme: Theme::Standard,
    distance_km: 50.0,
    road: Road {
        aspect: RoadAspect {
            dividers: HIGHWAY_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[TUNNEL_LANE],
            incoming: &[TUNNEL_LANE],
        },
        sceneries: Sceneries {
            left: TUNNEL_SCENERY,
            right: TUNNEL_SCENERY,
        },
        shoulders: Shoulders {
            left: TUNNEL_SHOULDERS_BOTH,
            right: TUNNEL_SHOULDERS_BOTH,
        },
    },
};

const S04_FRENCH_MOTORWAY: Stage = Stage {
    name: "A16 / Calais",
    description: "Bienvenue en France. French autoroute — toll plazas ahead, but traffic opens up fast.",
    icon: theme::STAGE_HIGHWAY,
    theme: Theme::Standard,
    distance_km: 100.0,
    road: Road {
        lanes: Lanes {
            outgoing: &[MOTORWAY_OUT, MOTORWAY_OUT],
            incoming: &[MOTORWAY_IN, MOTORWAY_IN],
        },
        sceneries: Sceneries {
            left: RURAL_SCENERY,
            right: HIGHWAY_SCENERY,
        },
        shoulders: Shoulders {
            left: HIGHWAY_SHOULDERS,
            right: MOTORWAY_POLES_R,
        },
        ..ROAD_MOTORWAY_2X2
    },
};

const S05_PARIS: Stage = Stage {
    name: "Paris",
    description: "Arc de Triomphe roundabout. Twelve lanes merge into chaos. Parisian drivers use horns liberally.",
    icon: theme::STAGE_CITY,
    theme: Theme::Standard,
    distance_km: 80.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[EURO_CITY_OUT, EURO_CITY_OUT],
            incoming: &[EURO_CITY_IN, EURO_CITY_IN],
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

const S06_BELGIUM: Stage = Stage {
    name: "Belgium / E40",
    description: "Flat Belgian motorway. Some of the most densely trafficked roads in Europe. Watch the trucks.",
    icon: theme::STAGE_HIGHWAY,
    theme: Theme::Standard,
    distance_km: 350.0,
    road: Road {
        lanes: Lanes {
            outgoing: &[MOTORWAY_OUT, MOTORWAY_OUT, MOTORWAY_OUT],
            incoming: &[MOTORWAY_IN, MOTORWAY_IN],
        },
        sceneries: Sceneries {
            left: TOWN_SCENERY,
            right: TOWN_SCENERY,
        },
        ..ROAD_MOTORWAY_3X2
    },
};

const S07_AUTOBAHN: Stage = Stage {
    name: "German Autobahn",
    description: "No speed limit on this stretch. 340 km/h is legal. Traffic still overtakes you. Floor it.",
    icon: theme::STAGE_HIGHWAY,
    theme: Theme::Standard,
    distance_km: 450.0,
    road: Road {
        aspect: RoadAspect {
            dividers: HIGHWAY_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[AUTOBAHN_FAST_OUT, AUTOBAHN_FAST_OUT, AUTOBAHN_FAST_OUT],
            incoming: &[AUTOBAHN_SLOW_IN, AUTOBAHN_SLOW_IN],
        },
        sceneries: Sceneries {
            left: HIGHWAY_SCENERY,
            right: HIGHWAY_SCENERY,
        },
        shoulders: Shoulders {
            left: HIGHWAY_SHOULDERS,
            right: MOTORWAY_POLES_R,
        },
    },
};

const S08_FRANKFURT: Stage = Stage {
    name: "Frankfurt",
    description: "Germany's financial hub. Skyline of glass towers ahead. Speed drops back to city limits.",
    icon: theme::STAGE_CITY_OUTSKIRTS,
    theme: Theme::Standard,
    distance_km: 80.0,
    road: Road {
        lanes: Lanes {
            outgoing: &[EURO_CITY_OUT],
            incoming: &[EURO_CITY_IN],
        },
        sceneries: Sceneries {
            left: CITY_SCENERY,
            right: TOWN_SCENERY,
        },
        ..ROAD_CITY_2X2
    },
};

const S09_AUSTRIA: Stage = Stage {
    name: "Austrian Motorway",
    description: "Into Austria. Mountains appear on the horizon. Alpine air through the window.",
    icon: theme::STAGE_WILD_HILLS,
    theme: Theme::Standard,
    distance_km: 250.0,
    road: Road {
        lanes: Lanes {
            outgoing: &[ALPINE_MOTORWAY_OUT, ALPINE_MOTORWAY_OUT],
            incoming: &[ALPINE_MOTORWAY_IN, ALPINE_MOTORWAY_IN],
        },
        sceneries: Sceneries {
            left: MOUNTAIN_SCENERY,
            right: HIGHWAY_SCENERY,
        },
        ..ROAD_MOTORWAY_2X2
    },
};

const S10_SWISS_ALPS: Stage = Stage {
    name: "Swiss/Austrian Alps",
    description: "Switchbacks, guardrails, and ice. One lane each way. 3,000m peaks all around. Take it slow.",
    icon: theme::STAGE_MOUNTAIN,
    theme: Theme::Winter,
    distance_km: 150.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[ALPS_LANE],
            incoming: &[ALPS_LANE],
        },
        sceneries: Sceneries {
            left: ALPINE_SCENERY,
            right: ALPINE_SCENERY,
        },
        shoulders: Shoulders {
            left: ALPS_SHOULDERS,
            right: ALPS_SHOULDERS,
        },
    },
};

const S11_BRENNER: Stage = Stage {
    name: "Brenner Pass",
    description: "Historic Alpine crossing at 1,370m. Road works every season. Icy patches linger well into spring.",
    icon: theme::STAGE_MOUNTAIN,
    theme: Theme::Winter,
    distance_km: 100.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[BRENNER_LANE],
            incoming: &[BRENNER_LANE],
        },
        sceneries: Sceneries {
            left: ALPINE_SCENERY,
            right: MOUNTAIN_SCENERY,
        },
        shoulders: Shoulders {
            left: ALPS_SHOULDERS,
            right: ALPS_SHOULDERS,
        },
    },
};

const S12_NORTH_ITALY: Stage = Stage {
    name: "Northern Italy / A22",
    description: "Coming down from the Alps into the Po Valley. Speed climbs. Warm Italian air ahead.",
    icon: theme::STAGE_HIGHWAY,
    theme: Theme::Standard,
    distance_km: 180.0,
    road: Road {
        lanes: Lanes {
            outgoing: &[ALPINE_MOTORWAY_OUT, ALPINE_MOTORWAY_OUT],
            incoming: &[ALPINE_MOTORWAY_IN, ALPINE_MOTORWAY_IN],
        },
        sceneries: Sceneries {
            left: EURO_VILLAGE_SCENERY,
            right: MOUNTAIN_SCENERY,
        },
        ..ROAD_MOTORWAY_2X2
    },
};

const S13_MILAN: Stage = Stage {
    name: "Milan",
    description: "Italian fashion capital. Historic centro storico on each side. Traffic is creative, not orderly.",
    icon: theme::STAGE_CITY,
    theme: Theme::Standard,
    distance_km: 80.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[EURO_CITY_OUT, EURO_CITY_OUT],
            incoming: &[EURO_CITY_IN, EURO_CITY_IN],
        },
        sceneries: Sceneries {
            left: CITY_SCENERY,
            right: EURO_VILLAGE_SCENERY,
        },
        shoulders: Shoulders {
            left: SIDEWALK_SHOULDERS,
            right: SIDEWALK_SHOULDERS,
        },
    },
};

const S14_FRENCH_RIVIERA: Stage = Stage {
    name: "French Riviera / A8",
    description: "Azure coast. Palm trees, azure sea on your right, limestone cliffs on your left. Worth every kilometre.",
    icon: theme::STAGE_COASTAL,
    theme: Theme::Standard,
    distance_km: 200.0,
    road: Road {
        lanes: Lanes {
            outgoing: &[MOTORWAY_OUT, MOTORWAY_OUT],
            incoming: &[MOTORWAY_IN],
        },
        sceneries: Sceneries {
            left: MOUNTAIN_SCENERY,
            right: COASTAL_SCENERY,
        },
        ..ROAD_MOTORWAY_2X2
    },
};

const S15_MONACO: Stage = Stage {
    name: "Monaco",
    description: "Principality of Monaco. Cobblestones, hairpin bends, parked Ferraris on the shoulder. Slowest stage, best scenery.",
    icon: theme::STAGE_SPECIAL,
    theme: Theme::Standard,
    distance_km: 80.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[MONACO_LANE],
            incoming: &[MONACO_LANE],
        },
        sceneries: Sceneries {
            left: COASTAL_SCENERY,
            right: CITY_SCENERY,
        },
        shoulders: Shoulders {
            left: MONACO_SHOULDERS,
            right: MONACO_SHOULDERS,
        },
    },
};

const S16_BARCELONA: Stage = Stage {
    name: "Barcelona",
    description: "Capital of Catalonia. Gaudí's towers on the horizon. La Rambla a block away. You made it.",
    icon: theme::STAGE_SPECIAL,
    theme: Theme::Standard,
    distance_km: 80.0,
    road: Road {
        lanes: Lanes {
            outgoing: &[EURO_CITY_OUT, EURO_CITY_OUT],
            incoming: &[EURO_CITY_IN, EURO_CITY_IN],
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
    name: "Eurotrip",
    author: "claude",
    description: "London to Barcelona. English Channel tunnel, Parisian chaos, limitless German Autobahn, Alpine ice, Italian sunshine, Monaco cobblestones. Every stage feels different.",
    stages: &[
        S01_LONDON,
        S02_M20,
        S03_CHANNEL_TUNNEL,
        S04_FRENCH_MOTORWAY,
        S05_PARIS,
        S06_BELGIUM,
        S07_AUTOBAHN,
        S08_FRANKFURT,
        S09_AUSTRIA,
        S10_SWISS_ALPS,
        S11_BRENNER,
        S12_NORTH_ITALY,
        S13_MILAN,
        S14_FRENCH_RIVIERA,
        S15_MONACO,
        S16_BARCELONA,
    ],
    distance_scale: 0.019,
    speed_scale: 2.0,
    lives: 3,
};
