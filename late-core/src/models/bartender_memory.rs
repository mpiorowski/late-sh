//! What @bartender remembers about a patron.
//!
//! Two kinds of memory ([`MemoryKind`]). An exchange memory is one order
//! he answered, kept on his own: the patron's order, his reply, and up to
//! [`TRAILING_MESSAGE_CAP`] more lines the patron said in that room from
//! the order until [`TRAILING_WINDOW_SECS`] after the reply landed. A
//! requested memory is what a patron asked him to remember with
//! "@bartender remember ..." ([`remember_request`]): never an order, only
//! the words to keep. The words are copied when a memory is stored, so it
//! lives by the rules below and nothing else: pruning or deleting the chat
//! log it came from leaves it alone.
//!
//! A patron keeps at most [`MEMORY_CAP`]. The newest [`REQUESTED_KEEP`]
//! requested memories are kept ahead of everything else, and every other
//! slot goes to the newest of the rest ([`evictions`]). The cap on the
//! protected ones is what keeps a patron who asks him to remember
//! everything from crowding out tonight.
//!
//! "forget me", said exactly ([`is_forget_request`]), wipes everything said
//! up to it and stamps `bartender_memory_resets`, so a memory still in
//! flight when it was said cannot land after the wipe.
//!
//! The two triggers are matched literally on the stored body, which is the
//! drunk-scrambled one, so `chat/slur.rs` protects [`memory_phrase_spans`]
//! the same way it protects the spending phrases.

use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio_postgres::{Client, Row, Transaction};
use uuid::Uuid;

use super::chat_message::ChatMessage;

/// Most memories one patron keeps.
pub const MEMORY_CAP: usize = 16;
/// Most requested memories kept ahead of the exchange memories. The rest
/// of the cap always goes to recency.
pub const REQUESTED_KEEP: usize = 8;
/// Most lines kept from what the patron said after ordering.
pub const TRAILING_MESSAGE_CAP: i64 = 5;
/// How long after the reply lands the patron's lines still count.
pub const TRAILING_WINDOW_SECS: i64 = 5;

/// The whole of a wipe request, once the bartender's handle is set aside.
const FORGET_WORDS: [&str; 2] = ["forget", "me"];
/// The word, right after the bartender's handle, that requests a memory.
const REMEMBER_WORD: &str = "remember";
/// What `chat/slur.rs` drops into a drunk patron's line. The forget matcher
/// ignores it before or after the phrase, the remember matcher between the
/// handle and "remember" or at the very end.
const HICCUP: &str = "*hic*";

/// One stored memory, newest first in the slice handed to [`evictions`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemorySlot {
    pub id: Uuid,
    pub requested: bool,
}

/// Which of a patron's memories go, given all of them newest first: the
/// newest [`REQUESTED_KEEP`] requested memories stay, then the newest of
/// the rest fill what is left of [`MEMORY_CAP`]. Older requested memories
/// beyond the protected few compete on recency like any exchange memory.
pub fn evictions(newest_first: &[MemorySlot]) -> Vec<Uuid> {
    let mut protected_left = REQUESTED_KEEP;
    let protected: Vec<bool> = newest_first
        .iter()
        .map(|slot| {
            let protect = slot.requested && protected_left > 0;
            if protect {
                protected_left -= 1;
            }
            protect
        })
        .collect();
    let mut open = MEMORY_CAP - (REQUESTED_KEEP - protected_left);
    newest_first
        .iter()
        .zip(protected)
        .filter_map(|(slot, protect)| match (protect, open) {
            (true, _) => None,
            (false, 0) => Some(slot.id),
            (false, _) => {
                open -= 1;
                None
            }
        })
        .collect()
}

/// Whether `text` (the message with its reply quote stripped) is exactly
/// "forget me": any case, one or more ASCII spaces between the words, the
/// bartender's handle allowed once at either end (with a `,` or `:` after
/// it), a hiccup outside the phrase ignored. Anything more is conversation,
/// so "please forget me" or "forget me?" goes to the model like any other line.
pub fn is_forget_request(text: &str, bartender: &str) -> bool {
    let handle = bartender.trim().trim_start_matches('@');
    let mut words: Vec<&str> = text
        .split_whitespace()
        .filter(|word| *word != HICCUP)
        .collect();
    if words.first().is_some_and(|word| is_handle(word, handle)) {
        words.remove(0);
    } else if words.last().is_some_and(|word| is_handle(word, handle)) {
        words.pop();
    }
    words.len() == FORGET_WORDS.len()
        && words
            .iter()
            .zip(FORGET_WORDS)
            .all(|(word, expected)| word.eq_ignore_ascii_case(expected))
        && !forget_phrase_spans(text).is_empty()
}

