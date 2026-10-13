//! The day's road as a card to paste (GAME.md, "The road": the end of a
//! run is a share card). The arcade's grammar (`arcade/share.rs`: the
//! header, the closed glyph set, the footer that is a command and never a
//! URL) pointed at the road: three lanes by ten steps, the lane walked
//! lit by how each step went, everything else dark. It needs no legend
//! and gives nothing away: a card says how your day went on a road
//! everybody had, and falling at seven is a better card than walking it.
//!
//! Pure. Copying, the banner, and the metric live in `fight/input.rs`.

use crate::app::arcade::share::{Glyph, Row, ShareCard, puzzle_number, title};

use super::road::{LANES, Mark, STEPS};
use super::state::Sheet;

/// How one step prints.
fn glyph(mark: Mark) -> Glyph {
    match mark {
        Mark::Won => Glyph::Green,
        Mark::BrightWon => Glyph::Yellow,
        Mark::Ran => Glyph::Orange,
        Mark::Fell => Glyph::Boom,
        Mark::Cleared => Glyph::Blue,
        Mark::Cached => Glyph::White,
        // A fight still on is not a result; the card is only cut once the
        // road is over, so this is a step the day roll will wipe.
        Mark::Fighting => Glyph::Red,
    }
}

/// The card for `sheet`'s day, once the road is over for it (every step
/// taken, or the signal down); `None` while there is still road to walk.
pub fn road_card(sheet: &Sheet) -> Option<ShareCard> {
    if !sheet.road_over() || sheet.road.path.is_empty() {
        return None;
    }
    let fell = sheet
        .road
        .path
        .iter()
        .position(|trace| trace.mark == Mark::Fell);
    let result = match fell {
        Some(step) => format!("fell at {}", step + 1),
        None => "walked".to_string(),
    };
    let mut rows: Vec<Row> = (0..LANES)
        .map(|lane| {
            Row::Glyphs(
                (0..STEPS)
                    .map(|step| match sheet.road.path.get(step) {
                        Some(trace) if usize::from(trace.lane) == lane => glyph(trace.mark),
                        Some(_) | None => Glyph::Dark,
                    })
                    .collect(),
            )
        })
        .collect();
    let glyphs = match sheet.kills_today {
        1 => "1 glyph".to_string(),
        n => format!("{n} glyphs"),
    };
    rows.push(Row::Text(format!(
        "lv {} · {glyphs} · signal {}/{}",
        sheet.level,
        sheet.signal,
        sheet.max_signal()
    )));
    Some(ShareCard {
        title: title("the road", puzzle_number(sheet.day), Some(&result)),
        rows,
    })
}

#[cfg(test)]
#[path = "share_test.rs"]
mod share_test;
