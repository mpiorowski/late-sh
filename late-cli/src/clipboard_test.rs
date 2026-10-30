use std::path::PathBuf;

use image::{ImageFormat, Rgba, RgbaImage};

use super::png_from_file;

/// A scratch path unique to this test, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        Self(std::env::temp_dir().join(format!(
            "late-clipboard-test-{}-{name}",
            std::process::id()
        )))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A file copied in a file manager arrives as a path, in whatever format it
/// was saved: it is decoded and sent on as PNG with the same pixels.
#[test]
fn a_copied_image_file_becomes_png_with_its_pixels() {
    let scratch = Scratch::new("photo.jpg.bin");
    let mut source = RgbaImage::new(3, 2);
    source.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    source.put_pixel(2, 1, Rgba([0, 0, 255, 255]));
    // Saved as BMP under a misleading extension: the format comes from the
    // bytes, not the name.
    source
        .save_with_format(&scratch.0, ImageFormat::Bmp)
        .expect("write fixture");

    let png = png_from_file(&scratch.0).expect("an image file");

    assert_eq!(
        image::guess_format(&png).expect("format"),
        ImageFormat::Png
    );
    let decoded = image::load_from_memory(&png).expect("decode").into_rgba8();
    assert_eq!(decoded.dimensions(), (3, 2));
    assert_eq!(decoded.get_pixel(0, 0), &Rgba([255, 0, 0, 255]));
    assert_eq!(decoded.get_pixel(2, 1), &Rgba([0, 0, 255, 255]));
}

/// A PNG of nothing but a header: the dimensions it claims are read, no
/// pixel is decoded.
fn png_header_only(width: u32, height: u32) -> Vec<u8> {
    let mut chunk = Vec::new();
    chunk.extend_from_slice(b"IHDR");
    chunk.extend_from_slice(&width.to_be_bytes());
    chunk.extend_from_slice(&height.to_be_bytes());
    // 8-bit RGBA, deflate, no filter, no interlace.
    chunk.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    png.extend_from_slice(&13u32.to_be_bytes());
    png.extend_from_slice(&chunk);
    png.extend_from_slice(&crc32(&chunk).to_be_bytes());
    png
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = match crc & 1 {
                1 => (crc >> 1) ^ 0xEDB8_8320,
                _ => crc >> 1,
            };
        }
    }
    !crc
}

/// A small file claiming a huge picture is refused on its header, before
/// the decoder is given the memory for it.
#[test]
fn a_copied_file_claiming_a_huge_picture_is_refused_before_decoding() {
    let scratch = Scratch::new("huge.png");
    std::fs::write(&scratch.0, png_header_only(100_000, 100_000)).expect("write fixture");

    let err = png_from_file(&scratch.0).expect_err("over the caps");

    assert!(
        format!("{err:#}").contains("dimensions are too large"),
        "got: {err:#}"
    );
}

/// A copied file that is not a picture says so, naming the file.
#[test]
fn a_copied_file_that_is_not_an_image_is_refused() {
    let scratch = Scratch::new("notes.txt");
    std::fs::write(&scratch.0, "just some notes").expect("write fixture");

    let err = png_from_file(&scratch.0).expect_err("not an image");

    assert!(
        format!("{err:#}").contains("is not an image"),
        "got: {err:#}"
    );
}
