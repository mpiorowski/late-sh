//! The one writer of the runner's sheet (root CONTEXT.md, "per-user
//! state"): every command is one transaction over the locked row: lock,
//! roll the day if it turned, apply one rule, store the whole sheet,
//! commit. "Already spent" and "already down" are read off the locked
//! row, never off a session's mirror, so two devices and any number of
//! replicas queue on the lock and act one after the other.
//!
//! The crossing (GAME.md, "The crossing pass") is the one thing here that
//! touches two rows: a step onto it finds who sleeps there (the level
//! band, the day's caps in `deadchannel_ambushes`, the rows as they
//! stand), locks their row after the attacker's, and puts them on the
//! attacker's row as the fight (`Sheet::cross`); the answer does the same
//! the other way round (`Sheet::answer`). When such a fight ends, the
//! same transaction moves the bits between the two rows and closes the
//! ambush row, so what changed hands is written exactly once. Beside
//! every answer the service reads a `Lookout` for the session: who the
//! crossing ahead would meet, the answer owed, the latest fight on them.
//!
//! After the commit, the wire (GAME.md, "The three surfaces"): the room
//! sees the news, never the play-by-play. `Sheet::news` decides what is
//! news (a dropped signal, a level gained, the Old Signal put down, a step
//! off the ledge, a first kill, a bright glyph put down, a near miss, the
//! day's road walked to its end, a rival put down or lost to);
//! this file words it and posts it to #deadchannel as messages from the
//! voice. An ordinary kill, a card, a turn, a run, a rest, a cache, a
//! purchase, a glass, the locker, and the bits machine post nothing. The Old Signal also pays the mark's
//! chips after the commit (`pay_mark`: once per mark and at most once a
//! month, the door milestones' two gates; the debt is on the row as
//! `unpaid_mark` until the grant answers, so a grant that errors is
//! retried on the next command or reload) and grants the rankless `SIG`
//! profile badge, once per account.
//!
//! Orchestration only: the span, the metric, the log line per failure
//! mode, and the reply live here; `state.rs` returns data.

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use late_core::db::Db;
use late_core::models::chips::ChipMove;
use late_core::models::deadchannel_ambush::{Ambush, AmbushOutcome, NewAmbush};
use late_core::models::deadchannel_runner::DeadchannelRunner;
use late_core::models::profile_award::{
    DEADCHANNEL_OLD_SIGNAL_AWARD_CATEGORY, grant_unique_milestone_award,
};
use late_core::models::reward::DEADCHANNEL_OLD_SIGNAL_REWARD_KEY;
use late_core::models::user::User;
use tokio::sync::mpsc;
use tokio_postgres::{Client, GenericClient};
use tracing::{Instrument, info_span};
use uuid::Uuid;

use super::data::{self, ANSWER_HOURS, CROSSING_LEVEL_BAND};
use super::road::Node;
use super::state::{
    Answer, Applied, Command, Leavings, Lookout, News, Outcome, Pick, Prey, Rival, Sheet, Token,
    crossing_order,
};
use crate::app::chat::svc::ChatService;
use crate::app::common::primitives::thousands;
use crate::app::deadchannel::runner::state::Look;
use crate::app::games::chips::svc::ChipService;
use crate::metrics::{self, FightBeat, OldSignalPayout};

/// What a session's request came back with. Sent to the asking session
/// only.
#[derive(Debug, Clone)]
pub(crate) enum FightOutcome {
    Acted {
        sheet: Sheet,
        outcome: Outcome,
        lookout: Lookout,
    },
    Reloaded {
        sheet: Sheet,
        lookout: Lookout,
    },
    /// No standing runner for this user: the door is shut or was never
    /// opened. The city's gate should have kept them out; logged as such.
    NoRunner,
    ActionFailed,
}

#[derive(Clone)]
pub struct FightService {
    db: Db,
    chat: ChatService,
    chips: ChipService,
}

/// What a session asks the row for: a command of the machine's, or one
/// of the two steps that need another row found first. The two carry
/// the asker's name: the ambush row keeps both names as they were.
#[derive(Debug, Clone)]
enum Ask {
    Command(Command),
    /// A step onto the crossing on `lane`.
    Cross {
        lane: u8,
        username: String,
    },
    /// The answer to the crossing this runner is owed.
    Answer {
        username: String,
    },
}

/// One applied ask, as `act` hands it to the task: the runner row it ran
/// on (the Old Signal's payout keys on it), the sheet after, what
/// happened, and what a rival's fall or fate moved onto the hand.
struct Acted {
    runner_id: Uuid,
    generation: i32,
    sheet: Sheet,
    outcome: Outcome,
    /// Bits taken off a rival this ask put down; zero otherwise.
    taken: i64,
}

/// One reload, as `reload` hands it to the task: the runner row and the
/// sheet as settled.
struct Reloaded {
    runner_id: Uuid,
    generation: i32,
    sheet: Sheet,
}

impl FightService {
    pub fn new(db: Db, chat: ChatService, chips: ChipService) -> Self {
        Self { db, chat, chips }
    }