/// What to remember, when `text` (the message with its reply quote
/// stripped) requests a memory: the bartender's handle first (with a `,`
/// or `:` after it), then "remember" as the very next word (any case, a
/// `,` or `:` after it allowed), then the words to keep, trimmed. A hiccup
/// between the handle and "remember", or at the very end, is ignored. Only
/// the opening counts, so "@bartender please remember ..." or "remember
/// this @bartender" is talk. `Some("")` asks him to remember nothing.
pub fn remember_request<'a>(text: &'a str, bartender: &str) -> Option<&'a str> {
    let handle = bartender.trim().trim_start_matches('@');
    let (_, words) = remember_split(text, |word| is_handle(word, handle))?;
    let words = words.strip_suffix(HICCUP).map_or(words, str::trim_end);
    Some(words)
}

/// Where "remember" starts in `text` and the words after it, when `text`
/// opens with a word `is_bartender` accepts and then "remember".
fn remember_split(text: &str, is_bartender: impl Fn(&str) -> bool) -> Option<(usize, &str)> {
    let (handle, rest) = next_word(text)?;
    if !is_bartender(handle) {
        return None;
    }
    let (mut word, mut after) = next_word(rest)?;
    if word == HICCUP {
        (word, after) = next_word(after)?;
    }
    if !word
        .trim_end_matches([',', ':'])
        .eq_ignore_ascii_case(REMEMBER_WORD)
    {
        return None;
    }
    // `word` sits directly in front of `after`, and both are tails of `text`.
    Some((text.len() - after.len() - word.len(), after.trim()))
}

/// The first whitespace-delimited word of `text` and everything after it.
fn next_word(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    let end = text.find(char::is_whitespace).unwrap_or(text.len());
    (end > 0).then(|| text.split_at(end))
}

fn is_handle(word: &str, handle: &str) -> bool {
    word.trim_end_matches([',', ':'])
        .strip_prefix('@')
        .is_some_and(|name| name.eq_ignore_ascii_case(handle))
}

/// Byte ranges in `text` that drunk text must leave alone: every "forget
/// me", and a remember request from its "remember" to the end of the
/// message, so the bar still reads what the patron meant and keeps the
/// words as they typed them. The slur pass does not know the bartender's
/// handle, so here a request opens with any @mention; protecting a line
/// meant for someone else costs nothing but a few typos.
pub fn memory_phrase_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = forget_phrase_spans(text);
    spans.extend(remember_request_span(text));
    spans.sort_unstable();
    spans
}

/// Wording the patron-memory prompt reads literally when choosing an
/// alcoholic or nonalcoholic variant. These spans only protect drunk text;
/// the prompt still decides whether the patron positively requested alcohol.
pub fn alcohol_phrase_spans(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    for word in [
        "alcohol",
        "alcoholic",
        "intoxicating",
        "nonalcoholic",
        "nonintoxicating",
    ] {
        spans.extend(word_spans(text, word));
    }
    spans.extend(spaced_word_spans(text, "with", "alcohol"));
    spans.sort_unstable();
    spans
}

/// "forget" and "me" separated only by one or more literal ASCII spaces.
fn forget_phrase_spans(text: &str) -> Vec<(usize, usize)> {
    spaced_word_spans(text, FORGET_WORDS[0], FORGET_WORDS[1])
}

/// A remember request's "remember" through the end of `text`, past a
/// leading reply quote line the way the bartender reads it. A request can
/// run over several lines, so the slur pass, which otherwise protects line
/// by line, sets this span aside whole.
pub fn remember_request_span(text: &str) -> Option<(usize, usize)> {
    let start = match text.split_once('\n') {
        Some((first, rest)) if first.trim().starts_with("> ") && !rest.trim().is_empty() => {
            first.len() + 1
        }
        _ => 0,
    };
    let is_mention = |word: &str| {
        word.trim_end_matches([',', ':'])
            .strip_prefix('@')
            .is_some_and(|name| !name.is_empty())
    };
    remember_split(&text[start..], is_mention).map(|(at, _)| (start + at, text.len()))
}

