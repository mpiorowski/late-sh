pub trait Polarity {
    /// Flip module bits before glyph encoding.
    /// `false` - fg glyph.
    /// `true`  - bg (inverted).
    const INVERT: bool;
}

/// Dark data modules on a light background (standard QR).
pub struct DarkOnLight;

impl Polarity for DarkOnLight {
    const INVERT: bool = false;
}

/// Light data modules on a dark background (inverted).
pub struct LightOnDark;

impl Polarity for LightOnDark {
    const INVERT: bool = true;
}
