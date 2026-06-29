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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let cfg = AnalyzerConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn default_values() {
        let cfg = AnalyzerConfig::default();
        assert_eq!(cfg.fft_size, 1024);
        assert_eq!(cfg.band_count, 8);
        assert_eq!(cfg.target_hz, 15);
    }

    #[test]
    fn rejects_zero_target_hz() {
        let cfg = AnalyzerConfig {
            target_hz: 0,
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_non_power_of_two_fft() {
        let cfg = AnalyzerConfig {
            fft_size: 100,
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_wrong_band_count() {
        let cfg = AnalyzerConfig {
            band_count: 16,
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_zero_fft_size() {
        let cfg = AnalyzerConfig {
            fft_size: 0,
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn accepts_valid_powers_of_two() {
        for size in [256, 512, 1024, 2048, 4096] {
            let cfg = AnalyzerConfig {
                fft_size: size,
                ..Default::default()
            };
            assert!(cfg.validate().is_ok(), "fft_size={size} should be valid");
        }
    }
}
