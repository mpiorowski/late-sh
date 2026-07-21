//! The commentary engine: LoGD's one chat primitive (`lib/commentary.php`),
//! ported 1=1. Every talk room is a `section` of one shared table; this
//! module owns the pure rules — the room table (sections, display limits,
//! venue verbs), post preparation (trimming, run-breaking, emote baking,
//! rejections), the daily post allowance, and line composition for display.
//! The DB round-trips live in `svc`; the menus and typing state in `state`.
//!
//! Upstream quirks kept faithfully: the daily allowance is counted **among
//! the room's newest `display_limit` rows only** — once your posts scroll out
//! of the window, you may speak again; a non-"says" venue bakes its verb into
//! the body at post time (`:verb, "..."`), so a lament posted in the
//! graveyard still "despairs" when read through the gypsy's trance.

use rand::Rng;
use uuid::Uuid;

/// One comment as loaded for a room view (newest first from `svc`).
#[derive(Clone, Debug)]
pub struct CommentLine {
    /// The speaker; `None` is a system line.
    pub user_id: Option<Uuid>,
    /// The speaker's character name, snapshotted at post time.
    pub name: String,
    /// The stored body (emotes keep their marker; non-"says" venues arrive
    /// pre-baked as `:verb, "..."`).
    pub body: String,
    /// Whether the comment was posted today (feeds the post allowance).
    pub today: bool,
    /// The UTC day-number it was posted (feeds the new-post marker: on or
    /// after the reader's watermark renders marked).
    pub day: i64,
}

/// A commentary room: a section of the shared table plus its venue dressing.
/// Both shade variants read and write the same section — only the venue verb
/// and the way back differ, exactly like upstream's gypsy/graveyard pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommentRoom {
    /// The village square (`village.php`, section "village").
    Village,
    /// The Sleeping Stag's table talk (`inn.php`, section "inn").
    Inn,
    /// The etchings in the Crooked Wheel's tables (`modules/darkhorse.php`,
    /// section "darkhorse").
    DarkHorse,
    /// The gardens (`gardens.php`, section "gardens"): a pure social corner.
    Gardens,
    /// The veterans' rock (`rock.php`, section "veterans"), dragon-killers
    /// only.
    Veterans,
    /// The shade channel through the gypsy's paid trance (`gypsy.php`,
    /// section "shade").
    ShadeGypsy,
    /// The shade channel from the other side, free while dead (`shades.php`,
    /// same section).
    ShadeGrave,
    /// The clan lobby's waiting area (`lib/clan/waiting.php`, the one
    /// "waiting" section shared by every clan's hopefuls and members).
    Waiting,
    /// A clan's own hall (`clan_default.php`, section `clan-{id}`): speaks
    /// in the clan's custom verb and is the one venue exempt from the daily
    /// allowance (`talkform` skips the count for `clan-*` sections).
    ClanHall(Uuid),
}

impl CommentRoom {
    /// The shared-table section this room reads and writes.
    pub fn section(self) -> String {
        match self {
            CommentRoom::Village => "village".into(),
            CommentRoom::Inn => "inn".into(),
            CommentRoom::DarkHorse => "darkhorse".into(),
            CommentRoom::Gardens => "gardens".into(),
            CommentRoom::Veterans => "veterans".into(),
            CommentRoom::ShadeGypsy | CommentRoom::ShadeGrave => "shade".into(),
            CommentRoom::Waiting => "waiting".into(),
            CommentRoom::ClanHall(id) => format!("clan-{id}"),
        }
    }

    /// The room's display window (upstream's per-call `$limit`), also the
    /// base of the daily post allowance: village 25, inn 20, Crooked Wheel 10
    /// (the default), shade 25, gardens and the rock 30, the waiting area
    /// and clan halls 25.
    pub fn display_limit(self) -> usize {
        match self {
            CommentRoom::Village => 25,
            CommentRoom::Inn => 20,
            CommentRoom::DarkHorse => 10,
            CommentRoom::Gardens | CommentRoom::Veterans => 30,
            CommentRoom::ShadeGypsy | CommentRoom::ShadeGrave => 25,
            CommentRoom::Waiting | CommentRoom::ClanHall(_) => 25,
        }
    }

