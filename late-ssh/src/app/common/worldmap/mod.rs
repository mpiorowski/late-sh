//! The world map: the data, and the view that draws it.
//!
//! Two callers so far — realm's board and `/map` — and they share
//! everything except what the colours mean. `data` is the atlas; `view` is the
//! viewport arithmetic and the half-block painter.

pub mod data;
pub mod view;
