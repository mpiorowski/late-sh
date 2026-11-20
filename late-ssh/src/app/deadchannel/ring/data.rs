//! The ring's numbers (GAME.md, "The arena pass"). Both corners fight on
//! the same flat sheet: the level, the kit, the marks and the day's glass
//! stay outside, so a bout is the deck and the hand and nothing else.
//! The cards keep the shares they print on the road (`Powers::of`), read
//! off one attack and one defense that no runner owns.

use late_core::models::deadchannel_bout::{Bout, Side};

use super::state::{Duel, Ending, Refusal};
use super::svc::{ActRefusal, AnswerRefusal, BetRefusal, CallRefusal, Reply};
use crate::app::deadchannel::fight::data::RULES;
use crate::app::deadchannel::fight::state::Powers;

/// The attack and the defense both corners fight with: a strike of 10
/// against a block of 7.
const ATTACK: u32 = 16;
const DEFENSE: u32 = 12;
/// The signal both corners start with, and the most a siphon mends to.
/// Set by the balance contract (`policy_test.rs`): at 45 a bout on the
/// clock runs three rounds and seven in ten end in a knockout; higher,
/// the static piles up and the decks that feed on it run away.
pub(crate) const SIGNAL: i32 = 45;
/// Rounds in a bout. Past the last one the higher signal takes it.
pub(crate) const ROUNDS: u8 = 4;
/// Slots in a round, resolved in order. One more than the energy pays
/// for at a card apiece, so the round is short of energy and never of
/// slots: a free card and a burn's energy have somewhere to go, and where
/// the empty slot sits is a choice.
pub(crate) const SLOTS: usize = 4;

/// How long a call waits for its answer.
pub(crate) const CALL_SECS: i64 = 180;
/// From the accept to the bell: the time the street has to bet.
pub(crate) const BELL_SECS: i64 = 300;
/// What a round gets on the clock, from the bell or from the last reveal.
pub(crate) const ROUND_SECS: i64 = 30;
/// What the reveal of a resolved round takes before the next one's clock
/// starts.
pub(crate) const REVEAL_SECS: i64 = 6;
/// How long a finished bout stays on the board.
pub(crate) const AFTERMATH_SECS: i64 = 60;
/// Presses a bet takes. A press is one cache's worth of bits at the
/// bettor's level (`Rules::cache`).
pub(crate) const MAX_PRESSES: i32 = 3;

/// The share of a pool that is burned, and the share the winning fighter
/// takes; the rest is split among the bets on the winner.
pub(crate) const RAKE_PERCENT: i64 = 10;
pub(crate) const PURSE_PERCENT: i64 = 10;

/// What every card is worth in the ring, the same for both corners. No
/// glyph is at the table, so its hit and its noise are nothing.
pub(crate) fn powers() -> Powers {
    Powers::of(
        &RULES,
        RULES.strike(ATTACK, DEFENSE),
        RULES.block(DEFENSE),
        0,
        0,
    )
}

/// The wire's line for a call: it mentions the runner called, which is
/// how the call reaches them wherever they are.
pub(crate) fn call_line(bout: &Bout) -> String {
    format!(
        "{} calls @{} to the ring. {} minutes to answer, at RING.",
        bout.caller_username,
        bout.called_username,
        CALL_SECS / 60
    )
}

/// The wire's line for an accepted call.
pub(crate) fn set_line(bout: &Bout) -> String {
    format!(
        "{} and {} step into the ring. the bell in {} minutes. bits down at RING.",
        bout.caller_username,
        bout.called_username,
        BELL_SECS / 60
    )
}

/// The wire's line for a bout that ended.
pub(crate) fn result_line(bout: &Bout, duel: &Duel, ending: Ending) -> String {
    let name = |side: Side| bout.username(side);
    let fought = match ending {
        Ending::Knockout(winner) => format!(
            "{} put {} down in round {}.",
            name(winner),
            name(winner.other()),
            duel.round
        ),
        Ending::Points(winner) => format!(
            "{} took {} on points, {} to {}.",
            name(winner),
            name(winner.other()),
            duel.corner(winner).signal,
            duel.corner(winner.other()).signal
        ),
        Ending::Draw => format!(
            "{} and {} fought to a draw.",
            name(Side::Caller),
            name(Side::Called)
        ),
    };
    match bout.pool_caller + bout.pool_called {
        0 => fought,
        bits => format!("{fought} {bits} bits rode on it."),
    }
}

pub(crate) const NOBODY_TO_CALL_LINE: &str = "nobody with a mark is on the wire.";
pub(crate) const SLOTS_FULL_LINE: &str = "every slot is laid. take a card back first.";

/// The banner for a runner called while they are somewhere else.
pub(crate) fn called_banner(caller: &str) -> String {
    format!("{caller} calls you to the ring. Answer at RING, down in the city.")
}

/// The ring's word for an answer. `None` for an answer the board shows
/// by itself: a card laid, a deck read.
pub(crate) fn reply_line(reply: Reply) -> Option<String> {
    let line = match reply {
        Reply::Called => format!("the call is out. {} minutes.", CALL_SECS / 60),
        Reply::CallRefused(refusal) => match refusal {
            CallRefusal::Yourself => "you cannot call yourself.",
            CallRefusal::NotMarked => "the ropes are for runners who put the Old Signal down.",
            CallRefusal::TheyAreNotMarked => "they have no mark. the ring will not take them.",
            CallRefusal::Offline => "they are off the wire.",
            CallRefusal::RingTaken => "the ring is taken. wait for it to clear.",
            CallRefusal::AlreadyCalling => "you have a call out already.",
        }
        .to_string(),
        Reply::Accepted => format!("you are in. the bell in {} minutes.", BELL_SECS / 60),
        Reply::Declined => "the call is off.".to_string(),
        Reply::AnswerRefused(refusal) => match refusal {
            AnswerRefusal::Gone => "that call is gone.",
            AnswerRefusal::NotYours => "that call is not yours.",
            AnswerRefusal::RingTaken => "the ring is taken. the call waits.",
            AnswerRefusal::FoughtToday => "you two already fought today. the call is off.",
        }
        .to_string(),
        Reply::Deck(_) | Reply::Laid => return None,
        Reply::Staked { bits } => format!("{bits} bits down."),
        Reply::BetRefused(refusal) => match refusal {
            BetRefusal::Closed => "the book is shut.".to_string(),
            BetRefusal::OtherSide => "your bits are on the other corner.".to_string(),
            BetRefusal::Capped => format!("{MAX_PRESSES} presses is the limit."),
            BetRefusal::Short { by } => format!("{by} bits short of the stake."),
            BetRefusal::FightWaiting => "not with a glyph waiting on you.".to_string(),
            BetRefusal::OwnCornerOnly => "a fighter backs their own corner.".to_string(),
            BetRefusal::NoRunner => "you have no runner here.".to_string(),
        },
        Reply::ActRefused(refusal) => match refusal {
            ActRefusal::NotYours => "that is not your bout.",
            ActRefusal::Round(Refusal::Over) => "the bout is over.",
            ActRefusal::Round(Refusal::Locked) => "your round is locked.",
            ActRefusal::Round(Refusal::NoSlot | Refusal::NoCard) => "no card there.",
            ActRefusal::Round(Refusal::AlreadyLaid) => "that card is laid already.",
            ActRefusal::Round(Refusal::NoEnergy) => "not enough energy for that.",
        }
        .to_string(),
        Reply::Failed => "the ring is not answering. try again.".to_string(),
    };
    Some(line)
}
