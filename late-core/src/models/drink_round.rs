//! Credits bought for the house or one named patron, claimed on their order.
//!
//! Four rules live here and nowhere else.
//!
//! **What counts as asking.** [`ROUND_PHRASES`] is the closed list of things a
//! patron can say to @bartender to buy the house a round. It is deliberately
//! not a model's judgement: this is the only bartender action that spends more
//! than one drink's worth of chips, and the price scales with the room, so the
//! phrase itself is the confirmation. Two modules read this list: the
//! bartender, to decide, and `chat/slur.rs`, to keep a drunk patron's typing
//! from scrambling the one sentence that moves money. If the two ever read
//! different lists, the feature breaks for exactly the people most likely to
//! use it, so there is one list.
//!
//! **A personal gift is a phrase too.** [`GIFT_PHRASES`] is the closed list
//! of ways to put one drink on somebody else's tab, matched on the round's
//! rules (a statement that opens its clause, outside backticks), and
//! [`gift_drink_target`] accepts it only when the message names exactly one
//! recipient. `chat/slur.rs` reads both lists through
//! [`spending_phrase_spans`] so a drunk patron's spending instruction arrives
//! intact. A message is one order at most: [`bar_order`] reads both lists
//! once, and a gift and a round in the same message ring up neither.
//!
//! **What it costs.** [`ROUND_PRICE_PER_PATRON`] for every credit the round
//! actually granted, burned whole; a personal gift is one credit at
//! [`GIFT_DRINK_PRICE`]. Both pour the same [`ROUND_DRINK_POINTS`]; the
//! Nightcap's round pours 1:1 ([`Bar::drink_points`]). The one table of
//! every purchase, price and pour is in `late-ssh/src/app/chat/CONTEXT.md`
//! §9d.
//!
//! **What it hands over.** Not a drink: a [`DrinkCredit`], cashed only when the
//! patron walks up and orders one themselves. A pour makes someone type drunk
//! in public (`chat/slur.rs`), and that is not a thing to do to a person who
//! did not ask. Credits stack up to [`MAX_OPEN_CREDITS`], so a patron who was
//! heads-down through three rounds is owed three drinks rather than one, and
//! the cap rather than the schema is what stops a room being bought for
//! forever.

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};
use deadpool_postgres::GenericClient;
use tokio_postgres::{Row, Transaction};
use uuid::Uuid;

/// What the buyer pays per patron the round reaches. Matches
/// [`crate::models::drinks::DRINK_PRICE_MIN`], the cheapest thing at the bar:
/// a round is a lot of small kindnesses, not one grand one.
pub const ROUND_PRICE_PER_PATRON: i64 = 100;

/// What one named patron's drink costs the buyer. Twice the round's price a
/// head: a round pays for everyone online and most never collect, so its
/// premium pour is carried by the credits that expire. A gift is aimed at one
/// person who will drink it, so the same [`ROUND_DRINK_POINTS`] pour costs
/// more than a head of a round, and less than buying that buzz yourself.
pub const GIFT_DRINK_PRICE: i64 = 200;

/// The buzz a cashed tavern round records, regardless of what the bartender
/// named or priced the pour at. Four times what the buyer paid a head, and
/// sized against `drinks::DRUNK_LEVEL_THRESHOLDS`: buzzed starts at 300, so a
/// flat 300 landed exactly on the line and the first decay tick (334 an hour)
/// dropped the drinker back to tipsy within seconds of the pour. 400 buys
/// about eighteen minutes of the level the round is meant to hand out, and is
/// still gone in a bit over an hour like any other drink.
///
/// It is the tavern's number because a tavern round is bought for everyone
/// online: the buyer pays for twenty and maybe three walk up, so the pours
/// that do land carry the ones that never happen. See [`Bar::drink_points`].
pub const ROUND_DRINK_POINTS: i64 = 400;

/// Which bar sold a round (migration 191). Closed, because every read that
/// branches on it has to say what a Nightcap round means as well as a
/// tavern one: the two are priced the same a head and buy very different
/// things.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bar {
    /// The Clubhouse tavern: @bartender sold it, and it was bought for
    /// everyone online.
    Tavern,
    /// The Nightcap out back: the menu sold it, and it was bought for the
    /// six stools.
    Nightcap,
}