    /// Run one command for a session and answer on `reply`. `username` is
    /// for the wire lines and the logs; the row is keyed on `user_id`.
    pub(crate) fn act_task(
        &self,
        user_id: Uuid,
        username: String,
        command: Command,
        reply: mpsc::UnboundedSender<FightOutcome>,
    ) {
        let span = info_span!("deadchannel.fight.act_task", user_id = %user_id, username = %username, command = ?command);
        self.ask_task(user_id, username, Ask::Command(command), reply, span);
    }

    /// A step onto the crossing on `lane` for a session: who sleeps there
    /// is found under the lock.
    pub(crate) fn cross_task(
        &self,
        user_id: Uuid,
        username: String,
        lane: u8,
        reply: mpsc::UnboundedSender<FightOutcome>,
    ) {
        let span = info_span!("deadchannel.fight.cross_task", user_id = %user_id, username = %username, lane);
        let ask = Ask::Cross {
            lane,
            username: username.clone(),
        };
        self.ask_task(user_id, username, ask, reply, span);
    }

    /// The answer to the crossing this session's runner is owed.
    pub(crate) fn answer_task(
        &self,
        user_id: Uuid,
        username: String,
        reply: mpsc::UnboundedSender<FightOutcome>,
    ) {
        let span =
            info_span!("deadchannel.fight.answer_task", user_id = %user_id, username = %username);
        let ask = Ask::Answer {
            username: username.clone(),
        };
        self.ask_task(user_id, username, ask, reply, span);
    }

    fn ask_task(
        &self,
        user_id: Uuid,
        username: String,
        ask: Ask,
        reply: mpsc::UnboundedSender<FightOutcome>,
        span: tracing::Span,
    ) {
        let svc = self.clone();
        tokio::spawn(
            async move {
                let outcome = match svc.act(user_id, ask).await {
                    Ok(Some(Acted {
                        runner_id,
                        generation,
                        sheet,
                        mut outcome,
                        taken,
                    })) => {
                        metrics::record_deadchannel_fight(beat_for(&outcome.applied));
                        tracing::info!(applied = ?outcome.applied, level = sheet.level, signal = sheet.signal, rations_left = sheet.rations_left, bits = sheet.bits, crystals = sheet.crystals, taken, "fight command applied");
                        svc.post_news(&username, &sheet, &outcome.applied, taken)
                            .await;
                        // The debt on the row: this kill's mark, or one an
                        // earlier grant failed to settle. Either way the
                        // line lands under this answer.
                        if let Some(mark) = sheet.unpaid_mark {
                            outcome.lines.push(svc.pay_mark(user_id, runner_id, generation, mark).await);
                        }
                        if let Applied::Slain { marks } = outcome.applied {
                            svc.grant_old_signal_badge(user_id, marks).await;
                        }
                        let lookout = svc.lookout_for(&sheet).await;
                        FightOutcome::Acted {
                            sheet,
                            outcome,
                            lookout,
                        }
                    }
                    Ok(None) => {
                        tracing::warn!("fight command for a user with no standing runner");
                        FightOutcome::NoRunner
                    }
                    Err(error) => {
                        metrics::record_deadchannel_fight(FightBeat::Failed);
                        tracing::error!(error = ?error, "failed to apply fight command");
                        FightOutcome::ActionFailed
                    }
                };
                // A closed channel is a session that left; nothing to tell.
                let _ = reply.send(outcome);
            }
            .instrument(span),
        );
    }

    /// Lock, settle, apply, settle the rival's side if there is one,
    /// store, commit. Returns `None` when there is no standing runner.
    /// Nothing after the commit: what landed is what is answered.
    async fn act(&self, user_id: Uuid, ask: Ask) -> Result<Option<Acted>> {
        let today = Self::today();
        let now = Utc::now();
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let Some(row) = DeadchannelRunner::lock_standing(&*tx, user_id).await? else {
            return Ok(None);
        };
        let mut sheet = Sheet::from_row(&row).context("reading the runner's sheet")?;
        let rolled = settle_day(&*tx, &mut sheet, today, now).await?;
        // The rival fight still on the row after the roll, if any.
        let rival_on = sheet
            .fight
            .as_ref()
            .and_then(|fight| fight.rival().cloned());
        let mut outcome = match ask {
            Ask::Command(command) => sheet.apply(command, &mut rand::thread_rng()),
            Ask::Cross { lane, username } => {
                self.cross(&tx, &mut sheet, lane, &username, today, now)
                    .await?
            }
            Ask::Answer { username } => self.answer(&tx, &mut sheet, &username, today, now).await?,
        };
        let mut taken = 0;
        match (&outcome.applied, &rival_on) {
            (Applied::Ambushed { rival, stake }, _) => {
                taken = self
                    .settle_won(&*tx, &mut sheet, rival, *stake, today, now)
                    .await?;
                outcome.lines.push(data::took_line(&rival.username, taken));
            }
            (Applied::Downed { rival, bits_lost }, _) => {
                self.settle_lost(&*tx, rival, *bits_lost, today, now)
                    .await?;
            }
            (Applied::Escaped, Some(rival)) => {
                Ambush::end(&*tx, rival.ambush_id, AmbushOutcome::Ran, 0, now).await?;
            }
            _ => {}
        }
        let changed = rolled || !matches!(outcome.applied, Applied::Refused(_) | Applied::Resumed);
        if changed {
            DeadchannelRunner::store_sheet(&*tx, sheet.to_write()).await?;
        }
        tx.commit().await?;
        Ok(Some(Acted {
            runner_id: row.id,
            generation: row.reset_generation,
            sheet,
            outcome,
            taken,
        }))
    }

