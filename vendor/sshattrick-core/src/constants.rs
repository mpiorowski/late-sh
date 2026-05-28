use crate::geom::{Margin, Rect};
use glam::Vec2;

pub const UI_SCREEN_SIZE: (u16, u16) = (160, 50);

const PITCH_RECT: Rect = Rect {
    x: 0,
    y: 0,
    width: 160,
    height: 86,
};

pub const PITCH_INNER_RECT: Rect = PITCH_RECT.inner(Margin {
    horizontal: 3,
    vertical: 3,
});

pub const RED_AREA_RECT: Rect = Rect {
    x: MIN_X,
    y: GOALIE_AREA_MIN_Y,
    width: GOALIE_AREA_WIDTH,
    height: GOALIE_AREA_HEIGHT,
};

pub const RED_AREA_INNER_RECT: Rect = RED_AREA_RECT.inner(Margin {
    horizontal: 1,
    vertical: 1,
});

pub const BLUE_AREA_RECT: Rect = Rect {
    x: MAX_X - GOALIE_AREA_WIDTH,
    y: GOALIE_AREA_MIN_Y,
    width: GOALIE_AREA_WIDTH,
    height: GOALIE_AREA_HEIGHT,
};

pub const BLUE_AREA_INNER_RECT: Rect = BLUE_AREA_RECT.inner(Margin {
    horizontal: 1,
    vertical: 1,
});

pub const MIN_X: u16 = PITCH_INNER_RECT.x;
pub const MAX_X: u16 = PITCH_INNER_RECT.x + PITCH_INNER_RECT.width;
pub const MIN_Y: u16 = PITCH_INNER_RECT.y;
pub const MAX_Y: u16 = PITCH_INNER_RECT.y + PITCH_INNER_RECT.height;

pub const GOALIE_AREA_WIDTH: u16 = 8;
pub const GOALIE_AREA_MIN_Y: u16 = 30;
pub const GOALIE_AREA_MAX_Y: u16 = 56;
pub const GOALIE_AREA_HEIGHT: u16 = GOALIE_AREA_MAX_Y - GOALIE_AREA_MIN_Y;

pub const RED_INITIAL_POSITION: Vec2 = Vec2::new(20.0, 40.0);
pub const BLUE_INITIAL_POSITION: Vec2 = Vec2::new(132.0, 40.0);

pub const ACCELERATION: f32 = 0.0025;
pub const DECELERATION: f32 = 0.005;
pub const MAX_PLAYER_VELOCITY: f32 = 0.275;

// Exponential decay applied to puck velocity per millisecond.
// 0.998995 ≈ 0.99 over a 10 ms physics tick, so behaviour matches the previous
// per-tick formulation under the default tick rate but stays correct if the
// tick rate ever shifts or update bursts deliver variable deltatime.
pub const PUCK_FRICTION_PER_MS: f32 = 0.998_995;
pub const COFFICIENT_OF_WALL_BOUNCING: f32 = 0.25;

pub const SKATE_TRACE_LENGTH: usize = 512;

pub const AFTER_SHOOTING_COUNTER_MILLISECONDS: f32 = 50.0;
pub const AFTER_GOT_STOLEN_COUNTER_MILLISECONDS: f32 = 50.0;
pub const SHOOTING_DIRECTION_MODIFIER: f32 = 0.35;
pub const SHOOTING_DIRECTION_MAX_MAGNITUDE: f32 = 3.0;
pub const SHOOTING_POWER: f32 = 0.2;
pub const SHOOTING_VELOCITY_DAMPING: f32 = 0.85;
pub const SHOOTING_WINDUP_MILLISECONDS: f32 = 200.0;

pub const AREA_RESTITUTION: f32 = 0.01;
pub const PLAYER_PLAYER_RESTITUTION: f32 = 0.95;
pub const PUCK_RESTITUTION: f32 = 0.75;
pub const GOALIE_RESTITUTION: f32 = 0.8;
pub const PLAYER_SEPARATION_IMPULSE: f32 = 0.15;
