//! The undercity guide's copy: every key and every rule of the street,
//! the fight, the counters, and the wire, in the voice's register. Kept
//! out of the site guide (`app/help_modal`) on purpose: that one is fed to
//! the bot, and the street is for runners to read themselves.
//!
//! This is the one place a runner is told how the city works, so it is
//! kept exact: a key, a price, a rule, or a counter that changes changes
//! here in the same change (CONTEXT.md, §3b). Every noun passes the
//! screenshot test (static, signal, glyph, ration, bit, wire), lowercase.

/// One heading and the lines under it. A line is one sentence or two;
/// the renderer wraps it to the frame.
pub struct Section {
    pub title: &'static str,
    pub lines: &'static [&'static str],
}

/// The guide, top to bottom.
pub const SECTIONS: &[Section] = &[
    Section {
        title: "the street",
        lines: &[
            "arrows or hjkl walk. shift with them runs: six steps, stopping at the first door it reaches.",
            "enter at a door goes in. enter or esc comes back out to the street.",
            "0 goes back up to the clubhouse. so does enter at the wire, the cable by the stairs.",
            "? opens this guide. it opened by itself the first time you came down, and never will again.",
        ],
    },
    Section {
        title: "the sheet",
        lines: &[
            "the strip top right is you: level, signal, rations, bits, and what you carry.",
            "signal is your health, ten a level. at zero you are off the row until the roll, and the street keeps every bit you had on you.",
            "rations are fights. ten a day, one spent for every step into the static.",
            "bits are the city's money. chips never come down here and never buy anything that hits.",
            "the roll is midnight utc: signal and rations refill together, and nothing refills before then.",
            "upstairs the frame reads your rations and signal for you while you are off the street.",
        ],
    },
    Section {
        title: "the static",
        lines: &[
            "the screen at the end of the row is where the glyphs come from. enter there spends a ration and puts one in front of you.",
            "a attacks. r runs: two times in three it works, and when it does not the glyph gets a free swing.",
            "esc steps out. the fight stays on the row and is waiting when you step back in, for no ration.",
            "enter closes a finished scene.",
            "a kill pays bits and exp. exp climbs the levels, up to 15, and the wire hears every one.",
            "a dropped signal ends your day: the street takes the bits on you, you keep most of the exp, and the row is shut until the roll.",
        ],
    },
    Section {
        title: "the armorer",
        lines: &[
            "up and down walk the wall, tier 1 to 15. w buys the picked weapon, a buys the picked armor.",
            "bits only, and only a tier above the one you carry. the piece you hand back comes off the price at three quarters of what it cost.",
        ],
    },
    Section {
        title: "patch",
        lines: &[
            "p buys the signal back to full. a bit a point, times your level.",
            "not while the signal is down, not with a glyph waiting on you, and not when there is nothing to fix.",
        ],
    },
    Section {
        title: "the tailor",
        lines: &[
            "the mirror edits your face. up and down pick a row: hood, eyes, coat, mark. left and right walk its rack.",
            "t turns the tint. r shuffles the lot. s wears it. enter or esc leaves, and a draft not worn is dropped.",
            "the face is on every message you post in #deadchannel and on your profile.",
        ],
    },
    Section {
        title: "the rest of the row",
        lines: &[
            "the lockers, the bands, dead air, the board, and the bits machine are catalogs. nothing is for sale in them yet.",
            "the carts, the reader, and the stairs talk when you press enter. the railing over the drop shows the lower city until enter.",
        ],
    },
    Section {
        title: "the wire",
        lines: &[
            "#deadchannel is the game's own log. what happens to runners posts there as news: a signal dropped, a level gained, a first kill, a near miss, the last ration of the day.",
            "your name there wears your mark and your level: grey to 4, amber to 9, phosphor to 14, white at 15.",
            "p on a runner's message opens their profile, and under the bio a runner sees the runner: level, kit, glyphs down. people upstairs do not.",
            "/leave in #deadchannel shuts the door. the runner keeps its face and waits. /join #deadchannel opens it again.",
        ],
    },
];