    /// A step onto the crossing: the refusals the sheet would give first,
    /// so a refused step locks no sleeper, then who waits there (`find_prey`,
    /// the row claimed), then the step itself.
    async fn cross(
        &self,
        tx: &tokio_postgres::Transaction<'_>,
        sheet: &mut Sheet,
        lane: u8,
        username: &str,
        today: NaiveDate,
        now: DateTime<Utc>,
    ) -> Result<Outcome> {
        if sheet.fight.is_some() || sheet.crossing_refused(lane).is_some() {
            return Ok(sheet.cross(lane, None, &mut rand::thread_rng()));
        }
        let prey = self.find_prey(tx, sheet, username, today, now).await?;
        let outcome = sheet.cross(lane, prey, &mut rand::thread_rng());
        match outcome.applied {
            Applied::Crossed { .. } | Applied::Refused(_) => Ok(outcome),
            ref other => anyhow::bail!("a crossing answered {other:?}"),
        }
    }

    /// Who the crossing meets, their row locked and the ambush row
    /// claimed: the runners in the band the day's caps have not counted,
    /// in the day's order for this attacker, the first whose locked row
    /// still stands (signal up, no fight of their own). `None` with
    /// nobody left.
    async fn find_prey(
        &self,
        tx: &tokio_postgres::Transaction<'_>,
        sheet: &Sheet,
        username: &str,
        today: NaiveDate,
        now: DateTime<Utc>,
    ) -> Result<Option<Prey>> {
        let candidates = self.candidates(tx.client(), sheet, today).await?;
        for (user_id, their_name) in candidates {
            let Some(row) = DeadchannelRunner::lock_standing(tx, user_id).await? else {
                continue;
            };
            let mut theirs = Sheet::from_row(&row).context("reading a sleeper's sheet")?;
            if settle_day(tx, &mut theirs, today, now).await? {
                DeadchannelRunner::store_sheet(tx, theirs.to_write()).await?;
            }
            if !can_be_crossed(&theirs, sheet.level) {
                continue;
            }
            let ambush_id = Uuid::now_v7();
            let claimed = Ambush::insert(
                tx,
                &NewAmbush {
                    id: ambush_id,
                    day: today,
                    attacker_user_id: sheet.user_id,
                    defender_user_id: user_id,
                    attacker_username: username.to_string(),
                    defender_username: their_name.clone(),
                    step: Some(sheet.step()),
                    stake: theirs.crossing_stake(),
                    answer_to: None,
                    started_at: now,
                },
            )
            .await?;
            if claimed.is_none() {
                tracing::debug!(defender = %user_id, "crossing cap refused a sleeper the preview offered");
                continue;
            }
            return Ok(Some(Prey::of(
                &theirs,
                &their_name,
                Self::portrait_of(&row),
                ambush_id,
                false,
            )));
        }
        Ok(None)
    }

    /// The runners a crossing could meet tonight, in the day's order for
    /// this attacker: standing, within the band, not this runner, not
    /// crossed today, not paired with this runner today, and, as their
    /// rows stand unlocked, on the wire (signal up after the roll, no
    /// fight). The lock decides again.
    async fn candidates(
        &self,
        client: &Client,
        sheet: &Sheet,
        today: NaiveDate,
    ) -> Result<Vec<(Uuid, String)>> {
        let rows = DeadchannelRunner::list_standing_in_band(
            client,
            sheet.level - CROSSING_LEVEL_BAND,
            sheet.level + CROSSING_LEVEL_BAND,
        )
        .await?;
        let crossings = Ambush::crossings_of_day(client, today).await?;
        let mut ids = Vec::new();
        for row in &rows {
            if row.user_id == sheet.user_id {
                continue;
            }
            // The day's caps, as the indexes on the ambush table hold
            // them: crossed once a day, one crossing per pair a day.
            let counted = crossings.iter().any(|crossing| {
                crossing.defender_user_id == row.user_id
                    || (crossing.attacker_user_id == sheet.user_id
                        && crossing.defender_user_id == row.user_id)
                    || (crossing.defender_user_id == sheet.user_id
                        && crossing.attacker_user_id == row.user_id)
            });
            if counted {
                continue;
            }
            if can_be_crossed(&settled(row, today)?, sheet.level) {
                ids.push(row.user_id);
            }
        }
        crossing_order(today, sheet.user_id, &mut ids);
        let usernames = User::list_usernames_by_ids(client, &ids).await?;
        Ok(ids
            .into_iter()
            .filter_map(|id| usernames.get(&id).map(|name| (id, name.clone())))
            .collect())
    }

