//! The invite rules, pure: how a code is spelled on the SSH command line, who
//! may attach to whom, what turns an invitee into a regular, and the
//! per-session clock that turns keystrokes into active minutes.
//!
//! The bar a referral has to clear is deliberately never shown to players.
//! The Invites dialog says "an active regular" and nothing more, so the
//! numbers below live here and in `CONTEXT.md`, not in any UI copy.

use chrono::{DateTime, Duration, Utc};
use late_core::models::referral::{ActivityDay, InviteCodeOwner, ReferralStatus, normalize_code};
use uuid::Uuid;

/// What the inviter gets when their invitee becomes a regular.
pub const INVITER_REWARD_CHIPS: i64 = 50_000;
/// The invitee's welcome bonus, paid in the same transaction.
pub const INVITEE_BONUS_CHIPS: i64 = 10_000;
/// Paid invites per inviter per UTC month. A referral that qualifies past
/// the cap stays `qualified` and pays in a later month; it is never lost.
pub const MONTHLY_PAID_CAP: i64 = 3;

/// How long after creating an account its owner may still type an invite
/// code in Settings. An SSH-link attach happens on the first connect, well
/// inside it.
pub const ATTACH_WINDOW_DAYS: i64 = 7;
/// How long after an invitee's account is created their activity counts
/// toward the bar. Also how long every young account records active minutes,
/// so a code typed on day six still sees days one through five.
pub const JUDGING_WINDOW_DAYS: i64 = 60;

/// A day counts toward the bar only with at least this many active minutes.
pub const MIN_DAY_ACTIVE_MINUTES: i32 = 20;
/// Distinct counting days inside the judging window.
pub const MIN_ACTIVE_DAYS: usize = 14;
/// Active minutes across the whole window, counting days or not.
pub const MIN_TOTAL_ACTIVE_MINUTES: i64 = 20 * 60;

/// The SSH login name that carries an invite: `ssh invite-<code>@late.sh`.
/// A bare username cannot be the carrier, because plain `ssh late.sh` sends
/// the local `$USER` and would credit whoever's handle matches it.
pub const SSH_INVITE_PREFIX: &str = "invite-";

/// The code inside an SSH login name, or `None` for any other name.
pub fn ssh_invite_code(ssh_user: &str) -> Option<String> {
    let lowered = ssh_user.trim().to_ascii_lowercase();
    let code = lowered.strip_prefix(SSH_INVITE_PREFIX)?;
    normalize_code(code)
}

/// A code as typed into Settings: the bare code, or the login name, or the
/// whole command pasted from a friend's message.
pub fn typed_invite_code(raw: &str) -> Option<String> {
    let lowered = raw.trim().to_ascii_lowercase();
    let without_command = lowered.strip_prefix("ssh ").unwrap_or(&lowered).trim();
    let login = without_command
        .split_once('@')
        .map(|(login, _host)| login)
        .unwrap_or(without_command);
    let code = login.strip_prefix(SSH_INVITE_PREFIX).unwrap_or(login);
    normalize_code(code)
}

/// The command an inviter hands out.
pub fn ssh_invite_command(code: &str) -> String {
    format!("ssh {SSH_INVITE_PREFIX}{code}@late.sh")
}

/// Why an attach was refused. Every refusal is the invitee's own input or
/// timing; none of them is an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachRefusal {
    /// No account owns the code.
    UnknownCode,
    /// The code is the invitee's own.
    OwnCode,
    /// The invitee's account is past [`ATTACH_WINDOW_DAYS`].
    WindowClosed,
    /// The inviter's account is not older than the invitee's: nobody can
    /// invite someone who was here first, which also rules out two accounts
    /// inviting each other.
    InviterNewer,
    /// The invitee already has an inviter.
    AlreadyInvited,
}

impl AttachRefusal {
    /// What the Settings dialog says about it.
    pub fn message(self) -> &'static str {
        match self {
            Self::UnknownCode => "No account has that invite code.",
            Self::OwnCode => "That is your own invite code.",
            Self::WindowClosed => "Invite codes can only be added in your first week.",
            Self::InviterNewer => "That account joined after you did.",
            Self::AlreadyInvited => "You already have an inviter.",
        }
    }
}

