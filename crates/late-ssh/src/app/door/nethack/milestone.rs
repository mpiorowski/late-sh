//! Screen-scrape detectors for NetHack achievement milestones.
//!
//! late.sh only sees the remote game as terminal bytes (a `vt100` screen), so
//! the only way to notice a milestone is to watch for the exact strings the real
//! upstream NetHack 5.0.0 binary prints. These are pure string predicates over
//! the rendered screen contents; the once-per-session debounce and the actual
//! chip/badge grant live in `state.rs` / `award.rs`.
//!
//! ANTI-SPOOF (best effort, not bulletproof): the milestone markers are matched
//! only at the START of the top message line (row 0), where NetHack prints its
//! plines. Player-authored text doesn't land there: engravings read back as
//! `You read in the dust: …` (prefixed), named/called monsters and objects show
//! up embedded mid-sentence, and inventory/map/menu/scrollback aren't on the
//! message line at all. So the easy spoofs (engrave/name the marker, then look)
//! no longer pay out.
//!
//! ACCEPTED RESIDUAL RISK: a determined player who engineers a pline that
//! literally *begins* with a marker could still spoof a payout. We knowingly
//! accept that — these are cosmetic flair rewards, not a competitive economy,
//! and the only fully spoof-proof source (NetHack's host-side xlog/logfile)
//! would need a cross-crate signal we've decided isn't worth it.
//!
//! Strings verified against NetHack 5.0.0 source (the pinned build):
//! - Amulet pickup: `urgent_pline("The Amulet is bestowing a wish upon you!")`
//!   in `src/allmain.c`, gated on `u.uhave.amulet` (the *real* Amulet only — the
//!   "cheap plastic imitation" never sets it) and `!u.uevent.amulet_wish` (fires
//!   once per game). This is the reliable "got the real Amulet" signal; the
//!   inventory pickup line is useless because the fake renders identically.
//! - Ascension: the win sequence in `src/pray.c` prints, in order, the choir
//!   line, the immortality grant, then `You("ascend to the status of
//!   Demigod%s...")` (`"dess"` suffix when female). We require the choir
//!   *prelude* line to have led the message line earlier in the session before
//!   accepting the ascend line (guards against out-of-context scrollback too).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Milestone {
    Amulet,
    Ascension,
}

/// `urgent_pline` shown the instant the real Amulet of Yendor is first carried.
const AMULET_MARK: &str = "The Amulet is bestowing a wish upon you!";
/// First line of the ascension sequence (a plain `pline`, so it leads row 0).
const CHOIR_MARK: &str = "An invisible choir sings";
/// The winning line. `You(...)` prepends "You "; the marker stops before the
/// gender suffix so it matches both "Demigod..." and "Demigoddess...".
const ASCEND_MARK: &str = "You ascend to the status of Demigod";

/// The top message line (row 0), where NetHack prints plines, leading
/// whitespace stripped. This is the only place we trust milestone markers — see
/// the anti-spoof note at the top of the module.
fn message_line(screen_text: &str) -> &str {
    screen_text.lines().next().unwrap_or("").trim_start()
}

/// True when the message line announces the real-Amulet pickup.
pub fn has_amulet_pickup(screen_text: &str) -> bool {
    message_line(screen_text).starts_with(AMULET_MARK)
}

/// True when the message line shows the ascension *prelude* (the choir line).
/// Observing it earlier in the session is the corroboration required before a
/// later ascend line is trusted.
pub fn has_ascension_prelude(screen_text: &str) -> bool {
    message_line(screen_text).starts_with(CHOIR_MARK)
}

/// True when the message line shows the winning "You ascend to the status of
/// Demigod" line. Only meaningful in combination with a previously seen prelude.
pub fn has_ascension_line(screen_text: &str) -> bool {
    message_line(screen_text).starts_with(ASCEND_MARK)
}

/// End-of-game death signals. We deliberately avoid the message-line announce
/// "You die..." / "You turn to stone...": NetHack prints those in `done_in_by`
/// *before* the life-saving check in `done()`, so an amulet-of-life-saving
/// survivor flashes "You die..." and then lives. Instead we look for signals
/// that are only reached once the game is actually over (after life-saving has
/// resolved): the death-specific disclosure prompt, and the "REST IN PEACE"
/// tombstone. Quit shows "quit", save shows neither, ascension shows neither.
const DEATH_DISCLOSURE: &str = "what you had when you died";

/// True when the screen shows that this game ended in the player's death.
pub fn has_death(screen_text: &str) -> bool {
    if screen_text.contains(DEATH_DISCLOSURE) {
        return true;
    }
    // The tombstone's centered "REST IN PEACE"; require two of its words
    // together so ordinary text can't trip it. Shown only at true game over.
    screen_text.contains("REST") && screen_text.contains("PEACE")
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
        assert!(!has_ascension_prelude("The door opens."));
    }

    #[test]
    fn markers_must_lead_the_message_line_not_just_appear() {
        // Engraving read-back is prefixed, so it does not start the line.
        assert!(!has_amulet_pickup(
            "You read in the dust: The Amulet is bestowing a wish upon you!"
        ));
        assert!(!has_ascension_line(
            "You read in the dust: You ascend to the status of Demigod"
        ));
        // A named/called creature puts the text mid-sentence, not at the start.
        assert!(!has_ascension_line(
            "You see here a jackal called You ascend to the status of Demigod."
        ));
        // Only row 0 is trusted: a marker sitting in the map/menu body is ignored.
        assert!(!has_amulet_pickup(
            "Dlvl:3\nThe Amulet is bestowing a wish upon you!"
        ));
    }

    #[test]
    fn detects_death_but_not_lifesave_quit_or_save() {
        // End-of-game signals match.
        assert!(has_death("Do you want to see what you had when you died?"));
        assert!(has_death(
            "                     /    REST    \\\n                   /     PEACE      \\"
        ));
        // The pre-life-saving announce alone is NOT treated as death, so an
        // amulet-of-life-saving survivor doesn't get a spurious death event.
        assert!(!has_death("You die...--More--"));
        assert!(!has_death(
            "You die...  But wait... your medallion begins to glow!"
        ));
        // Quit and save are not deaths.
        assert!(!has_death("Do you want to see what you had when you quit?"));
        assert!(!has_death("Be seeing you..."));
        assert!(!has_death("You ascend to the status of Demigoddess..."));
    }
}