fn spaced_word_spans(text: &str, first: &str, second: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    for (start, end) in word_spans(text, first) {
        let rest = &text[end..];
        let gap = rest.len() - rest.trim_start_matches(' ').len();
        if gap == 0 {
            continue;
        }
        let second_start = end + gap;
        if let Some(&(_, second_end)) = word_spans(&text[second_start..], second)
            .first()
            .filter(|(at, _)| *at == 0)
        {
            spans.push((start, second_start + second_end));
        }
    }
    spans
}

/// Every whole-word, case-insensitive occurrence of the ASCII `word`.
fn word_spans(text: &str, word: &str) -> Vec<(usize, usize)> {
    // ASCII lowercasing keeps every byte where it was, so offsets found in
    // the lowered copy index the original.
    let lowered = text.to_ascii_lowercase();
    lowered
        .match_indices(word)
        .map(|(start, _)| (start, start + word.len()))
        .filter(|&(start, end)| {
            !text[..start].chars().next_back().is_some_and(is_word_char)
                && !text[end..].chars().next().is_some_and(is_word_char)
        })
        .collect()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether [`BartenderMemory::record_exchange`] or
/// [`BartenderMemory::record_requested`] kept the memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recorded {
    /// Stored, and this many older memories made room for it.
    Stored { evicted: usize },
    /// The patron said "forget me" after saying this one.
    Forgotten,
    /// This message's memory was already stored.
    AlreadyStored,
}

/// One memory as the bartender's prompt reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BartenderMemory {
    pub id: Uuid,
    /// When the patron said it.
    pub ordered_at: DateTime<Utc>,
    /// The order's whole body, or for a requested memory just what to
    /// remember.
    pub order_body: String,
    pub kind: MemoryKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemoryKind {
    /// An order he answered, kept on his own.
    Exchange {
        replied_at: DateTime<Utc>,
        reply_body: String,
        /// What the patron said after ordering, oldest first.
        trailing_bodies: Vec<String>,
    },
    /// "@bartender remember ...".
    Requested,
}

impl BartenderMemory {
    /// Store the exchange of `order` (the patron's message) and `reply`
    /// (the bartender's answer), with the patron's lines in the same room
    /// up to [`TRAILING_WINDOW_SECS`] after the reply. Call it once that
    /// window has closed. Every word is copied here, so the memory owes
    /// nothing to the chat log afterwards.
    ///
    /// One transaction under the patron's lock, shared with
    /// [`Self::record_requested`] and [`Self::forget`]: the reset check, the
    /// insert and the eviction all read the same rows, so two memories
    /// settling together cannot both see fifteen and leave seventeen.
    pub async fn record_exchange(
        client: &mut Client,
        order: &ChatMessage,
        reply: &ChatMessage,
    ) -> Result<Recorded> {
        let user_id = order.user_id;
        let Some(tx) = open_unless_forgotten(client, user_id, order.created).await? else {
            return Ok(Recorded::Forgotten);
        };
        let inserted = tx
            .execute(
                "INSERT INTO bartender_memories
                     (user_id, order_message_id, ordered_at, order_body,
                      requested, replied_at, reply_body, trailing_bodies)
                 VALUES ($1, $2, $3, $4, false, $5, $6,
                     ARRAY(
                         SELECT t.body FROM chat_messages t
                         WHERE t.user_id = $1
                           AND t.room_id = $7
                           AND (t.created, t.id) > ($3::timestamptz, $2::uuid)
                           AND t.created <= $5::timestamptz + make_interval(secs => $8::double precision)
                         ORDER BY t.created, t.id
                         LIMIT $9
                     ))
                 ON CONFLICT (order_message_id) DO NOTHING",
                &[
                    &user_id,
                    &order.id,
                    &order.created,
                    &order.body,
                    &reply.created,
                    &reply.body,
                    &order.room_id,
                    &(TRAILING_WINDOW_SECS as f64),
                    &TRAILING_MESSAGE_CAP,
                ],
            )
            .await?;
        settle(tx, user_id, inserted).await
    }

    /// Store `words`, what the patron asked to be remembered in `message`
    /// (see [`remember_request`]). Under the same lock and reset check as
    /// [`Self::record_exchange`].
    pub async fn record_requested(
        client: &mut Client,
        message: &ChatMessage,
        words: &str,
    ) -> Result<Recorded> {
        let user_id = message.user_id;
        let Some(tx) = open_unless_forgotten(client, user_id, message.created).await? else {
            return Ok(Recorded::Forgotten);
        };
        let inserted = tx
            .execute(
                "INSERT INTO bartender_memories
                     (user_id, order_message_id, ordered_at, order_body, requested)
                 VALUES ($1, $2, $3, $4, true)
                 ON CONFLICT (order_message_id) DO NOTHING",
                &[&user_id, &message.id, &message.created, &words],
            )
            .await?;
        settle(tx, user_id, inserted).await
    }

