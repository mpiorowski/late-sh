//! Screen-scrape detectors for NetHack achievement milestones.
//!
//! late.sh only sees the remote game as terminal bytes (a `vt100` screen), so
//! the only way to notice a milestone is to watch for the exact strings the real
//! upstream NetHack 5.0.0 binary prints. These are pure string predicates over
//! the rendered screen contents; the once-per-session debounce and the actual
//! chip/badge grant live in `state.rs` / `award.rs`.
//!
//! ANTI-SPOOF (best effort, not bulletproof): a milestone marker must be the
//! ENTIRE pline leading the top message line (row 0). Two rules compose:
//!
//! 1. The marker must start the line. Engravings read back as `You read in the
//!    dust: …` (prefixed), named/called objects show up embedded mid-sentence,
//!    and inventory/map/menu/scrollback aren't on the message line at all.
//! 2. Whatever follows the marker must be how NetHack itself ends a topline:
//!    nothing, the terminal `--More--`, or a TWO-space gap before the next
//!    queued message (win/tty/topl.c concatenates with two spaces). This kills
//!    the one spoof rule 1 left open: a pet named after a marker (in-game
//!    C-call, or `DOGNAME=`/`CATNAME=` in a pushed rc) LEADS its own plines,
//!    but continues them after a single space (`<marker> bites the newt!`),
//!    and NetHack's name munging collapses interior double spaces, so a name
//!    can't fabricate the two-space form.
//!
//! ACCEPTED RESIDUAL RISK: anything that gets a pline printed whose entire
//! sentence *is* a marker would still pay out; we know of no remaining
//! in-game text channel that yields one. These are cosmetic flair rewards,
//! not a competitive economy, and the only fully spoof-proof source
//! (NetHack's host-side xlog/logfile) would need a cross-crate signal we've
//! decided isn't worth it.
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
/// The full ascension-prelude pline from `src/pray.c` (Moloch's dark twin says
/// "chants, and you are bathed in darkness", so the full sentence is required).
const CHOIR_MARK: &str = "An invisible choir sings, and you are bathed in radiance...";
/// The winning line, both genders (`You("ascend to the status of Demigod%s...")`).
const ASCEND_MARKS: [&str; 2] = [
    "You ascend to the status of Demigod...",
    "You ascend to the status of Demigoddess...",
];

/// The top message line (row 0), where NetHack prints plines, leading
/// whitespace stripped. This is the only place we trust milestone markers — see
/// the anti-spoof note at the top of the module.
fn message_line(screen_text: &str) -> &str {
    screen_text.lines().next().unwrap_or("").trim_start()
}

/// True when `marker` is the whole pline leading the message line: the marker
/// starts the line, and what follows is one of NetHack's own topline endings
/// (nothing, the terminal `--More--`, or a two-space concatenation with the
/// next queued message). See the anti-spoof note at the top of the module.
fn marker_is_whole_pline(screen_text: &str, marker: &str) -> bool {
    match message_line(screen_text).strip_prefix(marker) {
        Some(rest) => {
            let rest = rest.trim_end();
            rest.is_empty() || rest == "--More--" || rest.starts_with("  ")
        }
        None => false,
    }
}

/// True when the message line announces the real-Amulet pickup.
pub fn has_amulet_pickup(screen_text: &str) -> bool {
    marker_is_whole_pline(screen_text, AMULET_MARK)
}

/// True when the message line shows the ascension *prelude* (the choir line).
/// Observing it earlier in the session is the corroboration required before a
/// later ascend line is trusted.
pub fn has_ascension_prelude(screen_text: &str) -> bool {
    marker_is_whole_pline(screen_text, CHOIR_MARK)
}

/// True when the message line shows the winning "You ascend to the status of
/// Demigod" line. Only meaningful in combination with a previously seen prelude.
pub fn has_ascension_line(screen_text: &str) -> bool {
    ASCEND_MARKS
        .iter()
        .any(|mark| marker_is_whole_pline(screen_text, mark))
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
