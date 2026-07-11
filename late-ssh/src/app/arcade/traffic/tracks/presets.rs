//! Reusable building blocks for track authoring.
//!
//! Tracks should compose these via struct-update syntax instead of duplicating
//! field-by-field configurations.  Example:
//!
//! ```ignore
//! use crate::app::arcade::traffic::tracks::presets;
//!
//! const FAST_LANE: Lane = Lane {
//!     style: theme::LANE_ASPHALT_PREMIUM,
//!     own_max_speed: 220.0,
//!     ..presets::HIGHWAY_LANE
//! };
//! ```

use crate::app::arcade::traffic::theme;
use crate::app::arcade::traffic::track::{
    Car, Divider, Lane, Lanes, Object, Road, RoadAspect, Sceneries, Scenery, Shoulder, Shoulders,
};

// ─── Car shapes ──────────────────────────────────────────────────────────────

pub const CAR_SEDAN: Car = Car {
    height: 3,
    incidence: 0.45,
};
pub const CAR_HATCHBACK: Car = Car {
    height: 3,
    incidence: 0.25,
};
pub const CAR_VAN: Car = Car {
    height: 5,
    incidence: 0.20,
};
pub const CAR_TRUCK: Car = Car {
    height: 7,
    incidence: 0.08,
};
pub const CAR_SEMI: Car = Car {
    height: 11,
    incidence: 0.02,
};
pub const CAR_PICKUP: Car = Car {
    height: 3,
    incidence: 0.35,
};
pub const CAR_RV: Car = Car {
    height: 7,
    incidence: 0.08,
};
pub const CAR_MICRO: Car = Car {
    height: 2,
    incidence: 0.40,
};
pub const CAR_CART: Car = Car {
    height: 4,
    incidence: 0.30,
};
pub const CAR_MONSTER: Car = Car {
    height: 14,
    incidence: 0.05,
};
pub const CAR_FIGHTER: Car = Car {
    height: 2,
    incidence: 0.45,
};
pub const CAR_FREIGHTER: Car = Car {
    height: 9,
    incidence: 0.20,
};

pub const CITY_CAR_MIX: &[Car] = &[CAR_SEDAN, CAR_HATCHBACK, CAR_VAN];
pub const HIGHWAY_CAR_MIX: &[Car] = &[CAR_SEDAN, CAR_HATCHBACK, CAR_VAN, CAR_TRUCK, CAR_SEMI];
pub const RURAL_CAR_MIX: &[Car] = &[CAR_HATCHBACK, CAR_VAN, CAR_TRUCK];
pub const AMERICAN_CAR_MIX: &[Car] = &[CAR_SEDAN, CAR_PICKUP, CAR_VAN, CAR_RV, CAR_TRUCK];
pub const INTERSTATE_CAR_MIX: &[Car] =
    &[CAR_SEDAN, CAR_PICKUP, CAR_VAN, CAR_TRUCK, CAR_SEMI, CAR_RV];
pub const EURO_CITY_MIX: &[Car] = &[CAR_MICRO, CAR_HATCHBACK, CAR_SEDAN, CAR_VAN];
pub const EURO_HIGHWAY_MIX: &[Car] = &[CAR_SEDAN, CAR_HATCHBACK, CAR_VAN, CAR_TRUCK, CAR_SEMI];
pub const FANTASY_MIX: &[Car] = &[CAR_HATCHBACK, CAR_SEDAN, CAR_CART, CAR_VAN];
pub const SPACE_SHIP_MIX: &[Car] = &[CAR_FIGHTER, CAR_SEDAN, CAR_VAN, CAR_FREIGHTER];
pub const CRAZY_MIX: &[Car] = &[
    CAR_MICRO,
    CAR_HATCHBACK,
    CAR_VAN,
    CAR_TRUCK,
    CAR_SEMI,
    CAR_MONSTER,
];

// ─── Lane templates ──────────────────────────────────────────────────────────

pub const CITY_LANE: Lane = Lane {
    style: theme::LANE_ASPHALT_STANDARD,
    own_min_speed: 0.0,
    own_max_speed: 150.0,
    passive_decel: 0.0,
    traffic_min_speed: 40.0,
    traffic_max_speed: 90.0,
    traffic_density: 0.4,
    traffic_cars: CITY_CAR_MIX,
    obstacles: &[],
};

