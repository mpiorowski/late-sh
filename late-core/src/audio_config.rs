#[derive(Debug, Clone)]
pub struct AnalyzerConfig {
    pub fft_size: usize,
    pub band_count: usize,
    pub gain: f32,
    pub target_hz: u64,
}

impl Default for AnalyzerConfig {
    fn default() -> Self {
        AnalyzerConfig {
            fft_size: 1024,
            band_count: 8,
            gain: 3.0,
            target_hz: 15,
        }
    }
}

impl AnalyzerConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.target_hz == 0 {
            return Err("target_hz must be >= 1".to_string());
        }
        if self.fft_size == 0 || (self.fft_size & (self.fft_size - 1)) != 0 {
            return Err("fft_size must be a power of two > 0".to_string());
        }
        if self.band_count != 8 {
            return Err("band_count must be 8 when VizFrame uses [f32; 8]".to_string());
        }
        Ok(())
    }
}
