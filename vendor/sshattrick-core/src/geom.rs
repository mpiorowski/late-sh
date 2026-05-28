#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Margin {
    pub horizontal: u16,
    pub vertical: u16,
}

impl Rect {
    pub const fn inner(self, m: Margin) -> Self {
        Self {
            x: self.x.saturating_add(m.horizontal),
            y: self.y.saturating_add(m.vertical),
            width: self.width.saturating_sub(m.horizontal.saturating_mul(2)),
            height: self.height.saturating_sub(m.vertical.saturating_mul(2)),
        }
    }

    pub const fn left(self) -> u16 {
        self.x
    }

    pub const fn right(self) -> u16 {
        self.x.saturating_add(self.width)
    }

    pub const fn top(self) -> u16 {
        self.y
    }

    pub const fn bottom(self) -> u16 {
        self.y.saturating_add(self.height)
    }
}