pub const HIGHWAY_LANE: Lane = Lane {
    style: theme::LANE_ASPHALT_PREMIUM,
    own_min_speed: 50.0,
    own_max_speed: 260.0,
    passive_decel: 0.0,
    traffic_min_speed: 60.0,
    traffic_max_speed: 150.0,
    traffic_density: 0.25,
    traffic_cars: HIGHWAY_CAR_MIX,
    obstacles: &[],
};

pub const RURAL_LANE: Lane = Lane {
    style: theme::LANE_ASPHALT_PATCHY,
    own_min_speed: 0.0,
    own_max_speed: 100.0,
    passive_decel: 2.0,
    traffic_min_speed: 30.0,
    traffic_max_speed: 80.0,
    traffic_density: 0.2,
    traffic_cars: RURAL_CAR_MIX,
    obstacles: &[],
};

pub const FOREST_LANE: Lane = Lane {
    style: theme::LANE_DIRT,
    own_min_speed: 0.0,
    own_max_speed: 70.0,
    passive_decel: 4.0,
    traffic_min_speed: 20.0,
    traffic_max_speed: 50.0,
    traffic_density: 0.1,
    traffic_cars: RURAL_CAR_MIX,
    obstacles: &[],
};

pub const INTERSTATE_LANE: Lane = Lane {
    style: theme::LANE_ASPHALT_PREMIUM,
    own_min_speed: 40.0,
    own_max_speed: 200.0,
    passive_decel: 0.0,
    traffic_min_speed: 65.0,
    traffic_max_speed: 130.0,
    traffic_density: 0.18,
    traffic_cars: INTERSTATE_CAR_MIX,
    obstacles: &[],
};

pub const MOTORWAY_LANE: Lane = Lane {
    style: theme::LANE_ASPHALT_PREMIUM,
    own_min_speed: 60.0,
    own_max_speed: 200.0,
    passive_decel: 0.0,
    traffic_min_speed: 80.0,
    traffic_max_speed: 140.0,
    traffic_density: 0.22,
    traffic_cars: EURO_HIGHWAY_MIX,
    obstacles: &[],
};

pub const AUTOBAHN_LANE: Lane = Lane {
    style: theme::LANE_ASPHALT_PREMIUM,
    own_min_speed: 0.0,
    own_max_speed: 340.0,
    passive_decel: 0.0,
    traffic_min_speed: 100.0,
    traffic_max_speed: 220.0,
    traffic_density: 0.12,
    traffic_cars: EURO_HIGHWAY_MIX,
    obstacles: &[],
};

pub const COBBLE_LANE: Lane = Lane {
    style: theme::LANE_COBBLESTONE,
    own_min_speed: 0.0,
    own_max_speed: 60.0,
    passive_decel: 2.0,
    traffic_min_speed: 18.0,
    traffic_max_speed: 50.0,
    traffic_density: 0.4,
    traffic_cars: EURO_CITY_MIX,
    obstacles: &[],
};

pub const MOUNTAIN_LANE: Lane = Lane {
    style: theme::LANE_ASPHALT_PATCHY,
    own_min_speed: 0.0,
    own_max_speed: 80.0,
    passive_decel: 3.0,
    traffic_min_speed: 20.0,
    traffic_max_speed: 60.0,
    traffic_density: 0.10,
    traffic_cars: RURAL_CAR_MIX,
    obstacles: &[],
};

pub const DESERT_LANE: Lane = Lane {
    style: theme::LANE_ASPHALT_CRACKED,
    own_min_speed: 0.0,
    own_max_speed: 140.0,
    passive_decel: 1.0,
    traffic_min_speed: 40.0,
    traffic_max_speed: 95.0,
    traffic_density: 0.1,
    traffic_cars: AMERICAN_CAR_MIX,
    obstacles: &[],
};