    /// The three rows of a runner's look, for the scene's other face. An
    /// unreadable look costs the fight its picture, not the fight.
    fn portrait_of(row: &DeadchannelRunner) -> [String; 3] {
        match Look::parse(&row.look) {
            Ok(look) => look.rows().map(|worn| worn.piece.row.to_string()),
            Err(error) => {
                tracing::error!(error = %error, user_id = %row.user_id, "runner look failed to parse for the crossing");
                [
                    "     ".to_string(),
                    "  @  ".to_string(),
                    "     ".to_string(),
                ]
            }
        }
    }

    /// The answer: the crossing this runner is owed, the one who did it
    /// locked and claimed as the fight. The caps on the answer row are
    /// one per crossing; the answerer's own refusals come first.
    async fn answer(
        &self,
        tx: &tokio_postgres::Transaction<'_>,
        sheet: &mut Sheet,
        username: &str,
        today: NaiveDate,
        now: DateTime<Utc>,
    ) -> Result<Outcome> {
        if !on_the_wire(sheet) {
            return Ok(sheet.answer(Answer::OffTheWire, &mut rand::thread_rng()));
        }
        let since = now - chrono::Duration::hours(ANSWER_HOURS);
        let Some(owed) = Ambush::answerable(tx.client(), sheet.user_id, since).await? else {
            return Ok(sheet.answer(Answer::Nobody, &mut rand::thread_rng()));
        };
        let Some(row) = DeadchannelRunner::lock_standing(tx, owed.attacker_user_id).await? else {
            return Ok(sheet.answer(Answer::OffTheWire, &mut rand::thread_rng()));
        };
        let mut theirs = Sheet::from_row(&row).context("reading the crosser's sheet")?;
        if settle_day(tx, &mut theirs, today, now).await? {
            DeadchannelRunner::store_sheet(tx, theirs.to_write()).await?;
        }
        if !on_the_wire(&theirs) {
            return Ok(sheet.answer(Answer::OffTheWire, &mut rand::thread_rng()));
        }
        let ambush_id = Uuid::now_v7();
        let claimed = Ambush::insert(
            tx,
            &NewAmbush {
                id: ambush_id,
                day: today,
                attacker_user_id: sheet.user_id,
                defender_user_id: owed.attacker_user_id,
                attacker_username: username.to_string(),
                defender_username: owed.attacker_username.clone(),
                step: None,
                stake: theirs.crossing_stake(),
                answer_to: Some(owed.id),
                started_at: now,
            },
        )
        .await?;
        if claimed.is_none() {
            anyhow::bail!("the crossing {} was answered under us", owed.id);
        }
        let prey = Prey::of(
            &theirs,
            &owed.attacker_username,
            Self::portrait_of(&row),
            ambush_id,
            true,
        );
        Ok(sheet.answer(Answer::Prey(prey), &mut rand::thread_rng()))
    }

    /// A rival put down: up to `stake` off their hand onto this sheet's
    /// (the machine's garnish first), and the ambush row closed. A rival
    /// who went dark mid-fight has nothing on the row to take.
    async fn settle_won(
        &self,
        client: &impl GenericClient,
        sheet: &mut Sheet,
        rival: &Rival,
        stake: i64,
        today: NaiveDate,
        now: DateTime<Utc>,
    ) -> Result<i64> {
        let taken = match DeadchannelRunner::lock_standing(client, rival.user_id).await? {
            Some(row) => {
                let mut theirs = Sheet::from_row(&row).context("reading the rival's sheet")?;
                settle_day(client, &mut theirs, today, now).await?;
                let taken = theirs.robbed(stake);
                DeadchannelRunner::store_sheet(client, theirs.to_write()).await?;
                taken
            }
            None => {
                tracing::warn!(rival = %rival.user_id, "rival went dark mid-fight; nothing to take");
                0
            }
        };
        sheet.earn(taken);
        Ambush::end(client, rival.ambush_id, AmbushOutcome::Won, taken, now).await?;
        Ok(taken)
    }

    /// The signal dropped to a rival: `bits_lost` onto their hand (their
    /// garnish first), and the ambush row closed. A rival who went dark
    /// is paid nothing; the bits are the street's.
    async fn settle_lost(
        &self,
        client: &impl GenericClient,
        rival: &Rival,
        bits_lost: i64,
        today: NaiveDate,
        now: DateTime<Utc>,
    ) -> Result<()> {
        match DeadchannelRunner::lock_standing(client, rival.user_id).await? {
            Some(row) => {
                let mut theirs = Sheet::from_row(&row).context("reading the rival's sheet")?;
                settle_day(client, &mut theirs, today, now).await?;
                theirs.earn(bits_lost);
                DeadchannelRunner::store_sheet(client, theirs.to_write()).await?;
            }
            None => {
                tracing::warn!(rival = %rival.user_id, bits_lost, "rival went dark mid-fight; the street keeps the bits");
            }
        }
        Ambush::end(
            client,
            rival.ambush_id,
            AmbushOutcome::Lost,
            -bits_lost,
            now,
        )
        .await?;
        Ok(())
    }

