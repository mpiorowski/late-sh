//! The Pulse tile's data: how many people were online over the last day,
//! as the peak of each ten-minute bucket. Pure; the sampler that feeds it
//! lives in `svc.rs`.

use std::collections::VecDeque;

use chrono::{DateTime, Utc};

/// One bucket's width.
pub const PULSE_BUCKET_SECS: i64 = 600;
/// A day of buckets.
pub const PULSE_BUCKETS: usize = 144;

/// Peaks per bucket, oldest first, keyed by the bucket's index since the
/// epoch. A bucket with no sample (the server was down) is simply absent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PulseHistory {
    buckets: VecDeque<(i64, u16)>,
}

impl PulseHistory {
    /// Fold one headcount into its bucket, keeping the bucket's peak, and
    /// forget buckets older than a day.
    pub fn record(&mut self, at: DateTime<Utc>, online: usize) {
        let index = bucket_index(at);
        let online = u16::try_from(online).unwrap_or(u16::MAX);
        match self.buckets.back_mut() {
            Some((last, peak)) if *last == index => *peak = (*peak).max(online),
            Some(_) | None => self.buckets.push_back((index, online)),
        }
        let oldest = index - PULSE_BUCKETS as i64 + 1;
        while let Some((first, _)) = self.buckets.front()
            && *first < oldest
        {
            self.buckets.pop_front();
        }
    }

    /// The last day ending at `now`: `PULSE_BUCKETS` entries, oldest first,
    /// `None` where nothing was sampled.
    pub fn series(&self, now: DateTime<Utc>) -> Vec<Option<u16>> {
        let newest = bucket_index(now);
        let oldest = newest - PULSE_BUCKETS as i64 + 1;
        let mut series = vec![None; PULSE_BUCKETS];
        for (index, peak) in &self.buckets {
            if (oldest..=newest).contains(index) {
                series[(index - oldest) as usize] = Some(*peak);
            }
        }
        series
    }
}

/// `series` squeezed or stretched to `width` columns, each the peak of the
/// buckets it covers, `None` where none of them was sampled.
pub fn columns(series: &[Option<u16>], width: usize) -> Vec<Option<u16>> {
    if series.is_empty() {
        return vec![None; width];
    }
    (0..width)
        .map(|column| {
            let start = column * series.len() / width;
            let end = ((column + 1) * series.len() / width).max(start + 1);
            series[start..end].iter().flatten().copied().max()
        })
        .collect()
}

fn bucket_index(at: DateTime<Utc>) -> i64 {
    at.timestamp().div_euclid(PULSE_BUCKET_SECS)
}

#[cfg(test)]
#[path = "pulse_test.rs"]
mod pulse_test;
