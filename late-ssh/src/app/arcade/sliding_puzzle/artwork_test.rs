use super::artwork::BUILTINS;

#[test]
fn embedded_pool_is_square_bounded_and_credited() {
    for artwork in BUILTINS {
        assert!(!artwork.title.is_empty());
        assert!(!artwork.credit.is_empty());
        assert!(artwork.bytes.len() <= crate::config::MAX_IMAGE_BYTES);
        let image = image::load_from_memory(artwork.bytes).expect("valid embedded image");
        assert!(image.width() > 0);
        assert_eq!(image.width(), image.height());
        assert!(
            u64::from(image.width()) * u64::from(image.height())
                <= crate::app::files::terminal_image::MAX_DECODED_IMAGE_PIXELS
        );
    }
}
