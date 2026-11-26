//! A session's side of the ring: its copy of the board, the deck this
//! runner brings, the cursor over the runners on the wire, the corner
//! picked for a bet, the ring's last word, and the one request that may
//! be out. Decides nothing: a key asks the service, the answer sets the
//! word, and the board arrives on the watch.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use late_core::models::deadchannel_bout::{Bout, BoutBet, Side};
use late_core::models::presence::PresenceRecord;
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use super::data::{self, AFTERMATH_SECS};
use super::state::Picks;
use super::svc::{Act, Board, LiveBout, Reply, RingBoard, RingService};
use crate::app::common::primitives::{Banner, Screen};
use crate::app::deadchannel::city::map::Landmark;
use crate::app::deadchannel::fight::cards::DRAFTS;
use crate::app::deadchannel::runner::svc::RunnerLooks;
use crate::app::state::App;

/// A marked runner on the wire: somebody this runner can call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Contender {
    pub user_id: Uuid,
    pub username: String,
    pub marks: i32,
}

/// Every marked runner on the wire but `me`, by name: who the panel
/// lists and the cursor walks.
pub(crate) fn roster(records: &[PresenceRecord], looks: &RunnerLooks, me: Uuid) -> Vec<Contender> {
    let mut roster: Vec<Contender> = Vec::new();
    for record in records {
        let Some(entry) = looks.get(&record.user_id) else {
            continue;
        };
        if record.user_id == me
            || entry.marks < 1
            || roster.iter().any(|other| other.user_id == record.user_id)
        {
            continue;
        }
        roster.push(Contender {
            user_id: record.user_id,
            username: record.username.clone(),
            marks: entry.marks,
        });
    }
    roster.sort_by(|a, b| a.username.cmp(&b.username));
    roster
}

/// `board` as `viewer` may hold it: every hand is open, but a corner's
/// slots are its own until the round comes out, so the copy a session
/// keeps has them blanked for everybody else (`Duel::view_for`). The
/// panel and the keys read only this copy.
pub(crate) fn seen_by(board: &Board, viewer: Uuid) -> RingBoard {
    let live = board.live.as_ref().map(|live| LiveBout {
        bout: live.bout.clone(),
        duel: live.duel.view_for(live.bout.side_of(viewer)),
        bets: live.bets.clone(),
    });
    Arc::new(Board {
        calls: board.calls.clone(),
        live,
        records: board.records.clone(),
    })
}

/// `user_id`'s bet on `live`.
pub(crate) fn bet_of(live: &LiveBout, user_id: Uuid) -> Option<&BoutBet> {
    live.bets.iter().find(|bet| bet.user_id == user_id)
}

/// The open call to `user_id`, the oldest first.
pub(crate) fn call_to(board: &Board, user_id: Uuid) -> Option<&Bout> {
    board
        .calls
        .iter()
        .find(|call| call.called_user_id == user_id)
}

/// `user_id`'s own open call.
pub(crate) fn call_from(board: &Board, user_id: Uuid) -> Option<&Bout> {
    board
        .calls
        .iter()
        .find(|call| call.caller_user_id == user_id)
}

/// What the ring is doing at a moment, for a session's panel and keys.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Phase<'a> {
    /// Nobody in the ring.
    Empty,
    /// A bout is set and the book is open until the bell.
    Set { live: &'a LiveBout, bell_in: i64 },
    /// The rounds are on. `clock` is what the round has left, `None`
    /// while it is past its time and the sweeper has not called it yet.
    On {
        live: &'a LiveBout,
        clock: Option<i64>,
    },
    /// It ended and is still on the board.
    Over { live: &'a LiveBout, clears_in: i64 },
}

/// The phase `live` is in at `now`.
pub(crate) fn phase(live: Option<&LiveBout>, now: DateTime<Utc>) -> Phase<'_> {
    let Some(live) = live else {
        return Phase::Empty;
    };
    if let Some(ended) = live.bout.ended_at {
        let clears_in = AFTERMATH_SECS - (now - ended).num_seconds();
        return match clears_in > 0 {
            true => Phase::Over { live, clears_in },
            false => Phase::Empty,
        };
    }
    let bell_in = live
        .bout
        .bell_at
        .map_or(0, |bell| (bell - now).num_seconds());
    if bell_in > 0 {
        return Phase::Set { live, bell_in };
    }
    let clock = live
        .bout
        .deadline_at
        .map(|deadline| (deadline - now).num_seconds())
        .filter(|left| *left > 0);
    Phase::On { live, clock }
}

/// What one tick of the session found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct RingTick {
    /// The board, the word or a clock moved.
    changed: bool,
    /// Bits moved on this runner's sheet (a stake, a pay, a purse) or
    /// their gate opened: the sheet mirror should re-read.
    reload_sheet: bool,
    /// A call to this runner just landed, from this name.
    called_by: Option<String>,
}

