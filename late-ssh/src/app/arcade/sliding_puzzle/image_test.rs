use std::{io::Cursor, path::PathBuf};

use image::{DynamicImage, ImageFormat};
use ratatui::{
    style::{Color, Style},
    text::Span,
};

use super::*;
use crate::config::MAX_IMAGE_BYTES;

struct TemporaryArtworkDirectory(PathBuf);

impl TemporaryArtworkDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "late-sh-sliding-puzzle-artwork-{}",
            uuid::Uuid::now_v7()
        ));
        std::fs::create_dir_all(&path).expect("create temporary artwork directory");
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TemporaryArtworkDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn square_png() -> Vec<u8> {
    let image = RgbaImage::from_pixel(16, 16, Rgba([30, 120, 220, 255]));
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("encode square png");
    bytes.into_inner()
}

fn tiled_png() -> Vec<u8> {
    let mut image = RgbaImage::new(30, 30);
    for tile in 0..9_u8 {
        let row = u32::from(tile / 3);
        let column = u32::from(tile % 3);
        let color = Rgba([
            tile.saturating_mul(20),
            255 - tile.saturating_mul(20),
            40,
            255,
        ]);
        for y in row * 10..(row + 1) * 10 {
            for x in column * 10..(column + 1) * 10 {
                image.put_pixel(x, y, color);
            }
        }
    }
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("encode tiled png");
    bytes.into_inner()
}

fn synthetic_preview(dimension: usize) -> InlineImagePreview {
    synthetic_preview_with_geometry(
        dimension,
        MIN_IMAGE_TILE_GEOMETRY.width,
        MIN_IMAGE_TILE_GEOMETRY.height,
    )
}

