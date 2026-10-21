//! The terminal round-trip probe: how long the bytes a session writes take to
//! reach the user's terminal, measured from the server with no client help.
//!
//! The render loop follows a pass with a DSR status query (`CSI 5 n`). Every
//! VT100-descended terminal answers `CSI 0 n`, and a terminal handles its
//! input in order, so the answer leaves only after it has parsed every byte
//! written before the query. The time to the answer is the network both
//! ways, whatever backlog sat ahead of the query, the user's ssh client (and
//! the `late` CLI, which passes both through untouched), and the terminal's
//! own parse: what a user waits between the server writing a frame and seeing
//! it. tmux and screen answer for the terminal they wrap, which is still the
//! user's side of the network. Each sample's `backlog` label is the bytes the
//! session wrote in the second before the query, the probing pass included,
//! so animation still in the pipe counts, not only the frame the query
//! followed.
//!
//! At most one probe is in flight per session. The SSH `data` handler strips
//! every reply from the inbound stream before the input queue
//! ([`strip_replies`]), so a reply never counts as input: it does not wake the
//! render loop, hold the post-input hot tick, reset the away clock, or dismiss
//! the splash. A reply split across two SSH reads slips through as input: the
//! parser ignores the `CSI n` it reassembles, but the bytes still count as
//! activity, and while the splash is up the leading `ESC` dismisses it. The
//! terminal writes the reply in one go, so a split takes a read boundary
//! inside four bytes. The probe then expires unanswered.
//!
//! This module only keeps time. The render loop and the `data` handler in
//! `ssh.rs` send the query, feed the replies, and own the metrics.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use late_core::MutexRecover;

/// DSR "report status". Answered with [`PROBE_REPLY`].
pub(crate) const PROBE_QUERY: &[u8] = b"\x1b[5n";
/// DSR "terminal OK", the only answer to [`PROBE_QUERY`].
pub(crate) const PROBE_REPLY: &[u8] = b"\x1b[0n";

/// How often a session is probed.
const PROBE_INTERVAL: Duration = Duration::from_secs(10);
/// Output this recent counts as sitting ahead of a probe, for the `backlog`
/// label: longer than any trip the dashboard would call healthy, shorter
/// than the probe interval, so animation frames still in the pipe count
/// without blaming output the terminal showed long ago.
const BACKLOG_WINDOW: Duration = Duration::from_secs(1);
/// A pass behind a [`ProbeBacklog::Large`] backlog is probed as soon as the
/// last probe is this old. Big repaints (a screen switch, a resize) and heavy
/// animation are the passes this probe exists to price; the interval alone
/// would rarely land on one.
const LARGE_FRAME_PROBE_GAP: Duration = Duration::from_secs(2);
/// A probe unanswered this long counts as lost. Matches the output-stall
/// disconnect in `ssh.rs`: a terminal this far behind is about to be cut.
/// Replies carry no tag, so after a loss the next probe waits a full
/// [`PROBE_INTERVAL`]: a reply slower than the timeout lands on nothing
/// instead of being credited to the next probe. A terminal slower than both
/// together is beyond what this probe prices.
const PROBE_TIMEOUT: Duration = Duration::from_secs(30);
/// Lost probes in a row before a session stops probing: its terminal does
/// not answer DSR.
const MAX_UNANSWERED_STREAK: u8 = 3;

const SMALL_BACKLOG_MAX: usize = 4 * 1024;
const MEDIUM_BACKLOG_MAX: usize = 32 * 1024;

/// The bytes the session wrote in the [`BACKLOG_WINDOW`] before a probe
/// (frames and terminal commands, the sending pass included), bucketed for
/// the `backlog` metric label. The question the label answers: do bytes
/// still in the pipe make the trip to the screen slower?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeBacklog {
    /// Nothing written in the window: the query went out alone.
    None,
    /// Up to 4 KiB, a few typed-key diffs.
    Small,
    /// Up to 32 KiB.
    Medium,
    /// Over 32 KiB: a full repaint of a big terminal, an image, animation.
    Large,
}

impl ProbeBacklog {
    pub(crate) fn from_bytes(bytes: usize) -> Self {
        match bytes {
            0 => Self::None,
            n if n <= SMALL_BACKLOG_MAX => Self::Small,
            n if n <= MEDIUM_BACKLOG_MAX => Self::Medium,
            _ => Self::Large,
        }
    }
}

