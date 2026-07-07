use std::{io::Cursor, sync::LazyLock};

use image::{ExtendedColorType, ImageEncoder, RgbaImage, codecs::png::PngEncoder};

use crate::app::files::terminal_image::TerminalImageData;
use crate::app::rooms::chess::state::{ChessColor, ChessPieceKind};

#[derive(Clone, Copy)]
pub enum GraphicsTier {
    Large,
    Medium,
}

const LARGE_COLS: u16 = 8;
const LARGE_ROWS: u16 = 4;
const MEDIUM_COLS: u16 = 6;
const MEDIUM_ROWS: u16 = 3;
const CELL_PX_W: u32 = 8;
const CELL_PX_H: u32 = 16;

const fn kind_index(kind: ChessPieceKind) -> usize {
    match kind {
        ChessPieceKind::Pawn => 0,
        ChessPieceKind::Knight => 1,
        ChessPieceKind::Bishop => 2,
        ChessPieceKind::Rook => 3,
        ChessPieceKind::Queen => 4,
        ChessPieceKind::King => 5,
    }
}

const fn color_index(color: ChessColor) -> usize {
    match color {
        ChessColor::White => 0,
        ChessColor::Black => 1,
    }
}

const SOURCE_BYTES: [[&[u8]; 6]; 2] = [
    [
        include_bytes!("../../../../assets/chess/pieces/large/white_pawn.png"),
        include_bytes!("../../../../assets/chess/pieces/large/white_knight.png"),
        include_bytes!("../../../../assets/chess/pieces/large/white_bishop.png"),
        include_bytes!("../../../../assets/chess/pieces/large/white_rook.png"),
        include_bytes!("../../../../assets/chess/pieces/large/white_queen.png"),
        include_bytes!("../../../../assets/chess/pieces/large/white_king.png"),
    ],
    [
        include_bytes!("../../../../assets/chess/pieces/large/black_pawn.png"),
        include_bytes!("../../../../assets/chess/pieces/large/black_knight.png"),
        include_bytes!("../../../../assets/chess/pieces/large/black_bishop.png"),
        include_bytes!("../../../../assets/chess/pieces/large/black_rook.png"),
        include_bytes!("../../../../assets/chess/pieces/large/black_queen.png"),
        include_bytes!("../../../../assets/chess/pieces/large/black_king.png"),
    ],
];

struct PieceImages {
    large: TerminalImageData,
    medium: TerminalImageData,
}

static GRAPHICS: LazyLock<[[PieceImages; 6]; 2]> = LazyLock::new(|| {
    std::array::from_fn(|c| std::array::from_fn(|k| build_piece(SOURCE_BYTES[c][k])))
});

pub fn graphics_image(
    color: ChessColor,
    kind: ChessPieceKind,
    tier: GraphicsTier,
) -> &'static TerminalImageData {
    let entry = &GRAPHICS[color_index(color)][kind_index(kind)];
    match tier {
        GraphicsTier::Large => &entry.large,
        GraphicsTier::Medium => &entry.medium,
    }
}

fn build_piece(src: &[u8]) -> PieceImages {
    let img = image::load_from_memory(src)
        .unwrap_or_else(|err| panic!("chess piece asset decode failed: {err}"))
        .to_rgba8();
    PieceImages {
        large: render_at(&img, LARGE_COLS, LARGE_ROWS),
        medium: render_at(&img, MEDIUM_COLS, MEDIUM_ROWS),
    }
}

/// Paste the source RGBA bottom-anchored, horizontally centred, on a transparent
/// canvas matching the tier's pixel size, then PNG-encode it for shipping.
fn render_at(src: &RgbaImage, cols: u16, rows: u16) -> TerminalImageData {
    let canvas_w = u32::from(cols) * CELL_PX_W;
    let canvas_h = u32::from(rows) * CELL_PX_H;
    let src_w = src.width();
    let src_h = src.height();
    debug_assert!(
        src_w <= canvas_w && src_h <= canvas_h,
        "chess piece source {src_w}x{src_h} exceeds canvas {canvas_w}x{canvas_h}",
    );
    let mut canvas = RgbaImage::from_pixel(canvas_w, canvas_h, image::Rgba([0, 0, 0, 0]));

    const BOTTOM_PAD_PX: u32 = 4;
    let x_off = canvas_w.saturating_sub(src_w) / 2;
    let y_off = canvas_h.saturating_sub(src_h).saturating_sub(BOTTOM_PAD_PX);
    image::imageops::overlay(&mut canvas, src, x_off.into(), y_off.into());

    let mut png = Vec::new();
    let encoder = PngEncoder::new(Cursor::new(&mut png));
    encoder
        .write_image(
            canvas.as_raw(),
            canvas_w,
            canvas_h,
            ExtendedColorType::Rgba8,
        )
        .expect("png encode of static chess canvas");

    TerminalImageData::new(png, None, cols, rows)
}