pub const PLAINS_LANE: Lane = Lane {
    style: theme::LANE_ASPHALT_STANDARD,
    own_min_speed: 0.0,
    own_max_speed: 160.0,
    passive_decel: 0.0,
    traffic_min_speed: 60.0,
    traffic_max_speed: 110.0,
    traffic_density: 0.10,
    traffic_cars: AMERICAN_CAR_MIX,
    obstacles: &[],
};

pub const CRYSTAL_LANE: Lane = Lane {
    style: theme::LANE_CRYSTAL,
    own_min_speed: 0.0,
    own_max_speed: 120.0,
    passive_decel: 0.0,
    traffic_min_speed: 25.0,
    traffic_max_speed: 80.0,
    traffic_density: 0.20,
    traffic_cars: FANTASY_MIX,
    obstacles: &[],
};

pub const SPACE_LANE: Lane = Lane {
    style: theme::LANE_SPACE,
    own_min_speed: 0.0,
    own_max_speed: 320.0,
    passive_decel: 0.0,
    traffic_min_speed: 70.0,
    traffic_max_speed: 210.0,
    traffic_density: 0.1,
    traffic_cars: SPACE_SHIP_MIX,
    obstacles: &[],
};

pub const TURBO_LANE: Lane = Lane {
    style: theme::LANE_NEON,
    own_min_speed: 0.0,
    own_max_speed: 600.0,
    passive_decel: 0.0,
    traffic_min_speed: 120.0,
    traffic_max_speed: 420.0,
    traffic_density: 0.1,
    traffic_cars: CRAZY_MIX,
    obstacles: &[],
};

pub const GRIDLOCK_LANE: Lane = Lane {
    style: theme::LANE_ASPHALT_STANDARD,
    own_min_speed: 0.0,
    own_max_speed: 28.0,
    passive_decel: 4.0,
    traffic_min_speed: 4.0,
    traffic_max_speed: 22.0,
    traffic_density: 0.8,
    traffic_cars: CRAZY_MIX,
    obstacles: &[],
};

// ─── Dividers ────────────────────────────────────────────────────────────────

pub const URBAN_DIVIDERS: Divider = Divider {
    primary: theme::DIV_YELLOW_SINGLE,
    lane: theme::DIV_WHITE_DASH,
};
pub const HIGHWAY_DIVIDERS: Divider = Divider {
    primary: theme::DIV_YELLOW_DOUBLE,
    lane: theme::DIV_WHITE_DASH,
};
pub const RURAL_DIVIDERS: Divider = Divider {
    primary: theme::DIV_YELLOW_DASH,
    lane: theme::DIV_FAINT,
};
pub const FOREST_DIVIDERS: Divider = Divider {
    primary: theme::DIV_FAINT,
    lane: theme::DIV_NONE,
};

// ─── Scenery objects ─────────────────────────────────────────────────────────

pub const CITY_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_BUILDING_HOUSE,
        incidence: 0.35,
    },
    Object {
        style: theme::OBJ_BUILDING_APARTMENTS,
        incidence: 0.25,
    },
    Object {
        style: theme::OBJ_SKYSCRAPER,
        incidence: 0.10,
    },
    Object {
        style: theme::OBJ_TREE_OAK,
        incidence: 0.20,
    },
    Object {
        style: theme::OBJ_BUSH,
        incidence: 0.10,
    },
];

pub const TOWN_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_BUILDING_HOUSE,
        incidence: 0.35,
    },
    Object {
        style: theme::OBJ_TREE_OAK,
        incidence: 0.20,
    },
    Object {
        style: theme::OBJ_GRASS,
        incidence: 0.20,
    },
    Object {
        style: theme::OBJ_BUSH,
        incidence: 0.10,
    },
];

pub const HIGHWAY_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_TREE_PINE,
        incidence: 0.50,
    },
    Object {
        style: theme::OBJ_TREE_OAK,
        incidence: 0.25,
    },
    Object {
        style: theme::OBJ_BUSH,
        incidence: 0.20,
    },
    Object {
        style: theme::OBJ_FLOWER,
        incidence: 0.05,
    },
];

pub const RURAL_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_TREE_OAK,
        incidence: 0.45,
    },
    Object {
        style: theme::OBJ_BUSH,
        incidence: 0.25,
    },
    Object {
        style: theme::OBJ_GRASS,
        incidence: 0.20,
    },
    Object {
        style: theme::OBJ_FLOWER,
        incidence: 0.10,
    },
];

