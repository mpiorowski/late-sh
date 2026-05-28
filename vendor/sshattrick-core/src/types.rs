use glam::Vec2;
use image::Rgba;

pub type AppResult<T> = Result<T, anyhow::Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameCommand {
    Up,
    Down,
    Left,
    Right,
    Shoot,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameSide {
    #[default]
    Red,
    Blue,
}

impl GameSide {
    pub fn opposite(self) -> Self {
        match self {
            GameSide::Red => GameSide::Blue,
            GameSide::Blue => GameSide::Red,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Palette {
    #[default]
    Dark,
    Light,
    Basket,
    Alt,
}

impl Palette {
    pub fn skate_trace_color(&self) -> Rgba<u8> {
        match self {
            Palette::Dark => Rgba([55, 55, 85, 255]),
            Palette::Light => Rgba([145, 215, 255, 255]),
            Palette::Basket => Rgba([55, 55, 85, 255]),
            Palette::Alt => Rgba([105, 55, 55, 255]),
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum Orientation {
    #[default]
    Up,
    UpLeft,
    Left,
    DownLeft,
    Down,
    DownRight,
    Right,
    UpRight,
}

impl Orientation {
    pub const MAX: usize = 8;

    pub fn next(self) -> Self {
        ((self as usize + 1) % Self::MAX).into()
    }

    pub fn previous(self) -> Self {
        ((self as usize + Self::MAX - 1) % Self::MAX).into()
    }

    pub fn shooting_direction(self) -> Vec2 {
        match self {
            Orientation::Up => Vec2::new(1.0, -1.0),
            Orientation::UpLeft => Vec2::new(0.0, -1.0),
            Orientation::Left => Vec2::new(-1.0, -1.0),
            Orientation::DownLeft => Vec2::new(-1.0, 0.0),
            Orientation::Down => Vec2::new(-1.0, 1.0),
            Orientation::DownRight => Vec2::new(0.0, 1.0),
            Orientation::Right => Vec2::new(1.0, 1.0),
            Orientation::UpRight => Vec2::new(1.0, 0.0),
        }
        .normalize()
    }
}

impl From<usize> for Orientation {
    fn from(value: usize) -> Self {
        match value {
            0 => Orientation::Up,
            1 => Orientation::UpLeft,
            2 => Orientation::Left,
            3 => Orientation::DownLeft,
            4 => Orientation::Down,
            5 => Orientation::DownRight,
            6 => Orientation::Right,
            7 => Orientation::UpRight,
            _ => panic!("Invalid orientation"),
        }
    }
}
