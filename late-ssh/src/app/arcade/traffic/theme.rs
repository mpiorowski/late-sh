//! Standard style library for the Traffic game.
//!
//! Every `pub const` in this module is a ready-made instance of one of the
//! style descriptor types defined in `track.rs`.  Track authors import these by
//! name and compose them via struct-update syntax or inline overrides.
//!
//! Adding a **new standard look** means adding a constant here.
//! Adding a **new [`Theme`] variant** means adding a branch to `tint` and to
//! `Theme::name` / `Theme::icon` in `track.rs`.
//!
//! Nothing in this file is mandatory.  A track can define its own style
//! instances inline without touching this file at all.

use ratatui::style::Color;

use super::track::{
    Cell, DividerStyle, LaneStyle, ObjectStyle, ObstacleStyle, SceneryStyle, ShoulderStyle, Sprite,
    Theme,
};

// ─── Theme tint ──────────────────────────────────────────────────────────────

/// Multiplicative colour tint for the active theme.  Applied inside each style
/// fn so callers never have to think about it.
pub fn tint(c: Color, theme: Theme) -> Color {
    let Color::Rgb(r, g, b) = c else {
        return c;
    };
    match theme {
        Theme::Standard => c,
        Theme::Winter => Color::Rgb(
            ((r as u16 + 60).min(255)) as u8,
            ((g as u16 + 60).min(255)) as u8,
            ((b as u16 + 80).min(255)) as u8,
        ),
        Theme::Desert => Color::Rgb(
            ((r as u16 * 11 / 10).min(255)) as u8,
            ((g as u16 * 9 / 10).min(255)) as u8,
            ((b as u16 * 7 / 10).min(255)) as u8,
        ),
    }
}

// ─── Trunk colour (shared by tree-bearing ObjectStyles) ─────────────────────

const TRUNK_BROWN: Color = Color::Rgb(110, 70, 30);

pub fn trunk_color(theme: Theme) -> Color {
    tint(TRUNK_BROWN, theme)
}

// ─── Stage icons ─────────────────────────────────────────────────────────────

pub const STAGE_METROPOLIS: &str = "🏙";
pub const STAGE_CITY: &str = "🏢";
pub const STAGE_CITY_OUTSKIRTS: &str = "🏘";
pub const STAGE_VILLAGE: &str = "🏡";
pub const STAGE_HIGHWAY: &str = "🛣";
pub const STAGE_WILD_PLAINS: &str = "🌾";
pub const STAGE_WILD_HILLS: &str = "⛰";
pub const STAGE_WILD_FOREST: &str = "🌲";
pub const STAGE_SLOPE_UP: &str = "↗";
pub const STAGE_SLOPE_DOWN: &str = "↘";
pub const STAGE_SPECIAL: &str = "★";
pub const STAGE_DESERT: &str = "🏜";
pub const STAGE_MOUNTAIN: &str = "🏔";
pub const STAGE_COASTAL: &str = "🌊";
pub const STAGE_BRIDGE: &str = "🌉";
pub const STAGE_TUNNEL: &str = "🕳";
pub const STAGE_CASTLE: &str = "🏰";
pub const STAGE_DRAGON: &str = "🐉";
pub const STAGE_MOON: &str = "🌙";
pub const STAGE_PLANET: &str = "🪐";
pub const STAGE_GALAXY: &str = "🌌";
pub const STAGE_CHAOS: &str = "💀";

// ─── Lane styles ─────────────────────────────────────────────────────────────

const ASPHALT_BG: Color = Color::Rgb(18, 18, 18);
const ASPHALT_PREMIUM_BG: Color = Color::Rgb(28, 28, 32);
const ASPHALT_PATCHY_BG: Color = Color::Rgb(22, 20, 18);
const ASPHALT_PATCH_FG: Color = Color::Rgb(70, 65, 60);
const GRAVEL_BG: Color = Color::Rgb(70, 60, 48);
const GRAVEL_FG: Color = Color::Rgb(120, 105, 90);
const DIRT_BG: Color = Color::Rgb(82, 56, 36);
const DIRT_FG: Color = Color::Rgb(120, 90, 60);
const LANE_GRASS_BG: Color = Color::Rgb(28, 60, 28);
const LANE_GRASS_FG: Color = Color::Rgb(70, 110, 60);