pub const VILLAGE_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_BUSH,
        incidence: 0.25,
    },
    Object {
        style: theme::OBJ_GRASS,
        incidence: 0.20,
    },
    Object {
        style: theme::OBJ_FLOWER,
        incidence: 0.10,
    },
    Object {
        style: theme::OBJ_BUILDING_HOUSE,
        incidence: 0.20,
    },
];

pub const FOREST_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_TREE_PINE,
        incidence: 0.80,
    },
    Object {
        style: theme::OBJ_BUSH,
        incidence: 0.20,
    },
];

pub const DESERT_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_GRASS,
        incidence: 0.40,
    },
    Object {
        style: theme::OBJ_BUSH,
        incidence: 0.40,
    },
    Object {
        style: theme::OBJ_TREE_PALM,
        incidence: 0.20,
    },
];

pub const PLAINS_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_GRASS,
        incidence: 0.45,
    },
    Object {
        style: theme::OBJ_WINDMILL,
        incidence: 0.20,
    },
    Object {
        style: theme::OBJ_BARN,
        incidence: 0.15,
    },
    Object {
        style: theme::OBJ_BILLBOARD,
        incidence: 0.10,
    },
    Object {
        style: theme::OBJ_BUSH,
        incidence: 0.10,
    },
];

pub const SOUTHWESTERN_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_CACTUS,
        incidence: 0.45,
    },
    Object {
        style: theme::OBJ_ROCK,
        incidence: 0.30,
    },
    Object {
        style: theme::OBJ_BUSH,
        incidence: 0.20,
    },
    Object {
        style: theme::OBJ_GRASS,
        incidence: 0.05,
    },
];

pub const MOUNTAIN_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_ROCK,
        incidence: 0.55,
    },
    Object {
        style: theme::OBJ_TREE_PINE,
        incidence: 0.30,
    },
    Object {
        style: theme::OBJ_BUSH,
        incidence: 0.15,
    },
];

pub const ALPINE_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_ROCK,
        incidence: 0.65,
    },
    Object {
        style: theme::OBJ_TREE_PINE,
        incidence: 0.25,
    },
    Object {
        style: theme::OBJ_BUSH,
        incidence: 0.10,
    },
];

pub const COASTAL_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_TREE_PALM,
        incidence: 0.40,
    },
    Object {
        style: theme::OBJ_FLOWER,
        incidence: 0.35,
    },
    Object {
        style: theme::OBJ_BUSH,
        incidence: 0.25,
    },
];

pub const EURO_VILLAGE_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_BUILDING_HOUSE,
        incidence: 0.30,
    },
    Object {
        style: theme::OBJ_CHURCH,
        incidence: 0.15,
    },
    Object {
        style: theme::OBJ_VINEYARD,
        incidence: 0.25,
    },
    Object {
        style: theme::OBJ_TREE_OAK,
        incidence: 0.20,
    },
    Object {
        style: theme::OBJ_BUSH,
        incidence: 0.10,
    },
];

pub const MAGIC_FOREST_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_MUSHROOM,
        incidence: 0.28,
    },
    Object {
        style: theme::OBJ_TREE_OAK,
        incidence: 0.30,
    },
    Object {
        style: theme::OBJ_CRYSTAL_SPIKE,
        incidence: 0.22,
    },
    Object {
        style: theme::OBJ_BUSH,
        incidence: 0.20,
    },
];

pub const DARK_REALM_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_DARK_TOWER,
        incidence: 0.22,
    },
    Object {
        style: theme::OBJ_CRYSTAL_SPIKE,
        incidence: 0.35,
    },
    Object {
        style: theme::OBJ_ROCK,
        incidence: 0.25,
    },
    Object {
        style: theme::OBJ_STAR,
        incidence: 0.18,
    },
];

pub const CRYSTAL_CAVE_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_CRYSTAL_SPIKE,
        incidence: 0.65,
    },
    Object {
        style: theme::OBJ_STAR,
        incidence: 0.25,
    },
    Object {
        style: theme::OBJ_ROCK,
        incidence: 0.10,
    },
];