impl Bar {
    /// The persisted `drink_rounds.bar` value.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Tavern => "tavern",
            Self::Nightcap => "nightcap",
        }
    }

    /// A bar the database wrote. A value outside the CHECK constraint is
    /// impossible, so this crashes rather than inventing a room.
    pub fn from_db(value: &str) -> Self {
        match value {
            "tavern" => Self::Tavern,
            "nightcap" => Self::Nightcap,
            other => panic!("unknown drink round bar in the database: {other}"),
        }
    }

    /// The buzz one drink off this round records: the buyer's own pour and
    /// every credit cashed against it.
    ///
    /// The tavern pours [`ROUND_DRINK_POINTS`], a flat premium over the
    /// price a head, because most of the room it was bought for will never
    /// come and collect. The Nightcap pours exactly what the buyer paid a
    /// head, chips to points 1:1 like every other drink at that bar: it was
    /// bought for the patrons on the stools, who are sitting at the bar and
    /// will drink it.
    pub const fn drink_points(self) -> i64 {
        match self {
            Self::Tavern => ROUND_DRINK_POINTS,
            Self::Nightcap => ROUND_PRICE_PER_PATRON,
        }
    }
}

/// How many unclaimed drinks one patron may have waiting at once.
///
/// Credits stack (migration 168) so the buyer of the round nobody was around
/// for is not buying air, but not without a ceiling: at
/// [`ROUND_DRINK_POINTS`] a head this banks 1,200 points against the 4,000
/// cap, a real night's drinking and not enough for one buyer to park somebody
/// at wasted. It is also the mechanic's only throttle now that the schema no
/// longer provides one, since a patron at the cap costs the next buyer
/// nothing: a room that will not drink stops being worth buying for.
pub const MAX_OPEN_CREDITS: i64 = 3;

/// How long an uncashed credit stays good. Long enough to cover a patron who
/// was mid-game when it landed and a night shift that logs in later, short
/// enough that the bar is not indefinitely on the hook for a round bought last
/// week.
pub const ROUND_CREDIT_TTL_HOURS: i64 = 24;

/// What every grant serializes on. The cap is counted from rows a concurrent
/// round may be inserting and cannot see, so rounds take this lock before
/// granting and hold it to commit; see [`DrinkRound::open`].
const ROUND_GRANT_LOCK: &str = "drink_round_grant";

/// Everything a patron can say to buy the house a round, lowercase.
///
/// Every phrase here has to survive `chat/slur.rs` unscrambled, which is why
/// the list is protected there rather than merely matched loosely here. The
/// list is long so a patron can order the way people do at a bar, but every
/// entry still names the whole house or says "on me": this list is a spending
/// authorization, and "a round" alone, which could be for anybody, is not on
/// it. A verb in front ("buy everyone a drink", "let's do a round for the
/// house") is a lead-in ([`LEAD_INS`]), not part of the phrase.
pub const ROUND_PHRASES: &[&str] = &[
    "round for all",
    "round for everyone",
    "round for everybody",
    "round for the house",
    "round for the bar",
    "round for the whole bar",
    "round for the room",
    "drinks for all",
    "drinks for everyone",
    "drinks for everybody",
    "drinks for the house",
    "drinks for the bar",
    "drinks for the whole bar",
    "drinks for the room",
    "drink for everyone",
    "drink for everybody",
    "drink for the house",
    "drink for the bar",
    "drink for the whole bar",
    "round of drinks for everyone",
    "round of drinks for everybody",
    "round of drinks for the house",
    "round of drinks for the bar",
    "shots for everyone",
    "shots for everybody",
    "shots for the house",
    "beers for everyone",
    "beers for everybody",
    "beers for the house",
    "one for everyone",
    "one for everybody",
    "one for the house",
    "everyone a drink",
    "everybody a drink",
    "the house a drink",
    "the bar a drink",
    "the whole bar a drink",
    "everyone a round",
    "everybody a round",
    "the house a round",
    "the bar a round",
    "the whole bar a round",
    "round on me",
    "round is on me",
    "round's on me",
    "round\u{2019}s on me",
    "round of drinks on me",
    "drinks on me",
    "drinks are on me",
    "drinks all round",
    "drinks all around",
];