pub const LANE_ASPHALT_PREMIUM: LaneStyle = LaneStyle {
    cell: |theme, _row, _col| {
        let bg = tint(ASPHALT_PREMIUM_BG, theme);
        Cell::new(" ", bg, bg)
    },
    bg: |theme| tint(ASPHALT_PREMIUM_BG, theme),
};

pub const LANE_ASPHALT_STANDARD: LaneStyle = LaneStyle {
    cell: |theme, _row, _col| {
        let bg = tint(ASPHALT_BG, theme);
        Cell::new(" ", bg, bg)
    },
    bg: |theme| tint(ASPHALT_BG, theme),
};

pub const LANE_ASPHALT_PATCHY: LaneStyle = LaneStyle {
    cell: |theme, row, _col| {
        let bg = tint(ASPHALT_PATCHY_BG, theme);
        if row.rem_euclid(7) == 0 {
            Cell::new(".", tint(ASPHALT_PATCH_FG, theme), bg)
        } else {
            Cell::new(" ", bg, bg)
        }
    },
    bg: |theme| tint(ASPHALT_PATCHY_BG, theme),
};

pub const LANE_GRAVEL: LaneStyle = LaneStyle {
    cell: |theme, row, _col| {
        let bg = tint(GRAVEL_BG, theme);
        if row.rem_euclid(3) == 0 {
            Cell::new(",", tint(GRAVEL_FG, theme), bg)
        } else {
            Cell::new(" ", bg, bg)
        }
    },
    bg: |theme| tint(GRAVEL_BG, theme),
};

pub const LANE_DIRT: LaneStyle = LaneStyle {
    cell: |theme, row, _col| {
        let bg = tint(DIRT_BG, theme);
        if row.rem_euclid(5) < 2 {
            Cell::new("·", tint(DIRT_FG, theme), bg)
        } else {
            Cell::new(" ", bg, bg)
        }
    },
    bg: |theme| tint(DIRT_BG, theme),
};

pub const LANE_GRASS: LaneStyle = LaneStyle {
    cell: |theme, row, _col| {
        let bg = tint(LANE_GRASS_BG, theme);
        if row.rem_euclid(6) == 0 {
            Cell::new("v", tint(LANE_GRASS_FG, theme), bg)
        } else {
            Cell::new(" ", bg, bg)
        }
    },
    bg: |theme| tint(LANE_GRASS_BG, theme),
};

const COBBLE_BG: Color = Color::Rgb(75, 70, 65);
const COBBLE_FG: Color = Color::Rgb(130, 118, 106);
const CRACKED_BG: Color = Color::Rgb(17, 15, 13);
const CRACK_FG: Color = Color::Rgb(60, 52, 44);

pub const LANE_COBBLESTONE: LaneStyle = LaneStyle {
    cell: |theme, row, col| {
        let bg = tint(COBBLE_BG, theme);
        if (row.wrapping_add(col as i32)).rem_euclid(3) == 0 {
            Cell::new("▪", tint(COBBLE_FG, theme), bg)
        } else {
            Cell::new(" ", bg, bg)
        }
    },
    bg: |theme| tint(COBBLE_BG, theme),
};

pub const LANE_ASPHALT_CRACKED: LaneStyle = LaneStyle {
    cell: |theme, row, col| {
        let bg = tint(CRACKED_BG, theme);
        let c = col as i32;
        if (row.rem_euclid(6) == 0 && c.rem_euclid(7) < 2)
            || (row.rem_euclid(11) == 4 && c.rem_euclid(5) == 0)
        {
            Cell::new("╌", tint(CRACK_FG, theme), bg)
        } else {
            Cell::new(" ", bg, bg)
        }
    },
    bg: |theme| tint(CRACKED_BG, theme),
};

const CRYSTAL_LANE_BG: Color = Color::Rgb(12, 28, 52);
const CRYSTAL_LANE_FG: Color = Color::Rgb(85, 190, 230);
const SPACE_ROAD_BG: Color = Color::Rgb(5, 7, 20);
const SPACE_ROAD_FG: Color = Color::Rgb(22, 30, 70);
const NEON_ROAD_BG: Color = Color::Rgb(5, 3, 20);
const NEON_ROAD_FG: Color = Color::Rgb(0, 240, 160);

