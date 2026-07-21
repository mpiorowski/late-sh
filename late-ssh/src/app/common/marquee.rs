//! Shared horizontal marquee for rows too long for their rail. Used by the
//! music stage's now-playing rows and the core block's connected-friends row.

/// Render `text` into a `width`-column window. Text that fits is returned
/// unchanged; longer text scrolls back and forth so the whole thing can be
/// read in place. `tick` advances once per world tick (~66ms); the window
/// holds briefly at each end before reversing so both edges stay readable.
pub(crate) fn marquee_text(text: &str, width: usize, tick: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if width == 0 || chars.len() <= width {
        return text.to_string();
    }
    let travel = chars.len() - width; // furthest left the window can scroll
    let hold = 20; // ticks paused at each extreme (~1.3s) before reversing
    let step = 3; // ticks per column of movement
    let sweep = travel * step;
    let period = 2 * hold + 2 * sweep;
    let t = tick % period;
    let offset = if t < hold {
        0
    } else if t < hold + sweep {
        (t - hold) / step
    } else if t < 2 * hold + sweep {
        travel
    } else {
        travel - (t - 2 * hold - sweep) / step
    }
    .min(travel);
    chars[offset..offset + width].iter().collect()
}
