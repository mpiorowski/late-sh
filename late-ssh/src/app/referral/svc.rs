//! Invites: orchestration. Attaching an invitee (from the SSH login name or
//! from Settings), loading the Invites dialog, recording newcomer minutes,
//! and the sweeper that judges, expires, and pays referrals. Every log line,
//! metric, and #lounge line for the feature is here; the rules are in
//! `state.rs` and the SQL in `late_core::models::referral`.
//!
//! Replica-ready by construction: every status move is a guarded `UPDATE`,
//! so every replica can sweep; the payout takes the referral row lock and
//! then a per-inviter advisory lock, so the monthly cap is exact however
//! many replicas pay at once. The Invites dialog is read on open, so there
//! is no notify channel.

use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use late_core::{
    db::Db,
    models::{
        chips::{ChipMove, UserChips},
        referral::{
            InviteCode, InvitedUser, NewcomerActivity, Referral, ReferralSource,
        },
        user::User,
    },
};
use tokio::sync::broadcast;
use tracing::{Instrument, info_span};
use uuid::Uuid;

use crate::{app::activity::publisher::ActivityPublisher, metrics};

use super::state::{
    AttachRefusal, INVITEE_BONUS_CHIPS, INVITER_REWARD_CHIPS, MONTHLY_PAID_CAP, Verdict,
    attach_check, can_still_attach, judged_until, verdict,
};

/// Dialog answers are per-session and short-lived.
const REFERRAL_EVENT_CAP: usize = 32;

/// How often the sweeper judges and pays. A referral qualifies over weeks,
/// so the only thing this sets is how long after crossing the bar the
/// payout lands.
const REFERRAL_SWEEP_INTERVAL: Duration = Duration::from_secs(10 * 60);

/// What the Invites dialog shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InviteOverview {
    pub code: String,
    /// Newest first.
    pub invited: Vec<InvitedUser>,
    pub inviter: Option<String>,
    /// Whether this account may still type a code: no inviter yet, and
    /// inside its first week.
    pub can_attach: bool,
}

#[derive(Clone, Debug)]
pub enum ReferralEvent {
    Overview {
        user_id: Uuid,
        overview: InviteOverview,
    },
    Attached {
        user_id: Uuid,
        inviter: String,
    },
    Refused {
        user_id: Uuid,
        refusal: AttachRefusal,
    },
    Failed {
        user_id: Uuid,
        message: String,
    },
}

/// How an attach ended, for the metric.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferralAttachOutcome {
    Attached,
    Refused(AttachRefusal),
    Failed,
}

/// What a sweep did to one referral, for the metric. Rows that are still
/// being judged record nothing: that is nearly every row on every sweep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferralSettlement {
    Qualified,
    Expired,
    Paid,
    /// Qualified but the inviter hit this month's cap; retried next sweep.
    Deferred,
    Failed,
}

/// How one reported newcomer minute landed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewcomerMinuteResult {
    Counted,
    /// Another session of the account already counted it.
    AlreadyCounted,
    Failed,
}

/// One payout attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PayOutcome {
    Paid { inviter_id: Uuid },
    Deferred,
    /// Another replica paid it first.
    AlreadySettled,
}

/// What one sweep settled, for the sweeper to report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct JudgeTally {
    pub qualified: u64,
    pub expired: u64,
}

#[derive(Clone)]
pub struct ReferralService {
    db: Db,
    /// The #lounge feed publisher. `None` in tests; a payout still settles,
    /// it just tells nobody.
    activity: Option<ActivityPublisher>,
    evt_tx: broadcast::Sender<ReferralEvent>,
}

impl ReferralService {
    pub fn new(db: Db) -> Self {
        let (evt_tx, _) = broadcast::channel(REFERRAL_EVENT_CAP);
        Self {
            db,
            activity: None,
            evt_tx,
        }
    }