pub const LAVA_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_ROCK,
        incidence: 0.70,
    },
    Object {
        style: theme::OBJ_CRYSTAL_SPIKE,
        incidence: 0.30,
    },
];

pub const SPACE_OBJECTS: &[Object] = &[
    Object {
        style: theme::OBJ_STAR,
        incidence: 0.55,
    },
    Object {
        style: theme::OBJ_NEBULA,
        incidence: 0.20,
    },
    Object {
        style: theme::OBJ_PLANET_SMALL,
        incidence: 0.12,
    },
    Object {
        style: theme::OBJ_COMET,
        incidence: 0.13,
    },
];

// ─── Sceneries ───────────────────────────────────────────────────────────────

pub const CITY_SCENERY: Scenery = Scenery {
    width: 14,
    background: theme::SCENERY_CONCRETE,
    objects: CITY_OBJECTS,
};
pub const TOWN_SCENERY: Scenery = Scenery {
    width: 14,
    background: theme::SCENERY_CONCRETE,
    objects: TOWN_OBJECTS,
};
pub const HIGHWAY_SCENERY: Scenery = Scenery {
    width: 12,
    background: theme::SCENERY_GRASS,
    objects: HIGHWAY_OBJECTS,
};
pub const RURAL_SCENERY: Scenery = Scenery {
    width: 10,
    background: theme::SCENERY_GRASS,
    objects: RURAL_OBJECTS,
};
pub const VILLAGE_SCENERY: Scenery = Scenery {
    width: 12,
    background: theme::SCENERY_GRASS,
    objects: VILLAGE_OBJECTS,
};
pub const FOREST_SCENERY: Scenery = Scenery {
    width: 14,
    background: theme::SCENERY_DIRT,
    objects: FOREST_OBJECTS,
};
pub const DESERT_SCENERY: Scenery = Scenery {
    width: 12,
    background: theme::SCENERY_DIRT,
    objects: DESERT_OBJECTS,
};
pub const PLAINS_SCENERY: Scenery = Scenery {
    width: 12,
    background: theme::SCENERY_GRASS,
    objects: PLAINS_OBJECTS,
};
pub const SOUTHWESTERN_SCENERY: Scenery = Scenery {
    width: 12,
    background: theme::SCENERY_SAND,
    objects: SOUTHWESTERN_OBJECTS,
};
pub const MOUNTAIN_SCENERY: Scenery = Scenery {
    width: 12,
    background: theme::SCENERY_DIRT,
    objects: MOUNTAIN_OBJECTS,
};
pub const ALPINE_SCENERY: Scenery = Scenery {
    width: 12,
    background: theme::SCENERY_SNOW,
    objects: ALPINE_OBJECTS,
};
pub const COASTAL_SCENERY: Scenery = Scenery {
    width: 12,
    background: theme::SCENERY_GRASS,
    objects: COASTAL_OBJECTS,
};
pub const TUNNEL_SCENERY: Scenery = Scenery {
    width: 4,
    background: theme::SCENERY_VOID,
    objects: &[],
};
pub const EURO_VILLAGE_SCENERY: Scenery = Scenery {
    width: 12,
    background: theme::SCENERY_GRASS,
    objects: EURO_VILLAGE_OBJECTS,
};
pub const MAGIC_FOREST_SCENERY: Scenery = Scenery {
    width: 12,
    background: theme::SCENERY_MAGIC,
    objects: MAGIC_FOREST_OBJECTS,
};
pub const DARK_REALM_SCENERY: Scenery = Scenery {
    width: 10,
    background: theme::SCENERY_MAGIC,
    objects: DARK_REALM_OBJECTS,
};
pub const CRYSTAL_CAVE_SCENERY: Scenery = Scenery {
    width: 8,
    background: theme::SCENERY_VOID,
    objects: CRYSTAL_CAVE_OBJECTS,
};
pub const LAVA_SCENERY: Scenery = Scenery {
    width: 10,
    background: theme::SCENERY_LAVA,
    objects: LAVA_OBJECTS,
};
pub const STARFIELD_SCENERY: Scenery = Scenery {
    width: 14,
    background: theme::SCENERY_STARFIELD,
    objects: SPACE_OBJECTS,
};

