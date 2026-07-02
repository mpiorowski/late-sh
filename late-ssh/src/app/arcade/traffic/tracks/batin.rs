use super::presets::*;
use crate::app::arcade::traffic::theme;
use crate::app::arcade::traffic::track::{
    Lane, Lanes, Obstacle, ObstacleEffect, Road, RoadAspect, Sceneries, Shoulder, Shoulders, Stage,
    Theme, Track,
};

// PRESETS

const B_CITY_LANE: Lane = Lane {
    own_max_speed: 120.0,
    obstacles: &[Obstacle {
        style: theme::OBSTACLE_POTHOLE_SMALL,
        frequency: 0.02,
        effects: &[ObstacleEffect::SpeedChange { affect: -0.3 }],
    }],
    ..CITY_LANE
};

const B_CITY_LANE_OUT: Lane = Lane {
    traffic_density: 0.4,
    ..B_CITY_LANE
};

const B_CITY_LANE_IN: Lane = Lane {
    own_max_speed: 150.0,
    traffic_density: 0.2,
    ..B_CITY_LANE
};

const B_TOWN_LANE: Lane = Lane {
    own_max_speed: 100.0,
    traffic_max_speed: 80.0,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_SMALL,
            frequency: 0.02,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.3 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_BIG,
            frequency: 0.01,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.6 }],
        },
        Obstacle {
            style: theme::OBSTACLE_SPEED_BUMP,
            frequency: 0.01,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.8 }],
        },
    ],
    ..CITY_LANE
};

const B_TOWN_LANE_OUT: Lane = Lane {
    traffic_density: 0.6,
    ..B_TOWN_LANE
};

const B_TOWN_LANE_IN: Lane = Lane {
    own_max_speed: 140.0,
    traffic_density: 0.4,
    ..B_TOWN_LANE
};

const B_OUTSKIRT_LANE: Lane = Lane {
    own_max_speed: 100.0,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_BIG,
            frequency: 0.02,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.6 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_SMALL,
            frequency: 0.03,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.3 }],
        },
    ],
    ..CITY_LANE
};

const B_OUTSKIRT_LANE_OUT: Lane = Lane {
    traffic_density: 0.3,
    ..B_OUTSKIRT_LANE
};

const B_OUTSKIRT_LANE_IN: Lane = Lane {
    own_max_speed: 120.0,
    traffic_density: 0.15,
    ..B_OUTSKIRT_LANE
};

const B_OUTSIDE_LANE: Lane = Lane {
    own_max_speed: 150.0,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_CRATER,
            frequency: 0.005,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.9 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_BIG,
            frequency: 0.02,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.6 }],
        },
    ],
    ..CITY_LANE
};

const B_OUTSIDE_LANE_OUT: Lane = Lane {
    traffic_density: 0.35,
    ..B_OUTSIDE_LANE
};

const B_OUTSIDE_LANE_OUT_SLOW: Lane = Lane {
    traffic_density: 0.15,
    own_max_speed: 100.0,
    traffic_max_speed: 60.0,
    style: theme::LANE_ASPHALT_PATCHY,
    ..B_OUTSIDE_LANE
};

const B_OUTSIDE_LANE_IN: Lane = Lane {
    own_max_speed: 200.0,
    traffic_max_speed: 110.0,
    traffic_density: 0.15,
    ..B_OUTSIDE_LANE
};

const B_OUTSIDE_LANE_IN_SLOW: Lane = Lane {
    own_max_speed: 140.0,
    traffic_max_speed: 100.0,
    traffic_density: 0.15,
    style: theme::LANE_ASPHALT_PATCHY,
    ..B_OUTSIDE_LANE
};

const B_RURAL_LANE: Lane = Lane {
    style: theme::LANE_GRAVEL,
    own_min_speed: 20.0,
    own_max_speed: 100.0,
    passive_decel: 0.0,
    traffic_min_speed: 30.0,
    traffic_max_speed: 60.0,
    traffic_density: 0.1,
    traffic_cars: RURAL_CAR_MIX,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_CRATER,
            frequency: 0.01,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.9 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_BIG,
            frequency: 0.05,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.6 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_SMALL,
            frequency: 0.1,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.3 }],
        },
    ],
};

const B_RURAL_LANE_OUT: Lane = Lane {
    traffic_density: 0.2,
    ..B_RURAL_LANE
};

const B_RURAL_LANE_OUT_SLOW: Lane = Lane {
    style: theme::LANE_DIRT,
    traffic_min_speed: 20.0,
    traffic_max_speed: 50.0,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_CRATER,
            frequency: 0.02,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.9 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_BIG,
            frequency: 0.1,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.6 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_SMALL,
            frequency: 0.3,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.3 }],
        },
    ],
    ..B_RURAL_LANE_OUT
};