/// Everything a patron can say to put one drink on somebody else's tab,
/// lowercase, with `@` standing for the `@user` and `@'s` for `@user's`.
///
/// Same bar as [`ROUND_PHRASES`]: a phrase here is a spending authorization,
/// so it has to read as an order and nothing else. "get @ one" is not on it
/// because "I'll get @x one of those" is ordinary talk. Whatever the patron
/// names (a beer, a shot), what lands on the tab is one drink credit.
pub const GIFT_PHRASES: &[&str] = &[
    "buy @ a drink",
    "buy @ a round",
    "buy @ a beer",
    "buy @ a shot",
    "get @ a drink",
    "get @ a round",
    "get @ a beer",
    "get @ a shot",
    "pour @ a drink",
    "pour @ a beer",
    "pour @ a shot",
    "pour @ one",
    "give @ a drink",
    "give @ a beer",
    "give @ a shot",
    "send @ a drink",
    "send @ a round",
    "send @ a beer",
    "send @ a shot",
    "drink for @",
    "beer for @",
    "shot for @",
    "one for @",
    "@'s drink on me",
    "@'s drink is on me",
    "@'s next drink on me",
    "@'s next drink is on me",
    "@'s next one on me",
    "@'s next one is on me",
];

/// Punctuation a phrase's last word may carry. Anything else glued to it
/// ("drink!!1", "drink:)") is not the phrase.
const GIFT_TRAILING_PUNCTUATION: &[char] = &['.', ',', '!', '?', ';', ':'];

/// One [`GIFT_PHRASES`] hit: the byte range of the phrase in the text it was
/// found in, and the recipient's handle without the `@`.
struct GiftPhrase<'a> {
    start: usize,
    end: usize,
    handle: &'a str,
}

/// The one patron a message puts a drink on the buyer's tab for, when it is an
/// order. The caller removes a composer's reply quote before checking it.
///
/// Looser than the old whole-message form, on the round's rules
/// ([`is_order`]): a [`GIFT_PHRASES`] entry in any case that opens a clause of
/// the message, as long as its sentence does not run on to a `?` and it is
/// not inside backticks. "drink for @mossy, she earned it" is an order; "can
/// you buy @mossy a drink?" gets an answer, and so do "don't buy @mossy a
/// drink" and "I already got a drink for @mossy". On top of that the message
/// must name exactly one patron besides
/// `bartender`: a second handle anywhere ("buy @alice a drink for @bob") makes
/// who pays for whom a guess, and a guess does not move chips.
pub fn gift_drink_target<'a>(text: &'a str, bartender: &str) -> Option<&'a str> {
    let mut target: Option<&str> = None;
    for (offset, segment) in spoken_segments(text) {
        for phrase in gift_phrases(segment) {
            if !is_order(text, offset + phrase.start, offset + phrase.end, bartender) {
                continue;
            }
            match target {
                None => target = Some(phrase.handle),
                Some(known) if known.eq_ignore_ascii_case(phrase.handle) => {}
                Some(_) => return None,
            }
        }
    }
    let target = target?;
    let names_only_target = mentioned_handles(text)
        .into_iter()
        .filter(|handle| !handle.eq_ignore_ascii_case(bartender))
        .all(|handle| handle.eq_ignore_ascii_case(target));
    match names_only_target {
        true => Some(target),
        false => None,
    }
}

/// Byte ranges in `text` covered by a [`GIFT_PHRASES`] entry, questions
/// included, so `chat/slur.rs` keeps a drunk question from scrambling into
/// something else.
fn gift_phrase_spans(text: &str) -> Vec<(usize, usize)> {
    gift_phrases(text)
        .into_iter()
        .map(|phrase| (phrase.start, phrase.end))
        .collect()
}

/// Every byte range in `text` a drunk patron's typing must leave alone: the
/// round phrases and the gift phrases, each widened back over the lead-in it
/// opens its clause with (a scrambled "please" would stop the phrase being an
/// order), in the order they appear. The one list `chat/slur.rs` protects,
/// built from the same matchers the bartender reads, so the two cannot drift
/// apart.
pub fn spending_phrase_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = round_phrase_spans(text);
    spans.extend(gift_phrase_spans(text));
    for span in &mut spans {
        if let Some(start) = lead_in_start(text, span.0, Address::Anyone) {
            span.0 = start;
        }
    }
    spans.sort_unstable();
    spans
}

/// What a message to the bartender asks the bar to ring up by itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarOrder<'a> {
    /// One drink on this patron's tab ([`gift_drink_target`]).
    Gift(&'a str),
    /// A round for the house ([`contains_round_request`]).
    Round,
    /// Nothing: the message goes to the bartender as conversation.
    Talk,
}