pub const LANE_CRYSTAL: LaneStyle = LaneStyle {
    cell: |theme, row, col| {
        let bg = tint(CRYSTAL_LANE_BG, theme);
        if (row + col as i32).rem_euclid(8) == 0 {
            Cell::new("✧", tint(CRYSTAL_LANE_FG, theme), bg)
        } else {
            Cell::new(" ", bg, bg)
        }
    },
    bg: |theme| tint(CRYSTAL_LANE_BG, theme),
};

pub const LANE_SPACE: LaneStyle = LaneStyle {
    cell: |theme, row, col| {
        let bg = tint(SPACE_ROAD_BG, theme);
        if row.rem_euclid(10) == 0 && (col as i32).rem_euclid(4) == 0 {
            Cell::new("·", tint(SPACE_ROAD_FG, theme), bg)
        } else {
            Cell::new(" ", bg, bg)
        }
    },
    bg: |theme| tint(SPACE_ROAD_BG, theme),
};

pub const LANE_NEON: LaneStyle = LaneStyle {
    cell: |theme, row, _col| {
        let bg = tint(NEON_ROAD_BG, theme);
        if row.rem_euclid(6) == 0 {
            Cell::new("·", tint(NEON_ROAD_FG, theme), bg)
        } else {
            Cell::new(" ", bg, bg)
        }
    },
    bg: |theme| tint(NEON_ROAD_BG, theme),
};

// ─── Divider styles ──────────────────────────────────────────────────────────

const YELLOW: Color = Color::Rgb(220, 180, 30);
const WHITE: Color = Color::Rgb(180, 180, 180);
const FAINT: Color = Color::Rgb(85, 85, 85);

pub const DIV_YELLOW_DOUBLE: DividerStyle = DividerStyle {
    cell: |_row, bg| Cell::new("‖", YELLOW, bg),
};
pub const DIV_YELLOW_SINGLE: DividerStyle = DividerStyle {
    cell: |_row, bg| Cell::new("│", YELLOW, bg),
};
pub const DIV_YELLOW_DASH: DividerStyle = DividerStyle {
    cell: |row, bg| {
        let sym = if row.rem_euclid(4) < 2 { "│" } else { " " };
        Cell::new(sym, YELLOW, bg)
    },
};
pub const DIV_YELLOW_DOTS: DividerStyle = DividerStyle {
    cell: |row, bg| {
        let sym = if row.rem_euclid(3) == 0 { "·" } else { " " };
        Cell::new(sym, YELLOW, bg)
    },
};
pub const DIV_WHITE_SINGLE: DividerStyle = DividerStyle {
    cell: |_row, bg| Cell::new("│", WHITE, bg),
};
pub const DIV_WHITE_DASH: DividerStyle = DividerStyle {
    cell: |row, bg| {
        let sym = if row.rem_euclid(4) < 2 { "│" } else { " " };
        Cell::new(sym, WHITE, bg)
    },
};
pub const DIV_WHITE_DOTS: DividerStyle = DividerStyle {
    cell: |row, bg| {
        let sym = if row.rem_euclid(3) == 0 { "·" } else { " " };
        Cell::new(sym, WHITE, bg)
    },
};
pub const DIV_FAINT: DividerStyle = DividerStyle {
    cell: |_row, bg| Cell::new("│", FAINT, bg),
};
pub const DIV_NONE: DividerStyle = DividerStyle {
    cell: |_row, bg| Cell::new(" ", bg, bg),
};

// ─── Scenery background styles ───────────────────────────────────────────────

const CONCRETE_BG: Color = Color::Rgb(60, 60, 60);
const SC_GRASS_BG: Color = Color::Rgb(20, 55, 20);
const SC_DIRT_BG: Color = Color::Rgb(70, 50, 30);
const VOID_BG: Color = Color::Rgb(2, 2, 6);
const SAND_BG: Color = Color::Rgb(175, 150, 80);
const SNOW_BG: Color = Color::Rgb(195, 208, 218);
const WATER_BG: Color = Color::Rgb(28, 78, 155);

