/// A cursor over a fixed ring of items, supporting forward and backward traversal
/// with wraparound. Inspired by `LinkedList::Cursor` but without `Option` — the
/// cursor always points at a valid item.
#[derive(Debug, Clone)]
pub struct RingCursor<T> {
    items: Vec<T>,
    index: usize,
}

impl<T> RingCursor<T> {
    pub fn new(items: Vec<T>) -> Self {
        assert!(!items.is_empty(), "RingCursor requires at least one item");
        Self { items, index: 0 }
    }

    pub fn current(&self) -> &T {
        &self.items[self.index]
    }

    pub fn move_next(&mut self) -> &T {
        self.index = (self.index + 1) % self.items.len();
        self.current()
    }

    pub fn move_prev(&mut self) -> &T {
        self.index = (self.index + self.items.len() - 1) % self.items.len();
        self.current()
    }

    pub fn set_index(&mut self, index: usize) {
        if index < self.items.len() {
            self.index = index;
        }
    }
}

impl<T: PartialEq> PartialEq<T> for RingCursor<T> {
    fn eq(&self, other: &T) -> bool {
        self.current() == other
    }
}