/// The one order a message places. A gift and a round in the same message are
/// two orders, and which one the patron meant to pay for is a guess, so
/// neither is rung up and the bartender answers instead.
pub fn bar_order<'a>(text: &'a str, bartender: &str) -> BarOrder<'a> {
    match (
        gift_drink_target(text, bartender),
        contains_round_request(text, bartender),
    ) {
        (Some(target), false) => BarOrder::Gift(target),
        (None, true) => BarOrder::Round,
        (Some(_), true) => BarOrder::Talk,
        (None, false) => BarOrder::Talk,
    }
}

/// Every [`GIFT_PHRASES`] hit in `text`, word by word on whitespace. Only the
/// phrase's last word may carry trailing punctuation, so "buy @alice! a
/// drink" is not the phrase.
fn gift_phrases(text: &str) -> Vec<GiftPhrase<'_>> {
    let mut words: Vec<(usize, &str)> = Vec::new();
    let mut offset = 0;
    for piece in text.split_inclusive(char::is_whitespace) {
        let word = piece.trim_end();
        if !word.is_empty() {
            words.push((offset, word));
        }
        offset += piece.len();
    }
    let mut found = Vec::new();
    for index in 0..words.len() {
        for pattern in GIFT_PHRASES {
            if let Some(phrase) = gift_phrase_at(&words[index..], pattern) {
                found.push(phrase);
            }
        }
    }
    found
}

/// The [`GIFT_PHRASES`] entry `pattern` starting at `words[0]`.
fn gift_phrase_at<'a>(words: &[(usize, &'a str)], pattern: &str) -> Option<GiftPhrase<'a>> {
    let expected: Vec<&str> = pattern.split(' ').collect();
    if words.len() < expected.len() {
        return None;
    }
    let last = expected.len() - 1;
    let mut handle = None;
    let mut end = 0;
    for (index, expected) in expected.into_iter().enumerate() {
        let (start, word) = words[index];
        let core = match index == last {
            true => word.trim_end_matches(GIFT_TRAILING_PUNCTUATION),
            false => word,
        };
        match expected {
            "@" => {
                let name = handle_at(core, 0)?;
                if core.len() != 1 + name.len() {
                    return None;
                }
                handle = Some(name);
            }
            "@'s" => {
                let name = handle_at(core, 0)?;
                if !matches!(&core[1 + name.len()..], "'s" | "\u{2019}s") {
                    return None;
                }
                handle = Some(name);
            }
            literal => {
                if !core.eq_ignore_ascii_case(literal) {
                    return None;
                }
            }
        }
        end = start + core.len();
    }
    Some(GiftPhrase {
        start: words[0].0,
        end,
        handle: handle.expect("every gift phrase has an @ slot"),
    })
}

/// Every `@handle` in `text`, code spans included: a stray name anywhere is
/// enough to make a gift's recipient ambiguous.
fn mentioned_handles(text: &str) -> Vec<&str> {
    text.char_indices()
        .filter(|(index, ch)| {
            *ch == '@'
                && text[..*index]
                    .chars()
                    .next_back()
                    .is_none_or(|before| !is_handle_char(before))
        })
        .filter_map(|(index, _)| handle_at(text, index))
        .collect()
}

/// The handle after the `@` at byte `at`, without trailing dots: a username
/// never ends in one (`sanitize_username_input`), so a sentence that ends on
/// a name ("a drink for @mossy.") ends with a full stop.
fn handle_at(text: &str, at: usize) -> Option<&str> {
    let rest = text[at..].strip_prefix('@')?;
    let len = rest
        .find(|ch: char| !is_handle_char(ch))
        .unwrap_or(rest.len());
    let handle = rest[..len].trim_end_matches('.');
    match handle.is_empty() {
        true => None,
        false => Some(handle),
    }
}

/// What a username may be made of (`User::next_available_username`).
fn is_handle_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.')
}

/// Byte ranges in `text` covered by a [`ROUND_PHRASES`] entry, in the order
/// they appear.
///
/// Matching is case-insensitive and bounded on both ends by a non-alphanumeric
/// character, so "turn around for all of us" is not an order for drinks. ASCII
/// lowercasing never changes a byte's width, so the ranges index `text` itself
/// and not the lowered copy.
pub fn round_phrase_spans(text: &str) -> Vec<(usize, usize)> {
    let haystack = text.to_ascii_lowercase();
    let mut spans = Vec::new();
    for phrase in ROUND_PHRASES {
        let mut from = 0;
        while let Some(offset) = haystack[from..].find(phrase) {
            let start = from + offset;
            let end = start + phrase.len();
            if is_bounded(&haystack, start, end) {
                spans.push((start, end));
            }
            from = start + 1;
        }
    }
    spans.sort_unstable();
    spans
}