/// The checks that need nothing but the two accounts. `AlreadyInvited` is
/// decided by the insert itself, so it is never returned here.
pub fn attach_check(
    invitee_id: Uuid,
    invitee_created: DateTime<Utc>,
    inviter: &InviteCodeOwner,
    now: DateTime<Utc>,
) -> Result<(), AttachRefusal> {
    if inviter.user_id == invitee_id {
        return Err(AttachRefusal::OwnCode);
    }
    if !can_still_attach(invitee_created, now) {
        return Err(AttachRefusal::WindowClosed);
    }
    if inviter.created >= invitee_created {
        return Err(AttachRefusal::InviterNewer);
    }
    Ok(())
}

/// Whether an account this old may still name an inviter.
pub fn can_still_attach(account_created: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now - account_created <= Duration::days(ATTACH_WINDOW_DAYS)
}

/// The end of an invitee's judging window.
pub fn judged_until(invitee_created: DateTime<Utc>) -> DateTime<Utc> {
    invitee_created + Duration::days(JUDGING_WINDOW_DAYS)
}

/// Whether the activity inside the judging window clears the bar.
pub fn qualifies(days: &[ActivityDay]) -> bool {
    let counting_days = days
        .iter()
        .filter(|day| day.active_minutes >= MIN_DAY_ACTIVE_MINUTES)
        .count();
    let total_minutes: i64 = days.iter().map(|day| i64::from(day.active_minutes)).sum();
    counting_days >= MIN_ACTIVE_DAYS && total_minutes >= MIN_TOTAL_ACTIVE_MINUTES
}

/// What a sweep decides for one pending referral.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Qualified,
    /// The window closed short of the bar.
    Expired,
    /// Short of the bar with time left.
    StillJudging,
}

pub fn verdict(days: &[ActivityDay], judged_until: DateTime<Utc>, now: DateTime<Utc>) -> Verdict {
    match (qualifies(days), now >= judged_until) {
        (true, _) => Verdict::Qualified,
        (false, true) => Verdict::Expired,
        (false, false) => Verdict::StillJudging,
    }
}

/// How an invitee's row reads in the inviter's list. A qualified row waiting
/// on the monthly cap reads like a pending one, so the list never hints at
/// the bar or the cap.
pub fn invitee_status_label(status: ReferralStatus) -> &'static str {
    match status {
        ReferralStatus::Pending | ReferralStatus::Qualified => "settling in",
        ReferralStatus::Paid => "rewarded",
        ReferralStatus::Expired => "didn't stick",
    }
}

/// One session's half of newcomer activity: notes input, and hands the
/// session at most one minute per wall-clock minute to report. Accounts past
/// their judging window carry an inert clock. Two sessions of one account
/// both report a minute they both typed in; the table's guard counts it once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewcomerClock {
    until: Option<DateTime<Utc>>,
    reported_minute: Option<i64>,
    pending: Option<DateTime<Utc>>,
}

impl NewcomerClock {
    pub fn new(account_created: DateTime<Utc>, now: DateTime<Utc>) -> Self {
        let until = judged_until(account_created);
        Self {
            until: (now < until).then_some(until),
            reported_minute: None,
            pending: None,
        }
    }

    /// A clock that never reports, for sessions with no account behind them.
    pub fn inert() -> Self {
        Self {
            until: None,
            reported_minute: None,
            pending: None,
        }
    }

    /// Input arrived at `now`.
    pub fn note_input(&mut self, now: DateTime<Utc>) {
        let Some(until) = self.until else {
            return;
        };
        if now >= until {
            self.until = None;
            self.pending = None;
            return;
        }
        if self.reported_minute != Some(minute_index(now)) {
            self.pending = Some(now);
        }
    }

    /// The minute to report, once.
    pub fn take_minute(&mut self) -> Option<DateTime<Utc>> {
        let minute = self.pending.take()?;
        self.reported_minute = Some(minute_index(minute));
        Some(minute)
    }
}

fn minute_index(at: DateTime<Utc>) -> i64 {
    at.timestamp().div_euclid(60)
}