    /// The lookout beside an answer or a reload, read after the commit
    /// off the rows as they stand. A read that fails costs the session
    /// the lookout, never the answer: logged and counted here, the empty
    /// lookout handed back, and the next answer or reload reads again.
    async fn lookout_for(&self, sheet: &Sheet) -> Lookout {
        let read = async {
            let client = self.db.get().await?;
            self.lookout(&client, sheet, Self::today(), Utc::now())
                .await
        }
        .await;
        match read {
            Ok(lookout) => lookout,
            Err(error) => {
                metrics::record_deadchannel_fight(FightBeat::LookoutFailed);
                tracing::error!(error = ?error, user_id = %sheet.user_id, "failed to read the lookout");
                Lookout::default()
            }
        }
    }

    /// What the session reads beside the sheet: who the crossing ahead
    /// would meet (the first of the day's order whose row stands, the
    /// ambush id nil until the step claims it), the answer owed and
    /// whether the one who did it is on the wire, and the latest fight on
    /// this runner inside the answer window. Nothing while a fight is on
    /// the row: none of it can be acted on then.
    async fn lookout(
        &self,
        client: &Client,
        sheet: &Sheet,
        today: NaiveDate,
        now: DateTime<Utc>,
    ) -> Result<Lookout> {
        if sheet.fight.is_some() {
            return Ok(Lookout::default());
        }
        let crossing_ahead = sheet.shut().is_none()
            && sheet.draft().is_none()
            && sheet
                .road
                .open_lanes()
                .into_iter()
                .any(|lane| sheet.node_ahead(lane) == Some(Node::Crossing));
        let crossing = match crossing_ahead {
            true => self.preview_prey(client, sheet, today).await?,
            false => None,
        };
        let since = now - chrono::Duration::hours(ANSWER_HOURS);
        let token = match Ambush::answerable(client, sheet.user_id, since).await? {
            Some(ambush) => {
                let prey =
                    match DeadchannelRunner::find_by_user_standing(client, ambush.attacker_user_id)
                        .await?
                    {
                        Some(row) => {
                            let theirs = settled(&row, today)?;
                            match on_the_wire(&theirs) {
                                true => Some(Prey::of(
                                    &theirs,
                                    &ambush.attacker_username,
                                    Self::portrait_of(&row),
                                    Uuid::nil(),
                                    true,
                                )),
                                false => None,
                            }
                        }
                        None => None,
                    };
                Some(Token { ambush, prey })
            }
            None => None,
        };
        let news = Ambush::latest_on(client, sheet.user_id, since).await?;
        Ok(Lookout {
            crossing,
            token,
            news,
        })
    }

    /// The first runner the crossing would meet, as their row stands.
    async fn preview_prey(
        &self,
        client: &Client,
        sheet: &Sheet,
        today: NaiveDate,
    ) -> Result<Option<Prey>> {
        for (user_id, their_name) in self.candidates(client, sheet, today).await? {
            let Some(row) = DeadchannelRunner::find_by_user_standing(client, user_id).await? else {
                continue;
            };
            let theirs = settled(&row, today)?;
            if !can_be_crossed(&theirs, sheet.level) {
                continue;
            }
            return Ok(Some(Prey::of(
                &theirs,
                &their_name,
                Self::portrait_of(&row),
                Uuid::nil(),
                false,
            )));
        }
        Ok(None)
    }

    /// The mark's chips, after the kill's commit, on the door milestones'
    /// two gates (`DEADCHANNEL_OLD_SIGNAL_REWARD_KEY`): once per mark, keyed
    /// `<runner row id>:<generation>:<mark>` (the row's `reset_generation`,
    /// so a mark earned again after a nuke claims under a key of its own),
    /// and at most once every 30 days per account.
    /// Returns the line the scene prints under the answer.
    ///
    /// The row owes the mark (`unpaid_mark`, migration 210) until the grant
    /// answers: paid or refused by the month's gate, the debt is settled;
    /// an error leaves it standing, logged and counted here, and the next
    /// command or reload on the row calls this again with the same event
    /// key, which the unique gate makes safe to repeat. A settle that fails
    /// after a paid grant is the one double call the gate absorbs: the
    /// retry is refused and settles then.
    async fn pay_mark(&self, user_id: Uuid, runner_id: Uuid, generation: i32, mark: i32) -> String {
        let event_key = format!("{runner_id}:{generation}:{mark}");
        let grant = self
            .chips
            .credit_run_cooldown_reward_template(
                user_id,
                DEADCHANNEL_OLD_SIGNAL_REWARD_KEY,
                &event_key,
                ChipMove::OldSignalSlain,
            )
            .await;
        let (payout, line) = match grant {
            Ok(grant) if grant.credited => {
                tracing::info!(mark, amount = grant.amount, "old signal paid");
                (
                    OldSignalPayout::Paid,
                    format!(
                        "the house pays {} chips for the broadcast.",
                        thousands(grant.amount)
                    ),
                )
            }
            Ok(_) => {
                tracing::info!(mark, "old signal inside the month, not paid");
                (
                    OldSignalPayout::InsideMonth,
                    data::OLD_SIGNAL_PAID_THIS_MONTH_LINE.to_string(),
                )
            }
            Err(error) => {
                tracing::error!(error = ?error, mark, "failed to pay the old signal");
                (
                    OldSignalPayout::Failed,
                    data::OLD_SIGNAL_TILL_JAMMED_LINE.to_string(),
                )
            }
        };
        metrics::record_deadchannel_old_signal_payout(payout);
        match payout {
            OldSignalPayout::Paid | OldSignalPayout::InsideMonth => {
                self.settle_mark(runner_id, mark).await;
            }
            OldSignalPayout::Failed => {}
        }
        line
    }