/// Whether the patron asked for a round. The bartender's gate.
///
/// Stricter than [`round_phrase_spans`], which is the slur guard's view and
/// protects the phrase wherever it turns up. An order is said as one
/// ([`is_order`]): "how much is a round for everyone?" gets an answer, not a
/// bill, and so does "no round for everyone tonight". Text inside backticks is
/// never an order either, since a code span is how a patron quotes the words
/// without saying them. Segments alternate outside/inside starting outside,
/// so an unbalanced backtick makes the rest of the message not an order,
/// which is the safe way to be wrong about money.
pub fn contains_round_request(text: &str, bartender: &str) -> bool {
    spoken_segments(text).any(|(offset, segment)| {
        round_phrase_spans(segment)
            .into_iter()
            .any(|(start, end)| is_order(text, offset + start, offset + end, bartender))
    })
}

/// The stretches of `text` outside backticks, each with its byte offset in
/// `text`. Phrases are looked for in these; what surrounds a phrase is read
/// from the whole message at that offset.
fn spoken_segments(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut next = 0;
    text.split('`')
        .map(move |segment| {
            let offset = next;
            next += segment.len() + 1;
            (offset, segment)
        })
        .step_by(2)
}

/// Whether the phrase at `[start, end)` of the whole message is said as an
/// order to `bartender`: it opens its clause and its sentence is not a
/// question. The one rule both lists are read on.
fn is_order(text: &str, start: usize, end: usize, bartender: &str) -> bool {
    !sentence_ends_in_question(text, end)
        && lead_in_start(text, start, Address::Only(bartender)).is_some()
}

/// Whether the sentence a phrase ending at `end` belongs to closes with a
/// question mark. The first terminator after the phrase decides; a line break
/// or the end of the text counts as a full stop. Code spans are skipped, so
/// "round for everyone in `#lounge`?" is still a question and a `?` quoted in
/// backticks is not one.
fn sentence_ends_in_question(text: &str, end: usize) -> bool {
    let mut in_code = false;
    for ch in text[end..].chars() {
        match ch {
            '`' => in_code = !in_code,
            '?' if !in_code => return true,
            '.' | '!' | '\n' if !in_code => return false,
            _ => {}
        }
    }
    false
}

/// What may stand between the start of a clause and an order: lowercase, the
/// apostrophe dropped ("I'll" reads as "ill"), several words where only the
/// run of them is a lead-in ("i want to" is one; "i" and "want" are not).
/// Anything else there makes the phrase part of a longer sentence ("don't buy
/// @x a drink", "I already got a drink for @x"), which is talk about a drink
/// and not an order for one, so nothing negative, past or conditional belongs
/// here.
const LEAD_INS: &[&str] = &[
    // Small talk on the way to the order.
    "a",
    "an",
    "another",
    "one more",
    "the",
    "this",
    "next",
    "and",
    "then",
    "also",
    "now",
    "so",
    "just",
    "please",
    "ok",
    "okay",
    "alright",
    "yes",
    "yeah",
    "yep",
    "sure",
    "well",
    "hey",
    "yo",
    "tonight",
    "bartender",
    "barkeep",
    // `chat/slur.rs`'s hiccup, which may land right before a wasted patron's
    // order.
    "hic",
    // Saying you are about to order.
    "ill",
    "i will",
    "id like to",
    "i would like to",
    "i want to",
    "i wanna",
    "im gonna",
    "i am gonna",
    "im going to",
    "i am going to",
    "let me",
    "lemme",
    "lets",
    "let us",
    "can i",
    "could i",
    "may i",
    "can you",
    "could you",
    "go ahead and",
    // The verb, when the phrase starts at what is being bought ("buy everyone
    // a drink", "put a drink for @x on my tab").
    "buy",
    "get",
    "pour",
    "give",
    "send",
    "do",
    "have",
    "put",
    "make it",
    "make that",
    "us",
];

/// Where a clause ends: "don't worry, round on me" is an order.
const CLAUSE_BREAKS: &[char] = &[',', '.', '!', '?', ';', ':', '\n'];

/// Whose `@name` may sit in front of an order.
#[derive(Clone, Copy)]
enum Address<'a> {
    /// The bartender's, and nobody else's: "@alice drinks on me" is about
    /// alice, not a round for the house.
    Only(&'a str),
    /// Anybody's. The slur guard's view, which does not know who is behind
    /// the bar and is better off protecting too much.
    Anyone,
}

