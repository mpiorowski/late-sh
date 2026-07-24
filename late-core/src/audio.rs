#[derive(Debug, Clone)]
pub struct VizFrame {
    pub bands: [f32; 8], // 0..1
    pub rms: f32,        // 0..1
    pub track_pos_ms: u64,
}
