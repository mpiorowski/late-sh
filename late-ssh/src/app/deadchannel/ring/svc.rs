//! Orchestration for the ring (GAME.md, "The arena pass"): the call and
//! its answer, the deck a runner brings, the bets, a corner's slots, the
//! sweeper on every replica (a call past its time lapses, the bell rings,
//! a round past its clock is called, a bout nobody settled is settled),
//! the pay after a bout, the wire's lines, and the notify worker that
//! keeps every replica's copy of the board in step. The only ring layer
//! touching the database, logging, and metrics; `state.rs` decides, this
//! file does.
//!
//! Replica rule: nothing here is a process-local source of truth. A bout
//! is a row; every change to it is made under its row lock or as one
//! conditional statement; a fighter's settle and a bet's pay are claims
//! on rows, taken in the same transaction as the bits they move. The
//! board each replica holds is a copy for drawing and for the sweeper's
//! clock, and what the sweeper does it does again under the lock.
//!
//! Lock order, everywhere: the bout's row, then a bet's, then one
//! runner's. The fight never takes a bout's row, so the two cannot cross.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use late_core::db::Db;
use late_core::models::deadchannel_bout::{
    AcceptResult, Bout, BoutBet, BoutOutcome, CallResult, NewCall, RingDeck, Side, StakeResult,
};
use tokio::sync::{mpsc, watch};
use tracing::{Instrument, info_span};
use uuid::Uuid;

use super::data::{
    self, AFTERMATH_SECS, BELL_SECS, CALL_SECS, MAX_PRESSES, REVEAL_SECS, ROUND_SECS,
};
use super::state::{self, Duel, Ending, Picks, Pool, SIDES, Settlement};
use crate::app::chat::svc::ChatService;
use crate::app::deadchannel::fight::data::RULES;
use crate::app::deadchannel::fight::state::{Applied, Command, Refusal};
use crate::app::deadchannel::fight::svc::FightService;
use crate::app::deadchannel::runner::svc::RunnerLooks;
use crate::app::presence::svc::PresenceService;
use crate::metrics::{self, RingBeat};
use crate::pg_listener::{Channel, Refresh, Signal, read_until_ok};

/// How often every replica looks at the ring's clocks.
const SWEEP_TICK: Duration = Duration::from_secs(1);
/// Every this many ticks the sweeper also re-reads the board and looks
/// for what a dead replica left: a lapsed call, an unsettled bout.
const SLOW_EVERY: u32 = 15;
/// How long an ended bout may sit unsettled before any sweeper settles
/// it: long enough for the replica that ended it to pay in the ordinary
/// case.
const UNSETTLED_GRACE_SECS: i64 = 20;

/// The bout in the ring: on, or ended and still on the board.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LiveBout {
    pub bout: Bout,
    /// The whole duel, both corners' slots included: a session keeps a
    /// copy cut down to what its runner may see (`session::seen_by`).
    pub duel: Duel,
    pub bets: Vec<BoutBet>,
}

/// What every session shares: the calls waiting for an answer, the bout
/// in the ring, and every runner's record.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Board {
    pub calls: Vec<Bout>,
    pub live: Option<LiveBout>,
    /// Wins and losses by runner. A runner who has neither is absent.
    pub records: HashMap<Uuid, (i32, i32)>,
}

pub(crate) type RingBoard = Arc<Board>;

