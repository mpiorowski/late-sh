//! The pulse: a day of headcount samples (`svc.rs` samples them) into the
//! columns of a sparkline, and the block glyphs each column climbs.

use chrono::{DateTime, Duration, Utc};
use late_core::models::presence_sample::{HISTORY_HOURS, HeadcountPoint};

/// The eight steps a cell climbs through before the row above it starts.
pub const STEPS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
/// What a column with no sample draws on its bottom row: a gap, not a
/// zero (the replica was down, or the day is younger than the window).
pub const GAP: char = '·';

/// The day before `now` cut into `width` equal spans, each the highest
/// headcount sampled inside it, `None` where nothing was.
pub fn columns(points: &[HeadcountPoint], now: DateTime<Utc>, width: usize) -> Vec<Option<i64>> {
    if width == 0 {
        return Vec::new();
    }
    let window = Duration::hours(HISTORY_HOURS);
    let start = now - window;
    let span_ms = window.num_milliseconds() as f64 / width as f64;
    let mut columns = vec![None; width];
    for point in points {
        if point.bucket < start || point.bucket > now {
            continue;
        }
        let offset = (point.bucket - start).num_milliseconds() as f64;
        let index = ((offset / span_ms) as usize).min(width - 1);
        columns[index] = columns[index].max(Some(point.online));
    }
    columns
}

/// The day's highest sample, the latest one on a tie.
pub fn peak(points: &[HeadcountPoint]) -> Option<HeadcountPoint> {
    points.iter().copied().max_by_key(|point| point.online)
}

/// One column's glyphs, bottom row first, over `rows` rows: `value` out of
/// `max` becomes eighths of the column's height, a full row `█`, the top
/// partial row its step, the rest blank. One person is never less than
/// one step. A gap is `GAP` on the bottom row alone; a zero is blank.
pub fn glyphs(column: Option<i64>, max: i64, rows: usize) -> Vec<char> {
    let mut out = vec![' '; rows];
    let Some(value) = column else {
        if rows > 0 {
            out[0] = GAP;
        }
        return out;
    };
    if value <= 0 || max <= 0 || rows == 0 {
        return out;
    }
    let total = (rows * STEPS.len()) as i64;
    let eighths = ((value * total + max / 2) / max).clamp(1, total) as usize;
    for (row, cell) in out.iter_mut().enumerate() {
        let below = row * STEPS.len();
        if eighths >= below + STEPS.len() {
            *cell = STEPS[STEPS.len() - 1];
        } else if eighths > below {
            *cell = STEPS[eighths - below - 1];
        }
    }
    out
}

/// How high a column stands against the day's peak, 0 to 1, for its
/// colour.
pub fn height(column: Option<i64>, max: i64) -> f32 {
    match (column, max > 0) {
        (Some(value), true) => (value as f32 / max as f32).clamp(0.0, 1.0),
        (Some(_), false) | (None, _) => 0.0,
    }
}

#[cfg(test)]
#[path = "pulse_test.rs"]
mod pulse_test;