    pub fn with_activity(mut self, activity: ActivityPublisher) -> Self {
        self.activity = Some(activity);
        self
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<ReferralEvent> {
        self.evt_tx.subscribe()
    }

    /// Load the Invites dialog for `user_id`, minting their code if this is
    /// the first time they open it.
    pub fn load_overview_task(&self, user_id: Uuid) {
        let service = self.clone();
        tokio::spawn(
            async move {
                match service.overview(user_id).await {
                    Ok(overview) => {
                        let _ = service
                            .evt_tx
                            .send(ReferralEvent::Overview { user_id, overview });
                    }
                    Err(error) => {
                        tracing::error!(error = ?error, %user_id, "failed to load invites");
                        let _ = service.evt_tx.send(ReferralEvent::Failed {
                            user_id,
                            message: "Could not load your invites. Try again.".to_string(),
                        });
                    }
                }
            }
            .instrument(info_span!("referral.load_overview", %user_id)),
        );
    }

    pub(super) async fn overview(&self, user_id: Uuid) -> Result<InviteOverview> {
        let client = self.db.get().await?;
        let code = InviteCode::ensure(&**client, user_id).await?;
        let invited = Referral::list_for_inviter(&**client, user_id).await?;
        let inviter = Referral::inviter_username(&**client, user_id).await?;
        let user = User::get(&client, user_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("user {user_id} not found"))?;
        let can_attach = inviter.is_none() && can_still_attach(user.created, Utc::now());
        Ok(InviteOverview {
            code,
            invited,
            inviter,
            can_attach,
        })
    }

    /// Attach `invitee_id` to the owner of `code`. Called with the code from
    /// an `invite-<code>` SSH login on the account's first connect, and from
    /// the Invites dialog. The answer goes out as a [`ReferralEvent`]; an SSH
    /// attach has no dialog listening, so its answer is the log and metric.
    pub fn attach_task(&self, invitee_id: Uuid, code: String, source: ReferralSource) {
        let service = self.clone();
        tokio::spawn(
            async move {
                let event = match service.attach(invitee_id, &code, source).await {
                    Ok(Ok(inviter)) => {
                        metrics::record_referral_attach(source, ReferralAttachOutcome::Attached);
                        tracing::info!(%invitee_id, inviter = %inviter, "invitee attached");
                        ReferralEvent::Attached {
                            user_id: invitee_id,
                            inviter,
                        }
                    }
                    Ok(Err(refusal)) => {
                        metrics::record_referral_attach(
                            source,
                            ReferralAttachOutcome::Refused(refusal),
                        );
                        tracing::info!(%invitee_id, ?refusal, "invite attach refused");
                        ReferralEvent::Refused {
                            user_id: invitee_id,
                            refusal,
                        }
                    }
                    Err(error) => {
                        metrics::record_referral_attach(source, ReferralAttachOutcome::Failed);
                        tracing::error!(error = ?error, %invitee_id, "failed to attach invitee");
                        ReferralEvent::Failed {
                            user_id: invitee_id,
                            message: "Could not add the invite code. Try again.".to_string(),
                        }
                    }
                };
                let _ = service.evt_tx.send(event);
            }
            .instrument(info_span!("referral.attach", %invitee_id, source = source.as_str())),
        );
    }

    /// The inviter's username on success.
    pub(super) async fn attach(
        &self,
        invitee_id: Uuid,
        code: &str,
        source: ReferralSource,
    ) -> Result<Result<String, AttachRefusal>> {
        let client = self.db.get().await?;
        let Some(inviter) = InviteCode::find_owner(&**client, code).await? else {
            return Ok(Err(AttachRefusal::UnknownCode));
        };
        let invitee = User::get(&client, invitee_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("invitee {invitee_id} not found"))?;
        if let Err(refusal) = attach_check(invitee_id, invitee.created, &inviter, Utc::now()) {
            return Ok(Err(refusal));
        }
        let attached = Referral::attach(
            &**client,
            invitee_id,
            inviter.user_id,
            source,
            judged_until(invitee.created),
        )
        .await?;
        match attached {
            true => Ok(Ok(inviter.username)),
            false => Ok(Err(AttachRefusal::AlreadyInvited)),
        }
    }

    /// Count one active minute for a young account. Fire-and-forget from the
    /// session tick, so this task logs its own failure.
    pub fn record_minute_task(&self, user_id: Uuid, minute: DateTime<Utc>) {
        let db = self.db.clone();
        tokio::spawn(async move {
            let result = match db.get().await {
                Ok(client) => NewcomerActivity::record_minute(&**client, user_id, minute).await,
                Err(error) => Err(error.into()),
            };
            match result {
                Ok(true) => metrics::record_newcomer_minute(NewcomerMinuteResult::Counted),
                Ok(false) => metrics::record_newcomer_minute(NewcomerMinuteResult::AlreadyCounted),
                Err(error) => {
                    metrics::record_newcomer_minute(NewcomerMinuteResult::Failed);
                    tracing::warn!(error = ?error, %user_id, "failed to record newcomer minute");
                }
            }
        });
    }

    /// The feature's one background loop. Every replica runs it; the guarded
    /// status moves are what keep each referral settled once.
    pub fn start_sweeper_task(&self) -> tokio::task::JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            loop {
                service
                    .sweep()
                    .instrument(info_span!("referral.sweep"))
                    .await;
                tokio::time::sleep(REFERRAL_SWEEP_INTERVAL).await;
            }
        })
    }

    /// One sweep: judge every pending referral, then pay every qualified
    /// one the cap allows. The orchestration layer for both: every failure
    /// is logged and counted here, and nothing below it logs.
    async fn sweep(&self) {
        match self.judge_pending(Utc::now()).await {
            Ok(tally) => {
                for _ in 0..tally.qualified {
                    metrics::record_referral_settlement(ReferralSettlement::Qualified);
                }
                for _ in 0..tally.expired {
                    metrics::record_referral_settlement(ReferralSettlement::Expired);
                }
            }
            Err(error) => {
                metrics::record_referral_settlement(ReferralSettlement::Failed);
                late_core::error_span!(
                    "referral_judge_failed",
                    error = ?error,
                    "failed to judge pending referrals"
                );
            }
        }

        let qualified = match self.db.get().await {
            Ok(client) => Referral::list_qualified(&**client).await,
            Err(error) => Err(error.into()),
        };
        let qualified = match qualified {
            Ok(qualified) => qualified,
            Err(error) => {
                metrics::record_referral_settlement(ReferralSettlement::Failed);
                late_core::error_span!(
                    "referral_pay_failed",
                    error = ?error,
                    "failed to list qualified referrals"
                );
                return;
            }
        };
        for invitee_id in qualified {
            match self.pay(invitee_id).await {
                Ok(PayOutcome::Paid { inviter_id }) => {
                    metrics::record_referral_settlement(ReferralSettlement::Paid);
                    tracing::info!(%inviter_id, %invitee_id, "referral paid");
                    if let Some(activity) = &self.activity {
                        activity.referral_rewarded_task(
                            inviter_id,
                            invitee_id,
                            INVITER_REWARD_CHIPS,
                            INVITEE_BONUS_CHIPS,
                        );
                    }
                }
                Ok(PayOutcome::Deferred) => {
                    metrics::record_referral_settlement(ReferralSettlement::Deferred);
                }
                Ok(PayOutcome::AlreadySettled) => {}
                Err(error) => {
                    metrics::record_referral_settlement(ReferralSettlement::Failed);
                    late_core::error_span!(
                        "referral_pay_failed",
                        error = ?error,
                        %invitee_id,
                        "failed to pay referral"
                    );
                }
            }
        }
    }

    /// Move every pending referral that has cleared the bar to `qualified`,
    /// and every one whose window closed short of it to `expired`.
    pub(super) async fn judge_pending(&self, now: DateTime<Utc>) -> Result<JudgeTally> {
        let client = self.db.get().await?;
        let mut tally = JudgeTally::default();
        for pending in Referral::list_pending(&**client).await? {
            let days = NewcomerActivity::days(
                &**client,
                pending.invitee_id,
                pending.invitee_created,
                pending.judged_until,
            )
            .await?;
            match verdict(&days, pending.judged_until, now) {
                Verdict::Qualified => {
                    if Referral::mark_qualified(&**client, pending.invitee_id).await? {
                        tally.qualified += 1;
                    }
                }
                Verdict::Expired => {
                    if Referral::expire(&**client, pending.invitee_id).await? {
                        tally.expired += 1;
                    }
                }
                Verdict::StillJudging => {}
            }
        }
        Ok(tally)
    }

    /// Pay one qualified referral if its inviter is under this month's cap.
    /// Both credits ride the transaction that marks the row paid.
    pub(super) async fn pay(&self, invitee_id: Uuid) -> Result<PayOutcome> {
        let mut client = self.db.get().await?;
        let tx = client.transaction().await?;
        let Some(inviter_id) = Referral::lock_qualified(&tx, invitee_id).await? else {
            return Ok(PayOutcome::AlreadySettled);
        };
        Referral::lock_inviter(&tx, inviter_id).await?;
        if Referral::paid_this_month(&tx, inviter_id).await? >= MONTHLY_PAID_CAP {
            return Ok(PayOutcome::Deferred);
        }
        Referral::mark_paid(&tx, invitee_id).await?;
        UserChips::apply(
            &*tx,
            inviter_id,
            ChipMove::ReferralReward,
            INVITER_REWARD_CHIPS,
            &invitee_id.to_string(),
        )
        .await?;
        UserChips::apply(
            &*tx,
            invitee_id,
            ChipMove::ReferralWelcome,
            INVITEE_BONUS_CHIPS,
            &inviter_id.to_string(),
        )
        .await?;
        tx.commit().await?;
        Ok(PayOutcome::Paid { inviter_id })
    }
}
