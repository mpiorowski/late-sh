use super::presets::*;
use crate::app::arcade::traffic::theme;
use crate::app::arcade::traffic::track::{
    Car, Lane, Lanes, Object, Obstacle, ObstacleEffect, Road, RoadAspect, Sceneries, Scenery,
    Shoulder, Shoulders, Stage, Theme, Track,
};

const TEST_LANE: Lane = Lane {
    style: theme::LANE_ASPHALT_STANDARD,
    own_min_speed: 0.0,
    own_max_speed: 150.0,
    passive_decel: 0.0,
    traffic_min_speed: 40.0,
    traffic_max_speed: 90.0,
    traffic_density: 0.4,
    traffic_cars: &[
        Car {
            height: 3,
            incidence: 0.4,
        },
        Car {
            height: 5,
            incidence: 0.4,
        },
        Car {
            height: 1,
            incidence: 0.1,
        },
        Car {
            height: 10,
            incidence: 0.1,
        },
    ],
    obstacles: &[
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_SMALL,
            frequency: 0.2,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.2 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_BIG,
            frequency: 0.1,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.5 }],
        },
        Obstacle {
            style: theme::OBSTACLE_SPEED_BUMP,
            frequency: 0.01,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.5 }],
        },
        Obstacle {
            style: theme::OBSTACLE_POTHOLE_CRATER,
            frequency: 0.05,
            effects: &[ObstacleEffect::SpeedChange { affect: -0.9 }],
        },
        Obstacle {
            style: theme::OBSTACLE_SPIKES,
            frequency: 0.01,
            effects: &[ObstacleEffect::Crash],
        },
        Obstacle {
            style: theme::OBSTACLE_FALLEN_TREE,
            frequency: 0.01,
            effects: &[ObstacleEffect::Crash],
        },
    ],
};

pub const TRACK: Track = Track {
    name: "Test",
    author: "odd",
    description: "Test track",
    stages: &[Stage {
        name: "Stage0",
        description: "This is stage0 test.",
        icon: "🏢",
        theme: Theme::Desert,
        distance_km: 20.0,
        road: Road {
            aspect: RoadAspect {
                dividers: URBAN_DIVIDERS,
            },
            lanes: Lanes {
                incoming: &[TEST_LANE, TEST_LANE],
                outgoing: &[TEST_LANE, TEST_LANE],
            },
            sceneries: Sceneries {
                left: Scenery {
                    width: 16,
                    background: theme::SCENERY_CONCRETE,
                    objects: &[Object {
                        style: theme::OBJ_BUILDING_HOUSE,
                        incidence: 1.0,
                    }],
                },
                right: Scenery {
                    width: 16,
                    background: theme::SCENERY_CONCRETE,
                    objects: &[Object {
                        style: theme::OBJ_BUILDING_HOUSE,
                        incidence: 1.0,
                    }],
                },
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
                        style: theme::SHOULDER_COUNTRY_ROAD,
                        repeat: 0,
                    },
                    Shoulder {
                        style: theme::SHOULDER_EMPTY,
                        repeat: 0,
                    },
                    Shoulder {
                        style: theme::SHOULDER_TREE_PALM,
                        repeat: 10,
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
                        style: theme::SHOULDER_RIVER,
                        repeat: 0,
                    },
                    Shoulder {
                        style: theme::SHOULDER_EMPTY,
                        repeat: 0,
                    },
                    Shoulder {
                        style: theme::SHOULDER_TREE_PINE,
                        repeat: 10,
                    },
                    Shoulder {
                        style: theme::SHOULDER_EMPTY,
                        repeat: 0,
                    },
                ],
            },
        },
    }],
    distance_scale: 0.5,
    speed_scale: 1.0,
    lives: 3,
};