    /// The venue's talk verb. Anything but "says" is baked into non-emote
    /// posts at post time (upstream converts them to `:verb, "..."`). A
    /// clan hall's is only the fallback — the clan's custom verb, when set,
    /// overrides it at the call sites (the session holds the clan row).
    pub fn verb(self) -> &'static str {
        match self {
            CommentRoom::Village | CommentRoom::Inn | CommentRoom::DarkHorse => "says",
            CommentRoom::Gardens => "whispers",
            CommentRoom::Veterans => "boasts",
            CommentRoom::ShadeGypsy => "projects",
            CommentRoom::ShadeGrave => "despairs",
            CommentRoom::Waiting | CommentRoom::ClanHall(_) => "says",
        }
    }

    /// Whether the daily allowance is skipped here: upstream's `talkform`
    /// never counts posts for `clan-*` sections — clan mates chat without
    /// limit (the waiting area is *not* exempt).
    pub fn allowance_exempt(self) -> bool {
        matches!(self, CommentRoom::ClanHall(_))
    }
}

/// Daily posts allowed in a room (upstream `round(limit/2)`), counted among
/// the newest `display_limit` rows only — see [`posts_left`].
pub fn posts_allowed(display_limit: usize) -> usize {
    display_limit.div_ceil(2)
}

/// Posts the player may still make: the allowance minus their posts from
/// today **within the loaded window**. Once older posts scroll out of the
/// window they stop counting, exactly as upstream ("once some of your
/// existing posts have moved out of the comment area, you'll be allowed to
/// post again"). Allowance-exempt venues (clan halls) report a bottomless
/// count.
pub fn posts_left(lines: &[CommentLine], me: Uuid, room: CommentRoom) -> usize {
    if room.allowance_exempt() {
        return usize::MAX;
    }
    let used = lines
        .iter()
        .filter(|l| l.today && l.user_id == Some(me))
        .count();
    posts_allowed(room.display_limit()).saturating_sub(used)
}

/// The longest raw line a venue accepts (upstream's talkform `maxlength`:
/// 200, less `strlen(verb) + 11` where the baked emote prefix will be added).
pub fn max_post_len(verb: &str) -> usize {
    if verb == "says" {
        200
    } else {
        200 - (verb.len() + 11)
    }
}

/// Prepare a typed line for the table (upstream `injectcommentary`): trim,
/// break unspaced 45-character runs, and bake the venue verb into non-emote
/// posts. Returns `None` for an empty or bare-marker post (the "silence"
/// rejection).
pub fn prepare_post(raw: &str, verb: &str) -> Option<String> {
    let body = break_long_runs(raw.trim());
    if body.is_empty() || body == ":" || body == "::" || body == "/me" {
        return None;
    }
    if verb != "says" && !is_emote(&body) {
        return Some(format!(":{verb}, \"{body}\""));
    }
    Some(body)
}

/// Leading `:` (which covers `::`) or `/me` marks a third-person action.
fn is_emote(body: &str) -> bool {
    body.starts_with(':') || body.starts_with("/me")
}

/// The drinks module's commentary hook (`modules/drinks/dohook.php`), fired
/// exactly where upstream's `modulehook("commentary")` sits — before the
/// run-breaking and verb baking of [`prepare_post`]: above 50 drunkenness
/// the venue verb gains "drunkenly" (which also turns a "says" room's post
/// into a baked emote, as upstream's `!= "says"` test then trips), and any
/// non-emote line is slurred while drunk at all. Returns `(line, verb)`.
pub fn apply_drunkenness(
    raw: &str,
    verb: &str,
    drunkenness: u32,
    rng: &mut impl Rng,
) -> (String, String) {
    let verb = if drunkenness > 50 {
        format!("drunkenly {verb}")
    } else {
        verb.to_string()
    };
    let line = raw.trim();
    let body = if drunkenness > 0 && !line.is_empty() && !is_emote(line) {
        drunkenize(line, drunkenness, rng)
    } else {
        line.to_string()
    };
    (body, verb)
}