/// One answered probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProbeSample {
    pub(crate) rtt: Duration,
    pub(crate) backlog: ProbeBacklog,
}

/// The round trips one backlog bucket saw over a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct TripTally {
    pub(crate) count: u32,
    pub(crate) fastest: Duration,
    pub(crate) slowest: Duration,
    pub(crate) total: Duration,
}

impl TripTally {
    fn add(&mut self, rtt: Duration) {
        self.fastest = match self.count {
            0 => rtt,
            _ => self.fastest.min(rtt),
        };
        self.slowest = self.slowest.max(rtt);
        self.total += rtt;
        self.count += 1;
    }

    /// `None` before the first trip.
    pub(crate) fn mean(&self) -> Option<Duration> {
        match self.count {
            0 => None,
            count => Some(self.total / count),
        }
    }
}

/// Everything a session's probes saw, for the summary line `ssh.rs` writes
/// when the session ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ProbeTally {
    pub(crate) none: TripTally,
    pub(crate) small: TripTally,
    pub(crate) medium: TripTally,
    pub(crate) large: TripTally,
    pub(crate) lost: u32,
    /// Three probes in a row went unanswered and probing stopped.
    pub(crate) retired: bool,
}

impl ProbeTally {
    fn bucket_mut(&mut self, backlog: ProbeBacklog) -> &mut TripTally {
        match backlog {
            ProbeBacklog::None => &mut self.none,
            ProbeBacklog::Small => &mut self.small,
            ProbeBacklog::Medium => &mut self.medium,
            ProbeBacklog::Large => &mut self.large,
        }
    }

    pub(crate) fn answered(&self) -> u32 {
        self.none.count + self.small.count + self.medium.count + self.large.count
    }

    /// The fastest trip the session made behind any backlog: the closest the
    /// probe gets to the user's plain network distance.
    pub(crate) fn fastest(&self) -> Option<Duration> {
        [self.none, self.small, self.medium, self.large]
            .into_iter()
            .filter(|trips| trips.count > 0)
            .map(|trips| trips.fastest)
            .min()
    }
}

/// What expiring a lost probe did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbeExpiry {
    /// Lost; the next probe waits a full interval.
    Unanswered,
    /// Lost, and the streak says the terminal never answers: the session
    /// stops probing.
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProbeState {
    Idle {
        next_due: Instant,
        /// When the last answered query went out. `None` (a new session, a
        /// query that never left, or a lost one) means a large backlog gets
        /// no early probe: the next one waits for `next_due`.
        last_sent: Option<Instant>,
        unanswered_streak: u8,
    },
    InFlight {
        sent_at: Instant,
        backlog: ProbeBacklog,
        unanswered_streak: u8,
    },
    Retired,
}

/// Shared by the session's render loop (which sends) and its SSH `data`
/// handler (which sees the replies).
pub(crate) struct TerminalProbe {
    state: Mutex<ProbeState>,
    /// Writes inside the last [`BACKLOG_WINDOW`], oldest first. Only the
    /// render loop touches it.
    recent_output: Mutex<VecDeque<(Instant, usize)>>,
    tally: Mutex<ProbeTally>,
}

