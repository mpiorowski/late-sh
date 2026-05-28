use crate::geom::Rect;
use crate::types::Palette;
use glam::{U16Vec2, Vec2};
use image::RgbaImage;
use std::{
    collections::{hash_map::Iter, HashMap},
    fmt::Debug,
};

#[derive(Debug, Default, Clone)]
pub struct HitBox {
    inner: HashMap<U16Vec2, ColliderType>,
    size: U16Vec2,
}

impl From<HashMap<U16Vec2, ColliderType>> for HitBox {
    fn from(value: HashMap<U16Vec2, ColliderType>) -> Self {
        let size = value
            .keys()
            .fold(U16Vec2::ZERO, |acc, p| acc.max(*p + U16Vec2::ONE));
        Self { inner: value, size }
    }
}

impl From<(Rect, ColliderType)> for HitBox {
    fn from(value: (Rect, ColliderType)) -> Self {
        let (rect, collider_type) = value;
        let size = U16Vec2::new(rect.width, rect.height);

        let mut inner = HashMap::new();
        for x in 0..rect.width {
            for y in 0..rect.height {
                inner.insert(U16Vec2::new(x, y), collider_type);
            }
        }
        Self { inner, size }
    }
}

impl HitBox {
    pub fn iter(&self) -> Iter<'_, U16Vec2, ColliderType> {
        self.inner.iter()
    }

    pub fn get(&self, k: &U16Vec2) -> Option<&ColliderType> {
        self.inner.get(k)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColliderType {
    Goalie,
    GoalieAreaVerticalSize,
    GoalieAreaHorizontalSide,
    Player,
    Puck,
    Stick,
    Catcher,
}

pub trait Body: Sprite {
    fn mass(&self) -> f32;

    fn rect(&self) -> Rect {
        Rect {
            x: self.position().x,
            y: self.position().y,
            width: self.hit_box().size.x,
            height: self.hit_box().size.y,
        }
    }

    fn previous_position(&self) -> U16Vec2;

    fn position(&self) -> U16Vec2;

    fn set_position(&mut self, position: U16Vec2);

    fn velocity(&self) -> Vec2;

    fn set_velocity(&mut self, velocity: Vec2);

    fn update_body(&mut self, _: f32) {}
}

pub trait Sprite {
    fn image(&self, palette: Palette) -> &RgbaImage;

    fn hit_box(&self) -> &HitBox;

    fn size(&self) -> U16Vec2 {
        self.hit_box().size
    }

    fn update_sprite(&mut self, _: f32) {}
}

pub trait Entity: Body + Debug + Send + Sync {
    fn update(&mut self, deltatime: f32) {
        self.update_body(deltatime);
        self.update_sprite(deltatime);
    }
}
