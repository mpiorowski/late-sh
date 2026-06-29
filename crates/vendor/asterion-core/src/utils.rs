use image::{Pixel, Rgb, RgbImage, Rgba, RgbaImage};
use rand::{rngs::ThreadRng, seq::IndexedRandom, RngExt};

pub type PlayerId = uuid::Uuid;

#[inline]
pub fn is_transparent(pixel: &Rgba<u8>, background: &Rgba<u8>) -> bool {
    pixel[3] == 0 || pixel.to_rgb() == background.to_rgb()
}

pub const MAX_USERNAME_LEN: usize = 12;

pub fn to_player_name(rng: &mut ThreadRng, name: &str) -> String {
    format!(
        "{}#{:03}",
        name.chars().take(MAX_USERNAME_LEN).collect::<String>(),
        rng.random_range(0..1000)
    )
}

pub fn random_minotaur_name() -> String {
    MINOTAUR_NAMES.choose(&mut rand::rng()).unwrap().to_string()
}

pub fn convert_rgb_to_rgba(rgb_image: &RgbImage, background: Rgb<u8>) -> RgbaImage {
    let (width, height) = rgb_image.dimensions();
    let mut rgba_image = RgbaImage::new(width, height);

    for (x, y, rgb_pixel) in rgb_image.enumerate_pixels() {
        let alpha = if rgb_pixel.to_rgb() == background {
            0
        } else {
            255
        };

        let Rgba([r, g, b, a]) = Rgba([rgb_pixel[0], rgb_pixel[1], rgb_pixel[2], alpha]);
        rgba_image.put_pixel(x, y, Rgba([r, g, b, a]));
    }

    rgba_image
}

pub struct GameColors {}

impl GameColors {
    pub const HERO: Rgba<u8> = Rgba([35, 35, 255, 255]);
    pub const OTHER_HERO: Rgba<u8> = Rgba([3, 255, 3, 255]);
    pub const MINOTAUR: Rgba<u8> = Rgba([225, 203, 3, 255]);
    pub const CHASING_MINOTAUR: Rgba<u8> = Rgba([255, 15, 0, 255]);
    pub const POWER_UP: Rgba<u8> = Rgba([255, 180, 244, 255]);
}

pub const MINOTAUR_NAMES: [&str; 7] = [
    "Ἀστερίων",
    "Μίνως",
    "Σαρπηδών",
    "Ῥαδάμανθυς",
    "Ἀμφιτρύων",
    "Πτερέλαος",
    "Τάφος",
];