impl TerminalProbe {
    /// The first probe is due on the first pass, so it trails the session's
    /// first full paint.
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            state: Mutex::new(ProbeState::Idle {
                next_due: now,
                last_sent: None,
                unanswered_streak: 0,
            }),
            recent_output: Mutex::new(VecDeque::new()),
            tally: Mutex::new(ProbeTally::default()),
        }
    }

    /// Records the `bytes_written` this pass handed to the channel, then
    /// claims the in-flight slot if a probe is due behind the window's
    /// backlog. `true` means the caller sends [`PROBE_QUERY`] now and calls
    /// [`Self::cancel`] if that send fails.
    pub(crate) fn try_start(&self, now: Instant, bytes_written: usize) -> bool {
        let backlog = self.record_output(now, bytes_written);
        let mut state = self.state.lock_recover();
        let ProbeState::Idle {
            next_due,
            last_sent,
            unanswered_streak,
        } = *state
        else {
            return false;
        };
        let due = match backlog {
            ProbeBacklog::Large => {
                now >= next_due || last_sent.is_some_and(|sent| now >= sent + LARGE_FRAME_PROBE_GAP)
            }
            ProbeBacklog::None | ProbeBacklog::Small | ProbeBacklog::Medium => now >= next_due,
        };
        if !due {
            return false;
        }
        *state = ProbeState::InFlight {
            sent_at: now,
            backlog,
            unanswered_streak,
        };
        true
    }

    /// Adds this pass's write to the window, drops the writes that fell out
    /// of it, and buckets what is left.
    fn record_output(&self, now: Instant, bytes_written: usize) -> ProbeBacklog {
        let mut recent = self.recent_output.lock_recover();
        if bytes_written > 0 {
            recent.push_back((now, bytes_written));
        }
        loop {
            match recent.front() {
                Some((at, _)) if *at + BACKLOG_WINDOW <= now => {
                    recent.pop_front();
                }
                Some(_) | None => break,
            }
        }
        ProbeBacklog::from_bytes(recent.iter().map(|(_, bytes)| bytes).sum())
    }

    /// The query claimed by [`Self::try_start`] never went out (the channel
    /// was busy or closed). Nothing is lost; the next probe waits a full
    /// interval.
    pub(crate) fn cancel(&self, now: Instant) {
        let mut state = self.state.lock_recover();
        if let ProbeState::InFlight {
            unanswered_streak, ..
        } = *state
        {
            *state = ProbeState::Idle {
                next_due: now + PROBE_INTERVAL,
                last_sent: None,
                unanswered_streak,
            };
        }
    }

    /// A reply arrived. `None` when no probe is waiting for one: a late reply
    /// to an expired probe, or a stray.
    pub(crate) fn answered(&self, now: Instant) -> Option<ProbeSample> {
        let mut state = self.state.lock_recover();
        let ProbeState::InFlight {
            sent_at, backlog, ..
        } = *state
        else {
            return None;
        };
        *state = ProbeState::Idle {
            next_due: now + PROBE_INTERVAL,
            last_sent: Some(sent_at),
            unanswered_streak: 0,
        };
        let rtt = now - sent_at;
        self.tally.lock_recover().bucket_mut(backlog).add(rtt);
        Some(ProbeSample { rtt, backlog })
    }

    /// What the session's probes saw so far.
    pub(crate) fn tally(&self) -> ProbeTally {
        *self.tally.lock_recover()
    }

    /// A query is out and its answer has not arrived.
    pub(crate) fn in_flight(&self) -> bool {
        matches!(*self.state.lock_recover(), ProbeState::InFlight { .. })
    }

    /// Gives up on a probe unanswered past [`PROBE_TIMEOUT`].
    pub(crate) fn expire(&self, now: Instant) -> Option<ProbeExpiry> {
        let mut state = self.state.lock_recover();
        let ProbeState::InFlight {
            sent_at,
            unanswered_streak,
            ..
        } = *state
        else {
            return None;
        };
        if now < sent_at + PROBE_TIMEOUT {
            return None;
        }
        let unanswered_streak = unanswered_streak + 1;
        let mut tally = self.tally.lock_recover();
        tally.lost += 1;
        if unanswered_streak >= MAX_UNANSWERED_STREAK {
            *state = ProbeState::Retired;
            tally.retired = true;
            return Some(ProbeExpiry::Retired);
        }
        *state = ProbeState::Idle {
            next_due: now + PROBE_INTERVAL,
            last_sent: None,
            unanswered_streak,
        };
        Some(ProbeExpiry::Unanswered)
    }
}

/// Removes every complete [`PROBE_REPLY`] from one inbound SSH read and
/// reports how many it held. The rest of the read is the user's input,
/// in order.
pub(crate) fn strip_replies(data: &[u8]) -> (Cow<'_, [u8]>, usize) {
    if !data
        .windows(PROBE_REPLY.len())
        .any(|window| window == PROBE_REPLY)
    {
        return (Cow::Borrowed(data), 0);
    }
    let mut input = Vec::with_capacity(data.len());
    let mut replies = 0;
    let mut rest = data;
    while !rest.is_empty() {
        if rest.starts_with(PROBE_REPLY) {
            replies += 1;
            rest = &rest[PROBE_REPLY.len()..];
        } else {
            input.push(rest[0]);
            rest = &rest[1..];
        }
    }
    (Cow::Owned(input), replies)
}