fn synthetic_preview_with_geometry(
    dimension: usize,
    tile_width: u16,
    tile_height: u16,
) -> InlineImagePreview {
    let width = dimension * usize::from(tile_width);
    let height = dimension * usize::from(tile_height);
    (0..height)
        .map(|row| {
            Line::from(
                (0..width)
                    .map(|column| {
                        let source_cell = row / usize::from(tile_height) * dimension
                            + column / usize::from(tile_width);
                        let marker = char::from(b'A' + source_cell as u8);
                        Span::styled(
                            marker.to_string(),
                            Style::default().fg(Color::Indexed(source_cell as u8)),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

#[test]
fn sliding_puzzle_image_source_is_deterministic_from_snapshot_seed() {
    assert_eq!(placeholder_image_url(0), PLACEHOLDER_IMAGE_URLS[0]);
    assert_eq!(placeholder_image_url(1), PLACEHOLDER_IMAGE_URLS[1]);
    assert_eq!(placeholder_image_url(2), PLACEHOLDER_IMAGE_URLS[2]);
    assert_eq!(placeholder_image_url(3), PLACEHOLDER_IMAGE_URLS[0]);
    assert_eq!(placeholder_image_url(4), PLACEHOLDER_IMAGE_URLS[1]);
}

#[test]
fn sliding_puzzle_image_tiles_use_the_largest_safe_terminal_geometry() {
    let wide_board = Rect::new(0, 0, 94, 25);
    assert_eq!(
        image_tile_geometry(wide_board, Difficulty::Easy),
        Some(ImageTileGeometry {
            width: 16,
            height: 8,
        })
    );
    assert_eq!(
        image_tile_geometry(wide_board, Difficulty::Medium),
        Some(ImageTileGeometry {
            width: 12,
            height: 6,
        })
    );
    assert_eq!(
        image_tile_geometry(wide_board, Difficulty::Hard),
        Some(ImageTileGeometry {
            width: 10,
            height: 5,
        })
    );

    assert_eq!(
        image_tile_geometry(Rect::new(0, 0, 54, 19), Difficulty::Hard),
        Some(ImageTileGeometry {
            width: 6,
            height: 3,
        })
    );
    assert_eq!(
        image_tile_geometry(Rect::new(0, 0, 500, 500), Difficulty::Hard),
        Some(ImageTileGeometry {
            width: 16,
            height: 8,
        })
    );
    assert_eq!(
        image_tile_geometry(Rect::new(0, 0, 29, 14), Difficulty::Hard),
        None
    );
}

#[test]
fn sliding_puzzle_image_request_key_tracks_terminal_geometry() {
    let settings = InlineImageRenderSettings::default();
    let compact = image_request_key(
        7,
        Difficulty::Hard,
        ImageTileGeometry {
            width: 6,
            height: 3,
        },
        settings,
    );
    let expanded = image_request_key(
        7,
        Difficulty::Hard,
        ImageTileGeometry {
            width: 10,
            height: 5,
        },
        settings,
    );

    assert_ne!(compact, expanded);
}

/// Decodes one prepared cell of `board` back to pixels. Cells are square
/// `MIN_IMAGE_TILE_GEOMETRY` rasters, so callers can sample them directly.
fn cell_pixels(images: &NativePuzzleImageSet, board: &[u8], destination: usize) -> RgbaImage {
    let cell = images.cell_image(board, destination).expect("cell image");
    assert_eq!(cell.display_cols, MIN_IMAGE_TILE_GEOMETRY.width);
    assert_eq!(cell.display_rows, MIN_IMAGE_TILE_GEOMETRY.height);
    image::load_from_memory(cell.png_bytes.as_slice())
        .expect("decode cell png")
        .to_rgba8()
}

fn tiled_tiles() -> NativePuzzleImageSet {
    render_terminal_puzzle_tiles(
        &tiled_png(),
        3,
        MIN_IMAGE_TILE_GEOMETRY,
        TerminalImageProtocol::Kitty,
        None,
    )
    .expect("render native puzzle tiles")
}

#[test]
fn sliding_puzzle_scrambled_cells_carry_their_solved_position_artwork() {
    let images = tiled_tiles();
    let board = [8, 0, 1, 2, 3, 4, 5, 6, 7];

    // (4, 4) is clear of both the centred tile label and the 1px gap border.
    // Tile 8 is source block 7, tile 1 is source block 0, and the gap is the
    // flattened background. The comparison is approximate because Lanczos3
    // rings against the hard colour edges of the fixture; the three expected
    // blocks are far enough apart that the tolerance cannot confuse them.
    let sample = |destination| cell_pixels(&images, &board, destination).get_pixel(4, 4).0;
    assert_resampled_to(sample(0), [140, 115, 40, 255]);
    assert_resampled_to(sample(1), [0, 0, 0, 255]);
    assert_resampled_to(sample(2), [0, 255, 40, 255]);
}

/// Asserts a sampled pixel came from the expected flat source region, with
/// enough slack for resampling ringing near a block edge.
#[track_caller]
fn assert_resampled_to(actual: [u8; 4], expected: [u8; 4]) {
    const TOLERANCE: i32 = 24;
    let close = actual
        .iter()
        .zip(expected)
        .all(|(a, e)| (i32::from(*a) - i32::from(e)).abs() <= TOLERANCE);
    assert!(close, "expected roughly {expected:?}, got {actual:?}");
}

#[test]
fn sliding_puzzle_native_move_changes_only_the_tile_and_gap_payloads() {
    let images = render_terminal_puzzle_tiles(
        &tiled_png(),
        3,
        MIN_IMAGE_TILE_GEOMETRY,
        TerminalImageProtocol::Kitty,
        None,
    )
    .expect("render native puzzle tiles");
    let before = [1, 2, 3, 4, 5, 6, 7, 0, 8];
    let after = [1, 2, 3, 4, 5, 6, 0, 7, 8];
    let changed = (0..before.len())
        .filter(|index| {
            images
                .cell_image(&before, *index)
                .expect("before cell")
                .cache_key()
                != images
                    .cell_image(&after, *index)
                    .expect("after cell")
                    .cache_key()
        })
        .collect::<Vec<_>>();

    assert_eq!(changed, vec![6, 7]);
    assert!((0..before.len()).all(|index| {
        let image = images.cell_image(&before, index).expect("cell image");
        image.display_cols == MIN_IMAGE_TILE_GEOMETRY.width
            && image.display_rows == MIN_IMAGE_TILE_GEOMETRY.height
            && image.is_opaque()
    }));
}

#[test]
fn sliding_puzzle_native_tiles_flatten_transparent_artwork() {
    let mut source = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(90, 90, Rgba([255, 0, 0, 0])))
        .write_to(&mut source, ImageFormat::Png)
        .expect("encode transparent png");
    let images = render_terminal_puzzle_tiles(
        source.get_ref(),
        3,
        MIN_IMAGE_TILE_GEOMETRY,
        TerminalImageProtocol::Kitty,
        Some(0x123456),
    )
    .expect("render native puzzle tiles");

    assert!(images.is_opaque());
}

#[test]
fn sliding_puzzle_native_cells_label_only_while_unsolved() {
    let images = tiled_tiles();
    let unsolved = [1, 2, 3, 4, 5, 6, 7, 0, 8];
    let solved = [1, 2, 3, 4, 5, 6, 7, 8, 0];

    // Both boards put tile 1 in destination 0, so the centre pixel differs
    // only because the unsolved cell carries an amber label over the artwork.
    assert_eq!(
        cell_pixels(&images, &unsolved, 0).get_pixel(24, 24).0,
        [184, 122, 43, 255]
    );
    assert_eq!(
        cell_pixels(&images, &solved, 0).get_pixel(24, 24).0,
        [0, 255, 40, 255]
    );
}

#[test]
fn sliding_puzzle_solved_native_cells_carry_a_completion_banner() {
    let images = tiled_tiles();
    let solved = [1, 2, 3, 4, 5, 6, 7, 8, 0];
    let amber = Rgba([184, 122, 43, 255]);
    let has_amber = |destination| {
        cell_pixels(&images, &solved, destination)
            .pixels()
            .any(|pixel| *pixel == amber)
    };

    // The banner is centred across the top of the board, so it lands in the
    // middle cell of the first row and nowhere near the left one. Solved cells
    // carry no tile labels, so amber can only be the banner.
    assert!(has_amber(1), "the banner cell should carry the banner");
    assert!(!has_amber(0), "cells clear of the banner stay unlabelled");
}

#[tokio::test]
async fn sliding_puzzle_local_file_url_renders_without_network_access() {
    let artwork = TemporaryArtworkDirectory::new();
    let path = artwork.path().join("local.png");
    std::fs::write(&path, square_png()).expect("write local artwork");
    let source = reqwest::Url::from_file_path(&path)
        .expect("local artwork URL")
        .to_string();

    let key = image_request_key(
        0,
        Difficulty::Easy,
        MIN_IMAGE_TILE_GEOMETRY,
        InlineImageRenderSettings::default(),
    );
    let (preview, bytes) = render_preview_from_directory(key, source, artwork.path(), None)
        .await
        .expect("render local artwork");

    assert_eq!(preview.len(), 9);
    assert!(preview.iter().all(|line| line.spans.len() == 18));
    // The bytes come back so the session can render the next size without
    // going to the source again.
    assert_eq!(bytes.as_slice(), square_png());
}

#[tokio::test]
async fn sliding_puzzle_resize_rerenders_from_cached_source_bytes() {
    // The source is never written to disk, so reaching for it would fail.
    // Only the cached bytes can satisfy this request.
    let missing = std::env::temp_dir().join(format!("late-sh-absent-{}", uuid::Uuid::now_v7()));
    let cached = Arc::new(square_png());
    let wider = image_request_key(
        0,
        Difficulty::Easy,
        ImageTileGeometry {
            width: 8,
            height: 4,
        },
        InlineImageRenderSettings::default(),
    );

    let (preview, bytes) = render_preview_from_directory(
        wider,
        "https://example.invalid/unreachable.png".to_string(),
        &missing,
        Some(Arc::clone(&cached)),
    )
    .await
    .expect("re-render from cached bytes");

    assert_eq!(preview.len(), 12);
    assert!(preview.iter().all(|line| line.spans.len() == 24));
    assert!(Arc::ptr_eq(&bytes, &cached));
}

#[test]
fn sliding_puzzle_a_stale_preview_still_seeds_the_source_cache() {
    let settings = InlineImageRenderSettings::default();
    let initial = image_request_key(7, Difficulty::Easy, MIN_IMAGE_TILE_GEOMETRY, settings);
    let resized = image_request_key(
        7,
        Difficulty::Easy,
        ImageTileGeometry {
            width: 8,
            height: 4,
        },
        settings,
    );
    let mut images = ImageTiles::new();
    images.toggle(7, Difficulty::Easy);
    images.sync_request(initial);
    images.sync_request(resized);

    // The in-flight request was superseded by a resize. Its preview is now
    // useless, but its download is exactly what the new size needs.
    assert!(!images.apply_result(initial, Ok((synthetic_preview(3), Arc::new(square_png())))));
    assert!(images.source_bytes.contains_key(&initial.source_index));
}

#[tokio::test]
async fn sliding_puzzle_local_file_url_cannot_escape_the_artwork_directory() {
    let parent = TemporaryArtworkDirectory::new();
    let artwork = parent.path().join("artwork");
    std::fs::create_dir(&artwork).expect("create artwork root");
    let outside = parent.path().join("outside.png");
    std::fs::write(&outside, square_png()).expect("write outside image");
    let source = reqwest::Url::from_file_path(&outside)
        .expect("outside file URL")
        .to_string();

    let error = read_local_image_bytes(&source, &artwork, MAX_IMAGE_BYTES)
        .await
        .expect_err("outside image must be rejected");

    assert!(error.to_string().contains("outside the artwork directory"));
}

#[cfg(unix)]
#[tokio::test]
async fn sliding_puzzle_local_file_url_rejects_symlink_escape() {
    let parent = TemporaryArtworkDirectory::new();
    let artwork = parent.path().join("artwork");
    std::fs::create_dir(&artwork).expect("create artwork root");
    let outside = parent.path().join("outside.png");
    std::fs::write(&outside, square_png()).expect("write outside image");
    let link = artwork.join("linked.png");
    std::os::unix::fs::symlink(&outside, &link).expect("create escaping symlink");
    let source = reqwest::Url::from_file_path(&link)
        .expect("symlink file URL")
        .to_string();

    let error = read_local_image_bytes(&source, &artwork, MAX_IMAGE_BYTES)
        .await
        .expect_err("escaping symlink must be rejected");

    assert!(error.to_string().contains("outside the artwork directory"));
}

#[tokio::test]
async fn sliding_puzzle_local_file_url_respects_the_image_size_limit() {
    let artwork = TemporaryArtworkDirectory::new();
    let path = artwork.path().join("large.png");
    std::fs::write(&path, [0_u8; 5]).expect("write oversized fixture");
    let source = reqwest::Url::from_file_path(&path)
        .expect("oversized file URL")
        .to_string();

    let error = read_local_image_bytes(&source, artwork.path(), 4)
        .await
        .expect_err("oversized image must be rejected");

    assert!(error.to_string().contains("image is too large"));
}

#[tokio::test]
async fn sliding_puzzle_http_url_still_uses_the_remote_loader() {
    let missing_artwork_directory =
        std::env::temp_dir().join(format!("late-sh-missing-artwork-{}", uuid::Uuid::now_v7()));

    let bytes = read_local_image_bytes(
        "https://example.com/art.png",
        &missing_artwork_directory,
        MAX_IMAGE_BYTES,
    )
    .await
    .expect("classify remote URL");

    assert!(bytes.is_none());
}

#[tokio::test]
async fn sliding_puzzle_missing_local_file_fails_without_network_access() {
    let artwork = TemporaryArtworkDirectory::new();
    let missing = artwork.path().join("missing.png");
    let source = reqwest::Url::from_file_path(&missing)
        .expect("missing file URL")
        .to_string();

    let error = read_local_image_bytes(&source, artwork.path(), MAX_IMAGE_BYTES)
        .await
        .expect_err("missing file must fail");

    assert!(
        error
            .to_string()
            .contains("local artwork file is unavailable")
    );
}

#[tokio::test]
async fn sliding_puzzle_non_image_local_file_fails_decode() {
    let artwork = TemporaryArtworkDirectory::new();
    let path = artwork.path().join("not-an-image.txt");
    std::fs::write(&path, b"not an image").expect("write invalid image fixture");
    let source = reqwest::Url::from_file_path(&path)
        .expect("invalid image file URL")
        .to_string();
    let bytes = read_local_image_bytes(&source, artwork.path(), MAX_IMAGE_BYTES)
        .await
        .expect("read invalid image fixture")
        .expect("file URL returns bytes");

    let error = render_image_bytes(bytes, 18, 9, InlineImageRenderSettings::default())
        .await
        .expect_err("invalid image data must fail decode");

    assert!(error.to_string().contains("failed to decode image"));
}

#[test]
fn sliding_puzzle_scrambled_tiles_use_their_solved_image_fragments() {
    let preview = synthetic_preview(3);
    let board = [8, 0, 1, 2, 3, 4, 5, 6, 7];

    let first =
        tile_fragment(&preview, 3, board[0], MIN_IMAGE_TILE_GEOMETRY).expect("tile 8 fragment");
    assert_eq!(first.len(), usize::from(MIN_IMAGE_TILE_GEOMETRY.height));
    assert!(first.iter().all(|line| {
        line.spans.len() == usize::from(MIN_IMAGE_TILE_GEOMETRY.width)
            && line.spans.iter().all(|span| span.content == "H")
    }));
    assert_eq!(first[0].spans[0].style.fg, Some(Color::Indexed(7)));

    assert!(tile_fragment(&preview, 3, board[1], MIN_IMAGE_TILE_GEOMETRY).is_none());

    let third =
        tile_fragment(&preview, 3, board[2], MIN_IMAGE_TILE_GEOMETRY).expect("tile 1 fragment");
    assert!(
        third
            .iter()
            .flat_map(|line| &line.spans)
            .all(|span| span.content == "A")
    );
}

#[test]
fn sliding_puzzle_gap_and_incomplete_preview_leave_numbered_fallback_available() {
    let preview = synthetic_preview(3);
    assert!(tile_fragment(&preview, 3, 0, MIN_IMAGE_TILE_GEOMETRY).is_none());
    assert!(tile_fragment(&preview[..1], 3, 1, MIN_IMAGE_TILE_GEOMETRY).is_none());
    assert!(tile_fragment(&preview, 3, 9, MIN_IMAGE_TILE_GEOMETRY).is_none());
}

#[test]
fn sliding_puzzle_stale_results_are_ignored_and_failure_rearms_after_view_toggle() {
    let settings = InlineImageRenderSettings::default();
    let stale_settings = InlineImageRenderSettings {
        background_rgb: Some(0x112233),
        ..settings
    };
    let current = image_request_key(7, Difficulty::Easy, MIN_IMAGE_TILE_GEOMETRY, settings);
    let stale = image_request_key(7, Difficulty::Easy, MIN_IMAGE_TILE_GEOMETRY, stale_settings);
    let mut images = ImageTiles::new();

    images.toggle(7, Difficulty::Easy);
    assert!(images.sync_request(current));
    assert!(!images.sync_request(current));
    assert_eq!(images.status_for(7, Difficulty::Easy), ImageStatus::Loading);

    assert!(!images.apply_preview(stale, Ok(synthetic_preview(3))));
    assert_eq!(images.status_for(7, Difficulty::Easy), ImageStatus::Loading);

    assert!(images.apply_preview(current, Err("source unavailable".to_string())));
    assert_eq!(images.status_for(7, Difficulty::Easy), ImageStatus::Failed);
    assert!(!images.should_request(current));

    images.toggle(7, Difficulty::Easy);
    images.toggle(7, Difficulty::Easy);
    assert!(!images.sync_request(current));
    assert_eq!(images.status_for(7, Difficulty::Easy), ImageStatus::Loading);
    assert!(images.should_request(current));
}

#[test]
fn sliding_puzzle_successful_preview_is_cached_by_request_key() {
    let settings = InlineImageRenderSettings::default();
    let key = image_request_key(11, Difficulty::Medium, MIN_IMAGE_TILE_GEOMETRY, settings);
    let mut images = ImageTiles::new();

    images.toggle(11, Difficulty::Medium);
    assert!(images.sync_request(key));
    assert!(images.apply_preview(key, Ok(synthetic_preview(4))));
    assert_eq!(
        images.status_for(11, Difficulty::Medium),
        ImageStatus::Ready
    );
    assert!(images.preview_for(11, Difficulty::Medium).is_some());
    assert!(!images.should_request(key));
}

#[test]
fn sliding_puzzle_preview_work_is_serialized() {
    let settings = InlineImageRenderSettings::default();
    let initial = image_request_key(7, Difficulty::Easy, MIN_IMAGE_TILE_GEOMETRY, settings);
    let resized = image_request_key(
        7,
        Difficulty::Easy,
        ImageTileGeometry {
            width: 8,
            height: 4,
        },
        settings,
    );
    let mut images = ImageTiles::new();

    images.toggle(7, Difficulty::Easy);
    images.sync_request(initial);
    assert!(images.claim_preview_request(initial));

    images.sync_request(resized);
    assert!(!images.claim_preview_request(resized));

    assert!(!images.apply_preview(initial, Ok(synthetic_preview(3))));
    assert!(images.claim_preview_request(resized));
}

#[test]
fn sliding_puzzle_preview_cache_evicts_superseded_requests() {
    let settings = InlineImageRenderSettings::default();
    let initial = image_request_key(7, Difficulty::Easy, MIN_IMAGE_TILE_GEOMETRY, settings);
    let resized = image_request_key(
        7,
        Difficulty::Easy,
        ImageTileGeometry {
            width: 8,
            height: 4,
        },
        settings,
    );
    let mut images = ImageTiles::new();

    images.toggle(7, Difficulty::Easy);
    images.sync_request(initial);
    assert!(images.apply_preview(initial, Ok(synthetic_preview(3))));
    images.sync_request(resized);
    assert!(images.apply_preview(resized, Ok(synthetic_preview_with_geometry(3, 8, 4))));

    images.sync_request(initial);
    assert!(images.preview_for(7, Difficulty::Easy).is_none());
}

#[test]
fn sliding_puzzle_prepared_native_tiles_are_reused_across_board_changes() {
    let settings = InlineImageRenderSettings::default();
    let artwork = image_request_key(7, Difficulty::Easy, MIN_IMAGE_TILE_GEOMETRY, settings);
    let board = vec![1, 2, 3, 4, 5, 6, 7, 0, 8];
    let replacement = vec![1, 2, 3, 0, 5, 6, 4, 7, 8];
    let native_tiles = render_terminal_puzzle_tiles(
        &tiled_png(),
        3,
        MIN_IMAGE_TILE_GEOMETRY,
        TerminalImageProtocol::Kitty,
        None,
    )
    .expect("render native puzzle tiles");
    let mut images = ImageTiles::new();

    images.toggle(7, Difficulty::Easy);
    images.sync_request(artwork);
    images.apply_preview(artwork, Ok(synthetic_preview(3)));
    images.set_native_tiles_for_test(artwork, native_tiles);
    let prepared = images
        .native_tiles_for(7, Difficulty::Easy)
        .expect("prepared native tiles");
    let before = prepared
        .cache_key_for_board(&board)
        .expect("board cache key");
    let after = prepared
        .cache_key_for_board(&replacement)
        .expect("replacement cache key");

    assert_ne!(before, after);
    assert!(prepared.cell_image(&board, 0).is_some());
    assert!(prepared.cell_image(&replacement, 0).is_some());
    assert!(!images.poll(
        7,
        Difficulty::Easy,
        MIN_IMAGE_TILE_GEOMETRY,
        settings,
        Some(TerminalImageProtocol::Kitty),
    ));
    assert!(images.native_tiles_for(7, Difficulty::Easy).is_some());
}

#[test]
fn sliding_puzzle_retry_rearms_the_selected_request_before_poll_synchronizes_it() {
    let settings = InlineImageRenderSettings::default();
    let failed = image_request_key(8, Difficulty::Easy, MIN_IMAGE_TILE_GEOMETRY, settings);
    let other = image_request_key(7, Difficulty::Easy, MIN_IMAGE_TILE_GEOMETRY, settings);
    let mut images = ImageTiles::new();

    images.toggle(8, Difficulty::Easy);
    assert!(images.sync_request(failed));
    assert!(images.apply_preview(failed, Err("source unavailable".to_string())));
    images.toggle(8, Difficulty::Easy);

    assert!(images.sync_request(other));
    images.toggle(8, Difficulty::Easy);
    assert!(images.sync_request(failed));
    assert!(images.should_request(failed));
}

#[test]
fn sliding_puzzle_prepared_native_tiles_survive_a_view_toggle() {
    let settings = InlineImageRenderSettings::default();
    let artwork = image_request_key(7, Difficulty::Easy, MIN_IMAGE_TILE_GEOMETRY, settings);
    let mut images = ImageTiles::new();
    images.toggle(7, Difficulty::Easy);
    images.sync_request(artwork);
    images.apply_preview(artwork, Ok(synthetic_preview(3)));
    images.set_native_tiles_for_test(artwork, tiled_tiles());

    // Off and back on. Re-encoding costs one blocking job per board cell, so
    // the prepared set has to outlive the request that produced it.
    images.toggle(7, Difficulty::Easy);
    images.poll(
        7,
        Difficulty::Easy,
        MIN_IMAGE_TILE_GEOMETRY,
        settings,
        Some(TerminalImageProtocol::Kitty),
    );
    assert!(images.native_tiles_for(7, Difficulty::Easy).is_none());

    images.toggle(7, Difficulty::Easy);
    images.poll(
        7,
        Difficulty::Easy,
        MIN_IMAGE_TILE_GEOMETRY,
        settings,
        Some(TerminalImageProtocol::Kitty),
    );
    assert!(
        images.native_tiles_for(7, Difficulty::Easy).is_some(),
        "toggling the view must not throw the encoded board away"
    );
}

#[test]
fn sliding_puzzle_native_render_survives_a_failed_chafa_preview() {
    let settings = InlineImageRenderSettings::default();
    let key = image_request_key(7, Difficulty::Easy, MIN_IMAGE_TILE_GEOMETRY, settings);
    let mut images = ImageTiles::new();
    images.toggle(7, Difficulty::Easy);
    images.sync_request(key);

    // The Chafa path fits by aspect ratio and rejects non-square sources; the
    // native path resizes exactly and does not care.
    images.apply_preview(
        key,
        Err("image preview has unexpected dimensions".to_string()),
    );
    images.set_native_tiles_for_test(key, tiled_tiles());

    assert!(images.native_tiles_for(7, Difficulty::Easy).is_some());
    assert_eq!(images.status_for(7, Difficulty::Easy), ImageStatus::Ready);
}

#[test]
fn sliding_puzzle_double_tapping_the_view_key_rearms_a_native_failure() {
    let settings = InlineImageRenderSettings::default();
    let artwork = image_request_key(7, Difficulty::Easy, MIN_IMAGE_TILE_GEOMETRY, settings);
    let mut images = ImageTiles::new();
    images.toggle(7, Difficulty::Easy);
    images.sync_request(artwork);
    images.apply_preview(artwork, Ok(synthetic_preview(3)));
    images.apply_native_result(
        NativeImageRequestKey {
            artwork,
            protocol: TerminalImageProtocol::Kitty,
        },
        Err("encoder unavailable".to_string()),
    );

    // Both presses land inside one 66ms tick, so `poll` never observes the
    // numbered view. `toggle` has to re-arm the retry by itself.
    images.toggle(7, Difficulty::Easy);
    images.toggle(7, Difficulty::Easy);

    assert!(images.native_failure.is_none());
}

#[test]
fn sliding_puzzle_release_frees_every_cached_raster() {
    let settings = InlineImageRenderSettings::default();
    let key = image_request_key(7, Difficulty::Easy, MIN_IMAGE_TILE_GEOMETRY, settings);
    let mut images = ImageTiles::new();
    images.toggle(7, Difficulty::Easy);
    images.sync_request(key);
    images.apply_result(key, Ok((synthetic_preview(3), Arc::new(tiled_png()))));
    images.set_native_tiles_for_test(key, tiled_tiles());

    assert!(images.release(), "release reports that it freed something");
    assert!(images.preview.is_none());
    assert!(images.native_render.is_none());
    assert!(images.source_bytes.is_empty());
    // The chosen view is a user preference, not a cache: returning to the
    // board reloads the artwork rather than silently dropping to numbers.
    assert_eq!(images.view(), TileView::Image);
    assert!(
        !images.release(),
        "a second release has nothing left to free"
    );
}

/// A request still in flight when the board closes delivers into a buffer
/// only `poll` reads, and `poll` stops running. Left there, one encoded set
/// outlives everything `release` just dropped, for the rest of the session.
#[test]
fn sliding_puzzle_release_discards_a_result_that_lands_after_the_board_closes() {
    let settings = InlineImageRenderSettings::default();
    let key = image_request_key(7, Difficulty::Easy, MIN_IMAGE_TILE_GEOMETRY, settings);
    let mut images = ImageTiles::new();
    images.toggle(7, Difficulty::Easy);
    images.sync_request(key);
    assert!(images.claim_preview_request(key), "request is in flight");

    // The spawned task finishes after the player has already left the board.
    images.release();
    images
        .result_tx
        .try_send((key, Ok((synthetic_preview(3), Arc::new(tiled_png())))))
        .expect("in-flight task delivers its result");

    images.release();
    assert!(
        images.result_rx.try_recv().is_err(),
        "the buffered result was dropped rather than held for the session"
    );

    // Draining must not wedge the next request: the discarded result is no
    // longer around to clear `in_flight`, so `release` has to do it.
    images.sync_request(key);
    assert!(
        images.claim_preview_request(key),
        "returning to the board can request again"
    );
}

#[test]
fn sliding_puzzle_new_render_key_marks_the_frame_dirty() {
    let settings = InlineImageRenderSettings::default();
    let initial = image_request_key(7, Difficulty::Easy, MIN_IMAGE_TILE_GEOMETRY, settings);
    let changed = image_request_key(
        7,
        Difficulty::Easy,
        MIN_IMAGE_TILE_GEOMETRY,
        InlineImageRenderSettings {
            background_rgb: Some(0x112233),
            ..settings
        },
    );
    let mut images = ImageTiles::new();

    assert!(images.sync_request(initial));
    assert!(!images.sync_request(initial));
    assert!(images.sync_request(changed));
}