/// Where the lead-in of a phrase starting at `start` begins, when the phrase
/// opens its clause: what stands between the last [`CLAUSE_BREAKS`] character
/// outside a code span and the phrase is nothing but the `@name` being spoken
/// to and [`LEAD_INS`]. `None` is a phrase further into a sentence, or one
/// with a code span in the way. Erring toward `None` is the safe way to be
/// wrong about money; the bartender answers with the words to say.
fn lead_in_start(text: &str, start: usize, address: Address<'_>) -> Option<usize> {
    let mut clause_start = 0;
    let mut in_code = false;
    for (index, ch) in text[..start].char_indices() {
        match ch {
            '`' => in_code = !in_code,
            ch if !in_code && CLAUSE_BREAKS.contains(&ch) => clause_start = index + ch.len_utf8(),
            _ => {}
        }
    }
    let lead_in = &text[clause_start..start];
    if lead_in.contains('`') {
        return None;
    }
    let mut words: Vec<String> = Vec::new();
    for word in lead_in.split_whitespace() {
        match (handle_at(word, 0), address) {
            (Some(_), Address::Anyone) => {}
            (Some(name), Address::Only(spoken_to)) => {
                if !name.eq_ignore_ascii_case(spoken_to) {
                    return None;
                }
            }
            (None, _) => {
                let bare: String = word
                    .chars()
                    .filter(|ch| !matches!(ch, '\'' | '\u{2019}'))
                    .collect();
                let bare = bare.trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
                if !bare.is_empty() {
                    words.push(bare.to_ascii_lowercase());
                }
            }
        }
    }
    match is_lead_in(&words) {
        true => Some(start - lead_in.trim_start().len()),
        false => None,
    }
}

/// Whether `words` is a run of [`LEAD_INS`] entries and nothing else.
fn is_lead_in(words: &[String]) -> bool {
    if words.is_empty() {
        return true;
    }
    LEAD_INS.iter().any(|entry| {
        let entry: Vec<&str> = entry.split(' ').collect();
        words.len() >= entry.len()
            && entry
                .iter()
                .zip(words)
                .all(|(expected, word)| *expected == word.as_str())
            && is_lead_in(&words[entry.len()..])
    })
}

/// Whether `[start, end)` sits on word boundaries rather than inside a longer
/// word.
fn is_bounded(text: &str, start: usize, end: usize) -> bool {
    let before_ok = match text[..start].chars().next_back() {
        Some(ch) => !ch.is_alphanumeric(),
        None => true,
    };
    let after_ok = match text[end..].chars().next() {
        Some(ch) => !ch.is_alphanumeric(),
        None => true,
    };
    before_ok && after_ok
}

/// A round that was bought. The row carries no total: the `chip_ledger` row
/// keyed on this id is the record of what it cost and, by the price, of how
/// many it reached. The credit rows are the round's roster and nothing more,
/// though since migration 168 they are at least a stable one: a credit belongs
/// to the round that bought it for as long as it exists, where the old
/// one-per-patron scheme let a later round take an expired credit over in
/// place and shrink an old round's roster after the fact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrinkRound {
    pub id: Uuid,
    /// `None` once the buyer deletes their account; the round and its credits
    /// outlive them.
    pub buyer_user_id: Option<Uuid>,
    pub price_per_patron: i64,
    /// Where it was bought: what the tab board counts and what a credit off
    /// it pours ([`Bar::drink_points`]).
    pub bar: Bar,
    pub created: DateTime<Utc>,
}

impl From<Row> for DrinkRound {
    fn from(row: Row) -> Self {
        Self {
            id: row.get("id"),
            buyer_user_id: row.get("buyer_user_id"),
            price_per_patron: row.get("price_per_patron"),
            bar: Bar::from_db(row.get("bar")),
            created: row.get("created"),
        }
    }
}

/// A round and the patrons it actually reached. `patron_ids` is what the
/// buyer owes for: never the candidate list, always the credits that landed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoundGrant {
    pub round: DrinkRound,
    pub patron_ids: Vec<Uuid>,
}

impl RoundGrant {
    pub fn patron_count(&self) -> i64 {
        self.patron_ids.len() as i64
    }

    /// What the buyer is charged: one price per credit that landed.
    pub fn total_chips(&self) -> i64 {
        self.patron_count() * self.round.price_per_patron
    }
}

