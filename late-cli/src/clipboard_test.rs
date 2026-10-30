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
