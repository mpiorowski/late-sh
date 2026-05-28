use crate::geom::Rect;
use crate::{
    constants::*,
    traits::{Body, Entity, HitBox, Sprite},
    types::*,
    utils::PLAYER_IMAGE_DATA,
};
use glam::{I16Vec2, U16Vec2, Vec2};
use image::RgbaImage;

#[derive(Debug, Default)]
pub struct ShootingState {
    pub direction: Option<Vec2>,
    pub counter: f32,
}

impl ShootingState {
    pub fn reset(&mut self) {
        self.direction = None;
        self.counter = 0.0;
    }

    pub fn shot_towards(&mut self, deltatime: f32) -> Option<Vec2> {
        if self.is_shooting() {
            self.counter += deltatime;
            if self.counter > SHOOTING_WINDUP_MILLISECONDS {
                let direction = self.direction.expect("Direction should exist");
                self.reset();
                return Some(direction);
            }
        }
        None
    }

    pub fn shoot(&mut self, direction: Vec2) {
        self.direction = Some(direction);
        self.counter = 0.001;
    }

    pub fn is_shooting(&self) -> bool {
        self.counter > 0.0
    }
}

#[derive(Debug, Default)]
pub struct Player {
    pub side: GameSide,
    previous_position: Vec2,
    position: Vec2,
    pub velocity: Vec2,
    previous_orientation: Orientation,
    pub orientation: Orientation,
    pub new_orientation: Option<Orientation>,
    pub shooting_state: ShootingState,
    pub after_shooting_counter: f32,
    pub after_got_stolen_counter: f32,
}

impl Body for Player {
    fn mass(&self) -> f32 {
        50.0
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
        // Treat previous_orientation as a per-tick snapshot. Anything that
        // wants to know "did the player rotate this tick?" reads it after
        // rotate() runs (which happens later in update_running).
        self.previous_orientation = self.orientation;

        if self.velocity.length() > MAX_PLAYER_VELOCITY {
            self.velocity = MAX_PLAYER_VELOCITY * self.velocity.normalize()
        }

        self.position += self.velocity * deltatime;

        // Handle counters
        if self.after_shooting_counter > 0.0 {
            self.after_shooting_counter = (self.after_shooting_counter - deltatime).max(0.0);
        }

        if self.after_got_stolen_counter > 0.0 {
            self.after_got_stolen_counter = (self.after_got_stolen_counter - deltatime).max(0.0);
        }
    }
}

impl Sprite for Player {
    fn image(&self, _palette: Palette) -> &RgbaImage {
        &PLAYER_IMAGE_DATA
            .get(&self.side)
            .expect("Image data should exist")
            .images[self.orientation as usize]
    }

    fn hit_box(&self) -> &HitBox {
        &PLAYER_IMAGE_DATA
            .get(&self.side)
            .expect("Image data should exist")
            .hit_boxes[self.orientation as usize]
    }
}

impl Entity for Player {}

impl Player {
    pub fn just_rotated(&self) -> bool {
        self.previous_orientation != self.orientation
    }

    /// Raw float position. Use this for sub-pixel direction math (e.g. the
    /// player-player separation normal): `position()` truncates to `U16Vec2`,
    /// which collapses to the same coordinate when two players overlap within
    /// a single pixel and leaves the normal at zero.
    pub fn position_float(&self) -> Vec2 {
        self.position
    }

    pub fn previous_hit_box(&self) -> &HitBox {
        &PLAYER_IMAGE_DATA
            .get(&self.side)
            .expect("Image data should exist")
            .hit_boxes[self.previous_orientation as usize]
    }
}

impl Player {
    fn initial_state(side: GameSide) -> (Vec2, Orientation) {
        match side {
            GameSide::Red => (RED_INITIAL_POSITION, Orientation::Right),
            GameSide::Blue => (BLUE_INITIAL_POSITION, Orientation::Left),
        }
    }

    pub fn new(side: GameSide) -> Self {
        let (position, orientation) = Self::initial_state(side);
        Self {
            side,
            previous_position: position,
            position,
            velocity: Vec2::ZERO,
            previous_orientation: orientation,
            orientation,
            new_orientation: None,
            shooting_state: ShootingState::default(),
            after_shooting_counter: 0.0,
            after_got_stolen_counter: 0.0,
        }
    }

