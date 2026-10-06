//! Shared horizontal marquee for rows too long for their rail. Used by the
//! music stage's now-playing rows and the core block's connected-friends row.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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
/// scrolls it instead of returning it unchanged. Measured in display columns,
/// so a wide glyph (an emoji badge, CJK) counts for the two it paints.
pub(crate) fn marquee_scrolls(text: &str, width: usize) -> bool {
    width > 0 && UnicodeWidthStr::width(text) > width
}

pub(crate) fn marquee_text(text: &str, width: usize, tick: usize) -> String {
    let columns = UnicodeWidthStr::width(text);
    if width == 0 || columns <= width {
        return text.to_string();
    }
    let travel = columns - width; // furthest left the window can scroll
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
    column_window(text, offset, width)
}

/// The `width` display columns of `text` starting at column `offset`. A wide
/// glyph cut by either edge of the window becomes a space, so the result is
/// always exactly `width` columns and never drifts the row past its rail.
/// Zero-width characters ride along with the glyph before them.
fn column_window(text: &str, offset: usize, width: usize) -> String {
    let end = offset + width;
    let mut out = String::new();
    let mut column = 0usize;
    let mut last_kept = false;
    for ch in text.chars() {
        let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if ch_width == 0 {
            if last_kept {
                out.push(ch);
            }
            continue;
        }
        let start = column;
        column += ch_width;
        last_kept = start >= offset && column <= end;
        match (last_kept, start < end && column > offset) {
            (true, _) => out.push(ch),
            (false, true) => out.push_str(&" ".repeat(column.min(end) - start.max(offset))),
            (false, false) => {}
        }
        if column >= end {
            break;
        }
    }
    out
}
