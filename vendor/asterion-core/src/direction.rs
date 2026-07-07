use strum_macros::{Display, EnumIter};

#[derive(Debug, Copy, Clone, Display, PartialEq, Eq, Hash, EnumIter)]
pub enum Direction {
    North,
    East,
    South,
    West,
    NorthEast,
    SouthEast,
    SouthWest,
    NorthWest,
}

impl Direction {
    pub fn opposite(&self) -> Self {
        match self {
            Self::North => Self::South,
            Self::East => Self::West,
            Self::South => Self::North,
            Self::West => Self::East,
            Self::NorthEast => Self::SouthWest,
            Self::SouthWest => Self::NorthEast,
            Self::SouthEast => Self::NorthWest,
            Self::NorthWest => Self::SouthEast,
        }
    }

    pub fn as_offset(&self) -> (isize, isize) {
        match self {
            Self::North => (0, -1),
            Self::East => (1, 0),
            Self::South => (0, 1),
            Self::West => (-1, 0),
            Self::NorthEast => (1, -1),
            Self::SouthEast => (1, 1),
            Self::SouthWest => (-1, 1),
            Self::NorthWest => (-1, -1),
        }
    }

    pub fn rotate_clockwise(&self) -> Self {
        match self {
            Self::North => Self::NorthEast,
            Self::East => Self::SouthEast,
            Self::South => Self::SouthWest,
            Self::West => Self::NorthWest,
            Self::NorthEast => Self::East,
            Self::SouthEast => Self::South,
            Self::SouthWest => Self::West,
            Self::NorthWest => Self::North,
        }
    }

    pub fn rotate_counter_clockwise(&self) -> Self {
        match self {
            Self::North => Self::NorthWest,
            Self::East => Self::NorthEast,
            Self::South => Self::SouthEast,
            Self::West => Self::SouthWest,
            Self::NorthEast => Self::North,
            Self::SouthEast => Self::East,
            Self::SouthWest => Self::South,
            Self::NorthWest => Self::West,
        }
    }
}