    /// Clear the debt for `mark` on the row. A failure here is logged and
    /// left: the next touch retries the grant, and the gate refuses it.
    async fn settle_mark(&self, runner_id: Uuid, mark: i32) {
        let settled = match self.db.get().await {
            Ok(client) => DeadchannelRunner::settle_mark(&client, runner_id, mark).await,
            Err(error) => Err(error),
        };
        match settled {
            Ok(true) => {}
            Ok(false) => {
                tracing::warn!(mark, "old signal mark was already settled or superseded");
            }
            Err(error) => {
                tracing::error!(error = ?error, mark, "failed to settle the old signal mark");
            }
        }
    }

    /// The news the wire carries, worded. Fire-and-forget through chat,
    /// which logs its own failure; a level gained carries the face, the
    /// one moment worth a picture. Every runner named is an `@` mention,
    /// so the line lands in their mentions too. A crossing names who lost
    /// what to whom; `taken` is what a rival put down lost to this runner.
    async fn post_news(&self, username: &str, sheet: &Sheet, applied: &Applied, taken: i64) {
        let me = format!("@{username}");
        for news in sheet.news(applied) {
            let body = match news {
                News::Dropped { bits_lost, step } => {
                    let took = match bits_lost {
                        0 => "the street found nothing on them.".to_string(),
                        n => format!("the street took {n} bits."),
                    };
                    format!("{me}'s signal dropped at step {step} of the road. {took}")
                }
                News::Ambushed {
                    rival,
                    step: Some(step),
                } => {
                    let them = format!("@{}", rival.username);
                    let lost = match taken {
                        0 => "lost nothing".to_string(),
                        n => format!("lost {n} bits to {me}"),
                    };
                    format!(
                        "{me} crossed {them} at step {step} and won. {them} never woke and {lost}."
                    )
                }
                News::Ambushed { rival, step: None } => {
                    let them = format!("@{}", rival.username);
                    let lost = match taken {
                        0 => "lost nothing".to_string(),
                        n => format!("lost {n} bits to {me}"),
                    };
                    format!("{me} answered {them} and won. {them} {lost}.")
                }
                News::Downed {
                    rival,
                    step,
                    bits_lost,
                } => {
                    let them = format!("@{}", rival.username);
                    let lost = match bits_lost {
                        0 => format!("lost their signal, with no bits on hand for {them}"),
                        n => format!("lost their signal and {n} bits to {them}"),
                    };
                    match step {
                        Some(step) => format!(
                            "{me} crossed {them} at step {step} and went down. {them} never woke. {me} {lost}."
                        ),
                        None => format!("{me} answered {them} and went down again. {me} {lost}."),
                    }
                }
                News::Leveled { level } => {
                    self.with_face(sheet.user_id, format!("{me} is level {level}."))
                        .await
                }
                News::Slain { marks } => {
                    let title = data::title(marks).expect("a slain runner has a mark");
                    self.with_face(
                        sheet.user_id,
                        format!(
                            "{me} put down the Old Signal. mark {marks}, {title}. back to level 1."
                        ),
                    )
                    .await
                }
                News::SteppedOff => {
                    format!("{me} stepped off the ledge. level 1, bare hands, starting over.")
                }
                News::FirstBlood { foe } => {
                    format!("{me} put down their first {foe}. the static will remember.")
                }
                News::BrightDown { foe } => {
                    format!("{me} put down a bright {foe}. it left a crystal.")
                }
                News::NearMiss { foe, signal } => {
                    format!("{me} put down the {foe} with {signal} signal left.")
                }
                News::RoadWalked {
                    kills,
                    runs,
                    signal,
                    max_signal,
                } => {
                    let glyphs = match kills {
                        1 => "1 glyph down".to_string(),
                        n => format!("{n} glyphs down"),
                    };
                    let runs = match runs {
                        0 => String::new(),
                        1 => ", 1 run".to_string(),
                        n => format!(", {n} runs"),
                    };
                    format!("{me} walked the road. {glyphs}{runs}, signal {signal}/{max_signal}.")
                }
            };
            self.chat.post_wire_line_task(body);
        }
    }

