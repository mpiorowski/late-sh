pub mod canvas;
pub mod client;
pub mod color;
pub mod ops;
pub mod wire;

pub use canvas::{Canvas, CellValue, Glyph, Pos, DEFAULT_HEIGHT, DEFAULT_WIDTH};
pub use client::Client;
pub use color::RgbColor;
pub use ops::{CanvasOp, CellWrite, ColShift, RowShift};
pub use wire::{ClientMsg, ClientOpId, Peer, Seq, ServerMsg, UserId};
