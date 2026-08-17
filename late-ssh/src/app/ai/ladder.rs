//! Escalating per-user, per-room mention cooldowns for the ghost bots.
//!
//! Rapid-fire mentions of @bot or @bartender in one room climb a ladder of
//! growing cooldowns, so one patron cannot fill the room with bot replies;
//! coming back after a quiet spell is free again. The state is process-global
//! (single-replica by design, like the clubhouse `SharedLobby`): the ghost
//! responder loops step it when they answer, and every SSH session holds a
//! read handle so the composer can warn the author at submit time that a
//! mentioned bot is still cooling down.
//!
//! Graybeard is absent on purpose: his flat per-user cooldown lives in his
//! responder loop and mentions of him never show a cooldown banner.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use late_core::MutexRecover;
use uuid::Uuid;

/// @bot climbs steeply: he is the one people milk in front of the room, and
/// deep Q&A has no business monopolizing a social surface.
const BOT_LADDER: &[Duration] = &[
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(120),
    Duration::from_secs(300),
];

/// @bartender stays gentle: several quick orders in a row is the designed
/// path to drunk, so the ladder must not price that loop out.
const BARTENDER_LADDER: &[Duration] = &[
    Duration::from_secs(15),
    Duration::from_secs(30),
    Duration::from_secs(60),
];

/// Quiet time (no answered mention) after which a ladder falls back to its
/// first step.
const LADDER_RESET_AFTER: Duration = Duration::from_secs(15 * 60);

/// Which throttled ghost bot a mention targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LadderBot {
    Bot,
    Bartender,
}

impl LadderBot {
    /// The bot's chat handle, without the `@`. Single source for both the
    /// responder identity and composer mention detection.
    pub const fn handle(self) -> &'static str {
        match self {
            LadderBot::Bot => "bot",
            LadderBot::Bartender => "bartender",
        }
    }

    fn ladder(self) -> &'static [Duration] {
        match self {
            LadderBot::Bot => BOT_LADDER,
            LadderBot::Bartender => BARTENDER_LADDER,
        }
    }
}

/// What the responder loop should do with a mention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Answer,
    Throttled { remaining: Duration },
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    /// Answered mentions in the current run; picks the active cooldown rung.
    answered: u32,
    last_answered: Instant,
}

/// Cooldown in force after the nth answered mention (1-based); past the end
/// the ladder holds at its top rung.
fn cooldown_after(bot: LadderBot, answered: u32) -> Duration {
    let ladder = bot.ladder();
    let idx = (answered.saturating_sub(1) as usize).min(ladder.len() - 1);
    ladder[idx]
}

/// The pure ladder state machine, time injected for tests.
#[derive(Debug, Default)]
pub(crate) struct Ladders {
    entries: HashMap<(LadderBot, Uuid, Uuid), Entry>,
}

impl Ladders {
    /// Answer or throttle a mention, stepping the ladder on answer. Throttled
    /// attempts do not step: re-asking during a cooldown never extends it.
    pub(crate) fn check_and_step(
        &mut self,
        bot: LadderBot,
        user_id: Uuid,
        room_id: Uuid,
        now: Instant,
    ) -> Decision {
        let key = (bot, user_id, room_id);
        match self.entries.get_mut(&key) {
            Some(entry) if now.duration_since(entry.last_answered) < LADDER_RESET_AFTER => {
                let ready_at = entry.last_answered + cooldown_after(bot, entry.answered);
                if now < ready_at {
                    Decision::Throttled {
                        remaining: ready_at.duration_since(now),
                    }
                } else {
                    entry.answered = entry.answered.saturating_add(1);
                    entry.last_answered = now;
                    Decision::Answer
                }
            }
            _ => {
                self.entries.insert(
                    key,
                    Entry {
                        answered: 1,
                        last_answered: now,
                    },
                );
                Decision::Answer
            }
        }
    }

    /// Time left before `bot` would answer this user in this room again, or
    /// `None` when a mention would be answered now. Read-only: never steps.
    pub(crate) fn remaining(
        &self,
        bot: LadderBot,
        user_id: Uuid,
        room_id: Uuid,
        now: Instant,
    ) -> Option<Duration> {
        let entry = self.entries.get(&(bot, user_id, room_id))?;
        if now.duration_since(entry.last_answered) >= LADDER_RESET_AFTER {
            return None;
        }
        let ready_at = entry.last_answered + cooldown_after(bot, entry.answered);
        if now < ready_at {
            Some(ready_at.duration_since(now))
        } else {
            None
        }
    }
}

/// Process-global shared ladders: ghost loops step, sessions peek.
#[derive(Clone, Default)]
pub struct MentionLadders {
    inner: Arc<Mutex<Ladders>>,
}

impl MentionLadders {
    pub fn new() -> Self {
        Self::default()
    }

    /// The throttled bots, for callers scanning a message for their handles.
    pub const ALL_BOTS: [LadderBot; 2] = [LadderBot::Bot, LadderBot::Bartender];

    pub fn check_and_step(&self, bot: LadderBot, user_id: Uuid, room_id: Uuid) -> Decision {
        self.inner
            .lock_recover()
            .check_and_step(bot, user_id, room_id, Instant::now())
    }

    pub fn remaining(&self, bot: LadderBot, user_id: Uuid, room_id: Uuid) -> Option<Duration> {
        self.inner
            .lock_recover()
            .remaining(bot, user_id, room_id, Instant::now())
    }
}

#[cfg(test)]
#[path = "ladder_test.rs"]
mod ladder_test;
