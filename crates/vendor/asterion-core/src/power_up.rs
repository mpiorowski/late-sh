use strum_macros::{Display, EnumIter};

#[derive(Debug, Clone, Copy, Display, Hash, PartialEq, Eq, EnumIter)]
pub enum PowerUp {
    Speed,
    Vision,
    Memory,
}
