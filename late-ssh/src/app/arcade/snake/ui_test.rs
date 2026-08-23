use super::*;
use crate::app::common::theme;

/// The board paints every cell on the palette's selection fill, and twelve
/// AMOLED palettes reuse an accent as that very fill (`success` on Greenery
/// is byte-identical to `bg_selection`), so a raw token can render fg == bg
/// and the snake, its food, or the power-up star simply vanishes. Every glyph
/// on every palette must clear the legibility floor against the fill it sits
/// on; a simple two-themes-differ assertion passed with a blank board.
#[test]
fn board_glyphs_stay_legible_on_every_palette() {
    for option in theme::OPTIONS {
        theme::set_current_by_id(option.id);
        let fill = theme::BG_SELECTION();
        // The terminal palette's fill has no RGB reading; the terminal owns
        // those colors and no ratio can be computed for them.
        if theme::contrast_ratio(fill, fill).is_none() {
            continue;
        }

        let glyphs = [
            ("food", glyph_color(&ThingKind::Food, &CobraState::Alive)),
            ("star", glyph_color(&ThingKind::Drug, &CobraState::Alive)),
            ("rock", glyph_color(&ThingKind::Rock, &CobraState::Alive)),
            ("snake", glyph_color(&ThingKind::Cobra, &CobraState::Alive)),
            (
                "powered snake",
                glyph_color(&ThingKind::Cobra, &CobraState::PoweredUp),
            ),
            ("frame", glyph_color(&ThingKind::Edge, &CobraState::Alive)),
        ];
        for (name, color) in glyphs {
            // A `Reset` token (the terminal palette's bright text) follows
            // the terminal's own foreground, the one pair the user already
            // made legible; no ratio exists to check for it.
            let Some(ratio) = theme::contrast_ratio(color, fill) else {
                continue;
            };
            assert!(
                ratio >= theme::MIN_GLYPH_CONTRAST,
                "the {name} is unreadable on {}: contrast {ratio:.2} against the board fill",
                option.id
            );
        }
    }
}