    pub fn reset(&mut self) {
        let (position, orientation) = Self::initial_state(self.side);
        self.position = position;
        self.velocity = Vec2::ZERO;
        self.orientation = orientation;
        self.new_orientation = None;
        self.shooting_state.reset();
        self.after_shooting_counter = 0.0;
    }

    pub fn rotate(&mut self, new_orientation: Orientation) {
        self.previous_orientation = self.orientation;
        let previous_head_position = self.position + self.head_position_offset().as_vec2();
        self.orientation = new_orientation;
        self.new_orientation = None;
        // After rotating, realign the player so that the head position did not change
        let new_head_position = self.position + self.head_position_offset().as_vec2();

        self.position = self.position + previous_head_position - new_head_position;
    }

    pub fn undo_rotation(&mut self) {
        let previous_head_position = self.position + self.head_position_offset().as_vec2();
        self.orientation = self.previous_orientation;
        // After rotating, realign the player so that the head position did not change
        let new_head_position = self.position + self.head_position_offset().as_vec2();
        self.position = self.position + previous_head_position - new_head_position;
    }

    pub fn catcher_position(&self) -> U16Vec2 {
        (self.position.as_i16vec2() + self.puck_catcher_offset()).as_u16vec2()
    }

    fn puck_catcher_offset(&self) -> I16Vec2 {
        match self.orientation {
            Orientation::Up => I16Vec2::new(18, 0),
            Orientation::UpLeft => I16Vec2::new(12, -2),
            Orientation::Left => I16Vec2::new(0, 0),
            Orientation::DownLeft => I16Vec2::new(-2, 1),
            Orientation::Down => I16Vec2::new(0, 6),
            Orientation::DownRight => I16Vec2::new(1, 15),
            Orientation::Right => I16Vec2::new(6, 18),
            Orientation::UpRight => I16Vec2::new(15, 12),
        }
    }

    pub fn head_position_offset(&self) -> U16Vec2 {
        let (x, y) = match self.orientation {
            Orientation::Up => (4, 3),
            Orientation::UpLeft => (5, 10),
            Orientation::Left => (3, 13),
            Orientation::DownLeft => (10, 7),
            Orientation::Down => (13, 2),
            Orientation::DownRight => (7, 2),
            Orientation::Right => (2, 4),
            Orientation::UpRight => (2, 5),
        };
        U16Vec2::new(x, y)
    }

    pub fn maybe_bounce_against_rect(&mut self, rect: Rect, bouncing_coefficient: f32) {
        if (self.position.x as u16) < rect.left() {
            let extra_distance = rect.left() as f32 - self.position.x;
            let bounced_distance = extra_distance * bouncing_coefficient;
            self.position.x = rect.left() as f32 + bounced_distance;
            self.velocity.x *= -bouncing_coefficient;
        } else if (self.position.x as u16 + self.size().x) > rect.right() {
            let extra_distance = self.position.x + self.size().x as f32 - rect.right() as f32;
            let bounced_distance = extra_distance * bouncing_coefficient;
            self.position.x =
                ((rect.right().saturating_sub(self.size().x)) as f32 - bounced_distance).max(0.0);
            self.velocity.x *= -bouncing_coefficient;
        }

        if (self.position.y as u16) < rect.top() {
            let extra_distance = rect.top() as f32 - self.position.y;
            let bounced_distance = extra_distance * bouncing_coefficient;
            self.position.y = rect.top() as f32 + bounced_distance;
            self.velocity.y *= -bouncing_coefficient;
        } else if (self.position.y as u16 + self.size().y) > rect.bottom() {
            let extra_distance = self.position.y + self.size().y as f32 - rect.bottom() as f32;
            let bounced_distance = extra_distance * bouncing_coefficient;
            self.position.y = ((rect.bottom() - self.size().y) as f32 - bounced_distance).max(0.0);
            self.velocity.y *= -bouncing_coefficient;
        }
    }
}