    /// Wipe everything the bartender remembers about `user_id` that was
    /// said at or before `said_at` (the "forget me" message's own time),
    /// and refuse any such memory still on its way in. A memory said after
    /// it stays, even one that reached the lock first. Returns how many
    /// memories were forgotten.
    pub async fn forget(client: &mut Client, user_id: Uuid, said_at: DateTime<Utc>) -> Result<u64> {
        let tx = client.transaction().await?;
        lock_patron(&tx, user_id).await?;
        let forgotten = tx
            .execute(
                "DELETE FROM bartender_memories WHERE user_id = $1 AND ordered_at <= $2",
                &[&user_id, &said_at],
            )
            .await?;
        tx.execute(
            "INSERT INTO bartender_memory_resets (user_id, reset_at)
             VALUES ($1, $2)
             ON CONFLICT (user_id) DO UPDATE
             SET reset_at = GREATEST(bartender_memory_resets.reset_at, EXCLUDED.reset_at)",
            &[&user_id, &said_at],
        )
        .await?;
        tx.commit().await?;
        Ok(forgotten)
    }

    /// Everything the bartender remembers about `user_id`, oldest first.
    pub async fn list_for_user(client: &Client, user_id: Uuid) -> Result<Vec<Self>> {
        let rows = client
            .query(
                "SELECT id, ordered_at, order_body, requested,
                        replied_at, reply_body, trailing_bodies
                 FROM bartender_memories
                 WHERE user_id = $1
                 ORDER BY ordered_at, id",
                &[&user_id],
            )
            .await?;
        Ok(rows.iter().map(Self::from_row).collect())
    }

    fn from_row(row: &Row) -> Self {
        let kind = if row.get("requested") {
            MemoryKind::Requested
        } else {
            // The table's check guarantees an exchange memory its reply.
            MemoryKind::Exchange {
                replied_at: row.get("replied_at"),
                reply_body: row.get("reply_body"),
                trailing_bodies: row.get("trailing_bodies"),
            }
        };
        Self {
            id: row.get("id"),
            ordered_at: row.get("ordered_at"),
            order_body: row.get("order_body"),
            kind,
        }
    }
}

/// The patron's transaction, under their lock, unless they said "forget
/// me" at or after `said_at`.
async fn open_unless_forgotten(
    client: &mut Client,
    user_id: Uuid,
    said_at: DateTime<Utc>,
) -> Result<Option<Transaction<'_>>> {
    let tx = client.transaction().await?;
    lock_patron(&tx, user_id).await?;
    let forgotten = tx
        .query_opt(
            "SELECT 1 FROM bartender_memory_resets
             WHERE user_id = $1 AND reset_at >= $2",
            &[&user_id, &said_at],
        )
        .await?
        .is_some();
    Ok((!forgotten).then_some(tx))
}

/// After an insert of `inserted` rows: make room under the cap and commit.
async fn settle(tx: Transaction<'_>, user_id: Uuid, inserted: u64) -> Result<Recorded> {
    if inserted == 0 {
        return Ok(Recorded::AlreadyStored);
    }
    let slots: Vec<MemorySlot> = tx
        .query(
            "SELECT id, requested FROM bartender_memories
             WHERE user_id = $1
             ORDER BY ordered_at DESC, id DESC",
            &[&user_id],
        )
        .await?
        .into_iter()
        .map(|row| MemorySlot {
            id: row.get("id"),
            requested: row.get("requested"),
        })
        .collect();
    let evicted = evictions(&slots);
    if !evicted.is_empty() {
        tx.execute(
            "DELETE FROM bartender_memories WHERE user_id = $1 AND id = ANY($2)",
            &[&user_id, &evicted],
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Recorded::Stored {
        evicted: evicted.len(),
    })
}

async fn lock_patron(tx: &Transaction<'_>, user_id: Uuid) -> Result<()> {
    tx.query_one(
        "SELECT pg_advisory_xact_lock(
           hashtextextended(concat_ws(':', 'bartender_memory', ($1::uuid)::text), 0)
         )",
        &[&user_id],
    )
    .await?;
    Ok(())
}
