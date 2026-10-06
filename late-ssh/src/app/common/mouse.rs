//! Click and wheel bookkeeping for a modal surface. The renderer records where
//! each control (`T`) and scrollable pane (`P`) landed in the last painted
//! frame; input looks a pointer position up in that record. Offsets belong to
//! panes, independently of keyboard selection and editable text.
use std::cell::{Cell, RefCell};

use ratatui::layout::Rect;

pub(crate) struct MouseState<T, P> {
    hits: RefCell<Vec<(Rect, T)>>,
    panes: RefCell<Vec<(Rect, P, usize)>>,
    offsets: RefCell<Vec<(P, usize)>>,
    reveal: Cell<bool>,
    size: Cell<(u16, u16)>,
    valid: Cell<bool>,
}

impl<T, P> Default for MouseState<T, P> {
    fn default() -> Self {
        Self {
            hits: RefCell::new(Vec::new()),
            panes: RefCell::new(Vec::new()),
            offsets: RefCell::new(Vec::new()),
            reveal: Cell::new(false),
            size: Cell::new((0, 0)),
            valid: Cell::new(false),
        }
    }
}

impl<T: Copy, P: Copy + PartialEq> MouseState<T, P> {
    pub(crate) fn begin(&self, size: (u16, u16)) {
        self.clear_surface();
        if self.size.replace(size) != size {
            self.reveal.set(true);
        }
        self.valid.set(true);
    }

    pub(crate) fn clear_surface(&self) {
        self.hits.borrow_mut().clear();
        self.panes.borrow_mut().clear();
    }

    pub(crate) fn invalidate(&self) {
        self.valid.set(false);
    }

    pub(crate) fn reveal_selection(&self) {
        self.reveal.set(true);
        self.invalidate();
    }

    pub(crate) fn reset_pane(&self, pane: P) {
        self.set_offset(pane, 0);
        self.invalidate();
    }

    pub(crate) fn finish(&self) {
        self.reveal.set(false);
    }

    pub(crate) fn hit(&self, rect: Rect, target: T) {
        if !rect.is_empty() {
            self.hits.borrow_mut().push((rect, target));
        }
    }

    pub(crate) fn target(&self, x: u16, y: u16, size: (u16, u16)) -> Option<T> {
        if !self.valid.get() || self.size.get() != size {
            return None;
        }
        self.hits
            .borrow()
            .iter()
            .rev()
            .find_map(|(rect, target)| rect.contains((x, y).into()).then_some(*target))
    }

    /// Register a pane of `rows` lines shown in `area` and return its scroll
    /// offset, moved to show `focus` when the keyboard asked for a reveal.
    pub(crate) fn pane(&self, area: Rect, pane: P, rows: usize, focus: usize) -> usize {
        let height = area.height as usize;
        let max = rows.saturating_sub(height);
        let mut offset = self.offset(pane).min(max);
        if self.reveal.get() && height > 0 {
            if focus < offset {
                offset = focus;
            } else if focus >= offset + height {
                offset = focus + 1 - height;
            }
            offset = offset.min(max);
        }
        self.set_offset(pane, offset);
        self.panes.borrow_mut().push((area, pane, max));
        offset
    }

    pub(crate) fn scroll(&self, x: u16, y: u16, delta: isize, size: (u16, u16)) {
        if !self.valid.get() || self.size.get() != size {
            return;
        }
        let hovered = self
            .panes
            .borrow()
            .iter()
            .rev()
            .find(|(area, _, _)| area.contains((x, y).into()))
            .map(|(_, pane, max)| (*pane, *max));
        if let Some((pane, max)) = hovered {
            self.set_offset(
                pane,
                self.offset(pane).saturating_add_signed(delta).min(max),
            );
            self.hits.borrow_mut().clear();
        }
    }

    /// Where the next recorded hit will land; pass it to `translate` after
    /// painting a virtual body.
    pub(crate) fn mark(&self) -> usize {
        self.hits.borrow().len()
    }

    /// Translate virtual-body hits recorded since `mark` into the visible
    /// viewport, dropping clipped cells so an invisible control can never be
    /// activated.
    pub(crate) fn translate(&self, mark: usize, area: Rect, offset: usize) {
        let mut hits = self.hits.borrow_mut();
        let local = hits.split_off(mark);
        for (rect, target) in local {
            let top = usize::from(rect.y).max(offset);
            let bottom = usize::from(rect.bottom()).min(offset + usize::from(area.height));
            if bottom > top && rect.x < area.width {
                hits.push((
                    Rect::new(
                        area.x + rect.x,
                        area.y + (top - offset) as u16,
                        rect.width.min(area.width - rect.x),
                        (bottom - top) as u16,
                    ),
                    target,
                ));
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn hits(&self) -> Vec<(Rect, T)> {
        self.hits.borrow().clone()
    }

    fn offset(&self, pane: P) -> usize {
        self.offsets
            .borrow()
            .iter()
            .find_map(|(p, offset)| (*p == pane).then_some(*offset))
            .unwrap_or(0)
    }

    fn set_offset(&self, pane: P, value: usize) {
        let mut offsets = self.offsets.borrow_mut();
        match offsets.iter_mut().find(|(p, _)| *p == pane) {
            Some(entry) => entry.1 = value,
            None => offsets.push((pane, value)),
        }
    }
}

#[cfg(test)]
#[path = "mouse_test.rs"]
mod mouse_test;
