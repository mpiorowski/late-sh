use super::player::Player;
use crate::{
    constants::*,
    traits::{Body, Entity, HitBox, Sprite},
    types::{GameSide, Palette},
    utils::GOALIE_IMAGE_DATA,
};
use glam::{U16Vec2, Vec2};
use image::RgbaImage;

#[derive(Debug, Default)]
pub struct Goalie {
    position: Vec2,
    side: GameSide,
    pub saves: usize,
    was_colliding_with_puck: bool,
    random_target_y: Option<f32>,
    random_target_timer_ms: f32,
}

impl Goalie {
    pub fn new(side: GameSide) -> Self {
        let mut g = Self {
            side,
            position: Vec2::ZERO,
            saves: 0,
            was_colliding_with_puck: false,
            random_target_y: None,
            random_target_timer_ms: 0.0,
        };
        g.position = match side {
            GameSide::Red => Vec2::new(MIN_X.into(), RED_INITIAL_POSITION.y),
            GameSide::Blue => Vec2::new((MAX_X - g.size().x).into(), BLUE_INITIAL_POSITION.y),
        };
        g
    }

    /// Edge-triggered save counter. Records that the puck is or isn't
    /// overlapping the goalie this tick and bumps `saves` exactly once on the
    /// transition from "not touching" to "touching a free puck" - so a puck
    /// the goalie carries past via possession doesn't count, and a slow puck
    /// pressed against the goalie counts once, not once per frame.
    pub fn register_puck_contact(&mut self, colliding: bool, puck_is_free: bool) {
        if colliding && !self.was_colliding_with_puck && puck_is_free {
            self.saves += 1;
        }
        self.was_colliding_with_puck = colliding;
    }

    pub fn align_to_player(&mut self, player: &Player) {
        let offset = player.head_position_offset();
        self.set_position(player.position() + offset - U16Vec2::new(0, 2));
    }

    /// Drift toward a random Y inside the goalie's area; pick a new target
    /// every ~0.8-2 s. Used by practice mode to give the keeper movement
    /// without a controlling player.
    pub fn random_walk(&mut self, deltatime: f32) {
        let (min_y, max_y) = self.target_y_bounds();
        self.random_target_timer_ms -= deltatime;
        if self.random_target_timer_ms <= 0.0 || self.random_target_y.is_none() {
            let span = (max_y - min_y).max(0.0);
            self.random_target_y = Some(min_y + rand::random::<f32>() * span);
            self.random_target_timer_ms = 800.0 + rand::random::<f32>() * 1200.0;
        }

        if let Some(target) = self.random_target_y {
            let speed = 0.04; // px / ms
            let delta = target - self.position.y;
            let max_step = speed * deltatime;
            let step = if delta.abs() < max_step {
                delta
            } else {
                delta.signum() * max_step
            };
            self.position.y = (self.position.y + step).clamp(min_y, max_y);
        }
    }

    fn target_y_bounds(&self) -> (f32, f32) {
        let inner = match self.side {
            GameSide::Red => RED_AREA_INNER_RECT,
            GameSide::Blue => BLUE_AREA_INNER_RECT,
        };
        let min_y = inner.y as f32;
        let max_y = (inner.y + inner.height - self.size().y) as f32;
        (min_y, max_y)
    }
}

impl Body for Goalie {
    fn mass(&self) -> f32 {
        f32::INFINITY
    }

    fn previous_position(&self) -> U16Vec2 {
        self.position()
    }

    fn position(&self) -> U16Vec2 {
        self.position.as_u16vec2()
    }

    fn set_position(&mut self, position: U16Vec2) {
        let inner_area = match self.side {
            GameSide::Red => RED_AREA_INNER_RECT,
            GameSide::Blue => BLUE_AREA_INNER_RECT,
        };
        let min_y = inner_area.y;
        let max_y = inner_area.y + inner_area.height - self.size().y;
        self.position.y = position.y.clamp(min_y, max_y) as f32;
    }

    fn velocity(&self) -> Vec2 {
        Vec2::ZERO
    }

    fn set_velocity(&mut self, _velocity: Vec2) {}

    fn update_body(&mut self, _deltatime: f32) {}
}

impl Sprite for Goalie {
    fn image(&self, _palette: Palette) -> &RgbaImage {
        &GOALIE_IMAGE_DATA
            .get(&self.side)
            .expect("There should be goalie data")
            .images[0]
    }

    fn hit_box(&self) -> &HitBox {
        &GOALIE_IMAGE_DATA
            .get(&self.side)
            .expect("There should be goalie data")
            .hit_boxes[0]
    }
}

impl Entity for Goalie {}
