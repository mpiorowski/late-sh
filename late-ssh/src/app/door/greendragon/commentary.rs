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
    /// The etchings in the Dark Horse's tables (`modules/darkhorse.php`,
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
}

impl CommentRoom {
    /// The shared-table section this room reads and writes.
    pub fn section(self) -> &'static str {
        match self {
            CommentRoom::Village => "village",
            CommentRoom::Inn => "inn",
            CommentRoom::DarkHorse => "darkhorse",
            CommentRoom::Gardens => "gardens",
            CommentRoom::Veterans => "veterans",
            CommentRoom::ShadeGypsy | CommentRoom::ShadeGrave => "shade",
        }
    }

    /// The room's display window (upstream's per-call `$limit`), also the
    /// base of the daily post allowance: village 25, inn 20, Dark Horse 10
    /// (the default), shade 25, gardens and the rock 30.
    pub fn display_limit(self) -> usize {
        match self {
            CommentRoom::Village => 25,
            CommentRoom::Inn => 20,
            CommentRoom::DarkHorse => 10,
            CommentRoom::Gardens | CommentRoom::Veterans => 30,
            CommentRoom::ShadeGypsy | CommentRoom::ShadeGrave => 25,
        }
    }

    /// The venue's talk verb. Anything but "says" is baked into non-emote
    /// posts at post time (upstream converts them to `:verb, "..."`).
    pub fn verb(self) -> &'static str {
        match self {
            CommentRoom::Village | CommentRoom::Inn | CommentRoom::DarkHorse => "says",
            CommentRoom::Gardens => "whispers",
            CommentRoom::Veterans => "boasts",
            CommentRoom::ShadeGypsy => "projects",
            CommentRoom::ShadeGrave => "despairs",
        }
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
/// post again").
pub fn posts_left(lines: &[CommentLine], me: Uuid, room: CommentRoom) -> usize {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn line(user: Uuid, today: bool) -> CommentLine {
        CommentLine {
            user_id: Some(user),
            name: "Tester".into(),
            body: "hello".into(),
            today,
        }
    }

    #[test]
    fn allowance_is_half_the_window_rounded_up() {
        // round(25/2)=13, round(20/2)=10, round(10/2)=5, round(30/2)=15.
        assert_eq!(posts_allowed(CommentRoom::Village.display_limit()), 13);
        assert_eq!(posts_allowed(CommentRoom::Inn.display_limit()), 10);
        assert_eq!(posts_allowed(CommentRoom::DarkHorse.display_limit()), 5);
        assert_eq!(posts_allowed(CommentRoom::Gardens.display_limit()), 15);
    }

    #[test]
    fn posts_left_counts_only_my_posts_from_today() {
        let me = Uuid::from_u128(1);
        let other = Uuid::from_u128(2);
        let lines = vec![
            line(me, true),
            line(me, true),
            line(me, false), // yesterday's post scrolled back in: free
            line(other, true),
        ];
        assert_eq!(posts_left(&lines, me, CommentRoom::DarkHorse), 3);
        assert_eq!(posts_left(&lines, other, CommentRoom::DarkHorse), 4);
    }

    #[test]
    fn says_rooms_store_the_body_untouched() {
        assert_eq!(
            prepare_post("  hello there  ", "says").unwrap(),
            "hello there"
        );
    }

    #[test]
    fn verb_rooms_bake_the_venue_verb() {
        assert_eq!(
            prepare_post("who turned out the light", "despairs").unwrap(),
            ":despairs, \"who turned out the light\""
        );
        // An explicit emote keeps its own action, any venue.
        assert_eq!(
            prepare_post(":rattles his chains", "despairs").unwrap(),
            ":rattles his chains"
        );
    }

    #[test]
    fn empty_and_bare_marker_posts_are_rejected() {
        for raw in ["", "   ", ":", "::", "/me", " /me "] {
            assert!(prepare_post(raw, "says").is_none(), "{raw:?}");
        }
    }

    #[test]
    fn long_runs_are_broken_like_upstream() {
        let raw = "a".repeat(100);
        let broken = prepare_post(&raw, "says").unwrap();
        // A space after char 45, then after 46 more (the breaker starts the
        // next window's count at zero, like upstream's consumed `$2`).
        assert_eq!(
            broken,
            format!("{} {} {}", "a".repeat(45), "a".repeat(46), "a".repeat(9))
        );
    }

    #[test]
    fn composition_quotes_speech_and_unfolds_emotes() {
        assert_eq!(compose_line("Ada", "hello"), "Ada says, \"hello\"");
        assert_eq!(compose_line("Ada", ":waves"), "Ada waves");
        assert_eq!(compose_line("Ada", "/me waves"), "Ada waves");
        assert_eq!(
            compose_line("Ada", ":despairs, \"why\""),
            "Ada despairs, \"why\""
        );
        // System lines render bare.
        assert_eq!(compose_line("", "The ground shakes."), "The ground shakes.");
    }

    #[test]
    fn verb_rooms_shrink_the_typing_budget() {
        assert_eq!(max_post_len("says"), 200);
        assert_eq!(max_post_len("despairs"), 181);
    }
}