const B_RURAL_LANE_IN: Lane = Lane {
    own_max_speed: 120.0,
    traffic_max_speed: 50.0,
    traffic_density: 0.1,
    ..B_RURAL_LANE
};

const B_RURAL_LANE_IN_SLOW: Lane = Lane {
    style: theme::LANE_DIRT,
    own_max_speed: 80.0,
    traffic_min_speed: 15.0,
    traffic_max_speed: 40.0,
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_CRATER,
            frequency: 0.02,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.9 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_BIG,
            frequency: 0.1,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.6 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_SMALL,
            frequency: 0.3,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.3 }],
        },
    ],
    ..B_RURAL_LANE_IN
};

// STAGES

const S01_CLUJ: Stage = Stage {
    name: "Cluj-Napoca",
    description: "My city of birth. Year 1998. Got our Dacia 1310 ready to go - washed and filled up. Car is full of bags of things we'll mostly bring back intact. 3 kids in the car. Let's go.",
    icon: theme::STAGE_CITY,
    theme: Theme::Standard,
    distance_km: 8.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_CITY_LANE_OUT, B_CITY_LANE_OUT],
            incoming: &[B_CITY_LANE_IN, B_CITY_LANE_IN],
        },
        sceneries: Sceneries {
            left: CITY_SCENERY,
            right: CITY_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_HARD_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_SIDEWALK,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_HARD_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_SIDEWALK,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S02_SANNICOARA: Stage = Stage {
    name: "Sannicoara",
    description: "Moving out of the city.",
    icon: theme::STAGE_CITY_OUTSKIRTS,
    theme: Theme::Standard,
    distance_km: 2.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_OUTSKIRT_LANE_OUT],
            incoming: &[B_OUTSKIRT_LANE_IN, B_OUTSKIRT_LANE_IN],
        },
        sceneries: Sceneries {
            left: CITY_SCENERY,
            right: HIGHWAY_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_SIDEWALK,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
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
                    repeat: 20,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S03_APAHIDA: Stage = Stage {
    name: "Apahida",
    description: "This is a town close to the main city (Cluj). I live here now :)",
    icon: theme::STAGE_CITY_OUTSKIRTS,
    theme: Theme::Standard,
    distance_km: 3.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_OUTSKIRT_LANE_OUT],
            incoming: &[B_OUTSKIRT_LANE_IN],
        },
        sceneries: Sceneries {
            left: CITY_SCENERY,
            right: HIGHWAY_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_SIDEWALK,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RAILROAD,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_TREE_OAK,
                    repeat: 1,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S04_JUCU: Stage = Stage {
    name: "Jucu",
    description: "Jucu is actually a group of 3 villages. We're moving alongside all of them, not really through.",
    icon: theme::STAGE_WILD_PLAINS,
    theme: Theme::Standard,
    distance_km: 4.0,
    road: Road {
        aspect: RoadAspect {
            dividers: HIGHWAY_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[
                B_OUTSIDE_LANE_OUT,
                B_OUTSIDE_LANE_OUT,
                B_OUTSIDE_LANE_OUT_SLOW,
            ],
            incoming: &[B_OUTSIDE_LANE_IN, B_OUTSIDE_LANE_IN],
        },
        sceneries: Sceneries {
            left: RURAL_SCENERY,
            right: RURAL_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RAILROAD,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S05_E576_1: Stage = Stage {
    name: "E576/1",
    description: "Rural area begins.",
    icon: theme::STAGE_WILD_PLAINS,
    theme: Theme::Standard,
    distance_km: 7.0,
    road: Road {
        aspect: RoadAspect {
            dividers: HIGHWAY_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_OUTSIDE_LANE_OUT, B_OUTSIDE_LANE_OUT_SLOW],
            incoming: &[B_OUTSIDE_LANE_IN, B_OUTSIDE_LANE_IN_SLOW],
        },
        sceneries: Sceneries {
            left: RURAL_SCENERY,
            right: RURAL_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RAILROAD,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S06_RASCRUCI: Stage = Stage {
    name: "Rascruci",
    description: "Boring village.",
    icon: theme::STAGE_WILD_PLAINS,
    theme: Theme::Standard,
    distance_km: 2.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_OUTSIDE_LANE_OUT],
            incoming: &[B_OUTSIDE_LANE_IN],
        },
        sceneries: Sceneries {
            left: RURAL_SCENERY,
            right: RURAL_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RAILROAD,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RIVER,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S07_BONTIDA: Stage = Stage {
    name: "Bontida",
    description: "Another boring village.",
    icon: theme::STAGE_WILD_PLAINS,
    theme: Theme::Standard,
    distance_km: 2.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_OUTSIDE_LANE_OUT],
            incoming: &[B_OUTSIDE_LANE_IN],
        },
        sceneries: Sceneries {
            left: RURAL_SCENERY,
            right: RURAL_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RAILROAD,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S08_FUNDATURA: Stage = Stage {
    name: "Fundatura",
    description: "Yet another boring village.",
    icon: theme::STAGE_WILD_PLAINS,
    theme: Theme::Standard,
    distance_km: 2.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_OUTSIDE_LANE_OUT],
            incoming: &[B_OUTSIDE_LANE_IN],
        },
        sceneries: Sceneries {
            left: RURAL_SCENERY,
            right: RURAL_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RAILROAD,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RIVER,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S09_ICLOD: Stage = Stage {
    name: "Iclod",
    description: "Boring village, again...",
    icon: theme::STAGE_VILLAGE,
    theme: Theme::Standard,
    distance_km: 2.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_OUTSIDE_LANE_OUT, B_OUTSIDE_LANE_OUT_SLOW],
            incoming: &[B_OUTSIDE_LANE_IN],
        },
        sceneries: Sceneries {
            left: VILLAGE_SCENERY,
            right: VILLAGE_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RAILROAD,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RIVER,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S10_LIVADA: Stage = Stage {
    name: "Livada",
    description: "Aaaand... it's another boring village.",
    icon: theme::STAGE_VILLAGE,
    theme: Theme::Standard,
    distance_km: 2.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_OUTSIDE_LANE_OUT, B_OUTSIDE_LANE_OUT_SLOW],
            incoming: &[B_OUTSIDE_LANE_IN],
        },
        sceneries: Sceneries {
            left: RURAL_SCENERY,
            right: VILLAGE_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RAILROAD,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RIVER,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S11_BAITA: Stage = Stage {
    name: "Baita",
    description: "Coming up to something else...",
    icon: theme::STAGE_WILD_PLAINS,
    theme: Theme::Standard,
    distance_km: 2.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_OUTSIDE_LANE_OUT],
            incoming: &[B_OUTSIDE_LANE_IN],
        },
        sceneries: Sceneries {
            left: RURAL_SCENERY,
            right: RURAL_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RAILROAD,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RIVER,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S12_GHERLA: Stage = Stage {
    name: "Gherla",
    description: "Hey, a town! It is pretty nice. There's also a prison nearby.",
    icon: theme::STAGE_CITY_OUTSKIRTS,
    theme: Theme::Standard,
    distance_km: 5.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_TOWN_LANE_OUT, B_TOWN_LANE_OUT],
            incoming: &[B_TOWN_LANE_IN, B_TOWN_LANE_IN],
        },
        sceneries: Sceneries {
            left: TOWN_SCENERY,
            right: TOWN_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_HARD_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_PARKED_CAR,
                    repeat: 4,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_HARD_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_SIDEWALK,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S13_E576_2: Stage = Stage {
    name: "E576/2",
    description: "Road in the middle of nowhere. Getting close to a larger town, then it's all countryside.",
    icon: theme::STAGE_WILD_PLAINS,
    theme: Theme::Standard,
    distance_km: 8.0,
    road: Road {
        aspect: RoadAspect {
            dividers: HIGHWAY_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_OUTSIDE_LANE_OUT],
            incoming: &[B_OUTSIDE_LANE_IN],
        },
        sceneries: Sceneries {
            left: RURAL_SCENERY,
            right: RURAL_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_TREE_OAK,
                    repeat: 3,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RAILROAD,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S14_DEJ: Stage = Stage {
    name: "Dej",
    description: "Quickly touching this larger town before heading deep into countryside. Crossing the railroad here.",
    icon: theme::STAGE_CITY_OUTSKIRTS,
    theme: Theme::Standard,
    distance_km: 3.0,
    road: Road {
        aspect: RoadAspect {
            dividers: URBAN_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_OUTSKIRT_LANE_OUT, B_OUTSKIRT_LANE_OUT],
            incoming: &[B_OUTSKIRT_LANE_IN, B_OUTSKIRT_LANE_IN],
        },
        sceneries: Sceneries {
            left: TOWN_SCENERY,
            right: RURAL_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_TREE_OAK,
                    repeat: 3,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_TREE_OAK,
                    repeat: 3,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RAILROAD,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S15_MANASTIREA: Stage = Stage {
    name: "Manastirea",
    description: "Wild countryside begins.",
    icon: theme::STAGE_WILD_PLAINS,
    theme: Theme::Standard,
    distance_km: 2.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_OUTSIDE_LANE_OUT],
            incoming: &[B_OUTSIDE_LANE_IN],
        },
        sceneries: Sceneries {
            left: RURAL_SCENERY,
            right: FOREST_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_RIVER,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S16_DJ161D_1: Stage = Stage {
    name: "DJ161D/1",
    description: "Wild countryside continues. Next village in 7km",
    icon: theme::STAGE_WILD_HILLS,
    theme: Theme::Standard,
    distance_km: 7.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_OUTSIDE_LANE_OUT_SLOW],
            incoming: &[B_OUTSIDE_LANE_IN_SLOW],
        },
        sceneries: Sceneries {
            left: FOREST_SCENERY,
            right: FOREST_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S17_NIRES: Stage = Stage {
    name: "Nires",
    description: "A large-ish village close to our destination.",
    icon: theme::STAGE_VILLAGE,
    theme: Theme::Standard,
    distance_km: 2.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_RURAL_LANE_OUT],
            incoming: &[B_RURAL_LANE_IN],
        },
        sceneries: Sceneries {
            left: FOREST_SCENERY,
            right: VILLAGE_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S18_DJ161D_2: Stage = Stage {
    name: "DJ161D/2",
    description: "Really getting rugged now. Next village is a long boring one, in 5km.",
    icon: theme::STAGE_WILD_HILLS,
    theme: Theme::Standard,
    distance_km: 5.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_OUTSIDE_LANE_OUT_SLOW],
            incoming: &[B_OUTSIDE_LANE_IN_SLOW],
        },
        sceneries: Sceneries {
            left: FOREST_SCENERY,
            right: FOREST_SCENERY,
        },
        shoulders: NO_SHOULDERS,
    },
};

const S19_UNGURAS: Stage = Stage {
    name: "Unguras",
    description: "A long village. Last one before we get there.",
    icon: theme::STAGE_VILLAGE,
    theme: Theme::Standard,
    distance_km: 4.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_RURAL_LANE_OUT],
            incoming: &[B_RURAL_LANE_IN],
        },
        sceneries: Sceneries {
            left: VILLAGE_SCENERY,
            right: VILLAGE_SCENERY,
        },
        shoulders: Shoulders {
            left: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
            right: &[
                Shoulder {
                    style: theme::SHOULDER_SOFT_EDGE,
                    repeat: 0,
                },
                Shoulder {
                    style: theme::SHOULDER_EMPTY,
                    repeat: 0,
                },
            ],
        },
    },
};

const S20_DJ161D_3: Stage = Stage {
    name: "DJ161D/3",
    description: "Final stretch before destination. I usually was very carsick by this point :D.",
    icon: theme::STAGE_WILD_HILLS,
    theme: Theme::Standard,
    distance_km: 3.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_RURAL_LANE_OUT_SLOW],
            incoming: &[B_RURAL_LANE_IN_SLOW],
        },
        sceneries: Sceneries {
            left: FOREST_SCENERY,
            right: FOREST_SCENERY,
        },
        shoulders: NO_SHOULDERS,
    },
};

const S21_BATIN: Stage = Stage {
    name: "Batin",
    description: "We're here. Can't wait to play with the cat.",
    icon: theme::STAGE_SPECIAL,
    theme: Theme::Standard,
    distance_km: 2.0,
    road: Road {
        aspect: RoadAspect {
            dividers: RURAL_DIVIDERS,
        },
        lanes: Lanes {
            outgoing: &[B_RURAL_LANE_OUT_SLOW],
            incoming: &[B_RURAL_LANE_IN_SLOW],
        },
        sceneries: Sceneries {
            left: FOREST_SCENERY,
            right: VILLAGE_SCENERY,
        },
        shoulders: NO_SHOULDERS,
    },
};

// TRACK

pub const TRACK: Track = Track {
    name: "Batin",
    author: "odd",
    description: "A road trip I used to make when I was a kid. From the city to our countryside house. Starts decently and ends with extremely rugged roads.",
    stages: &[
        S01_CLUJ,
        S02_SANNICOARA,
        S03_APAHIDA,
        S04_JUCU,
        S05_E576_1,
        S06_RASCRUCI,
        S07_BONTIDA,
        S08_FUNDATURA,
        S09_ICLOD,
        S10_LIVADA,
        S11_BAITA,
        S12_GHERLA,
        S13_E576_2,
        S14_DEJ,
        S15_MANASTIREA,
        S16_DJ161D_1,
        S17_NIRES,
        S18_DJ161D_2,
        S19_UNGURAS,
        S20_DJ161D_3,
        S21_BATIN,
    ],
    distance_scale: 0.5,
    speed_scale: 2.0,
    lives: 1,
};
