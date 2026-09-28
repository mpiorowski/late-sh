//! The undercity guide's copy: every key and every rule of the street,
//! the fight, the counters, and the wire, in the voice's register. Kept
//! out of the site guide (`app/help_modal`) on purpose: that one is fed to
//! the bot, and the street is for runners to read themselves.
//!
//! This is the one place a runner is told how the city works, so it is
//! kept exact: a key, a price, a rule, or a counter that changes changes
//! here in the same change (CONTEXT.md, §3b). Every noun passes the
//! screenshot test (static, signal, glyph, ration, bit, wire), lowercase.
//!
//! Prose carries two marks the renderer reads: `` `f` `` is a key, drawn
//! as a lit chip, and `*40,000 chips*` is a number or a name that matters,
//! drawn bright. Nothing else in the copy uses a backtick or an asterisk.

use crate::app::deadchannel::city::map::Neon;

/// One heading, the neon it burns in, and what sits under it.
pub struct Section {
    pub title: &'static str,
    pub neon: Neon,
    pub blocks: &'static [Block],
}

/// The pieces a section is built from, each drawn its own way so the
/// guide reads at a glance instead of as a wall.
pub enum Block {
    /// The game's loop, stage by stage, drawn as one chain of arrows.
    Loop(&'static [&'static str]),
    /// What the whole game pays, boxed apart: a title and its lines.
    Prize {
        title: &'static str,
        lines: &'static [&'static str],
    },
    /// Keys and what each does, laid out as a grid of chips.
    Keys(&'static [Key]),
    /// Numbers that run the day, the number bright and its meaning beside.
    Figures(&'static [Figure]),
    /// One rule, a sentence or two, as a bullet. Marks allowed.
    Rule(&'static str),
}

pub struct Key {
    pub key: &'static str,
    pub does: &'static str,
}

pub struct Figure {
    pub value: &'static str,
    pub means: &'static str,
}

/// The guide, top to bottom. The first section is the whole game in a
/// screen, for the runner who reads nothing else; every rule it states is
/// spelled out again in its own section under it.
pub const SECTIONS: &[Section] = &[
    Section {
        title: "the short version",
        neon: Neon::Amber,
        blocks: &[
            Block::Loop(&[
                "fight glyphs",
                "buy gear",
                "hit level 15",
                "kill the Old Signal",
                "go again",
            ]),
            Block::Prize {
                title: "the prize",
                lines: &[
                    "*40,000 chips* for putting down the Old Signal, once every 30 days",
                    "a *mark* behind your name on the wire that never comes off",
                ],
            },
            Block::Keys(&[
                Key {
                    key: "←↑↓→",
                    does: "walk (hjkl too)",
                },
                Key {
                    key: "enter",
                    does: "go in a door",
                },
                Key {
                    key: "f",
                    does: "fight",
                },
                Key {
                    key: "a",
                    does: "attack",
                },
                Key {
                    key: "r",
                    does: "run",
                },
                Key {
                    key: "p",
                    does: "patch to full",
                },
                Key {
                    key: "0",
                    does: "back up",
                },
                Key {
                    key: "?",
                    does: "this guide",
                },
            ]),
            Block::Figures(&[
                Figure {
                    value: "10",
                    means: "fights a day",
                },
                Figure {
                    value: "00:00 utc",
                    means: "signal and fights refill",
                },
                Figure {
                    value: "0 signal",
                    means: "the street takes your bits, done till the refill",
                },
            ]),
            Block::Rule(
                "*gear wins fights.* at the armorer `w` buys the weapon and `a` the armor, bits only, one tier above what you carry. buy every tier you can afford before you step in.",
            ),
            Block::Rule(
                "`p` heals to full from anywhere for a bit a point, times your level. patch before a fight, not after a drop.",
            ),
            Block::Rule(
                "at *level 15* with the exp to leave it, the next step into the static meets *the Old Signal*: *240* signal, *36* attack, *22* defense, and no bits.",
            ),
            Block::Rule(
                "put it down and you wake at level 1 with bare hands and fifty bits. the *mark* is your paragon level (▚3╬2 on the wire): a point of attack and defense each, up to five, and every level costs a little more.",
            ),
        ],
    },
    Section {
        title: "the street",
        neon: Neon::Cyan,
        blocks: &[
            Block::Keys(&[
                Key {
                    key: "←↑↓→",
                    does: "walk (hjkl too)",
                },
                Key {
                    key: "shift+walk",
                    does: "run six steps, stopping at the first door",
                },
                Key {
                    key: "enter",
                    does: "at a door: go in",
                },
                Key {
                    key: "enter/esc",
                    does: "inside: back out to the street",
                },
                Key {
                    key: "0",
                    does: "back up to the clubhouse",
                },
                Key {
                    key: "esc",
                    does: "with nothing open: up to the chat, #lounge open",
                },
                Key {
                    key: "?",
                    does: "this guide",
                },
            ]),
            Block::Rule(
                "`enter` at the wire, the cable by the stairs, also goes back up to the clubhouse.",
            ),
            Block::Rule(
                "you are not alone down here. every runner who came down stands on the street until they log out: lit and carrying a light while they are looking, dim where they left off while they are somewhere else upstairs. so are you.",
            ),
            Block::Rule(
                "this guide opened by itself the first time you came down, and never will again.",
            ),
        ],
    },
    Section {
        title: "the sheet",
        neon: Neon::Green,
        blocks: &[
            Block::Rule(
                "the strip top right is you: level, signal, rations, bits, and what you carry.",
            ),
            Block::Rule(
                "*signal* is your health, ten a level. at zero you are off the row until the roll, and the street keeps every bit you had on you.",
            ),
            Block::Rule(
                "*rations* are fights. ten a day, one spent for every step into the static.",
            ),
            Block::Rule(
                "*bits* are the city's money. chips never come down here and never buy anything that hits.",
            ),
            Block::Rule(
                "*the roll* is midnight utc: signal and rations refill together, and nothing refills before then.",
            ),
            Block::Rule(
                "upstairs the frame reads your rations and signal for you while you are off the street.",
            ),
        ],
    },
    Section {
        title: "the static",
        neon: Neon::White,
        blocks: &[
            Block::Keys(&[
                Key {
                    key: "enter",
                    does: "at the screen: spend a ration, meet a glyph",
                },
                Key {
                    key: "f",
                    does: "the same, from anywhere on the street",
                },
                Key {
                    key: "a",
                    does: "attack",
                },
                Key {
                    key: "r/esc",
                    does: "run: two times in three it works",
                },
                Key {
                    key: "enter",
                    does: "close a finished scene",
                },
            ]),
            Block::Rule(
                "the screen at the end of the row is where the glyphs come from. a run that fails gives the glyph a free swing. there is no stepping out of a fight.",
            ),
            Block::Rule(
                "a dropped connection finds the fight waiting on the row when you step back in, for no ration.",
            ),
            Block::Rule(
                "a kill pays *bits* and *exp*. exp climbs the levels, up to *15*, and the wire hears every one.",
            ),
            Block::Rule(
                "a dropped signal ends your day: the street takes the bits on you, you keep most of the exp, and the row is shut until the roll.",
            ),
        ],
    },
    Section {
        title: "the old signal",
        neon: Neon::Red,
        blocks: &[
            Block::Rule(
                "at *15*, once you have the exp to leave it, the next step in meets the Old Signal instead of a glyph.",
            ),
            Block::Rule(
                "put it down and you wake at *level 1*: bare hands, fifty bits, and a mark. the mark stays, adds a point of attack and defense (up to five), and makes every level cost a little more.",
            ),
            Block::Rule(
                "a kill pays *40,000 chips*, at most *once every 30 days*: the one way bits ever turn into chips. a second mark inside the month is yours all the same. your face, your tailor's rack, and your badges stay. the first kill earns [SIG].",
            ),
        ],
    },
    Section {
        title: "the armorer",
        neon: Neon::Red,
        blocks: &[
            Block::Keys(&[
                Key {
                    key: "↑↓",
                    does: "walk the wall, tier 1 to 15",
                },
                Key {
                    key: "w",
                    does: "buy the picked weapon",
                },
                Key {
                    key: "a",
                    does: "buy the picked armor",
                },
            ]),
            Block::Rule(
                "bits only, and only a tier above the one you carry. the piece you hand back comes off the price at *three quarters* of what it cost.",
            ),
        ],
    },
    Section {
        title: "patch",
        neon: Neon::White,
        blocks: &[
            Block::Rule(
                "`p` opens patch from anywhere on the street. `p` again buys the signal back to full. *a bit a point, times your level.*",
            ),
            Block::Rule(
                "not while the signal is down, not with a glyph waiting on you, not once you are spent for the day (the roll brings it back for nothing), and not when there is nothing to fix.",
            ),
        ],
    },
    Section {
        title: "the tailor",
        neon: Neon::Magenta,
        blocks: &[
            Block::Keys(&[
                Key {
                    key: "↑↓",
                    does: "pick a row: hood, eyes, coat, mark",
                },
                Key {
                    key: "←→",
                    does: "walk its rack",
                },
                Key {
                    key: "t",
                    does: "turn the tint",
                },
                Key {
                    key: "r",
                    does: "shuffle the lot",
                },
                Key {
                    key: "s",
                    does: "wear it",
                },
                Key {
                    key: "enter/esc",
                    does: "leave: a draft not worn is dropped",
                },
            ]),
            Block::Rule(
                "the mirror edits your face. the rack opens by level: three pieces a slot and a tint every three levels, white alone at 15. what you have opened stays opened.",
            ),
            Block::Rule(
                "the face is on every message you post in #deadchannel and on your profile.",
            ),
        ],
    },
    Section {
        title: "the rest of the row",
        neon: Neon::Cyan,
        blocks: &[
            Block::Rule(
                "the lockers, the bands, dead air, the board, and the bits machine are catalogs. nothing is for sale in them yet.",
            ),
            Block::Rule(
                "the carts, the reader, and the stairs talk when you press `enter`. the railing over the drop shows the lower city until `enter`.",
            ),
        ],
    },
    Section {
        title: "the wire",
        neon: Neon::Green,
        blocks: &[
            Block::Rule(
                "*#deadchannel* is the game's own log. what happens to runners posts there as news: a signal dropped, a level gained, the Old Signal put down, a first kill, a near miss, the last ration of the day.",
            ),
            Block::Rule(
                "your name there wears your mark and your level in the newest tint it opened: grey to 3, phosphor to 6, cyan to 9, magenta to 12, red to 14, white at 15. marks ride behind the Signal's glyph: ▚3╬2.",
            ),
            Block::Rule(
                "`p` on a runner's message opens their profile, and under the bio a runner sees the runner: level, kit, glyphs down. people upstairs do not.",
            ),
            Block::Rule(
                "/leave in #deadchannel shuts the door. the runner keeps its face and waits. /join #deadchannel opens it again.",
            ),
        ],
    },
];