pub(crate) struct RingSession {
    /// The tick copy of the process-shared board, as this runner may see
    /// it ([`seen_by`]).
    pub board: RingBoard,
    /// The deck this runner brings: read when the panel first opens.
    pub picks: Option<Picks>,
    /// The cursor over the roster.
    pub cursor: usize,
    /// The corner a bet goes on.
    pub corner: Side,
    /// The ring's last word: the answer to this session's last request.
    pub word: Option<String>,
    in_flight: bool,
    /// The call to this runner last announced, so one call rings once.
    rung_for: Option<Uuid>,
    /// The bout the sheet was last re-read for, and how much of its
    /// settling had landed then.
    reloaded_for: Option<(Uuid, bool)>,
    /// The clock the panel last painted, so a countdown owes a frame
    /// once a second and no more.
    painted_secs: Option<i64>,
    user_id: Uuid,
    username: String,
    svc: RingService,
    board_rx: watch::Receiver<RingBoard>,
    reply_tx: mpsc::UnboundedSender<Reply>,
    reply_rx: mpsc::UnboundedReceiver<Reply>,
}

impl RingSession {
    pub(crate) fn new(user_id: Uuid, username: String, svc: RingService) -> Self {
        let (reply_tx, reply_rx) = mpsc::unbounded_channel();
        let board_rx = svc.subscribe();
        let board = seen_by(&board_rx.borrow(), user_id);
        Self {
            board,
            picks: None,
            cursor: 0,
            corner: Side::Caller,
            word: None,
            in_flight: false,
            rung_for: None,
            reloaded_for: None,
            painted_secs: None,
            user_id,
            username,
            svc,
            board_rx,
            reply_tx,
            reply_rx,
        }
    }

    /// Step up to the ropes: the last word is cleared and the deck is
    /// read if it never was.
    pub(crate) fn open(&mut self) {
        self.word = None;
        if self.picks.is_none() {
            self.request(|session| {
                session
                    .svc
                    .deck_task(session.user_id, None, session.reply_tx.clone());
            });
        }
    }

    /// One request out at a time: `ask` runs unless one is.
    fn request(&mut self, ask: impl FnOnce(&Self)) {
        if self.in_flight {
            return;
        }
        self.in_flight = true;
        ask(self);
    }

    pub(crate) fn cursor_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub(crate) fn cursor_down(&mut self, roster: usize) {
        self.cursor = (self.cursor + 1).min(roster.saturating_sub(1));
    }

    /// The other corner for the next bet.
    pub(crate) fn swap_corner(&mut self) {
        self.corner = self.corner.other();
    }

    /// Call the runner under the cursor.
    pub(crate) fn call(&mut self, roster: &[Contender]) {
        let Some(picked) = roster.get(self.cursor.min(roster.len().saturating_sub(1))) else {
            self.word = Some(data::NOBODY_TO_CALL_LINE.to_string());
            return;
        };
        let called = picked.user_id;
        self.request(|session| {
            session.svc.call_task(
                session.user_id,
                session.username.clone(),
                called,
                session.reply_tx.clone(),
            );
        });
    }

    /// Take the call to this runner.
    pub(crate) fn accept(&mut self) {
        let Some(call) = call_to(&self.board, self.user_id).map(|call| call.id) else {
            return;
        };
        self.request(|session| {
            session
                .svc
                .answer_task(session.user_id, call, true, session.reply_tx.clone());
        });
    }

    /// Say no to the call to this runner, or with none take back this
    /// runner's own.
    pub(crate) fn decline(&mut self) {
        let Some(call) = call_to(&self.board, self.user_id)
            .or(call_from(&self.board, self.user_id))
            .map(|call| call.id)
        else {
            return;
        };
        self.request(|session| {
            session
                .svc
                .answer_task(session.user_id, call, false, session.reply_tx.clone());
        });
    }

    /// Flip the pick at draft `index` and store the deck.
    pub(crate) fn flip_pick(&mut self, index: usize) {
        let Some(picks) = self.picks else {
            return;
        };
        if index >= DRAFTS.len() {
            return;
        }
        let flipped = picks.flipped(index);
        self.request(|session| {
            session
                .svc
                .deck_task(session.user_id, Some(flipped), session.reply_tx.clone());
        });
    }

    /// One press of a bet on the bout that is set: a fighter's goes on
    /// their own corner, anybody else's on the corner picked.
    pub(crate) fn bet(&mut self, now: DateTime<Utc>) {
        let Phase::Set { live, .. } = phase(self.board.live.as_ref(), now) else {
            return;
        };
        let side = live.bout.side_of(self.user_id).unwrap_or(self.corner);
        let bout = live.bout.id;
        self.request(|session| {
            session.svc.bet_task(
                session.user_id,
                session.username.clone(),
                bout,
                side,
                session.reply_tx.clone(),
            );
        });
    }

    /// A fighter's key on hand card `hand`: lay it in the first empty
    /// slot, or take it back if it is laid.
    pub(crate) fn card(&mut self, hand: u8) {
        let Some((bout, side)) = self.my_corner() else {
            return;
        };
        let Some(live) = self.board.live.as_ref() else {
            return;
        };
        let slots = live.duel.corner(side).slots;
        let act = match slots.iter().position(|laid| *laid == Some(hand)) {
            Some(slot) => Act::Clear { slot },
            None => match slots.iter().position(Option::is_none) {
                Some(slot) => Act::Place { slot, hand },
                None => {
                    self.word = Some(data::SLOTS_FULL_LINE.to_string());
                    return;
                }
            },
        };
        self.act(bout, act);
    }

