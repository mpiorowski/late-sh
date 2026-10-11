//! The game's shared vocabulary of characters. Game-level on purpose:
//! the haunting borrows it, stage-4 spawns will render with it, and one
//! alphabet across both is what makes the stage-1 clock glitch
//! retroactive foreshadowing.

/// The fixed glyph alphabet: the characters the city's fauna is made of.
/// Distinct on purpose from the static shades (`░▒▓`, `haunt/ui.rs`):
/// static is noise, glyphs are creatures.
pub(crate) const GLYPH_ALPHABET: [char; 10] = ['▖', '▘', '▝', '▗', '▚', '▞', '╬', '╪', '╫', '┼'];

/// What a runner can wear as their mark: the alphabet less the Signal's
/// `╬`. That one is the Old Signal's own, the paragon count behind the
/// level in the badge (`▚3╬2`, GAME.md "The mark"), so nobody wears it
/// for free. The tailor's mark row, the join's dice, and the look's
/// parse all read this; the fauna keeps the whole alphabet.
pub(crate) const MARK_ALPHABET: [char; 9] = ['▖', '▘', '▝', '▗', '▚', '▞', '╪', '╫', '┼'];
