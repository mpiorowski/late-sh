//! A long body in a modal, scrolled under a scrollbar: the offset, the
//! geometry the last draw published, and a thumb drag. Shared by the modals
//! that page through text (the paper, the News reader); each keeps its own
//! content, keys, page size, and what closing means.
//!
//! The draw publishes the viewport through `&self`, and input reads it back
//! on the next key, so the state sits in cells.

use std::cell::Cell;

use ratatui::layout::{Position, Rect};

use crate::app::input::{MouseButton, MouseEvent, MouseEventKind};

/// Geometry and wrapped extent published by the draw, in frame cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ScrollViewport {
    pub popup: Rect,
    pub body: Rect,
    pub close: Rect,
    pub track: Rect,
    pub content_height: u16,
}

impl ScrollViewport {
    fn max_scroll(self) -> u16 {
        self.content_height.saturating_sub(self.body.height)
    }
}

/// What a mouse event leaves for the modal to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollMouse {
    /// Nothing more: the view scrolled, dragged, or had nothing to do.
    Done,
    /// A click on the close control.
    Close,
    /// A click on the track above the thumb: a page up, by the modal's page.
    PageUp,
    /// A click on the track below the thumb.
    PageDown,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ScrollView {
    offset: Cell<u16>,
    viewport: Cell<ScrollViewport>,
    drag_grab: Cell<Option<u16>>,
}

impl ScrollView {
    pub(crate) fn offset(&self) -> u16 {
        self.offset.get()
    }

    /// Clamped to the last drawn body. With no body to clamp against (a
    /// resize not yet redrawn, or a frame too small to print) the offset
    /// stays put, and the next full draw clamps it to the real extent.
    pub(crate) fn scroll(&self, delta: i32) {
        let viewport = self.viewport.get();
        if viewport.body.is_empty() {
            return;
        }
        let next = i32::from(self.offset.get()).saturating_add(delta);
        self.offset
            .set(next.clamp(0, i32::from(viewport.max_scroll())) as u16);
    }

    pub(crate) fn scroll_to_top(&self) {
        self.offset.set(0);
    }

    pub(crate) fn scroll_to_bottom(&self) {
        self.scroll(i32::from(u16::MAX));
    }

    pub(crate) fn viewport(&self) -> ScrollViewport {
        self.viewport.get()
    }

    /// The draw's geometry. A changed one ends a drag, and the offset is
    /// clamped to it.
    pub(crate) fn set_viewport(&self, viewport: ScrollViewport) {
        if self.viewport.replace(viewport) != viewport {
            self.cancel_drag();
        }
        self.scroll(0);
    }

    /// Forget the drawn geometry until the next draw (a resize).
    pub(crate) fn invalidate(&self) {
        self.viewport.set(ScrollViewport::default());
        self.cancel_drag();
    }

    pub(crate) fn thumb(&self) -> Rect {
        let viewport = self.viewport.get();
        let track = viewport.track;
        if track.is_empty() || viewport.max_scroll() == 0 {
            return Rect::default();
        }
        let height = (u32::from(track.height) * u32::from(viewport.body.height)
            / u32::from(viewport.content_height))
        .max(1)
        .min(u32::from(track.height)) as u16;
        let travel = track.height - height;
        let top = (u32::from(self.offset.get()) * u32::from(travel)
            + u32::from(viewport.max_scroll()) / 2)
            / u32::from(viewport.max_scroll());
        Rect::new(track.x, track.y + top as u16, 1, height)
    }

    pub(crate) fn cancel_drag(&self) {
        self.drag_grab.set(None);
    }

    /// The mouse over the view: the wheel scrolls inside the popup, the
    /// thumb drags, a release ends a drag. A click on the close control or
    /// on the bare track is the modal's to act on.
    pub(crate) fn handle_mouse(&self, mouse: MouseEvent) -> ScrollMouse {
        if mouse.kind == MouseEventKind::Up {
            self.cancel_drag();
            return ScrollMouse::Done;
        }
        let (Some(x), Some(y)) = (mouse.x.checked_sub(1), mouse.y.checked_sub(1)) else {
            return ScrollMouse::Done;
        };
        let point = Position::new(x, y);
        let viewport = self.viewport.get();
        match mouse.kind {
            MouseEventKind::Down if mouse.button == Some(MouseButton::Left) => {
                self.cancel_drag();
                if viewport.close.contains(point) {
                    return ScrollMouse::Close;
                }
                let thumb = self.thumb();
                if thumb.contains(point) {
                    self.drag_grab.set(Some(y.saturating_sub(thumb.y)));
                    ScrollMouse::Done
                } else if viewport.track.contains(point) && y < thumb.y {
                    ScrollMouse::PageUp
                } else if viewport.track.contains(point) {
                    ScrollMouse::PageDown
                } else {
                    ScrollMouse::Done
                }
            }
            MouseEventKind::Drag if mouse.button == Some(MouseButton::Left) => {
                self.drag_to(y);
                ScrollMouse::Done
            }
            MouseEventKind::ScrollUp if viewport.popup.contains(point) => {
                self.scroll(-3);
                ScrollMouse::Done
            }
            MouseEventKind::ScrollDown if viewport.popup.contains(point) => {
                self.scroll(3);
                ScrollMouse::Done
            }
            _ => ScrollMouse::Done,
        }
    }

    fn drag_to(&self, y: u16) {
        let Some(grab) = self.drag_grab.get() else {
            return;
        };
        let viewport = self.viewport.get();
        let travel = viewport.track.height.saturating_sub(self.thumb().height);
        if travel == 0 {
            return;
        }
        let top = (i32::from(y) - i32::from(viewport.track.y) - i32::from(grab))
            .clamp(0, i32::from(travel)) as u32;
        self.offset.set(
            ((top * u32::from(viewport.max_scroll()) + u32::from(travel) / 2) / u32::from(travel))
                as u16,
        );
    }
}