// ─── Shoulders ───────────────────────────────────────────────────────────────

pub const SIDEWALK_SHOULDERS: &[Shoulder] = &[
    Shoulder {
        style: theme::SHOULDER_HARD_EDGE,
        repeat: 0,
    },
    Shoulder {
        style: theme::SHOULDER_SIDEWALK,
        repeat: 0,
    },
];

pub const HIGHWAY_SHOULDERS: &[Shoulder] = &[
    Shoulder {
        style: theme::SHOULDER_HARD_EDGE,
        repeat: 0,
    },
    Shoulder {
        style: theme::SHOULDER_POLES,
        repeat: 6,
    },
];

pub const RURAL_SHOULDERS: &[Shoulder] = &[Shoulder {
    style: theme::SHOULDER_SOFT_EDGE,
    repeat: 0,
}];

pub const FOREST_SHOULDERS: &[Shoulder] = &[
    Shoulder {
        style: theme::SHOULDER_SOFT_EDGE,
        repeat: 0,
    },
    Shoulder {
        style: theme::SHOULDER_TREE_PINE,
        repeat: 4,
    },
];

pub const NO_SHOULDERS: Shoulders = Shoulders {
    left: &[],
    right: &[],
};

pub const GUARDRAIL_SHOULDERS: &[Shoulder] = &[Shoulder {
    style: theme::SHOULDER_GUARDRAIL,
    repeat: 0,
}];

pub const CRASH_BARRIER_SHOULDERS: &[Shoulder] = &[
    Shoulder {
        style: theme::SHOULDER_CRASH_BARRIER,
        repeat: 0,
    },
    Shoulder {
        style: theme::SHOULDER_EMPTY,
        repeat: 0,
    },
];

pub const MAGIC_RUNE_SHOULDERS: &[Shoulder] = &[
    Shoulder {
        style: theme::SHOULDER_MAGIC_RUNE,
        repeat: 10,
    },
    Shoulder {
        style: theme::SHOULDER_EMPTY,
        repeat: 0,
    },
];

pub const SPACE_BEACON_SHOULDERS: &[Shoulder] = &[Shoulder {
    style: theme::SHOULDER_SPACE_BEACON,
    repeat: 0,
}];

pub const NEON_BARRIER_SHOULDERS: &[Shoulder] = &[Shoulder {
    style: theme::SHOULDER_NEON_BARRIER,
    repeat: 0,
}];

// ─── Pre-built Road configurations (builder framework) ──────────────────────
//
// Use struct-update syntax to customise scenery or lanes while reusing the
// divider, shoulder, and base lane configuration.  Example:
//
//   road: Road {
//       sceneries: Sceneries { left: FOREST_SCENERY, right: MOUNTAIN_SCENERY },
//       ..ROAD_HIGHWAY_2X2
//   }

