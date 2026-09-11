use super::puzzle_art::{normalize_image, parse_submission, prepare};
use crate::config::MAX_IMAGE_BYTES;
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use std::io::Cursor;

fn fixture(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(
        width,
        height,
        Rgba([40, 80, 120, 255]),
    ))
    .write_to(&mut bytes, format)
    .unwrap();
    bytes.into_inner()
}

#[test]
fn submissions_accept_one_url_and_optional_title() {
    assert_eq!(
        parse_submission("https://example.com/a.png").unwrap(),
        (
            "https://example.com/a.png".into(),
            "Community artwork".into()
        )
    );
    assert_eq!(
        parse_submission("Night Garden\nhttps://example.com/a.png")
            .unwrap()
            .1,
        "Night Garden"
    );
    assert!(parse_submission("just a conversation").is_err());
    assert!(parse_submission("https://example.com/a.png https://example.com/b.png").is_err());
    assert!(parse_submission("https://user:pass@example.com/a.png").is_err());
    assert!(parse_submission(&format!("https://example.com/a.png {}", "a".repeat(121))).is_err());
}

#[test]
fn normalization_rejects_invalid_small_nonsquare_and_unsupported_images() {
    assert!(normalize_image(b"not an image".to_vec()).is_err());
    assert!(normalize_image(fixture(255, 255, ImageFormat::Png)).is_err());
    assert!(normalize_image(fixture(256, 300, ImageFormat::Png)).is_err());
    assert!(normalize_image(fixture(256, 256, ImageFormat::Gif)).is_err());
    assert!(normalize_image(vec![0; MAX_IMAGE_BYTES + 1]).is_err());
    let output = normalize_image(fixture(256, 256, ImageFormat::Png)).unwrap();
    assert_eq!(image::guess_format(&output).unwrap(), ImageFormat::Png);
    let image = image::load_from_memory(&output).unwrap();
    assert_eq!((image.width(), image.height()), (256, 256));
}

#[tokio::test]
async fn disabled_storage_rejects_before_any_download() {
    let error = prepare(None, "https://example.invalid/a.png")
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("storage is disabled"));
}