/// An open credit and who is behind it, for the bartender's line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenCredit {
    pub round_id: Uuid,
    pub buyer_user_id: Option<Uuid>,
    pub expires_at: DateTime<Utc>,
}

impl From<Row> for OpenCredit {
    fn from(row: Row) -> Self {
        Self {
            round_id: row.get("round_id"),
            buyer_user_id: row.get("buyer_user_id"),
            expires_at: row.get("expires_at"),
        }
    }
}

/// A credit that was just spent, and what the patron still has behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CashedCredit {
    pub round_id: Uuid,
    pub buyer_user_id: Option<Uuid>,
    /// The bar that bought the round, which decides what this pour is worth
    /// ([`Bar::drink_points`]). A credit is good at either bar; what it
    /// pours is set by where it was bought, not where it is drunk.
    pub bar: Bar,
    /// Open credits still on the patron's tab after this pour. The bartender
    /// says this number out loud, so it is counted by the same statement that
    /// spends the credit rather than read back after it.
    pub remaining: i64,
}

impl From<Row> for CashedCredit {
    fn from(row: Row) -> Self {
        Self {
            round_id: row.get("round_id"),
            buyer_user_id: row.get("buyer_user_id"),
            bar: Bar::from_db(row.get("bar")),
            remaining: row.get("remaining"),
        }
    }
}

impl DrinkRound {
    /// The rounds behind a batch of ledger refs, keyed by id: one primary-key
    /// scan. Ids matching nothing are absent.
    pub async fn find_by_ids(
        client: &impl GenericClient,
        ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Self>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = client
            .query("SELECT * FROM drink_rounds WHERE id = ANY($1)", &[&ids])
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                let round = Self::from(row);
                (round.id, round)
            })
            .collect())
    }

    /// Open a round and grant its credits, returning the patrons it reached.
    ///
    /// `candidates` is who was at the bar; the grant skips anyone already
    /// holding `max_open` unexpired credits, so a round bought into a room
    /// that has not been drinking costs the buyer almost nothing. Expired
    /// credits are neither counted nor re-used: they are rows nobody can
    /// drink, and the only thing that ever mattered about them was that they
    /// used to block the patron's one slot. `patron_ids` comes from the
    /// insert's own `RETURNING`, not from a count taken beforehand, so what
    /// the caller charges for and what the bar actually poured cannot
    /// disagree even when two rounds race.
    ///
    /// Every round takes [`ROUND_GRANT_LOCK`] before granting. The cap is a
    /// read-then-write over rows a concurrent round is inserting and cannot
    /// see, so without it two rounds landing together would both read a
    /// patron at two open credits and both grant a third. Rounds are rare and
    /// the lock covers two statements, so serializing every round against
    /// every other is the cheapest way to make the cap mean the number it
    /// says. It also fixes the order the patrons' rows are locked in, which
    /// two overlapping rounds working from differently ordered presence reads
    /// could otherwise deadlock on.
    ///
    /// The chips move in `chips.rs`; the caller owns the transaction that
    /// makes the grant and the charge atomic.
    pub async fn open(
        tx: &Transaction<'_>,
        buyer_user_id: Uuid,
        price_per_patron: i64,
        bar: Bar,
        candidates: &[Uuid],
        ttl_hours: i64,
        max_open: i64,
    ) -> Result<RoundGrant> {
        tx.query_one(
            "SELECT pg_advisory_xact_lock(hashtextextended($1, 0))",
            &[&ROUND_GRANT_LOCK],
        )
        .await?;

        let row = tx
            .query_one(
                "INSERT INTO drink_rounds (buyer_user_id, price_per_patron, bar)
                 VALUES ($1, $2, $3)
                 RETURNING *",
                &[&buyer_user_id, &price_per_patron, &bar.as_str()],
            )
            .await?;
        let round = Self::from(row);

        let candidates = candidates.to_vec();
        let rows = tx
            .query(
                "INSERT INTO drink_credits (round_id, user_id, expires_at)
                 SELECT $1, patron, current_timestamp + make_interval(hours => $3::int)
                 FROM unnest($2::uuid[]) AS patron
                 WHERE (
                     SELECT count(*)
                     FROM drink_credits held
                     WHERE held.user_id = patron
                       AND held.cashed_at IS NULL
                       AND held.expires_at > current_timestamp
                 ) < $4::bigint
                 ON CONFLICT (round_id, user_id) DO NOTHING
                 RETURNING user_id",
                &[&round.id, &candidates, &(ttl_hours as i32), &max_open],
            )
            .await?;

        Ok(RoundGrant {
            round,
            patron_ids: rows.into_iter().map(|row| row.get("user_id")).collect(),
        })
    }
}