pub const ROAD_CITY_2X2: Road = Road {
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

pub const ROAD_TOWN_2X2: Road = Road {
    aspect: RoadAspect {
        dividers: URBAN_DIVIDERS,
    },
    lanes: Lanes {
        incoming: &[CITY_LANE, CITY_LANE],
        outgoing: &[CITY_LANE, CITY_LANE],
    },
    sceneries: Sceneries {
        left: TOWN_SCENERY,
        right: TOWN_SCENERY,
    },
    shoulders: Shoulders {
        left: SIDEWALK_SHOULDERS,
        right: SIDEWALK_SHOULDERS,
    },
};

pub const ROAD_HIGHWAY_2X2: Road = Road {
    aspect: RoadAspect {
        dividers: HIGHWAY_DIVIDERS,
    },
    lanes: Lanes {
        incoming: &[HIGHWAY_LANE, HIGHWAY_LANE],
        outgoing: &[HIGHWAY_LANE, HIGHWAY_LANE],
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

pub const ROAD_HIGHWAY_3X2: Road = Road {
    lanes: Lanes {
        incoming: &[HIGHWAY_LANE, HIGHWAY_LANE],
        outgoing: &[HIGHWAY_LANE, HIGHWAY_LANE, HIGHWAY_LANE],
    },
    ..ROAD_HIGHWAY_2X2
};

pub const ROAD_RURAL_1X1: Road = Road {
    aspect: RoadAspect {
        dividers: RURAL_DIVIDERS,
    },
    lanes: Lanes {
        incoming: &[RURAL_LANE],
        outgoing: &[RURAL_LANE],
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

pub const ROAD_FOREST_1X1: Road = Road {
    aspect: RoadAspect {
        dividers: FOREST_DIVIDERS,
    },
    lanes: Lanes {
        incoming: &[FOREST_LANE],
        outgoing: &[FOREST_LANE],
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

pub const ROAD_MOTORWAY_2X2: Road = Road {
    aspect: RoadAspect {
        dividers: HIGHWAY_DIVIDERS,
    },
    lanes: Lanes {
        incoming: &[MOTORWAY_LANE, MOTORWAY_LANE],
        outgoing: &[MOTORWAY_LANE, MOTORWAY_LANE],
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

pub const ROAD_MOTORWAY_3X2: Road = Road {
    lanes: Lanes {
        incoming: &[MOTORWAY_LANE, MOTORWAY_LANE],
        outgoing: &[MOTORWAY_LANE, MOTORWAY_LANE, MOTORWAY_LANE],
    },
    ..ROAD_MOTORWAY_2X2
};

pub const ROAD_INTERSTATE_2X2: Road = Road {
    aspect: RoadAspect {
        dividers: HIGHWAY_DIVIDERS,
    },
    lanes: Lanes {
        incoming: &[INTERSTATE_LANE, INTERSTATE_LANE],
        outgoing: &[INTERSTATE_LANE, INTERSTATE_LANE],
    },
    sceneries: Sceneries {
        left: PLAINS_SCENERY,
        right: PLAINS_SCENERY,
    },
    shoulders: Shoulders {
        left: HIGHWAY_SHOULDERS,
        right: HIGHWAY_SHOULDERS,
    },
};

pub const ROAD_MOUNTAIN_1X1: Road = Road {
    aspect: RoadAspect {
        dividers: RURAL_DIVIDERS,
    },
    lanes: Lanes {
        incoming: &[MOUNTAIN_LANE],
        outgoing: &[MOUNTAIN_LANE],
    },
    sceneries: Sceneries {
        left: MOUNTAIN_SCENERY,
        right: MOUNTAIN_SCENERY,
    },
    shoulders: Shoulders {
        left: GUARDRAIL_SHOULDERS,
        right: GUARDRAIL_SHOULDERS,
    },
};

pub const ROAD_CRYSTAL_1X1: Road = Road {
    aspect: RoadAspect {
        dividers: FOREST_DIVIDERS,
    },
    lanes: Lanes {
        incoming: &[CRYSTAL_LANE],
        outgoing: &[CRYSTAL_LANE],
    },
    sceneries: Sceneries {
        left: CRYSTAL_CAVE_SCENERY,
        right: CRYSTAL_CAVE_SCENERY,
    },
    shoulders: Shoulders {
        left: MAGIC_RUNE_SHOULDERS,
        right: MAGIC_RUNE_SHOULDERS,
    },
};

pub const ROAD_SPACE_2X2: Road = Road {
    aspect: RoadAspect {
        dividers: FOREST_DIVIDERS,
    },
    lanes: Lanes {
        incoming: &[SPACE_LANE, SPACE_LANE],
        outgoing: &[SPACE_LANE, SPACE_LANE],
    },
    sceneries: Sceneries {
        left: STARFIELD_SCENERY,
        right: STARFIELD_SCENERY,
    },
    shoulders: Shoulders {
        left: SPACE_BEACON_SHOULDERS,
        right: SPACE_BEACON_SHOULDERS,
    },
};

pub const ROAD_SPACE_3X2: Road = Road {
    lanes: Lanes {
        incoming: &[SPACE_LANE, SPACE_LANE],
        outgoing: &[SPACE_LANE, SPACE_LANE, SPACE_LANE],
    },
    ..ROAD_SPACE_2X2
};
