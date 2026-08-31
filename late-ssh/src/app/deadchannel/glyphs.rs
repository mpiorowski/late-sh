//! The game's shared vocabulary of characters. Game-level on purpose:
//! the haunting borrows it, stage-4 spawns will render with it, and one
//! alphabet across both is what makes the stage-1 clock glitch
//! retroactive foreshadowing.

/// The fixed glyph alphabet: the characters the city's fauna is made of.
/// Distinct on purpose from the static shades (`░▒▓`, `haunt/ui.rs`):
/// static is noise, glyphs are creatures.
pub(crate) const GLYPH_ALPHABET: [char; 10] = ['▖', '▘', '▝', '▗', '▚', '▞', '╬', '╪', '╫', '┼'];
