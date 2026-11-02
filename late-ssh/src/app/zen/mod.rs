//! The tile tree behind Home: the things you keep alive (the
//! bonsai, the aquarium, the pet) plus your rooms and the current track,
//! as a tiling layout you split, resize, and restyle yourself, persisted
//! per device. Home holds up to nine named layouts (`presets`), switched
//! with `Ctrl+F` then a digit; the stock `zen` one is also `/zen`. The
//! split tree, the rect math, the tile chrome and the layout set have
//! tests beside them.

pub mod bigclock;
pub mod input;
pub mod layout;
pub mod presets;
pub mod rows;
pub mod state;
pub mod ui;
