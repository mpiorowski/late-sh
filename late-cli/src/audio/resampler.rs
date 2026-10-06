pub(super) struct StreamingLinearResampler {
    channels: usize,
    source_rate: u32,
    target_rate: u32,
    position: f64,
    previous_frame: Option<Vec<f32>>,
}

impl StreamingLinearResampler {
    pub(super) fn new(channels: usize, source_rate: u32, target_rate: u32) -> Self {
        Self {
            channels,
            source_rate,
            target_rate,
            position: 0.0,
            previous_frame: None,
        }
    }

    pub(super) fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if self.channels == 0 || input.is_empty() || !input.len().is_multiple_of(self.channels) {
            return Vec::new();
        }

        if self.source_rate == self.target_rate {
            self.previous_frame = Some(input[input.len() - self.channels..input.len()].to_vec());
            return input.to_vec();
        }

        let input_frames = input.len() / self.channels;
        let combined_frames = input_frames + usize::from(self.previous_frame.is_some());
        if combined_frames < 2 {
            self.previous_frame = Some(input.to_vec());
            return Vec::new();
        }

        let step = self.source_rate as f64 / self.target_rate as f64;
        let available_intervals = (combined_frames - 1) as f64;
        let mut output = Vec::new();

        while self.position < available_intervals {
            let left_idx = self.position.floor() as usize;
            let right_idx = left_idx + 1;
            let frac = (self.position - left_idx as f64) as f32;
            for channel in 0..self.channels {
                let left = self.frame_sample(input, left_idx, channel);
                let right = self.frame_sample(input, right_idx, channel);
                output.push(left + (right - left) * frac);
            }
            self.position += step;
        }

        self.position -= available_intervals;
        self.previous_frame = Some(input[input.len() - self.channels..input.len()].to_vec());
        output
    }

    fn frame_sample(&self, input: &[f32], frame_idx: usize, channel: usize) -> f32 {
        if let Some(prev) = &self.previous_frame {
            if frame_idx == 0 {
                return prev[channel];
            }
            return input[(frame_idx - 1) * self.channels + channel];
        }

        input[frame_idx * self.channels + channel]
    }
}

#[cfg(test)]
#[path = "resampler_test.rs"]
mod resampler_test;
