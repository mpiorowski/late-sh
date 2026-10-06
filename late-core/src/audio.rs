/// Spectrum bands in a `viz` frame, low to high. The CLI analyzer sends
/// this many; the pair socket stretches an older CLI's 8 to match.
pub const VIZ_BANDS: usize = 16;

#[derive(Debug, Clone)]
pub struct VizFrame {
    pub bands: [f32; VIZ_BANDS], // 0..1
    pub rms: f32,                // 0..1
    pub track_pos_ms: u64,
}
