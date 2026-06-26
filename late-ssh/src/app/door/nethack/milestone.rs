//! Screen-scrape detectors for NetHack achievement milestones.
//!
//! late.sh only sees the remote game as terminal bytes (a `vt100` screen), so
//! the only way to notice a milestone is to watch the message line for the exact
//! strings the real upstream NetHack 5.0.0 binary prints. These are pure string
//! predicates over the rendered screen contents; the once-per-session debounce
//! and the actual chip/badge grant live in `state.rs` / `award.rs`.
//!
//! Strings verified against NetHack 5.0.0 source (the pinned build):
//! - Amulet pickup: `urgent_pline("The Amulet is bestowing a wish upon you!")`
//!   in `src/allmain.c`, gated on `u.uhave.amulet` (the *real* Amulet only — the
//!   "cheap plastic imitation" never sets it) and `!u.uevent.amulet_wish` (fires
//!   once per game). This is the reliable "got the real Amulet" signal; the
//!   inventory pickup line is useless because the fake renders identically.
//! - Ascension: the win sequence in `src/pray.c` prints, in order, the choir
//!   line, the immortality grant, then `You("ascend to the status of
//!   Demigod%s...")` (`"dess"` suffix when female). We require a *prelude* line
//!   to have been seen earlier in the session before accepting the ascend line,
//!   so a single engraved/renamed string can't spoof a 20k payout.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Milestone {
    Amulet,
    Ascension,
}

/// `urgent_pline` shown the instant the real Amulet of Yendor is first carried.
const AMULET_MARK: &str = "The Amulet is bestowing a wish upon you!";
/// First line of the ascension sequence.
const CHOIR_MARK: &str = "An invisible choir sings";
/// The deity's gift line, immediately before the ascend line.
const IMMORTALITY_MARK: &str = "grant thee the gift of Immortality";
/// The winning line itself; substring is gender-agnostic ("Demigoddess"
/// contains "Demigod").
const ASCEND_MARK: &str = "ascend to the status of Demigod";

/// True when the screen shows the real-Amulet pickup message.
pub fn has_amulet_pickup(screen_text: &str) -> bool {
    screen_text.contains(AMULET_MARK)
}

/// True when the screen shows an ascension *prelude* line. Observing one of
/// these earlier in the session is the corroboration required before a later
/// ascend line is trusted.
pub fn has_ascension_prelude(screen_text: &str) -> bool {
    screen_text.contains(CHOIR_MARK) || screen_text.contains(IMMORTALITY_MARK)
}

/// True when the screen shows the winning "ascend to the status of Demigod"
/// line. Only meaningful in combination with a previously seen prelude.
pub fn has_ascension_line(screen_text: &str) -> bool {
    screen_text.contains(ASCEND_MARK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_real_amulet_pickup() {
        assert!(has_amulet_pickup(
            "  The Amulet is bestowing a wish upon you!--More--"
        ));
        // The inventory pickup line is intentionally NOT a trigger (fakes match).
        assert!(!has_amulet_pickup("f - the Amulet of Yendor."));
        assert!(!has_amulet_pickup("You see here a spellbook."));
    }

    #[test]
    fn detects_ascension_line_both_genders() {
        assert!(has_ascension_line("You ascend to the status of Demigod..."));
        assert!(has_ascension_line(
            "You ascend to the status of Demigoddess..."
        ));
        assert!(!has_ascension_line("You feel like a new man."));
    }

    #[test]
    fn detects_ascension_prelude() {
        assert!(has_ascension_prelude(
            "An invisible choir sings, and you are bathed in radiance...--More--"
        ));
        assert!(has_ascension_prelude(
            "In return for thy service, I grant thee the gift of Immortality!"
        ));
        assert!(!has_ascension_prelude("The door opens."));
    }
}
