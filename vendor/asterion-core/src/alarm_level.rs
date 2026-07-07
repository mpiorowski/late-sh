use crate::GameColors;
use image::Rgba;
use strum_macros::Display;

#[derive(Clone, Copy, Debug, Display, PartialEq, PartialOrd)]
pub enum AlarmLevel {
    NoMinotaurs,
    NotChasing,
    ChasingOtherHero,
    ChasingHero,
}

impl AlarmLevel {
    pub fn rgba(&self) -> Rgba<u8> {
        match self {
            Self::NoMinotaurs | Self::NotChasing => Rgba([255; 4]),
            Self::ChasingOtherHero => GameColors::MINOTAUR,
            Self::ChasingHero => GameColors::CHASING_MINOTAUR,
        }
    }
}
