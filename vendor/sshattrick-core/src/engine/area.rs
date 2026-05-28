use core::f32;
use std::collections::HashMap;

use crate::{
    constants::*,
    traits::{Body, ColliderType, Entity, HitBox, Sprite},
    types::{GameSide, Palette},
};
use glam::{U16Vec2, Vec2};
use image::RgbaImage;

#[derive(Debug, Default)]
pub struct Area {
    position: U16Vec2,
    image: RgbaImage,
    hit_box: HitBox,
}

impl Area {
    pub fn new(side: GameSide) -> Self {
        let rect = match side {
            GameSide::Red => RED_AREA_RECT,
            GameSide::Blue => BLUE_AREA_RECT,
        };

        let position = U16Vec2::new(rect.x, rect.y);

        let mut hit_box = HashMap::new();
        for x in 0..rect.width {
            hit_box.insert(U16Vec2::new(x, 0), ColliderType::GoalieAreaHorizontalSide);
            hit_box.insert(
                U16Vec2::new(x, rect.height - 1),
                ColliderType::GoalieAreaHorizontalSide,
            );
        }

        let x = match side {
            GameSide::Red => rect.width - 1,
            GameSide::Blue => 0,
        };
        for y in 1..rect.height - 1 {
            hit_box.insert(U16Vec2::new(x, y), ColliderType::GoalieAreaVerticalSize);
        }

        Self {
            position,
            image: RgbaImage::new(0, 0),
            hit_box: hit_box.into(),
        }
    }
}

impl Body for Area {
    fn mass(&self) -> f32 {
        f32::INFINITY
    }
    fn previous_position(&self) -> U16Vec2 {
        self.position
    }

    fn position(&self) -> U16Vec2 {
        self.position
    }

    fn set_position(&mut self, _position: U16Vec2) {}

    fn velocity(&self) -> Vec2 {
        Vec2::ZERO
    }

    fn set_velocity(&mut self, _velocity: Vec2) {}

    fn update_body(&mut self, _deltatime: f32) {}
}

impl Sprite for Area {
    fn image(&self, _palette: Palette) -> &RgbaImage {
        &self.image
    }

    fn hit_box(&self) -> &HitBox {
        &self.hit_box
    }
}

impl Entity for Area {}