/// The letter → slur table (`drunkenize.php`), byte-for-byte.
const SLURS: [(u8, &str); 16] = [
    (b'a', "aa"),
    (b'e', "ee"),
    (b'f', "ff"),
    (b'h', "hh"),
    (b'i', "iy"),
    (b'l', "ll"),
    (b'm', "mm"),
    (b'n', "nn"),
    (b'o', "oo"),
    (b'r', "rr"),
    (b's', "sss"),
    (b'u', "oo"),
    (b'v', "vv"),
    (b'w', "ww"),
    (b'y', "yy"),
    (b'z', "zz"),
];

/// Whether a `*hic*` sits at `i` or starts up to four bytes before it.
fn near_hic(s: &[u8], i: usize) -> bool {
    (0..5).any(|back| {
        let at = i.saturating_sub(back);
        s.len() >= at + 5 && &s[at..at + 5] == b"*hic*"
    })
}

/// Slur a drunk line (`drinks_drunkenize`, ported 1=1): until the
/// replacement count reaches `drunkenness/500` of the original length,
/// either (9-in-10) double the *first* occurrence of a random slur letter —
/// case-matched, skipped when it sits inside a `*hic*` — or (1-in-10) drop
/// a `*hic*` at a random spot, nudged forward out of an existing one by
/// upstream's five stagger checks. Repeated picks of the same letter compound
/// at the first occurrence ("aa" → "aaa"), exactly the upstream quirk.
/// Adjacent hics collapse to `*hic*hic*` afterward. (Upstream's backtick
/// color-code skip and its `noslur` player pref have no analog here: bodies
/// carry no color codes and we ship no per-player prefs.)
pub fn drunkenize(line: &str, drunkenness: u32, rng: &mut impl Rng) -> String {
    let base_len = line.len().max(1);
    let mut s: Vec<u8> = line.as_bytes().to_vec();
    let mut replacements: usize = 0;
    // PHP: while (replacements/strlen(straight) < level/500).
    while replacements * 500 < drunkenness as usize * base_len {
        if rng.gen_range(0..=9) != 0 {
            let (letter, slur) = SLURS[rng.gen_range(0..SLURS.len())];
            if let Some(x) = s.iter().position(|b| b.to_ascii_lowercase() == letter)
                && !near_hic(&s, x)
            {
                let rep = if s[x] != letter {
                    slur.to_uppercase()
                } else {
                    slur.to_string()
                };
                s.splice(x..=x, rep.into_bytes());
                replacements += 1;
            }
        } else {
            let mut x = rng.gen_range(0..=s.len());
            let hic_at = |s: &[u8], at: usize| s.len() >= at + 5 && &s[at..at + 5] == b"*hic*";
            // Upstream's five sequential shifts, each reading the moved x.
            if hic_at(&s, x) {
                x += 5;
            }
            if hic_at(&s, x.saturating_sub(1)) {
                x += 4;
            }
            if hic_at(&s, x.saturating_sub(2)) {
                x += 3;
            }
            if hic_at(&s, x.saturating_sub(3)) {
                x += 2;
            }
            if hic_at(&s, x.saturating_sub(4)) {
                x += 1;
            }
            let x = x.min(s.len());
            s.splice(x..x, *b"*hic*");
            replacements += 1;
        }
    }
    let mut out = String::from_utf8_lossy(&s).into_owned();
    while out.contains("*hic**hic*") {
        out = out.replace("*hic**hic*", "*hic*hic*");
    }
    out
}

/// Insert a space after any 45-character unbroken run (upstream's
/// `([^\s]{45})([^\s])` → `$1 $2`, applied left to right).
fn break_long_runs(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 45);
    let mut run = 0usize;
    for ch in s.chars() {
        if ch.is_whitespace() {
            run = 0;
        } else {
            run += 1;
            if run > 45 {
                out.push(' ');
                // The breaking character starts outside the next window,
                // like the consumed `$2` of upstream's match.
                run = 0;
            }
        }
        out.push(ch);
    }
    out
}

/// Compose a stored comment into its rendered line (upstream's view path):
/// an emote marker swaps in the speaker's name; a system line (no name)
/// renders bare; anything else is quoted speech.
pub fn compose_line(name: &str, body: &str) -> String {
    for marker in ["::", ":", "/me"] {
        if let Some(rest) = body.strip_prefix(marker) {
            return format!("{name} {}", rest.trim_start());
        }
    }
    if name.is_empty() {
        return body.to_string();
    }
    format!("{name} says, \"{body}\"")
}