    /// The first Old Signal kill's badge. `NOT EXISTS` idempotent, so it is
    /// asked on every kill and a grant that failed heals on the next one.
    /// Fire-and-forget after the commit; logs its own failure.
    async fn grant_old_signal_badge(&self, user_id: Uuid, marks: i32) {
        let granted = async {
            let client = self.db.get().await?;
            grant_unique_milestone_award(
                &client,
                user_id,
                DEADCHANNEL_OLD_SIGNAL_AWARD_CATEGORY,
                i64::from(marks),
            )
            .await
        }
        .await;
        match granted {
            Ok(true) => tracing::info!(marks, "old signal badge granted"),
            Ok(false) => {}
            Err(error) => {
                tracing::error!(error = ?error, marks, "failed to grant the old signal badge");
            }
        }
    }

    /// `body` with the runner's portrait under it, three rows of plain
    /// text: the level line and the Old Signal line, the two moments worth
    /// a picture.
    async fn with_face(&self, user_id: Uuid, mut body: String) -> String {
        if let Some(look) = self.look_of(user_id).await {
            for worn in look.rows() {
                body.push('\n');
                body.push_str(worn.piece.row);
            }
        }
        body
    }

    /// The runner's look for the face lines. A missing or unreadable look
    /// costs the line its picture, not the line.
    async fn look_of(&self, user_id: Uuid) -> Option<Look> {
        let row: Result<Option<DeadchannelRunner>> = async {
            let client = self.db.get().await?;
            DeadchannelRunner::find_by_user(&client, user_id).await
        }
        .await;
        match row {
            Ok(Some(row)) => match Look::parse(&row.look) {
                Ok(look) => Some(look),
                Err(error) => {
                    tracing::error!(error = %error, user_id = %user_id, "runner look failed to parse for the level line");
                    None
                }
            },
            Ok(None) => None,
            Err(error) => {
                tracing::error!(error = ?error, user_id = %user_id, "failed to read the runner look for the level line");
                None
            }
        }
    }

    /// Re-read the sheet for a session's mirror: at connect for a standing
    /// runner, on every directory edge, and on the descent. Every action
    /// answers with the locked row's sheet, so between those the mirror
    /// catches up with a change made elsewhere on the next press. A mark
    /// the row still owes is paid here too, so a grant lost to a restart
    /// heals at the next connect; its line goes to the log only.
    pub(crate) fn reload_task(&self, user_id: Uuid, reply: mpsc::UnboundedSender<FightOutcome>) {
        let svc = self.clone();
        let span = info_span!("deadchannel.fight.reload_task", user_id = %user_id);
        tokio::spawn(
            async move {
                match svc.reload(user_id).await {
                    Ok(Some(Reloaded {
                        runner_id,
                        generation,
                        sheet,
                    })) => {
                        if let Some(mark) = sheet.unpaid_mark {
                            let line = svc.pay_mark(user_id, runner_id, generation, mark).await;
                            tracing::info!(mark, line = %line, "old signal mark settled on reload");
                        }
                        let lookout = svc.lookout_for(&sheet).await;
                        let _ = reply.send(FightOutcome::Reloaded { sheet, lookout });
                    }
                    Ok(None) => {
                        let _ = reply.send(FightOutcome::NoRunner);
                    }
                    Err(error) => {
                        tracing::error!(error = ?error, "failed to reload the runner's sheet");
                    }
                }
            }
            .instrument(span),
        );
    }

    /// The sheet as it stands after the day roll, for the lounge
    /// (`deadchannel/lounge`): whether a runner can hit is read off the
    /// settled row, never off a mirror. `None` for no standing runner.
    pub(crate) async fn settled_sheet(&self, user_id: Uuid) -> Result<Option<Sheet>> {
        Ok(self.reload(user_id).await?.map(|reloaded| reloaded.sheet))
    }

    /// A lounge glyph's leavings onto a runner's sheet, through the same
    /// lock as every other command (`Command::Pocket`). The lounge posts
    /// the wire line for the kill itself; this logs and counts the pay.
    /// `None` for no standing runner: the hitter left between the hit and
    /// the kill, and the leavings are theirs no longer.
    pub(crate) async fn pocket(&self, user_id: Uuid, leavings: Leavings) -> Result<Option<Sheet>> {
        let acted = self
            .act(user_id, Ask::Command(Command::Pocket(leavings)))
            .instrument(
                info_span!("deadchannel.fight.pocket", user_id = %user_id, leavings = ?leavings),
            )
            .await?;
        Ok(acted.map(|acted| {
            metrics::record_deadchannel_fight(beat_for(&acted.outcome.applied));
            tracing::info!(user_id = %user_id, applied = ?acted.outcome.applied, level = acted.sheet.level, "lounge leavings pocketed");
            acted.sheet
        }))
    }