pub struct DrinkCredit;

impl DrinkCredit {
    /// Who a batch of one-person gift rounds were bought for, keyed by round
    /// id, for the ledger's "for @user" detail. A gift round has exactly one
    /// credit; handed a house round this keeps whichever patron came last,
    /// so callers pass gift rounds only.
    pub async fn recipients_for_rounds(
        client: &impl GenericClient,
        round_ids: &[Uuid],
    ) -> Result<HashMap<Uuid, Uuid>> {
        if round_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = client
            .query(
                "SELECT round_id, user_id FROM drink_credits WHERE round_id = ANY($1)",
                &[&round_ids],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.get("round_id"), row.get("user_id")))
            .collect())
    }

    /// The credit the patron would drink next: the one closest to going cold,
    /// out of however many they are holding. Read before pouring so the bar
    /// knows the pour is comped; who bought it comes from [`DrinkCredit::cash`],
    /// which may spend a different credit than was open here.
    pub async fn find_open(
        client: &impl GenericClient,
        user_id: Uuid,
    ) -> Result<Option<OpenCredit>> {
        let row = client
            .query_opt(
                "SELECT c.round_id, c.expires_at, r.buyer_user_id
                 FROM drink_credits c
                 JOIN drink_rounds r ON r.id = c.round_id
                 WHERE c.user_id = $1
                   AND c.cashed_at IS NULL
                   AND c.expires_at > current_timestamp
                 ORDER BY c.expires_at, c.created
                 LIMIT 1",
                &[&user_id],
            )
            .await?;
        Ok(row.map(OpenCredit::from))
    }

    /// How many drinks the patron has waiting, right now: the number the
    /// Nightcap menu prints next to the house beer, so a patron can see the
    /// free pours they are holding instead of finding out by ordering.
    /// Expired credits are not drinks; they are not counted.
    pub async fn count_open(client: &impl GenericClient, user_id: Uuid) -> Result<i64> {
        let row = client
            .query_one(
                "SELECT count(*)::BIGINT AS open
                 FROM drink_credits
                 WHERE user_id = $1
                   AND cashed_at IS NULL
                   AND expires_at > current_timestamp",
                &[&user_id],
            )
            .await?;
        Ok(row.get("open"))
    }

    /// Spend one of the patron's open credits on the drink in front of them:
    /// the one closest to expiring, so a banked drink is never lost to the
    /// clock while a fresher one sits behind it.
    ///
    /// The row is picked `FOR UPDATE SKIP LOCKED`, so two orders landing
    /// together take two different credits (two orders are two drinks) and
    /// neither waits on the other; with only one credit open the loser takes
    /// nothing and pays for their own drink. `None` means there was nothing to
    /// spend: never granted, all drunk, or expired between the read and the
    /// pour.
    ///
    /// `remaining` excludes the cashed row by id rather than being counted
    /// afterwards, because a data-modifying CTE's write is not visible to the
    /// rest of its own statement. It can still over-count by one in the
    /// two-simultaneous-orders race: the other pour's credit is locked but
    /// not yet committed, so both snapshots still count it. One optimistic
    /// scripted line, self-correcting on the next order; accepted.
    pub async fn cash(client: &impl GenericClient, user_id: Uuid) -> Result<Option<CashedCredit>> {
        let row = client
            .query_opt(
                "WITH cashed AS (
                    UPDATE drink_credits
                    SET cashed_at = current_timestamp
                    WHERE id = (
                        SELECT id
                        FROM drink_credits
                        WHERE user_id = $1
                          AND cashed_at IS NULL
                          AND expires_at > current_timestamp
                        ORDER BY expires_at, created
                        LIMIT 1
                        FOR UPDATE SKIP LOCKED
                    )
                    RETURNING id, round_id
                 )
                 SELECT
                     cashed.round_id,
                     r.buyer_user_id,
                     r.bar,
                     (SELECT count(*)
                      FROM drink_credits held
                      WHERE held.user_id = $1
                        AND held.cashed_at IS NULL
                        AND held.expires_at > current_timestamp
                        AND held.id <> cashed.id) AS remaining
                 FROM cashed
                 JOIN drink_rounds r ON r.id = cashed.round_id",
                &[&user_id],
            )
            .await?;
        Ok(row.map(CashedCredit::from))
    }
}