pub const SCENERY_CONCRETE: SceneryStyle = SceneryStyle {
    bg: |theme| tint(CONCRETE_BG, theme),
};
pub const SCENERY_GRASS: SceneryStyle = SceneryStyle {
    bg: |theme| tint(SC_GRASS_BG, theme),
};
pub const SCENERY_DIRT: SceneryStyle = SceneryStyle {
    bg: |theme| tint(SC_DIRT_BG, theme),
};
pub const SCENERY_VOID: SceneryStyle = SceneryStyle {
    bg: |theme| tint(VOID_BG, theme),
};
pub const SCENERY_SAND: SceneryStyle = SceneryStyle {
    bg: |theme| tint(SAND_BG, theme),
};
pub const SCENERY_SNOW: SceneryStyle = SceneryStyle {
    bg: |theme| tint(SNOW_BG, theme),
};
pub const SCENERY_WATER: SceneryStyle = SceneryStyle {
    bg: |theme| tint(WATER_BG, theme),
};

const MAGIC_BG: Color = Color::Rgb(42, 12, 68);
const LAVA_BG: Color = Color::Rgb(95, 28, 8);
const STARFIELD_BG: Color = Color::Rgb(3, 3, 12);

pub const SCENERY_MAGIC: SceneryStyle = SceneryStyle {
    bg: |theme| tint(MAGIC_BG, theme),
};
pub const SCENERY_LAVA: SceneryStyle = SceneryStyle {
    bg: |theme| tint(LAVA_BG, theme),
};
pub const SCENERY_STARFIELD: SceneryStyle = SceneryStyle {
    bg: |theme| tint(STARFIELD_BG, theme),
};

// ─── Object styles ───────────────────────────────────────────────────────────

const TREE_PINE_GREEN: Color = Color::Rgb(55, 175, 55);
const TREE_OAK_GREEN: Color = Color::Rgb(80, 160, 70);
const TREE_PALM_GREEN: Color = Color::Rgb(120, 180, 70);
const FLOWER_PINK: Color = Color::Rgb(200, 110, 160);
const BUSH_GREEN: Color = Color::Rgb(70, 130, 60);
const GRASS_BLADE: Color = Color::Rgb(70, 130, 70);
const BUILDING_GRAY: Color = Color::Rgb(140, 140, 140);
const STAR_WHITE: Color = Color::Rgb(220, 220, 220);

pub const OBJ_GRASS: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 1,
        glyphs: &[&["ʷ"]],
        fg: tint(GRASS_BLADE, theme),
    },
    has_trunk: false,
};
pub const OBJ_BUSH: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 1,
        glyphs: &[&["❀"]],
        fg: tint(BUSH_GREEN, theme),
    },
    has_trunk: false,
};
pub const OBJ_TREE_PINE: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 1,
        glyphs: &[&["▲"], &["▲"], &["│"]],
        fg: tint(TREE_PINE_GREEN, theme),
    },
    has_trunk: true,
};
pub const OBJ_TREE_OAK: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 1,
        glyphs: &[&["◉"], &["│"]],
        fg: tint(TREE_OAK_GREEN, theme),
    },
    has_trunk: true,
};
pub const OBJ_TREE_PALM: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 1,
        glyphs: &[&["✤"], &["│"]],
        fg: tint(TREE_PALM_GREEN, theme),
    },
    has_trunk: true,
};
pub const OBJ_BUILDING_HOUSE: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 3,
        glyphs: &[&["╱", "▔", "╲"], &["│", "░", "│"]],
        fg: tint(BUILDING_GRAY, theme),
    },
    has_trunk: false,
};
pub const OBJ_BUILDING_APARTMENTS: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 3,
        glyphs: &[&["│", "▦", "│"], &["│", "▦", "│"], &["│", "░", "│"]],
        fg: tint(BUILDING_GRAY, theme),
    },
    has_trunk: false,
};
pub const OBJ_SKYSCRAPER: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 3,
        glyphs: &[
            &[" ", "▲", " "],
            &["│", "▦", "│"],
            &["│", "▦", "│"],
            &["│", "▦", "│"],
            &["│", "▦", "│"],
            &["│", "░", "│"],
        ],
        fg: tint(BUILDING_GRAY, theme),
    },
    has_trunk: false,
};
pub const OBJ_FLOWER: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 1,
        glyphs: &[&["✿"]],
        fg: tint(FLOWER_PINK, theme),
    },
    has_trunk: false,
};
pub const OBJ_STAR: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 1,
        glyphs: &[&["✦"]],
        fg: tint(STAR_WHITE, theme),
    },
    has_trunk: false,
};

