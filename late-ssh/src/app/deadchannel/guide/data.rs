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
                "walk the road",
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
                    does: "the road",
                },
                Key {
                    key: "1-5",
                    does: "play a card",
                },
                Key {
                    key: "e",
                    does: "end your turn",
                },
                Key {
                    key: "a",
                    does: "auto: play the turn for me",
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
                    means: "steps of road a day, five of them fights",
                },
                Figure {
                    value: "00:00 utc",
                    means: "signal and steps refill, and the road is a new one",
                },
                Figure {
                    value: "0 signal",
                    means: "the street takes your bits, done till the refill",
                },
            ]),
            Block::Rule(
                "`f` opens *the road*: ten steps, three lanes, the same road for every runner today. each step goes to your lane or the one beside it. half the steps are glyphs; the rest are a *rest* or a *cache*.",
            ),
            Block::Rule(
                "a fight is a *hand of cards*: five drawn, three energy a turn, and the glyph shows its next move before you play. `a` plays the obvious turn for you, every turn if you like. at levels *3*, *6*, *9*, and *12* you pick a *new card* for the deck.",
            ),
            Block::Rule(
                "*gear is the cards.* your weapon is what a strike hits for, your armor what a block holds. at the armorer `w` buys the weapon and `a` the armor, bits only. a level pays for about *one tier of each*: every level you gain, go back for the next piece.",
            ),
            Block::Rule(
                "the road shows you the fight before you take it. *grim* means your gear is behind: `g` fights the glyph a level below, for half the pay.",
            ),
            Block::Rule(
                "`p` heals to full from anywhere for a bit a point, times your level. patch before a fight, not after a drop.",
            ),
            Block::Rule(
                "two nodes of every road are *bright* glyphs. they are harder, pay double bits, and always leave a *crystal*. a crystal buys a glass at dead air, and three buy the next piece up at the blade shop, no bits asked.",
            ),
            Block::Rule(
                "at *level 15* with the exp to leave it, the next glyph on the road is *the Old Signal* instead: *290* signal, *39* attack, *22* defense, and no bits.",
            ),
            Block::Rule(
                "put it down and you wake at level 1 with bare hands, fifty bits, an empty locker, and no crystals. the *mark* is your paragon level (▚3╬2 on the wire): a point of attack and defense each, up to five, and every level costs a little more.",
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
                "the strip top right is you: level, signal, rations, bits, what you owe the bits machine, your crystals, the glass in you, and what you carry.",
            ),
            Block::Rule(
                "*signal* is your health, ten a level. at zero you are off the row until the roll, and the street keeps every bit you had on you. the locker keeps the rest.",
            ),
            Block::Rule("*rations* are steps of the road. ten a day, one spent for every step."),
            Block::Rule(
                "*bits* are the city's money. chips never come down here and never buy anything that hits.",
            ),
            Block::Rule(
                "*crystals* are what bits cannot buy. a glyph of your level leaves one about one kill in twelve, a bright one always. a dropped signal never takes them; an Old Signal mark and the ledge do.",
            ),
            Block::Rule(
                "*the roll* is midnight utc: signal and rations refill together, the deck comes clean, the day's glass wears off, a new road is laid, and nothing refills before then.",
            ),
            Block::Rule(
                "upstairs the frame reads your rations and signal for you while you are off the street.",
            ),
        ],
    },
    Section {
        title: "the road",
        neon: Neon::White,
        blocks: &[
            Block::Keys(&[
                Key {
                    key: "enter",
                    does: "at the screen: open the road",
                },
                Key {
                    key: "f",
                    does: "the same, from anywhere on the street",
                },
                Key {
                    key: "↑↓",
                    does: "pick a lane the next step reaches",
                },
                Key {
                    key: "enter",
                    does: "step: fight the glyph, clear the deck at a rest, take a cache",
                },
                Key {
                    key: "g",
                    does: "on a glyph: fight the one a level down, half pay",
                },
                Key {
                    key: "s",
                    does: "once the road is over: copy the day's card",
                },
            ]),
            Block::Rule(
                "the screen at the end of the row is where the road starts. it is *ten steps* long and *three lanes* wide, the same road for every runner each day, and a new one at the roll.",
            ),
            Block::Rule(
                "a step spends a ration and goes to the lane you stand in or the one beside it. the map shows the whole road: read ahead, a rest two steps on may not be in reach from where the cache is.",
            ),
            Block::Rule(
                "*five steps* of every road are glyphs in every lane, the first and the last always. every road is five fights, whichever way you walk it.",
            ),
            Block::Rule(
                "a *rest* shakes every static card out of your deck. it never mends your signal: that is patch's.",
            ),
            Block::Rule(
                "a *cache* is bits for the taking: *half* of what the glyph of your level pays, and no fight.",
            ),
            Block::Rule(
                "under the map is what waits on the lane you picked. for a glyph: its signal, what it hits for, its moves in the order they come, its pay, and how the fight reads on the `a` key from where you stand, *easy*, *even*, *risky*, or *grim*. play the hand yourself and you beat the word.",
            ),
            Block::Rule(
                "exp levels you whether your gear is ready or not. when the glyph of your level reads grim, the one below still pays: half, but a kill is a kill. it never leaves a crystal.",
            ),
            Block::Rule(
                "*the bright glyph* sits on two nodes of every road, one lane each, so you walk to it or around it: the glyph of your level with *a third more signal*, hitting and holding harder. it pays *double bits*, the same exp, and always *a crystal*. it costs most of a signal to put down.",
            ),
            Block::Rule(
                "with your signal down or the road walked, the panel is the day's card: the lane you took, lit by how each step went. `s` copies it to paste anywhere.",
            ),
            Block::Rule(
                "a kill pays *bits* and *exp*. exp climbs the levels, up to *15*, and the wire hears every one.",
            ),
            Block::Rule(
                "a dropped signal ends your day where you stand: the street takes the bits on you, you keep most of the exp, and the road is shut until the roll.",
            ),
        ],
    },
    Section {
        title: "the hand",
        neon: Neon::Cyan,
        blocks: &[
            Block::Keys(&[
                Key {
                    key: "1-5",
                    does: "play the card in that slot",
                },
                Key {
                    key: "e",
                    does: "end your turn (space and enter too)",
                },
                Key {
                    key: "a",
                    does: "auto: play the obvious turn and end it",
                },
                Key {
                    key: "r/esc",
                    does: "run",
                },
                Key {
                    key: "enter",
                    does: "after the fight: back to the road",
                },
            ]),
            Block::Rule(
                "everyone starts with the same ten cards: five *strikes*, three *blocks*, a *surge*, a *wipe*. five are drawn a turn, you have *three energy* to play them, and what you do not play is discarded.",
            ),
            Block::Rule(
                "a *strike* hits for your attack less half the glyph's defense. a *surge* is two and a half strikes for two energy. the number is on the card, and a better weapon makes it bigger.",
            ),
            Block::Rule(
                "a *block* holds a bit over half your defense and *stays up until a hit eats it*. put it up while the glyph gathers and it is still there when the heavy lands.",
            ),
            Block::Rule(
                "the glyph shows its next move before you play: it *hits*; it *gathers*, which is nothing this turn and a *heavy* for double on the next; or it throws *noise*, two static cards and no damage. every glyph has its own order of moves and keeps to it.",
            ),
            Block::Rule(
                "a hit that gets through leaves a *static* card in your deck for the rest of the day. it does nothing but take a place in your hand. one energy throws it out for good, a *wipe* clears every one in your hand and blocks too, and a rest clears the deck. five is the most a deck holds.",
            ),
            Block::Rule(
                "`r` runs, and there are no dice in it: what the glyph meant to do this turn lands on your way out, against whatever block is up. run while it gathers and it costs nothing. the step stays spent and pays nothing. there is no other way out of a fight.",
            ),
            Block::Rule(
                "`a` is a fine way to play, and it plays every card you can draft. when a heavy lands it mutes it or puts two blocks up, it hits with everything else, and energy left over goes on a block if something is landing. it never gives up a hit to block a plain one and never thinks a turn ahead. the hand played well takes about a third of the damage.",
            ),
            Block::Rule(
                "a dropped connection finds the fight waiting on the row, the same hand in it, when you step back in.",
            ),
        ],
    },
    Section {
        title: "new cards",
        neon: Neon::Amber,
        blocks: &[
            Block::Rule(
                "at levels *3*, *6*, *9*, and *12* a new card is waiting for you on the road: one of two, the same two for everybody. `1` or `2` takes it, and the road waits until you do.",
            ),
            Block::Rule(
                "the card goes in *in place of* a strike or a block, so the deck is always ten cards. it stays until the mark; the next climb you pick again.",
            ),
            Block::Rule(
                "*level 3*, for a strike. *jab*: free, hits for three quarters of a strike. or *siphon*: a strike that mends you for half of what it hits.",
            ),
            Block::Rule(
                "*level 6*, for a block. *riposte*: hits for a block's worth plus all the block you have up, and the block stays. or *bulwark*: two energy, holds two blocks and a half.",
            ),
            Block::Rule(
                "*level 9*, for a strike, and both put static to work. *burn*: free, two more energy this turn, and one more again for every static card in your hand, burned up. or *ground*: a strike, plus one more for every static card in your hand, thrown out with it.",
            ),
            Block::Rule(
                "*level 12*, for a strike. *sever*: a strike that hits for two and a half once the glyph is at half its signal or less. or *mute*: two energy, and whatever the glyph meant to do this turn does nothing. a glyph that is only gathering has nothing to mute.",
            ),
            Block::Rule(
                "every number on a new card comes from your strike or your block, so the armorer makes them all bigger.",
            ),
        ],
    },
    Section {
        title: "the old signal",
        neon: Neon::Red,
        blocks: &[
            Block::Rule(
                "at *15*, once you have the exp to leave it, the next glyph you step onto is the Old Signal instead. it hits, floods you with noise, gathers, and comes down, and it has more signal than anything on the road: read the hand for this one.",
            ),
            Block::Rule(
                "put it down and you wake at *level 1*: bare hands, fifty bits, an empty locker, no crystals, and a mark. the mark stays, adds a point of attack and defense (up to five), and makes every level cost a little more. the debt stays too.",
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
            Block::Rule(
                "the whole wall is for sale from day one, if you have the bits. *a tier behind your level is a fight; two behind is a drop.* a tier ahead buys very little.",
            ),
        ],
    },
    Section {
        title: "patch",
        neon: Neon::White,
        blocks: &[
            Block::Rule(
                "`p` opens patch from anywhere on the street. `p` again buys the signal back to full. *a bit a point, times your level.* it does not touch the static in your deck.",
            ),
            Block::Rule(
                "not while the signal is down, not with a glyph waiting on you, not once you are spent for the day (the roll brings it back for nothing), and not when there is nothing to fix.",
            ),
        ],
    },
    Section {
        title: "the lockers",
        neon: Neon::Cyan,
        blocks: &[
            Block::Keys(&[
                Key {
                    key: "d",
                    does: "lock up everything on you",
                },
                Key {
                    key: "w",
                    does: "take everything out",
                },
            ]),
            Block::Rule(
                "a dropped signal never reaches the locker. it keeps *a tenth* of every deposit, rounded up, so a single bit is not worth the walk. taking it out is free.",
            ),
            Block::Rule(
                "not with a glyph waiting on you. an Old Signal mark empties it, and so does the ledge.",
            ),
        ],
    },
    Section {
        title: "the bits machine",
        neon: Neon::Amber,
        blocks: &[
            Block::Keys(&[
                Key {
                    key: "b",
                    does: "borrow up to the cap",
                },
                Key {
                    key: "r",
                    does: "feed it what you carry",
                },
            ]),
            Block::Rule(
                "it lends *fifty bits a level*, less what you already owe. enough at level 2 for a first weapon and a first coat.",
            ),
            Block::Rule(
                "it adds *a tenth* of every loan to the debt, rounded up, once. the debt never grows after that, and *half* of every glyph's and every cache's bits goes to it until you are square.",
            ),
            Block::Rule("a drop does not clear it. neither does a mark, or the ledge."),
        ],
    },
    Section {
        title: "dead air",
        neon: Neon::Amber,
        blocks: &[
            Block::Keys(&[
                Key {
                    key: "s",
                    does: "static on ice: attack",
                },
                Key {
                    key: "d",
                    does: "dead air, neat: defense",
                },
                Key {
                    key: "t",
                    does: "test pattern: more signal, and filled",
                },
            ]),
            Block::Rule(
                "the bar pours for *a crystal a glass*, *one glass a day*, and it wears off at the roll. the first two add a point and one more every four levels; the test pattern adds two signal a level and fills you.",
            ),
            Block::Rule(
                "not while the signal is down, not with a glyph waiting on you, and not once you are spent for the day.",
            ),
        ],
    },
    Section {
        title: "the blade shop",
        neon: Neon::Red,
        blocks: &[
            Block::Keys(&[
                Key {
                    key: "w",
                    does: "the next weapon up from yours",
                },
                Key {
                    key: "a",
                    does: "the next armor up from yours",
                },
            ]),
            Block::Rule(
                "the next tier up from what you carry, for *three crystals* and no bits. the armorer sells the same piece for bits, and the panel shows you what the wall asks. nothing is made past the top of the wall.",
            ),
        ],
    },
    Section {
        title: "the ledge",
        neon: Neon::White,
        blocks: &[
            Block::Keys(&[
                Key {
                    key: "r",
                    does: "at the railing: lean out",
                },
                Key {
                    key: "r again",
                    does: "step off",
                },
            ]),
            Block::Rule(
                "a step off starts you over: *level 1*, bare hands, nothing on you, no crystals, and nothing in the locker. any other key leans you back in.",
            ),
            Block::Rule(
                "your marks, your peak, your kills, your face, today's rations and the static in your deck, and the debt come down with you. not while your signal is down, not with nothing to lose, and the wire hears it.",
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
            Block::Rule("the bands and the board are catalogs. nothing is for sale in them yet."),
            Block::Rule(
                "a shop with its sign lit does something. a shop with its sign dark is *not open yet*, and says so at the door.",
            ),
            Block::Rule(
                "the noodle cart, the umbrella stall, the reader, and the stairs talk when you press `enter`. the railing over the drop shows the lower city until `enter`.",
            ),
        ],
    },
    Section {
        title: "the wire",
        neon: Neon::Green,
        blocks: &[
            Block::Rule(
                "*#deadchannel* is the game's own log. what happens to runners posts there as news: a signal dropped, a level gained, the Old Signal put down, a step off the ledge, a first kill, a bright glyph put down, a near miss, the day's road walked.",
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
