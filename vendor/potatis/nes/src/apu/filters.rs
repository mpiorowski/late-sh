use core::f32::consts::PI;

/// First-order RC filter for audio processing
/// Supports both high-pass and low-pass configurations
#[derive(Debug, Clone)]
pub struct FirstOrderFilter {
  a: f32,           // Filter coefficient
  prev_input: f32,  // Previous input sample (for high-pass)
  prev_output: f32, // Previous output sample
  is_highpass: bool,
}

impl FirstOrderFilter {
  /// Create a new first-order low-pass filter
  /// cutoff_freq: Cutoff frequency in Hz
  /// sample_rate: Sample rate in Hz
  pub fn new_lowpass(cutoff_freq: f32, sample_rate: f32) -> Self {
    let rc = 1.0 / (2.0 * PI * cutoff_freq);
    let dt = 1.0 / sample_rate;
    let a = dt / (rc + dt);

    Self {
      a,
      prev_input: 0.0,
      prev_output: 0.0,
      is_highpass: false,
    }
  }

  /// Create a new first-order high-pass filter
  /// cutoff_freq: Cutoff frequency in Hz
  /// sample_rate: Sample rate in Hz
  pub fn new_highpass(cutoff_freq: f32, sample_rate: f32) -> Self {
    let rc = 1.0 / (2.0 * PI * cutoff_freq);
    let dt = 1.0 / sample_rate;
    let a = rc / (rc + dt);

    Self {
      a,
      prev_input: 0.0,
      prev_output: 0.0,
      is_highpass: true,
    }
  }

  /// Process a single audio sample through the filter
  pub fn process(&mut self, input: f32) -> f32 {
    let output = if self.is_highpass {
      // High-pass: y[n] = a * (y[n-1] + x[n] - x[n-1])
      self.a * (self.prev_output + input - self.prev_input)
    } else {
      // Low-pass: y[n] = (1-a) * x[n] + a * y[n-1]
      (1.0 - self.a) * input + self.a * self.prev_output
    };

    self.prev_input = input;
    self.prev_output = output;
    output
  }
}

/// NES-specific audio filter chain that replicates the hardware filtering
/// Uses the three filters documented for authentic NES audio reproduction:
/// - High-pass at 90 Hz
/// - High-pass at 440 Hz  
/// - Low-pass at 14 kHz
#[derive(Debug)]
pub struct NesAudioFilters {
  hp_90hz: FirstOrderFilter,
  hp_440hz: FirstOrderFilter,
  lp_14khz: FirstOrderFilter,
}

impl NesAudioFilters {
  /// Create a new NES audio filter chain
  /// sample_rate: Audio sample rate in Hz (typically 44100)
  pub fn new(sample_rate: f32) -> Self {
    Self {
      hp_90hz: FirstOrderFilter::new_highpass(90.0, sample_rate),
      hp_440hz: FirstOrderFilter::new_highpass(440.0, sample_rate),
      lp_14khz: FirstOrderFilter::new_lowpass(14000.0, sample_rate),
    }
  }

  /// Process an audio sample through the complete NES filter chain
  pub fn process(&mut self, input: f32) -> f32 {
    // Apply filters in series: HP 90Hz -> HP 440Hz -> LP 14kHz
    let after_hp90 = self.hp_90hz.process(input);
    let after_hp440 = self.hp_440hz.process(after_hp90);
    self.lp_14khz.process(after_hp440)
  }
}

impl Default for NesAudioFilters {
  fn default() -> Self {
    // Default to 44.1 kHz sample rate
    Self::new(44100.0)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_lowpass_dc_response() {
    let mut filter = FirstOrderFilter::new_lowpass(1000.0, 44100.0);

    // DC signal (0 Hz) should pass through low-pass with minimal attenuation
    let dc_input = 1.0;
    let mut output = 0.0;

    // Let filter settle
    for _ in 0..1000 {
      output = filter.process(dc_input);
    }

    // DC should be mostly preserved (within 1% for 1kHz cutoff)
    assert!((output - dc_input).abs() < 0.01);
  }

  #[test]
  fn test_highpass_dc_blocking() {
    let mut filter = FirstOrderFilter::new_highpass(100.0, 44100.0);

    // DC signal should be blocked by high-pass
    let dc_input = 1.0;
    let mut output = 0.0;

    // Let filter settle
    for _ in 0..10000 {
      output = filter.process(dc_input);
    }

    // DC should be heavily attenuated
    assert!(output.abs() < 0.1);
  }

  #[test]
  fn test_nes_filters_basic() {
    let mut filters = NesAudioFilters::new(44100.0);

    // Test that filters can process samples without crashing
    let test_samples = [0.0, 0.5, 1.0, -0.5, -1.0, 0.0];

    for sample in test_samples.iter() {
      let output = filters.process(*sample);
      // Output should be finite
      assert!(output.is_finite());
    }
  }
}
