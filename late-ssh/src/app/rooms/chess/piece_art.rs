use std::{io::Cursor, sync::LazyLock};

use image::{ExtendedColorType, ImageEncoder, RgbaImage, codecs::png::PngEncoder};
use ratatui::text::Line;

use crate::app::files::inline_image::{
    InlineImageRenderSettings, InlineImageSymbolMode, render_rgba_preview,
};
use crate::app::files::terminal_image::TerminalImageData;
use crate::app::rooms::chess::state::{ChessColor, ChessPieceKind};

#[derive(Clone, Copy)]
pub enum GraphicsTier {
    Large,
    Medium,
}

#[derive(Clone, Copy)]
pub enum HalfBlockTier {
    Large,  // 4 rows of 8 chars (8x8 half-pixel canvas)
    Medium, // 3 rows of 6 chars (6x6 half-pixel canvas)
    Small,  // 2 rows of 4 chars (4x4 half-pixel canvas)
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

const SMALL_SOURCE_BYTES: [[&[u8]; 6]; 2] = [
    [
        include_bytes!("../../../../assets/chess/pieces/small/white_pawn.png"),
        include_bytes!("../../../../assets/chess/pieces/small/white_knight.png"),
        include_bytes!("../../../../assets/chess/pieces/small/white_bishop.png"),
        include_bytes!("../../../../assets/chess/pieces/small/white_rook.png"),
        include_bytes!("../../../../assets/chess/pieces/small/white_queen.png"),
        include_bytes!("../../../../assets/chess/pieces/small/white_king.png"),
    ],
    [
        include_bytes!("../../../../assets/chess/pieces/small/black_pawn.png"),
        include_bytes!("../../../../assets/chess/pieces/small/black_knight.png"),
        include_bytes!("../../../../assets/chess/pieces/small/black_bishop.png"),
        include_bytes!("../../../../assets/chess/pieces/small/black_rook.png"),
        include_bytes!("../../../../assets/chess/pieces/small/black_queen.png"),
        include_bytes!("../../../../assets/chess/pieces/small/black_king.png"),
    ],
];

struct PieceImages {
    large: TerminalImageData,
    medium: TerminalImageData,
}

struct PieceHalfBlock {
    large: Vec<Line<'static>>,
    medium: Vec<Line<'static>>,
    small: Vec<Line<'static>>,
}

static GRAPHICS: LazyLock<[[PieceImages; 6]; 2]> = LazyLock::new(|| {
    std::array::from_fn(|c| std::array::from_fn(|k| build_piece(SOURCE_BYTES[c][k])))
});

static HALF_BLOCK: LazyLock<[[PieceHalfBlock; 6]; 2]> = LazyLock::new(|| {
    std::array::from_fn(|c| std::array::from_fn(|k| build_half_block(SMALL_SOURCE_BYTES[c][k])))
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

pub fn half_block_line(
    color: ChessColor,
    kind: ChessPieceKind,
    tier: HalfBlockTier,
    sub: usize,
) -> Option<&'static Line<'static>> {
    let entry = &HALF_BLOCK[color_index(color)][kind_index(kind)];
    let lines = match tier {
        HalfBlockTier::Large => &entry.large,
        HalfBlockTier::Medium => &entry.medium,
        HalfBlockTier::Small => &entry.small,
    };
    lines.get(sub)
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

fn build_half_block(src: &[u8]) -> PieceHalfBlock {
    let img = image::load_from_memory(src)
        .unwrap_or_else(|err| panic!("chess piece small asset decode failed: {err}"))
        .to_rgba8();
    let canonical = normalize_to_canvas(&img, 8, 8);
    PieceHalfBlock {
        large: chafa_piece_lines(&canonical, 8, 4),
        medium: chafa_piece_lines(&downsample(&canonical, 6, 6), 6, 3),
        small: chafa_piece_lines(&downsample(&canonical, 4, 4), 4, 2),
    }
}

fn chafa_piece_lines(src: &RgbaImage, cols: u32, rows: u32) -> Vec<Line<'static>> {
    render_rgba_preview(
        src,
        cols,
        rows,
        InlineImageRenderSettings {
            symbol_mode: InlineImageSymbolMode::Default,
            background_rgb: None,
        },
    )
    .unwrap_or_default()
}

fn normalize_to_canvas(src: &RgbaImage, width: u32, height: u32) -> RgbaImage {
    debug_assert!(
        src.width() <= width && src.height() <= height,
        "symbol-render source {}x{} exceeds canvas {width}x{height}",
        src.width(),
        src.height(),
    );
    let mut canvas = RgbaImage::from_pixel(width, height, image::Rgba([0, 0, 0, 0]));
    let x_off = width.saturating_sub(src.width()) / 2;
    let y_off = height.saturating_sub(src.height());
    image::imageops::overlay(&mut canvas, src, x_off.into(), y_off.into());
    canvas
}

fn downsample(src: &RgbaImage, target_w: u32, target_h: u32) -> RgbaImage {
    image::DynamicImage::ImageRgba8(src.clone())
        .resize_exact(target_w, target_h, image::imageops::FilterType::Nearest)
        .to_rgba8()
}
