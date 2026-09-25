//! The one writer of the runner's sheet (root CONTEXT.md, "per-user
//! state"): every command is one transaction over the locked row: lock,
//! roll the day if it turned, apply one rule, store the whole sheet,
//! commit. "Already spent" and "already down" are read off the locked
//! row, never off a session's mirror, so two devices and any number of
//! replicas queue on the lock and act one after the other.
//!
//! After the commit, the wire (GAME.md, "The three surfaces"): the room
//! sees the news, never the play-by-play. `Sheet::news` decides what is
//! news (a dropped signal, a level gained, a first kill, a near miss, the
//! last ration of the day); this file words it and posts it to
//! #deadchannel as messages from the voice. An ordinary kill, a round, a
//! run, a purchase post nothing.
//!
//! Orchestration only: the span, the metric, the log line per failure
//! mode, and the reply live here; `state.rs` returns data.

use anyhow::{Context, Result};
use chrono::NaiveDate;
use late_core::db::Db;
use late_core::models::deadchannel_runner::DeadchannelRunner;
use tokio::sync::mpsc;
use tracing::{Instrument, info_span};
use uuid::Uuid;

use super::state::{Applied, Command, News, Outcome, Sheet};
use crate::app::chat::svc::ChatService;
use crate::app::deadchannel::runner::state::Look;
use crate::metrics::{self, FightBeat};

/// What a session's request came back with. Sent to the asking session
/// only.
#[derive(Debug, Clone)]
pub(crate) enum FightOutcome {
    Acted {
        sheet: Sheet,
        outcome: Outcome,
    },
    Reloaded {
        sheet: Sheet,
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
}

impl FightService {
    pub fn new(db: Db, chat: ChatService) -> Self {
        Self { db, chat }
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
        let svc = self.clone();
        let span = info_span!("deadchannel.fight.act_task", user_id = %user_id, username = %username, command = ?command);
        tokio::spawn(
            async move {
                let outcome = match svc.act(user_id, command).await {
                    Ok(Some((sheet, outcome))) => {
                        metrics::record_deadchannel_fight(beat_for(&outcome.applied));
                        tracing::info!(applied = ?outcome.applied, level = sheet.level, signal = sheet.signal, rations_left = sheet.rations_left, bits = sheet.bits, "fight command applied");
                        svc.post_news(&username, &sheet, &outcome.applied).await;
                        FightOutcome::Acted { sheet, outcome }
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

    /// Lock, settle, apply, store. Returns `None` when there is no standing
    /// runner.
    async fn act(&self, user_id: Uuid, command: Command) -> Result<Option<(Sheet, Outcome)>> {
        let today = Self::today();
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let Some(row) = DeadchannelRunner::lock_standing(&*tx, user_id).await? else {
            return Ok(None);
        };
        let mut sheet = Sheet::from_row(&row).context("reading the runner's sheet")?;
        let rolled = sheet.settle(today);
        let outcome = sheet.apply(command, &mut rand::thread_rng());
        let changed = rolled || !matches!(outcome.applied, Applied::Refused(_) | Applied::Resumed);
        if changed {
            DeadchannelRunner::store_sheet(&*tx, sheet.to_write()).await?;
        }
        tx.commit().await?;
        Ok(Some((sheet, outcome)))
    }

    /// The news the wire carries, worded. Fire-and-forget through chat,
    /// which logs its own failure; a level gained carries the face, the
    /// one moment worth a picture.
    async fn post_news(&self, username: &str, sheet: &Sheet, applied: &Applied) {
        for news in sheet.news(applied) {
            let body = match news {
                News::Dropped { bits_lost } => {
                    let took = match bits_lost {
                        0 => "the street found nothing on them.".to_string(),
                        n => format!("the street took {n} bits."),
                    };
                    format!("{username}'s signal dropped at the end of the row. {took}")
                }
                News::Leveled { level } => {
                    let mut body = format!("{username} is level {level}.");
                    if let Some(look) = self.look_of(sheet.user_id).await {
                        for worn in look.rows() {
                            body.push('\n');
                            body.push_str(worn.piece.row);
                        }
                    }
                    body
                }
                News::FirstBlood { foe } => {
                    format!("{username} put down their first {foe}. the static will remember.")
                }
                News::NearMiss { foe, signal } => {
                    format!("{username} put down the {foe} with {signal} signal left.")
                }
                News::LastRation {
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
                    format!(
                        "{username} spent the last ration. {glyphs}{runs}, signal {signal}/{max_signal}."
                    )
                }
            };
            self.chat.post_wire_line_task(body);
        }
    }

    /// The runner's look for the level line. A missing or unreadable look
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

    /// Re-read the sheet for a session's mirror on the descent. Every
    /// action answers with the locked row's sheet, so the mirror catches
    /// up with a change made elsewhere on the next press; only the
    /// descent needs a read of its own.
    pub(crate) fn reload_task(&self, user_id: Uuid, reply: mpsc::UnboundedSender<FightOutcome>) {
        let svc = self.clone();
        let span = info_span!("deadchannel.fight.reload_task", user_id = %user_id);
        tokio::spawn(
            async move {
                match svc.reload(user_id).await {
                    Ok(Some(sheet)) => {
                        let _ = reply.send(FightOutcome::Reloaded { sheet });
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

    /// A read, but through the same lock-and-settle path as an action, so
    /// the mirror never shows yesterday's bars: the day roll happens on
    /// the first touch, and the descent is a touch.
    async fn reload(&self, user_id: Uuid) -> Result<Option<Sheet>> {
        let today = Self::today();
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let Some(row) = DeadchannelRunner::lock_standing(&*tx, user_id).await? else {
            return Ok(None);
        };
        let mut sheet = Sheet::from_row(&row).context("reading the runner's sheet")?;
        if sheet.settle(today) {
            DeadchannelRunner::store_sheet(&*tx, sheet.to_write()).await?;
        }
        tx.commit().await?;
        Ok(Some(sheet))
    }

    pub fn today() -> NaiveDate {
        chrono::Utc::now().date_naive()
    }
}

fn beat_for(applied: &Applied) -> FightBeat {
    match applied {
        Applied::Refused(_) => FightBeat::Refused,
        Applied::Started => FightBeat::Started,
        Applied::Resumed => FightBeat::Resumed,
        Applied::Round => FightBeat::Round,
        Applied::Won { .. } => FightBeat::Won,
        Applied::Lost { .. } => FightBeat::Lost,
        Applied::Escaped => FightBeat::Escaped,
        Applied::Outfitted { .. } => FightBeat::Outfitted,
    }
}

#[cfg(test)]
#[path = "svc_test.rs"]
mod svc_test;