const CACTUS_GREEN: Color = Color::Rgb(55, 135, 55);
const ROCK_GRAY: Color = Color::Rgb(130, 120, 108);
const BARN_RED: Color = Color::Rgb(175, 55, 45);
const STEEL_FG: Color = Color::Rgb(155, 168, 178);
const CHURCH_STONE: Color = Color::Rgb(158, 152, 142);
const VINE_GREEN: Color = Color::Rgb(65, 135, 48);
const BILLBOARD_FG: Color = Color::Rgb(200, 200, 118);

pub const OBJ_CACTUS: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 1,
        glyphs: &[&["Ψ"], &["│"]],
        fg: tint(CACTUS_GREEN, theme),
    },
    has_trunk: true,
};
pub const OBJ_ROCK: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 2,
        glyphs: &[&["◢", "◣"]],
        fg: tint(ROCK_GRAY, theme),
    },
    has_trunk: false,
};
pub const OBJ_BARN: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 3,
        glyphs: &[&["╱", "▲", "╲"], &["│", "▓", "│"]],
        fg: tint(BARN_RED, theme),
    },
    has_trunk: false,
};
pub const OBJ_WINDMILL: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 1,
        glyphs: &[&["✛"], &["│"]],
        fg: tint(STEEL_FG, theme),
    },
    has_trunk: false,
};
pub const OBJ_CHURCH: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 3,
        glyphs: &[&[" ", "†", " "], &["│", "▦", "│"], &["│", "░", "│"]],
        fg: tint(CHURCH_STONE, theme),
    },
    has_trunk: false,
};
pub const OBJ_VINEYARD: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 1,
        glyphs: &[&["♣"]],
        fg: tint(VINE_GREEN, theme),
    },
    has_trunk: false,
};
pub const OBJ_BILLBOARD: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 3,
        glyphs: &[&["┌", "▬", "┐"], &["└", "─", "┘"]],
        fg: tint(BILLBOARD_FG, theme),
    },
    has_trunk: false,
};

const MUSHROOM_RED: Color = Color::Rgb(200, 55, 45);
const CRYSTAL_OBJ_FG: Color = Color::Rgb(100, 200, 235);
const DARK_TOWER_FG: Color = Color::Rgb(78, 58, 95);
const PLANET_BLUE: Color = Color::Rgb(45, 95, 200);
const NEBULA_FG: Color = Color::Rgb(155, 75, 200);
const COMET_FG: Color = Color::Rgb(235, 235, 170);

pub const OBJ_MUSHROOM: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 2,
        glyphs: &[&["◖", "◗"], &["└", "┘"]],
        fg: tint(MUSHROOM_RED, theme),
    },
    has_trunk: false,
};
pub const OBJ_CRYSTAL_SPIKE: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 1,
        glyphs: &[&["◆"], &["◇"]],
        fg: tint(CRYSTAL_OBJ_FG, theme),
    },
    has_trunk: false,
};
pub const OBJ_DARK_TOWER: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 3,
        glyphs: &[&[" ", "▲", " "], &["▐", "▓", "▌"], &["▐", "█", "▌"]],
        fg: tint(DARK_TOWER_FG, theme),
    },
    has_trunk: false,
};
pub const OBJ_PLANET_SMALL: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 2,
        glyphs: &[&["◑", "◐"]],
        fg: tint(PLANET_BLUE, theme),
    },
    has_trunk: false,
};
pub const OBJ_NEBULA: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 1,
        glyphs: &[&["≋"]],
        fg: tint(NEBULA_FG, theme),
    },
    has_trunk: false,
};
pub const OBJ_COMET: ObjectStyle = ObjectStyle {
    sprite: |theme| Sprite {
        width: 1,
        glyphs: &[&["★"]],
        fg: tint(COMET_FG, theme),
    },
    has_trunk: false,
};

// ─── Shoulder styles ─────────────────────────────────────────────────────────

const SIDEWALK_BG: Color = Color::Rgb(110, 110, 105);
const SIDEWALK_FG: Color = Color::Rgb(170, 170, 165);
const HARDEDGE_FG: Color = Color::Rgb(180, 180, 180);
const SOFTEDGE_FG: Color = Color::Rgb(110, 110, 110);
const POLE_FG: Color = Color::Rgb(160, 160, 160);
const RAIL_FG: Color = Color::Rgb(150, 130, 110);
const RIVER_FG: Color = Color::Rgb(70, 120, 200);
const PARKED_CAR_FG: Color = Color::Rgb(120, 130, 180);
const COUNTRY_FG: Color = Color::Rgb(90, 80, 60);
const TREE_PINE_DIM: Color = Color::Rgb(25, 105, 25);