/// What a corner does to its round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Act {
    Place { slot: usize, hand: u8 },
    Clear { slot: usize },
    Lock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallRefusal {
    Yourself,
    /// The caller has no mark: the ring is for runners who put the Old
    /// Signal down.
    NotMarked,
    TheyAreNotMarked,
    /// The runner called is not on the wire.
    Offline,
    RingTaken,
    AlreadyCalling,
    /// The caller's last call to this runner has not run out its time.
    TooSoon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnswerRefusal {
    /// The call was answered, taken back or lapsed already.
    Gone,
    /// Not a call to or from this runner.
    NotYours,
    RingTaken,
    FoughtToday,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BetRefusal {
    /// The bell rang, or the bout is not on.
    Closed,
    OtherSide,
    Capped,
    Short {
        by: i64,
    },
    /// A glyph is waiting on the runner's row.
    FightWaiting,
    /// A fighter backs their own corner or nobody.
    OwnCornerOnly,
    NoRunner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActRefusal {
    /// No bout on under that id, or this runner is not in it.
    NotYours,
    /// The round the runner was looking at came out before the key
    /// landed: nothing is laid or locked on a hand they have not seen.
    RoundGone,
    Round(state::Refusal),
}

/// What a session's request came back with. The board itself travels on
/// the watch; a reply says only what the asking session has to hear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reply {
    Called,
    CallRefused(CallRefusal),
    Accepted,
    /// A call declined by the runner called, or taken back by its caller.
    Declined,
    AnswerRefused(AnswerRefusal),
    /// The deck this runner brings, as the row holds it.
    Deck(Picks),
    /// The deck this runner just chose, stored.
    DeckStored(Picks),
    Staked {
        bits: i64,
    },
    BetRefused(BetRefusal),
    /// A card laid or taken back.
    Laid,
    /// The round locked.
    Locked,
    ActRefused(ActRefusal),
    Failed,
}

/// What a write under the bout's lock left the round at.
enum Step {
    /// Nothing was due.
    Waiting,
    Resolved,
    Ended(Box<Bout>),
}

struct Advance {
    step: Step,
    /// The round's clock ran out on a corner that had not locked.
    timed_out: bool,
}

#[derive(Clone)]
pub struct RingService {
    db: Db,
    fight: FightService,
    chat: ChatService,
    presence: PresenceService,
    runners_rx: watch::Receiver<RunnerLooks>,
    board_tx: watch::Sender<RingBoard>,
    is_draining: Arc<AtomicBool>,
}

impl RingService {
    pub fn new(
        db: Db,
        fight: FightService,
        chat: ChatService,
        presence: PresenceService,
        runners_rx: watch::Receiver<RunnerLooks>,
        is_draining: Arc<AtomicBool>,
    ) -> Self {
        let (board_tx, _) = watch::channel(RingBoard::default());
        Self {
            db,
            fight,
            chat,
            presence,
            runners_rx,
            board_tx,
            is_draining,
        }
    }

    /// The process-shared board. Empty until the listener's first load.
    pub(crate) fn subscribe(&self) -> watch::Receiver<RingBoard> {
        self.board_tx.subscribe()
    }

    /// What the notify worker subscribes to.
    pub const CHANNELS: &'static [Channel] = &[Channel::DeadchannelBout];

    /// Keep every replica's board in step with the channel. A notify
    /// names the bout that moved and the board is small, so every signal
    /// re-reads all of it.
    pub fn start_notify_worker(
        &self,
        mut signals: mpsc::UnboundedReceiver<Signal>,
    ) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            while let Some(signal) = signals.recv().await {
                match signal {
                    Signal::Resync => {
                        tracing::debug!("ring board resync");
                        read_until_ok(Refresh::RingBoard, || service.refresh()).await;
                    }
                    Signal::Notify { .. } => {
                        if let Err(error) = service.refresh().await {
                            tracing::error!(error = ?error, "failed to re-read the ring's board");
                        }
                    }
                }
            }
        })
    }

    /// Re-read the board and publish it.
    pub async fn refresh(&self) -> Result<()> {
        let client = self.db.get().await?;
        let now = Utc::now();
        let since = now - chrono::Duration::seconds(AFTERMATH_SECS);
        let mut calls = Vec::new();
        let mut fought = Vec::new();
        for bout in Bout::board(&**client, since).await? {
            match bout.is_accepted() {
                true => fought.push(bout),
                false => calls.push(bout),
            }
        }
        // The bout in the ring, or failing that the last one out of it.
        let on = fought.iter().position(|bout| bout.ended_at.is_none());
        let shown = match on {
            Some(at) => Some(fought.swap_remove(at)),
            None => fought.into_iter().max_by_key(|bout| bout.ended_at),
        };
        let live = match shown {
            Some(bout) => {
                let duel = duel_of(&bout)?;
                let bets = BoutBet::list(&**client, bout.id).await?;
                Some(LiveBout { bout, duel, bets })
            }
            None => None,
        };
        let records = Bout::records(&**client).await?;
        self.board_tx.send_replace(Arc::new(Board {
            calls,
            live,
            records,
        }));
        Ok(())
    }

    /// The sweeper, every [`SWEEP_TICK`], for the life of the process. A
    /// replica draining for a deploy stops: the newer one owns the rows.
    pub fn start_sweeper_task(&self) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            let mut ticks = 0u32;
            loop {
                tokio::time::sleep(SWEEP_TICK).await;
                if service.is_draining.load(Ordering::Relaxed) {
                    continue;
                }
                ticks = ticks.wrapping_add(1);
                service
                    .sweep(Utc::now(), ticks.is_multiple_of(SLOW_EVERY))
                    .instrument(info_span!("deadchannel.ring.sweep"))
                    .await;
            }
        })
    }

    /// One tick of the sweeper, off this replica's copy of the board:
    /// lapse the calls past their time, and ring the bell or call the
    /// round when the bout in the ring is due. A `slow` tick also does
    /// what the copy cannot show: it lapses and settles straight off the
    /// rows, then re-reads the board. Every failure is logged here.
    pub(crate) async fn sweep(&self, now: DateTime<Utc>, slow: bool) {
        let board = self.board_tx.borrow().clone();
        if slow || board.calls.iter().any(|call| call.expires_at <= now) {
            match self.expire_calls(now).await {
                Ok(()) => {}
                Err(error) => {
                    tracing::error!(error = ?error, "failed to lapse the ring's calls");
                }
            }
        }
        if let Some(live) = &board.live
            && is_due(&live.bout, &live.duel, now)
        {
            match self.advance(live.bout.id, now).await {
                Ok(()) => {}
                Err(error) => {
                    metrics::record_deadchannel_ring(RingBeat::Failed);
                    tracing::error!(error = ?error, bout_id = %live.bout.id, "failed to advance the bout");
                }
            }
        }
        if slow {
            match self.settle_the_unsettled(now).await {
                Ok(()) => {}
                Err(error) => {
                    tracing::error!(error = ?error, "failed to settle the unsettled bouts");
                }
            }
            if let Err(error) = self.refresh().await {
                tracing::error!(error = ?error, "failed to re-read the ring's board");
            }
        }
    }

    async fn expire_calls(&self, now: DateTime<Utc>) -> Result<()> {
        let client = self.db.get().await?;
        for call in Bout::expire_calls(&**client, now).await? {
            metrics::record_deadchannel_ring(RingBeat::Expired);
            tracing::info!(bout_id = %call.id, caller = %call.caller_username, called = %call.called_username, "ring call lapsed unanswered");
        }
        Ok(())
    }

    /// Do whatever the bout's clock says is due, under its lock: the
    /// sweeper's half of a round. A bout that moved on since the copy was
    /// read is left alone.
    async fn advance(&self, bout_id: Uuid, now: DateTime<Utc>) -> Result<()> {
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let Some(bout) = Bout::lock(&*tx, bout_id).await? else {
            return Ok(());
        };
        if !bout.is_accepted() || bout.ended_at.is_some() {
            return Ok(());
        }
        let mut duel = duel_of(&bout)?;
        let advance = advance_in(&tx, &bout, &mut duel, now).await?;
        tx.commit().await?;
        self.landed(bout_id, &duel, advance, now).await;
        Ok(())
    }

    /// What follows a committed write on a bout: the count, and for a
    /// bout that ended the wire's line and the pay.
    async fn landed(&self, bout_id: Uuid, duel: &Duel, advance: Advance, now: DateTime<Utc>) {
        if advance.timed_out {
            metrics::record_deadchannel_ring(RingBeat::TimedOut);
            tracing::info!(bout_id = %bout_id, round = duel.round, "ring round called on the clock");
        }
        match advance.step {
            Step::Waiting => {}
            Step::Resolved => {
                metrics::record_deadchannel_ring(RingBeat::Resolved);
                tracing::info!(bout_id = %bout_id, round = duel.round, "ring round resolved");
            }
            Step::Ended(bout) => {
                let ending = duel
                    .ending
                    .expect("a bout ends on the row only with its duel's ending");
                metrics::record_deadchannel_ring(RingBeat::Ended);
                tracing::info!(bout_id = %bout_id, ending = ?ending, rounds = duel.round, pool = bout.pool_caller + bout.pool_called, "ring bout ended");
                self.chat
                    .post_wire_line_task(data::result_line(&bout, duel, ending));
                self.settle(&bout, ending, now).await;
            }
        }
    }

    async fn settle_the_unsettled(&self, now: DateTime<Utc>) -> Result<()> {
        let unsettled = {
            let client = self.db.get().await?;
            Bout::unsettled(&**client).await?
        };
        let ended_by = now - chrono::Duration::seconds(UNSETTLED_GRACE_SECS);
        for bout in unsettled {
            if bout.ended_at.is_none_or(|ended| ended > ended_by) {
                continue;
            }
            tracing::warn!(bout_id = %bout.id, "ring bout found unsettled; settling");
            let ending = duel_of(&bout)?
                .ending
                .context("an ended bout holds a duel with no ending")?;
            self.settle(&bout, ending, now).await;
        }
        Ok(())
    }

    /// Settle a bout that ended: each fighter's gate and the winner's
    /// purse, then every bet. Each is its own transaction with its own
    /// claim, so two replicas settling the same bout pay each runner
    /// once, and one runner's failure costs nobody else theirs. A failure
    /// is logged and counted here, and the claim it rolled back with is
    /// there for the next sweep.
    async fn settle(&self, bout: &Bout, ending: Ending, now: DateTime<Utc>) {
        let pool = Pool {
            caller: bout.pool_caller,
            called: bout.pool_called,
        };
        let settlement = state::settle(pool, ending);
        for side in SIDES {
            let purse = match settlement {
                Settlement::Paid { winner, purse, .. } if winner == side => purse,
                Settlement::Paid { .. } | Settlement::Refund => 0,
            };
            match self.settle_fighter(bout, side, purse, now).await {
                Ok(Some(true)) => {
                    metrics::record_deadchannel_ring(RingBeat::Settled);
                    tracing::info!(bout_id = %bout.id, user_id = %bout.user(side), purse, "ring fighter settled");
                }
                Ok(Some(false)) => {
                    metrics::record_deadchannel_ring(RingBeat::PayLeft);
                    tracing::warn!(bout_id = %bout.id, user_id = %bout.user(side), purse, "ring fighter has no standing runner; gate and purse left");
                }
                Ok(None) => {}
                Err(error) => {
                    metrics::record_deadchannel_ring(RingBeat::SettleFailed);
                    tracing::error!(error = ?error, bout_id = %bout.id, user_id = %bout.user(side), "failed to settle a ring fighter");
                }
            }
        }
        let unpaid = async {
            let client = self.db.get().await?;
            BoutBet::unpaid(&**client, bout.id).await
        }
        .await;
        let unpaid = match unpaid {
            Ok(unpaid) => unpaid,
            Err(error) => {
                metrics::record_deadchannel_ring(RingBeat::SettleFailed);
                tracing::error!(error = ?error, bout_id = %bout.id, "failed to list the bout's unpaid bets");
                return;
            }
        };
        for bet in unpaid {
            let bits = state::pay(pool, settlement, bet.side, bet.bits);
            match self.pay_bet(&bet, bits, now).await {
                Ok(Some(true)) => {
                    metrics::record_deadchannel_ring(RingBeat::Paid);
                    tracing::info!(bout_id = %bout.id, user_id = %bet.user_id, staked = bet.bits, bits, "ring bet paid");
                }
                Ok(Some(false)) => {
                    metrics::record_deadchannel_ring(RingBeat::PayLeft);
                    tracing::warn!(bout_id = %bout.id, user_id = %bet.user_id, bits, "ring bettor has no standing runner; pay left");
                }
                Ok(None) => {}
                Err(error) => {
                    metrics::record_deadchannel_ring(RingBeat::SettleFailed);
                    tracing::error!(error = ?error, bout_id = %bout.id, user_id = %bet.user_id, "failed to pay a ring bet");
                }
            }
        }
    }

    /// One fighter's end of a bout: the gate it clears and the purse, in
    /// the transaction that takes the claim. `None` when another settle
    /// holds the claim; `Some(false)` when the runner no longer stands.
    async fn settle_fighter(
        &self,
        bout: &Bout,
        side: Side,
        purse: i64,
        now: DateTime<Utc>,
    ) -> Result<Option<bool>> {
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        if !Bout::claim_fighter_settle(&*tx, bout.id, side, now).await? {
            return Ok(None);
        }
        let user_id = bout.user(side);
        let Some(fought) = self
            .fight
            .apply_in(&tx, user_id, Command::RingFought)
            .await?
        else {
            tx.commit().await?;
            return Ok(Some(false));
        };
        let collected = match purse {
            0 => None,
            bits => {
                self.fight
                    .apply_in(&tx, user_id, Command::Collect { bits })
                    .await?
            }
        };
        tx.commit().await?;
        FightService::landed(user_id, &fought);
        if let Some(collected) = &collected {
            FightService::landed(user_id, collected);
        }
        Ok(Some(true))
    }

    /// One bet's pay, in the transaction that takes its claim. `None`
    /// when another settle holds the claim; `Some(false)` when the runner
    /// no longer stands.
    async fn pay_bet(&self, bet: &BoutBet, bits: i64, now: DateTime<Utc>) -> Result<Option<bool>> {
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        if BoutBet::claim_pay(&*tx, bet.bout_id, bet.user_id, bits, now)
            .await?
            .is_none()
        {
            return Ok(None);
        }
        let collected = match bits {
            0 => None,
            bits => {
                let Some(collected) = self
                    .fight
                    .apply_in(&tx, bet.user_id, Command::Collect { bits })
                    .await?
                else {
                    tx.commit().await?;
                    return Ok(Some(false));
                };
                Some(collected)
            }
        };
        tx.commit().await?;
        if let Some(collected) = &collected {
            FightService::landed(bet.user_id, collected);
        }
        Ok(Some(true))
    }

    /// Call `called` to the ring for a session and answer on `reply`.
    pub(crate) fn call_task(
        &self,
        user_id: Uuid,
        username: String,
        called: Uuid,
        reply: mpsc::UnboundedSender<Reply>,
    ) {
        let svc = self.clone();
        let span = info_span!("deadchannel.ring.call_task", user_id = %user_id, called = %called);
        self.run(span, reply, async move {
            svc.call(user_id, &username, called, Utc::now()).await
        });
    }

    /// Answer a call for a session: `accept` takes it, anything else ends
    /// it, from either end.
    pub(crate) fn answer_task(
        &self,
        user_id: Uuid,
        bout_id: Uuid,
        accept: bool,
        reply: mpsc::UnboundedSender<Reply>,
    ) {
        let svc = self.clone();
        let span = info_span!("deadchannel.ring.answer_task", user_id = %user_id, bout_id = %bout_id, accept);
        self.run(span, reply, async move {
            match accept {
                true => svc.accept(user_id, bout_id, Utc::now()).await,
                false => svc.decline(user_id, bout_id, Utc::now()).await,
            }
        });
    }

    /// Read the deck a runner brings, or store `picks` as it.
    pub(crate) fn deck_task(
        &self,
        user_id: Uuid,
        picks: Option<Picks>,
        reply: mpsc::UnboundedSender<Reply>,
    ) {
        let svc = self.clone();
        let span = info_span!("deadchannel.ring.deck_task", user_id = %user_id);
        self.run(span, reply, async move {
            let client = svc.db.get().await?;
            match picks {
                Some(picks) => {
                    RingDeck::set(&**client, user_id, &picks.to_json(), Utc::now()).await?;
                    Ok(Reply::DeckStored(picks))
                }
                None => Ok(Reply::Deck(picks_of(&client, user_id).await?)),
            }
        });
    }

    /// One press of a bet on `side` for a session.
    pub(crate) fn bet_task(
        &self,
        user_id: Uuid,
        username: String,
        bout_id: Uuid,
        side: Side,
        reply: mpsc::UnboundedSender<Reply>,
    ) {
        let svc = self.clone();
        let span = info_span!("deadchannel.ring.bet_task", user_id = %user_id, bout_id = %bout_id, side = side.as_str());
        self.run(span, reply, async move {
            svc.bet(user_id, &username, bout_id, side, Utc::now()).await
        });
    }

    /// One change to a corner's round for a session.
    pub(crate) fn act_task(
        &self,
        user_id: Uuid,
        bout_id: Uuid,
        round: u8,
        act: Act,
        reply: mpsc::UnboundedSender<Reply>,
    ) {
        let svc = self.clone();
        let span = info_span!("deadchannel.ring.act_task", user_id = %user_id, bout_id = %bout_id, round, act = ?act);
        self.run(span, reply, async move {
            svc.act(user_id, bout_id, round, act, Utc::now()).await
        });
    }

    /// Run one request and answer on `reply`: the one place a request's
    /// outcome is counted and logged.
    fn run(
        &self,
        span: tracing::Span,
        reply: mpsc::UnboundedSender<Reply>,
        work: impl Future<Output = Result<Reply>> + Send + 'static,
    ) {
        tokio::spawn(
            async move {
                let answer = match work.await {
                    Ok(answer) => answer,
                    Err(error) => {
                        tracing::error!(error = ?error, "ring request failed");
                        Reply::Failed
                    }
                };
                let beat = match answer {
                    Reply::Called => Some(RingBeat::Called),
                    Reply::CallRefused(_) => Some(RingBeat::CallRefused),
                    Reply::Accepted => Some(RingBeat::Accepted),
                    Reply::Declined => Some(RingBeat::Declined),
                    Reply::AnswerRefused(_) => Some(RingBeat::AcceptRefused),
                    // A read is not a beat.
                    Reply::Deck(_) => None,
                    Reply::DeckStored(_) => Some(RingBeat::DeckSet),
                    Reply::Staked { .. } => Some(RingBeat::Bet),
                    Reply::BetRefused(_) => Some(RingBeat::BetRefused),
                    // Laying a card is not a beat.
                    Reply::Laid | Reply::ActRefused(_) => None,
                    Reply::Locked => Some(RingBeat::Locked),
                    Reply::Failed => Some(RingBeat::Failed),
                };
                if let Some(beat) = beat {
                    metrics::record_deadchannel_ring(beat);
                }
                match answer {
                    Reply::Called | Reply::Accepted | Reply::Declined | Reply::Staked { .. } => {
                        tracing::info!(answer = ?answer, "ring request landed");
                    }
                    Reply::CallRefused(_)
                    | Reply::AnswerRefused(_)
                    | Reply::BetRefused(_)
                    | Reply::ActRefused(_)
                    | Reply::Deck(_)
                    | Reply::DeckStored(_)
                    | Reply::Laid
                    | Reply::Locked => {
                        tracing::debug!(answer = ?answer, "ring request answered");
                    }
                    Reply::Failed => {}
                }
                // A closed channel is a session that left; nothing to tell.
                let _ = reply.send(answer);
            }
            .instrument(span),
        );
    }

    /// One call: both runners marked, the one called on the wire, and
    /// then the row decides (one open call a caller, nobody calls while
    /// the ring is taken).
    pub(crate) async fn call(
        &self,
        caller: Uuid,
        caller_name: &str,
        called: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Reply> {
        if caller == called {
            return Ok(Reply::CallRefused(CallRefusal::Yourself));
        }
        let looks = self.runners_rx.borrow().clone();
        let marked = |user_id: &Uuid| looks.get(user_id).is_some_and(|entry| entry.marks >= 1);
        if !marked(&caller) {
            return Ok(Reply::CallRefused(CallRefusal::NotMarked));
        }
        if !marked(&called) {
            return Ok(Reply::CallRefused(CallRefusal::TheyAreNotMarked));
        }
        let records = self.presence.records();
        let Some(called_name) = records
            .iter()
            .find(|record| record.user_id == called)
            .map(|record| record.username.clone())
        else {
            return Ok(Reply::CallRefused(CallRefusal::Offline));
        };
        let client = self.db.get().await?;
        let new = NewCall {
            day: now.date_naive(),
            caller_user_id: caller,
            called_user_id: called,
            caller_username: caller_name.to_string(),
            called_username: called_name,
            called_at: now,
            expires_at: now + chrono::Duration::seconds(CALL_SECS),
        };
        match Bout::call(&**client, &new).await? {
            CallResult::Called(bout) => {
                self.chat.post_wire_line_task(data::call_line(&bout));
                self.chat
                    .post_voice_dm_task(bout.called_user_id, data::call_dm(&bout));
                Ok(Reply::Called)
            }
            CallResult::RingTaken => Ok(Reply::CallRefused(CallRefusal::RingTaken)),
            CallResult::AlreadyCalling => Ok(Reply::CallRefused(CallRefusal::AlreadyCalling)),
            CallResult::TooSoon => Ok(Reply::CallRefused(CallRefusal::TooSoon)),
        }
    }

    /// Take a call: both decks are read and locked in, round 1 is dealt,
    /// and the bell is set. The row's indexes say whether the ring and
    /// the pair's day are still free. A call to a pair that already
    /// fought today is ended on the spot instead of left to lapse.
    pub(crate) async fn accept(
        &self,
        user_id: Uuid,
        bout_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Reply> {
        let mut client = self.db.get().await?;
        let Some(call) = Bout::get(&**client, bout_id).await? else {
            return Ok(Reply::AnswerRefused(AnswerRefusal::Gone));
        };
        if call.called_user_id != user_id {
            return Ok(Reply::AnswerRefused(AnswerRefusal::NotYours));
        }
        if call.is_accepted() || call.ended_at.is_some() {
            return Ok(Reply::AnswerRefused(AnswerRefusal::Gone));
        }
        let mut decks = Vec::with_capacity(SIDES.len());
        for side in SIDES {
            let fighter = call.user(side);
            // A runner who walked off the ledge since the call has no
            // sheet: there is nobody to fight.
            let Some(sheet) = self.fight.settled_sheet(fighter).await? else {
                Bout::end_call(&**client, bout_id, BoutOutcome::Declined, now).await?;
                return Ok(Reply::AnswerRefused(AnswerRefusal::Gone));
            };
            decks.push(picks_of(&client, fighter).await?.deck(&sheet.inks));
        }
        let duel = Duel::deal(decks[0].clone(), decks[1].clone(), &mut rand::thread_rng());
        let bell_at = now + chrono::Duration::seconds(BELL_SECS);
        let accepted = Bout::accept(
            &mut **client,
            bout_id,
            &serde_json::json!(decks[0]),
            &serde_json::json!(decks[1]),
            &serde_json::to_value(&duel).context("writing the duel")?,
            bell_at,
            bell_at + chrono::Duration::seconds(ROUND_SECS),
            now,
        )
        .await?;
        match accepted {
            AcceptResult::Accepted(bout) => {
                self.chat.post_wire_line_task(data::set_line(&bout));
                Ok(Reply::Accepted)
            }
            AcceptResult::Gone => Ok(Reply::AnswerRefused(AnswerRefusal::Gone)),
            AcceptResult::RingTaken => Ok(Reply::AnswerRefused(AnswerRefusal::RingTaken)),
            AcceptResult::FoughtToday => {
                Bout::end_call(&**client, bout_id, BoutOutcome::Declined, now).await?;
                Ok(Reply::AnswerRefused(AnswerRefusal::FoughtToday))
            }
        }
    }

    /// End an open call from either end: the runner called says no, or
    /// the caller takes it back.
    pub(crate) async fn decline(
        &self,
        user_id: Uuid,
        bout_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Reply> {
        let client = self.db.get().await?;
        let Some(call) = Bout::get(&**client, bout_id).await? else {
            return Ok(Reply::AnswerRefused(AnswerRefusal::Gone));
        };
        if call.side_of(user_id).is_none() {
            return Ok(Reply::AnswerRefused(AnswerRefusal::NotYours));
        }
        match Bout::end_call(&**client, bout_id, BoutOutcome::Declined, now).await? {
            Some(_) => Ok(Reply::Declined),
            None => Ok(Reply::AnswerRefused(AnswerRefusal::Gone)),
        }
    }

    /// One press of a bet: the pool, the bet's row and the bits off the
    /// runner's sheet land together or not at all. The press is one
    /// cache's worth at the bettor's level.
    pub(crate) async fn bet(
        &self,
        user_id: Uuid,
        username: &str,
        bout_id: Uuid,
        side: Side,
        now: DateTime<Utc>,
    ) -> Result<Reply> {
        let Some(level) = self
            .runners_rx
            .borrow()
            .get(&user_id)
            .map(|entry| entry.level)
        else {
            return Ok(Reply::BetRefused(BetRefusal::NoRunner));
        };
        let bits = RULES.cache(level);
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let Some(bout) = Bout::lock(&*tx, bout_id).await? else {
            return Ok(Reply::BetRefused(BetRefusal::Closed));
        };
        match bout.side_of(user_id) {
            Some(own) if own != side => {
                return Ok(Reply::BetRefused(BetRefusal::OwnCornerOnly));
            }
            Some(_) | None => {}
        }
        if !Bout::add_to_pool(&*tx, bout_id, side, bits, now).await? {
            return Ok(Reply::BetRefused(BetRefusal::Closed));
        }
        match BoutBet::stake(
            &*tx,
            bout_id,
            user_id,
            username,
            side,
            bits,
            MAX_PRESSES,
            now,
        )
        .await?
        {
            StakeResult::Staked(_) => {}
            StakeResult::OtherSide => return Ok(Reply::BetRefused(BetRefusal::OtherSide)),
            StakeResult::Capped => return Ok(Reply::BetRefused(BetRefusal::Capped)),
        }
        let Some(outcome) = self
            .fight
            .apply_in(&tx, user_id, Command::Stake { bits })
            .await?
        else {
            return Ok(Reply::BetRefused(BetRefusal::NoRunner));
        };
        match outcome.applied {
            Applied::Staked { bits } => {
                tx.commit().await?;
                FightService::landed(user_id, &outcome);
                Ok(Reply::Staked { bits })
            }
            Applied::Refused(Refusal::Short { by }) => {
                Ok(Reply::BetRefused(BetRefusal::Short { by }))
            }
            Applied::Refused(Refusal::FightWaiting) => {
                Ok(Reply::BetRefused(BetRefusal::FightWaiting))
            }
            other => bail!("a stake came back as {other:?}"),
        }
    }

    /// One change to a corner's round, under the bout's lock. `round` is
    /// the round the runner was looking at: a key that lands after it
    /// came out is refused. A lock that finds both corners in and the
    /// bell rung resolves the round on the spot.
    pub(crate) async fn act(
        &self,
        user_id: Uuid,
        bout_id: Uuid,
        round: u8,
        act: Act,
        now: DateTime<Utc>,
    ) -> Result<Reply> {
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let Some(bout) = Bout::lock(&*tx, bout_id).await? else {
            return Ok(Reply::ActRefused(ActRefusal::NotYours));
        };
        let Some(side) = bout.side_of(user_id) else {
            return Ok(Reply::ActRefused(ActRefusal::NotYours));
        };
        if !bout.is_accepted() || bout.ended_at.is_some() {
            return Ok(Reply::ActRefused(ActRefusal::Round(state::Refusal::Over)));
        }
        let mut duel = duel_of(&bout)?;
        if duel.round != round {
            return Ok(Reply::ActRefused(ActRefusal::RoundGone));
        }
        let moved = match act {
            Act::Place { slot, hand } => duel.place(side, slot, hand),
            Act::Clear { slot } => duel.clear(side, slot),
            Act::Lock => duel.lock(side),
        };
        if let Err(refusal) = moved {
            return Ok(Reply::ActRefused(ActRefusal::Round(refusal)));
        }
        let advance = advance_in(&tx, &bout, &mut duel, now).await?;
        if matches!(advance.step, Step::Waiting) {
            Bout::store_round(
                &*tx,
                bout_id,
                bout.deadline_at,
                &serde_json::to_value(&duel).context("writing the duel")?,
            )
            .await?;
        }
        tx.commit().await?;
        self.landed(bout_id, &duel, advance, now).await;
        match act {
            Act::Lock => Ok(Reply::Locked),
            Act::Place { .. } | Act::Clear { .. } => Ok(Reply::Laid),
        }
    }
}

/// Whether the bout's clock asks for a write at `now`: the bell has rung
/// and either both corners are in or the round's time is up.
fn is_due(bout: &Bout, duel: &Duel, now: DateTime<Utc>) -> bool {
    if bout.ended_at.is_some() {
        return false;
    }
    let rung = bout.bell_at.is_some_and(|bell| bell <= now);
    let out_of_time = bout.deadline_at.is_some_and(|deadline| deadline <= now);
    rung && (duel.ready() || out_of_time)
}

/// Do what is due on a locked, accepted, unended bout and store it: a
/// round past its clock is called, a round both corners locked resolves,
/// and the bout ends or the next round goes on the clock. Nothing is
/// written when nothing is due.
async fn advance_in(
    tx: &tokio_postgres::Transaction<'_>,
    bout: &Bout,
    duel: &mut Duel,
    now: DateTime<Utc>,
) -> Result<Advance> {
    if !is_due(bout, duel, now) {
        return Ok(Advance {
            step: Step::Waiting,
            timed_out: false,
        });
    }
    let timed_out = !duel.ready();
    duel.call_time();
    duel.resolve(&mut rand::thread_rng());
    let state = serde_json::to_value(&*duel).context("writing the duel")?;
    let step = match duel.ending {
        Some(ending) => {
            let outcome = match ending.winner() {
                Some(Side::Caller) => BoutOutcome::Caller,
                Some(Side::Called) => BoutOutcome::Called,
                None => BoutOutcome::Draw,
            };
            let ended = Bout::end(tx, bout.id, outcome, &state, now)
                .await?
                .context("a locked, open bout refused its ending")?;
            Step::Ended(Box::new(ended))
        }
        None => {
            let deadline = now + chrono::Duration::seconds(REVEAL_SECS + ROUND_SECS);
            Bout::store_round(tx, bout.id, Some(deadline), &state).await?;
            Step::Resolved
        }
    };
    Ok(Advance { step, timed_out })
}

/// The duel an accepted bout's row holds.
fn duel_of(bout: &Bout) -> Result<Duel> {
    let state = bout
        .state
        .clone()
        .with_context(|| format!("accepted bout {} holds no duel", bout.id))?;
    serde_json::from_value(state).with_context(|| format!("reading the duel of bout {}", bout.id))
}

/// The picks a runner brings: the row's, or the opening four for a
/// runner who never chose. A row this build cannot read fails loudly.
async fn picks_of(client: &tokio_postgres::Client, user_id: Uuid) -> Result<Picks> {
    match RingDeck::get(client, user_id).await? {
        Some(cards) => Picks::parse(&cards)
            .with_context(|| format!("runner {user_id} holds a ring deck that is not four picks")),
        None => Ok(Picks::opening()),
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
