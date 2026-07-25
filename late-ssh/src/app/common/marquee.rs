//! Shared horizontal marquee for rows too long for their rail. Used by the
//! music stage's now-playing rows and the core block's connected-friends row.

/// Render `text` into a `width`-column window. Text that fits is returned
/// unchanged; longer text scrolls back and forth so the whole thing can be
/// read in place. `tick` advances once per world tick (~66ms); the window
/// holds briefly at each end before reversing so both edges stay readable.
/// Ticks per marquee step (~1s per step). Deliberately coarse: marquees are
/// ambience, not content. Every marquee output transition (including sweep
/// starts and reversals) lands on a tick that is a multiple of this because
/// the hold below is also a multiple of it, so the render gate only needs a
/// frame on these boundary ticks while a marquee scrolls.
pub(crate) const MARQUEE_STEP_TICKS: usize = 15;
/// Columns jumped per step: three columns once a second reads fast enough
/// while still paying only one frame per second.
pub(crate) const MARQUEE_STEP_COLUMNS: usize = 3;

/// True when `text` overruns a `width`-column rail, so [`marquee_text`]
/// scrolls it instead of returning it unchanged.
pub(crate) fn marquee_scrolls(text: &str, width: usize) -> bool {
    width > 0 && text.chars().count() > width
}

pub(crate) fn marquee_text(text: &str, width: usize, tick: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if width == 0 || chars.len() <= width {
        return text.to_string();
    }
    let travel = chars.len() - width; // furthest left the window can scroll
    let hold = 3 * MARQUEE_STEP_TICKS; // ticks paused at each extreme (~3s) before reversing
    let step = MARQUEE_STEP_TICKS;
    let sweep = travel.div_ceil(MARQUEE_STEP_COLUMNS) * step;
    let period = 2 * hold + 2 * sweep;
    let t = tick % period;
    let offset = if t < hold {
        0
    } else if t < hold + sweep {
        (t - hold) / step * MARQUEE_STEP_COLUMNS
    } else if t < 2 * hold + sweep {
        travel
    } else {
        travel.saturating_sub((t - 2 * hold - sweep) / step * MARQUEE_STEP_COLUMNS)
    }
    .min(travel);
    chars[offset..offset + width].iter().collect()
}