/// Helper: returns the transparent-background cell used when a shoulder is
/// off-row or intentionally empty.
#[inline]
fn blank(fallback_bg: Color) -> Cell {
    Cell::new(" ", fallback_bg, fallback_bg)
}

/// Helper: gate for the `repeat` field — `false` means render blank this row.
#[inline]
fn visible(row: i32, repeat: u8) -> bool {
    repeat == 0 || row.rem_euclid(repeat as i32) == 0
}

pub const SHOULDER_EMPTY: ShoulderStyle = ShoulderStyle {
    cell: |_theme, _row, _repeat, fallback_bg| blank(fallback_bg),
};
pub const SHOULDER_SIDEWALK: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("▦", tint(SIDEWALK_FG, theme), tint(SIDEWALK_BG, theme))
    },
};
pub const SHOULDER_HARD_EDGE: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("┃", tint(HARDEDGE_FG, theme), fallback_bg)
    },
};
pub const SHOULDER_SOFT_EDGE: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("·", tint(SOFTEDGE_FG, theme), fallback_bg)
    },
};
pub const SHOULDER_PARKED_CAR: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("▣", tint(PARKED_CAR_FG, theme), fallback_bg)
    },
};
pub const SHOULDER_POLES: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("●", tint(POLE_FG, theme), fallback_bg)
    },
};
pub const SHOULDER_RAILROAD: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("╫", tint(RAIL_FG, theme), fallback_bg)
    },
};
pub const SHOULDER_RIVER: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("▚", tint(RIVER_FG, theme), fallback_bg)
    },
};
pub const SHOULDER_COUNTRY_ROAD: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("░", tint(COUNTRY_FG, theme), fallback_bg)
    },
};
pub const SHOULDER_TREE_PINE: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("▲", tint(TREE_PINE_DIM, theme), fallback_bg)
    },
};
pub const SHOULDER_TREE_OAK: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("◉", tint(TREE_OAK_GREEN, theme), fallback_bg)
    },
};
pub const SHOULDER_TREE_PALM: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("✤", tint(TREE_PALM_GREEN, theme), fallback_bg)
    },
};

const GUARDRAIL_FG: Color = Color::Rgb(200, 200, 205);
const BARRIER_FG: Color = Color::Rgb(185, 185, 185);
const WIRE_FENCE_FG: Color = Color::Rgb(140, 128, 108);
const SAND_EDGE_FG: Color = Color::Rgb(205, 180, 100);

pub const SHOULDER_GUARDRAIL: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("═", tint(GUARDRAIL_FG, theme), fallback_bg)
    },
};
pub const SHOULDER_CRASH_BARRIER: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("█", tint(BARRIER_FG, theme), fallback_bg)
    },
};
pub const SHOULDER_WIRE_FENCE: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("┼", tint(WIRE_FENCE_FG, theme), fallback_bg)
    },
};
pub const SHOULDER_SAND_EDGE: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("░", tint(SAND_EDGE_FG, theme), fallback_bg)
    },
};

const MAGIC_RUNE_FG: Color = Color::Rgb(165, 75, 225);
const BEACON_FG: Color = Color::Rgb(245, 235, 80);
const NEON_BARRIER_FG: Color = Color::Rgb(0, 240, 160);

pub const SHOULDER_MAGIC_RUNE: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("Ω", tint(MAGIC_RUNE_FG, theme), fallback_bg)
    },
};
pub const SHOULDER_SPACE_BEACON: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        let sym = if row.rem_euclid(6) < 2 { "●" } else { "│" };
        Cell::new(sym, tint(BEACON_FG, theme), fallback_bg)
    },
};
pub const SHOULDER_NEON_BARRIER: ShoulderStyle = ShoulderStyle {
    cell: |theme, row, repeat, fallback_bg| {
        if !visible(row, repeat) {
            return blank(fallback_bg);
        }
        Cell::new("▌", tint(NEON_BARRIER_FG, theme), fallback_bg)
    },
};

// ─── Obstacle styles ─────────────────────────────────────────────────────────

