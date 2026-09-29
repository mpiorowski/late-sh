use image::{ImageFormat, Rgba, RgbaImage};

use super::*;

fn png(width: u32, height: u32, colour: [u8; 4]) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    RgbaImage::from_pixel(width, height, Rgba(colour))
        .write_to(&mut bytes, ImageFormat::Png)
        .expect("encode png");
    bytes.into_inner()
}

#[test]
fn a_thumbnail_is_shrunk_to_the_picture_column() {
    let shrunk = shrink(&png(320, 180, [10, 120, 200, 255])).expect("a png decodes");

    assert_eq!(
        (shrunk.width(), shrunk.height()),
        (THUMBNAIL_WIDTH, THUMBNAIL_HEIGHT)
    );
    assert!(
        shrunk
            .pixels()
            .all(|pixel| *pixel == Rgba([10, 120, 200, 255])),
        "a flat image stays the colour it was"
    );
}

#[test]
fn bytes_that_are_not_an_image_are_an_error() {
    assert!(shrink(b"<html>not found</html>").is_err());
}

#[test]
fn the_url_is_the_wide_thumbnail_of_the_video() {
    assert_eq!(
        thumbnail_url("dQw4w9WgXcQ"),
        "https://i.ytimg.com/vi/dQw4w9WgXcQ/mqdefault.jpg"
    );
}