    /// A read, but through the same lock-and-settle path as an action, so
    /// the mirror never shows yesterday's bars: the day roll happens on
    /// the first touch, and the descent is a touch. A rival fight the roll
    /// drops is closed on its row here too.
    async fn reload(&self, user_id: Uuid) -> Result<Option<Reloaded>> {
        let today = Self::today();
        let now = Utc::now();
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let Some(row) = DeadchannelRunner::lock_standing(&*tx, user_id).await? else {
            return Ok(None);
        };
        let mut sheet = Sheet::from_row(&row).context("reading the runner's sheet")?;
        if settle_day(&*tx, &mut sheet, today, now).await? {
            DeadchannelRunner::store_sheet(&*tx, sheet.to_write()).await?;
        }
        tx.commit().await?;
        Ok(Some(Reloaded {
            runner_id: row.id,
            generation: row.reset_generation,
            sheet,
        }))
    }

    pub fn today() -> NaiveDate {
        chrono::Utc::now().date_naive()
    }
}

/// The lazy day roll on a locked row, and the one thing it owes another
/// table: a rival fight the roll drops is closed on its ambush row as
/// dropped, since nothing moved. Every settle of a locked sheet goes
/// through here, the runner's own and a sleeper's or a rival's alike,
/// so no row is stored with its fight wiped and the ambush left open.
/// Returns whether the day rolled; the caller stores the sheet.
async fn settle_day(
    client: &impl GenericClient,
    sheet: &mut Sheet,
    today: NaiveDate,
    now: DateTime<Utc>,
) -> Result<bool> {
    let hanging = sheet
        .fight
        .as_ref()
        .and_then(|fight| fight.rival().cloned());
    let rolled = sheet.settle(today);
    if let (true, Some(rival)) = (rolled, hanging) {
        Ambush::end(client, rival.ambush_id, AmbushOutcome::Dropped, 0, now).await?;
        tracing::info!(ambush_id = %rival.ambush_id, user_id = %sheet.user_id, "crossing dropped by the day roll");
    }
    Ok(rolled)
}

/// Another runner's sheet as it would stand after the roll, read off an
/// unlocked row and stored nowhere: what the preview and the lookout
/// judge by. The lock decides again. An unreadable sheet is an error:
/// the row is the truth and it is wrong.
fn settled(row: &DeadchannelRunner, today: NaiveDate) -> Result<Sheet> {
    let mut theirs = Sheet::from_row(row).context("reading another runner's sheet")?;
    theirs.settle(today);
    Ok(theirs)
}

/// Whether a settled sheet is on the wire for the crossing: signal up
/// and no fight on the row.
fn on_the_wire(theirs: &Sheet) -> bool {
    theirs.fight.is_none() && !theirs.is_down()
}

/// Whether a settled sheet is a sleeper a runner of `level` could cross:
/// on the wire and within the band.
fn can_be_crossed(theirs: &Sheet, level: i32) -> bool {
    on_the_wire(theirs) && (theirs.level - level).abs() <= CROSSING_LEVEL_BAND
}

fn beat_for(applied: &Applied) -> FightBeat {
    match applied {
        Applied::Refused(_) => FightBeat::Refused,
        Applied::Started { pick: Pick::Fair } => FightBeat::Started,
        Applied::Started { pick: Pick::Lower } => FightBeat::SteppedDown,
        Applied::Started { pick: Pick::Bright } => FightBeat::Bright,
        Applied::Resumed => FightBeat::Resumed,
        Applied::Crossed { rival } => match rival.answer {
            true => FightBeat::Answered,
            false => FightBeat::Crossed,
        },
        Applied::Ambushed { .. } => FightBeat::Ambushed,
        Applied::Downed { .. } => FightBeat::Downed,
        Applied::Played { .. } => FightBeat::Played,
        Applied::Round => FightBeat::Round,
        Applied::Drafted { .. } => FightBeat::Drafted,
        Applied::Cleared { .. } => FightBeat::Cleared,
        Applied::Cached { .. } => FightBeat::Cached,
        Applied::Won { .. } => FightBeat::Won,
        Applied::Slain { .. } => FightBeat::Slain,
        Applied::Lost { .. } => FightBeat::Lost,
        Applied::Escaped => FightBeat::Escaped,
        Applied::Outfitted { .. } => FightBeat::Outfitted,
        Applied::Patched { .. } => FightBeat::Patched,
        Applied::Deposited { .. } => FightBeat::Deposited,
        Applied::Withdrew { .. } => FightBeat::Withdrew,
        Applied::Borrowed { .. } => FightBeat::Borrowed,
        Applied::Repaid { .. } => FightBeat::Repaid,
        Applied::Reset => FightBeat::Reset,
        Applied::Drank { .. } => FightBeat::Drank,
        Applied::Carted { .. } => FightBeat::Carted,
        Applied::Inked { .. } => FightBeat::Inked,
        Applied::Pocketed { .. } => FightBeat::Pocketed,
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
