use std::hash::Hash;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum View {
    Cone { radius: usize },
    Plane { radius: usize },
    Circle { radius: usize },
    Full,
}

impl View {
    pub fn radius(&self) -> usize {
        match self {
            Self::Cone { radius, .. } => *radius,
            Self::Plane { radius, .. } => *radius,
            Self::Circle { radius } => *radius,
            Self::Full => usize::MAX,
        }
    }
}
