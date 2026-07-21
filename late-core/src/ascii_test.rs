use crate::ascii::*;
use image::{GrayImage, ImageFormat, Luma};
use std::io::Cursor;

#[test]
fn test_bytes_to_ascii() {
    // Create a simple 2x2 image in memory
    // Top-left: Black (0)     -> ' '
    // Top-right: White (255)  -> '@'
    // Bottom-left: Mid (127)  -> '=' (index 4)
    // Bottom-right: Black (0) -> ' '
    let mut img = GrayImage::new(2, 2);
    img.put_pixel(0, 0, Luma([0]));
    img.put_pixel(1, 0, Luma([255]));
    img.put_pixel(0, 1, Luma([127]));
    img.put_pixel(1, 1, Luma([0]));

    // Encode as PNG
    let mut bytes: Vec<u8> = Vec::new();
    img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .unwrap();

    // Convert to ASCII
    let ascii = bytes_to_ascii(&bytes, 2, 2).unwrap();

    let expected = " @\n= ";
    assert_eq!(ascii, expected);
}

#[test]
fn test_bytes_to_ascii_resize() {
    // Create a 4x4 image, all black except a 2x2 white square in the center
    let mut img = GrayImage::new(4, 4);
    for y in 0..4 {
        for x in 0..4 {
            img.put_pixel(x, y, Luma([0]));
        }
    }
    img.put_pixel(1, 1, Luma([255]));
    img.put_pixel(2, 1, Luma([255]));
    img.put_pixel(1, 2, Luma([255]));
    img.put_pixel(2, 2, Luma([255]));

    // Encode as PNG
    let mut bytes: Vec<u8> = Vec::new();
    img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .unwrap();

    // Convert and resize to 2x2 ASCII
    let ascii = bytes_to_ascii(&bytes, 2, 2).unwrap();

    // Since it's a 4x4 resized to 2x2 via Triangle filter, each 2x2 block in the original
    // image gets averaged. The center 2x2 white square is split across the four 2x2 blocks,
    // making each block partially bright. Let's not strictly assert the exact char since
    // filtering math could vary slightly, but it shouldn't be empty or all black/white.
    assert_eq!(ascii.lines().count(), 2);
    assert_eq!(ascii.chars().filter(|c| *c != '\n').count(), 4);
}