    /// A fighter locks the round as it is laid.
    pub(crate) fn lock(&mut self) {
        if let Some((bout, _)) = self.my_corner() {
            self.act(bout, Act::Lock);
        }
    }

    /// The round goes with the key: it is the one on this runner's
    /// screen, and the ring refuses a key for a round that came out.
    fn act(&mut self, bout: Uuid, act: Act) {
        let Some(round) = self.board.live.as_ref().map(|live| live.duel.round) else {
            return;
        };
        self.request(|session| {
            session
                .svc
                .act_task(session.user_id, bout, round, act, session.reply_tx.clone());
        });
    }

    /// The bout this runner is fighting, unended, and their side of it.
    pub(crate) fn my_corner(&self) -> Option<(Uuid, Side)> {
        let live = self.board.live.as_ref()?;
        if live.bout.ended_at.is_some() {
            return None;
        }
        Some((live.bout.id, live.bout.side_of(self.user_id)?))
    }

    /// Copy the board if it moved, drain the answer, and say what the
    /// rest of the session has to do about it. `drawn` is whether the
    /// ring is on the screen (the panel, or the street's notice): only
    /// then does a moving clock owe a frame.
    fn tick(&mut self, now: DateTime<Utc>, drawn: bool) -> RingTick {
        let mut tick = RingTick::default();
        if self.board_rx.has_changed().unwrap_or(false) {
            self.board = seen_by(&self.board_rx.borrow_and_update(), self.user_id);
            tick.changed = true;
        }
        while let Ok(reply) = self.reply_rx.try_recv() {
            self.in_flight = false;
            tick.changed = true;
            match reply {
                Reply::Deck(picks) | Reply::DeckStored(picks) => self.picks = Some(picks),
                Reply::Staked { .. } => tick.reload_sheet = true,
                Reply::Called
                | Reply::CallRefused(_)
                | Reply::Accepted
                | Reply::Declined
                | Reply::AnswerRefused(_)
                | Reply::BetRefused(_)
                | Reply::Laid
                | Reply::Locked
                | Reply::ActRefused(_)
                | Reply::Failed => {}
            }
            self.word = data::reply_line(reply);
        }
        let board = &self.board;
        if let Some(call) = call_to(board, self.user_id)
            && self.rung_for != Some(call.id)
        {
            self.rung_for = Some(call.id);
            tick.called_by = Some(call.caller_username.clone());
        }
        // A bout this runner fought or bet on, once it ends and again
        // once everything of theirs on it is settled: the purse, the pay
        // and the open gate are all on the sheet by then.
        if let Some(live) = board.live.as_ref()
            && live.bout.ended_at.is_some()
        {
            let fighter = live.bout.side_of(self.user_id);
            let bet = bet_of(live, self.user_id);
            let fighter_settled = match fighter {
                Some(Side::Caller) => live.bout.caller_settled_at.is_some(),
                Some(Side::Called) => live.bout.called_settled_at.is_some(),
                None => true,
            };
            let settled = fighter_settled && bet.is_none_or(|bet| bet.paid_at.is_some());
            let mine = fighter.is_some() || bet.is_some();
            if mine && self.reloaded_for != Some((live.bout.id, settled)) {
                self.reloaded_for = Some((live.bout.id, settled));
                tick.reload_sheet = true;
            }
        }
        let secs = match phase(board.live.as_ref(), now) {
            Phase::Empty => board
                .calls
                .iter()
                .map(|call| (call.expires_at - now).num_seconds())
                .min(),
            Phase::Set { bell_in, .. } => Some(bell_in),
            Phase::On { clock, .. } => clock,
            Phase::Over { clears_in, .. } => Some(clears_in),
        };
        if drawn && secs != self.painted_secs {
            self.painted_secs = secs;
            tick.changed = true;
        }
        tick
    }
}

/// One tick of a session's ring: the board copy and the answer, the
/// sheet re-read when this runner's bits moved, and a banner when a call
/// lands for a runner who is not already looking at the ring. A frame is
/// owed only where the ring is drawn: the city, in the panel or in the
/// street's notice (`city/ui.rs::draw_ring_notice`).
pub(crate) fn tick(app: &mut App) -> bool {
    let drawn = app.screen == Screen::City;
    let at_the_ropes = drawn && app.city.panel() == Some(Landmark::Ring);
    let tick = app.ring.tick(Utc::now(), drawn);
    if tick.reload_sheet {
        app.fight.reload();
    }
    let mut changed = tick.changed && drawn;
    if let Some(caller) = tick.called_by
        && !at_the_ropes
    {
        app.banner = Some(Banner::info(&data::called_banner(&caller)));
        changed = true;
    }
    changed
}

#[cfg(test)]
#[path = "session_test.rs"]
mod session_test;
