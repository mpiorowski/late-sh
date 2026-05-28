use super::player::Player;
use crate::{
    constants::*,
    traits::{Body, Entity, HitBox, Sprite},
    types::*,
    utils::PUCKS_IMAGE_DATA,
};
use glam::{U16Vec2, Vec2};
use image::RgbaImage;

#[derive(Debug)]
pub struct Puck {
    previous_position: Vec2,
    position: Vec2,
    pub velocity: Vec2,
    pub possession: Option<GameSide>,
}

impl Body for Puck {
    fn mass(&self) -> f32 {
        1.0
    }

    fn previous_position(&self) -> U16Vec2 {
        self.previous_position.as_u16vec2()
    }

    fn position(&self) -> U16Vec2 {
        self.position.as_u16vec2()
    }

    fn set_position(&mut self, position: U16Vec2) {
        self.position = position.as_vec2();
    }

    fn velocity(&self) -> Vec2 {
        self.velocity
    }

    fn set_velocity(&mut self, velocity: Vec2) {
        self.velocity = velocity;
    }

    fn update_body(&mut self, deltatime: f32) {
        self.previous_position = self.position;
        self.velocity *= PUCK_FRICTION_PER_MS.powf(deltatime);
        self.position += self.velocity * deltatime;

        if (self.position.x as u16) < MIN_X {
            let extra_distance = MIN_X as f32 - self.position.x;
            let bounced_distance = extra_distance * COFFICIENT_OF_WALL_BOUNCING;
            self.position.x = MIN_X as f32 + bounced_distance;
            self.velocity.x *= -1.0;
        } else if (self.position.x as u16 + self.size().x) > MAX_X {
            let extra_distance = self.position.x + self.size().x as f32 - MAX_X as f32;
            let bounced_distance = extra_distance * COFFICIENT_OF_WALL_BOUNCING;
            self.position.x = ((MAX_X - self.size().x) as f32 - bounced_distance).round();
            self.velocity.x *= -1.0;
        }

        if (self.position.y as u16) < MIN_Y {
            let extra_distance = MIN_Y as f32 - self.position.y;
            let bounced_distance = extra_distance * COFFICIENT_OF_WALL_BOUNCING;
            self.position.y = MIN_Y as f32 + bounced_distance;
            self.velocity.y *= -1.0;
        } else if (self.position.y as u16 + self.size().y) > MAX_Y {
            let extra_distance = self.position.y + self.size().y as f32 - MAX_Y as f32;
            let bounced_distance = extra_distance * COFFICIENT_OF_WALL_BOUNCING;
            self.position.y = ((MAX_Y - self.size().y) as f32 - bounced_distance).round();
            self.velocity.y *= -1.0;
        }
    }
}

impl Sprite for Puck {
    fn image(&self, palette: Palette) -> &RgbaImage {
        &PUCKS_IMAGE_DATA
            .get(&palette)
            .expect("There should be puck data for this palette")
            .images[0]
    }

    fn hit_box(&self) -> &HitBox {
        &PUCKS_IMAGE_DATA
            .get(&Palette::default())
            .expect("There should be puck data for this palette")
            .hit_boxes[0]
    }
}

impl Entity for Puck {}

impl Puck {
    pub fn new() -> Self {
        let mut p = Self {
            previous_position: Vec2::ZERO,
            position: Vec2::ZERO,
            velocity: Vec2::ZERO,
            possession: None,
        };

        let (position, velocity) = if rand::random_bool(0.5) {
            (
                Vec2::new((MAX_X + MIN_X - p.size().x) as f32 / 2.0, MIN_Y as f32),
                Vec2::new(0.0, 0.05),
            )
        } else {
            (
                Vec2::new(
                    (MAX_X + MIN_X - p.size().x) as f32 / 2.0,
                    (MAX_Y - p.size().y) as f32,
                ),
                Vec2::new(0.0, -0.05),
            )
        };

        p.position = position;
        p.previous_position = position;
        p.velocity = velocity;
        p
    }

    pub fn has_scored(&self) -> Option<GameSide> {
        if self.position().x <= MIN_X
            && self.position().y >= GOALIE_AREA_MIN_Y
            && self.position().y <= GOALIE_AREA_MAX_Y - self.size().y
        {
            return Some(GameSide::Blue);
        }
        if self.position().x >= MAX_X - self.size().x
            && self.position().y >= GOALIE_AREA_MIN_Y
            && self.position().y <= GOALIE_AREA_MAX_Y - self.size().y
        {
            return Some(GameSide::Red);
        }
        None
    }

    pub fn attach_to_player(&mut self, player: &Player) {
        self.set_position(player.catcher_position());
        self.velocity = player.velocity;
        self.possession = Some(player.side);
    }
}