const POTHOLE_FG: Color = Color::Rgb(50, 50, 50);
const SPEEDBUMP_FG: Color = Color::Rgb(240, 220, 0);
const SPIKES_FG: Color = Color::Rgb(220, 40, 40);
const FALLEN_TREE_FG: Color = Color::Rgb(220, 40, 40);

pub const OBSTACLE_POTHOLE_SMALL: ObstacleStyle = ObstacleStyle {
    id: 1,
    label: "Pothole",
    glyphs: (["·", "◉", "·"], POTHOLE_FG),
};
pub const OBSTACLE_POTHOLE_BIG: ObstacleStyle = ObstacleStyle {
    id: 2,
    label: "BIG pothole",
    glyphs: (["◉", "◉", "◉"], POTHOLE_FG),
};
pub const OBSTACLE_POTHOLE_CRATER: ObstacleStyle = ObstacleStyle {
    id: 3,
    label: "CRATER",
    glyphs: (["╳", "◉", "╳"], POTHOLE_FG),
};
pub const OBSTACLE_SPEED_BUMP: ObstacleStyle = ObstacleStyle {
    id: 4,
    label: "Speed bump",
    glyphs: (["▁", "▂", "▁"], SPEEDBUMP_FG),
};
pub const OBSTACLE_SPIKES: ObstacleStyle = ObstacleStyle {
    id: 5,
    label: "SPIKES",
    glyphs: (["▲", "▲", "▲"], SPIKES_FG),
};
pub const OBSTACLE_FALLEN_TREE: ObstacleStyle = ObstacleStyle {
    id: 6,
    label: "Fallen tree",
    glyphs: (["≣", "≣", "≣"], FALLEN_TREE_FG),
};

const ICE_BLUE: Color = Color::Rgb(175, 210, 240);
const OIL_DARK: Color = Color::Rgb(28, 22, 38);
const SAND_DRIFT_FG: Color = Color::Rgb(220, 190, 108);
const ROADWORK_FG: Color = Color::Rgb(240, 138, 28);
const ANIMAL_FG: Color = Color::Rgb(158, 118, 68);

pub const OBSTACLE_ICE_PATCH: ObstacleStyle = ObstacleStyle {
    id: 7,
    label: "Ice patch",
    glyphs: (["░", "░", "░"], ICE_BLUE),
};
pub const OBSTACLE_OIL_SPILL: ObstacleStyle = ObstacleStyle {
    id: 8,
    label: "Oil spill",
    glyphs: (["▒", "▒", "▒"], OIL_DARK),
};
pub const OBSTACLE_SAND_DRIFT: ObstacleStyle = ObstacleStyle {
    id: 9,
    label: "Sand drift",
    glyphs: (["~", "≈", "~"], SAND_DRIFT_FG),
};
pub const OBSTACLE_ROADWORK: ObstacleStyle = ObstacleStyle {
    id: 10,
    label: "Roadwork",
    glyphs: (["▲", "▣", "▲"], ROADWORK_FG),
};
pub const OBSTACLE_ANIMAL: ObstacleStyle = ObstacleStyle {
    id: 11,
    label: "Animal!",
    glyphs: (["(", "A", ")"], ANIMAL_FG),
};

const MAGIC_TRAP_FG: Color = Color::Rgb(185, 55, 228);
const DRAGON_FIRE_FG: Color = Color::Rgb(240, 118, 22);
const ASTEROID_FG: Color = Color::Rgb(155, 140, 120);
const METEOR_FG: Color = Color::Rgb(255, 195, 75);

pub const OBSTACLE_MAGIC_TRAP: ObstacleStyle = ObstacleStyle {
    id: 12,
    label: "Magic trap!",
    glyphs: (["✧", "◇", "✧"], MAGIC_TRAP_FG),
};
pub const OBSTACLE_DRAGON_FIRE: ObstacleStyle = ObstacleStyle {
    id: 13,
    label: "Dragon fire!",
    glyphs: (["≈", "Λ", "≈"], DRAGON_FIRE_FG),
};
pub const OBSTACLE_ASTEROID_CHUNK: ObstacleStyle = ObstacleStyle {
    id: 14,
    label: "Asteroid chunk",
    glyphs: (["◢", "◉", "◣"], ASTEROID_FG),
};
pub const OBSTACLE_METEOR: ObstacleStyle = ObstacleStyle {
    id: 15,
    label: "METEOR!",
    glyphs: (["★", "●", "★"], METEOR_FG),
};
