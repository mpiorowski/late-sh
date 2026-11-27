//! The payout timers: when each milestone behind a cooldown window can pay
//! this account again. The window is the same one the grant enforces
//! (`GamePayoutKey::Cooldown`): the milestone's newest `cooldown` claim plus
//! its template's `cooldown_seconds`. Nothing here is stored; the Games hub
//! reads it when it opens (`ChipService::cooldown_timers`) and the landings
//! print it against the clock.

use std::collections::HashMap;

use anyhow::Context;
use chrono::{DateTime, Utc};
use late_core::models::game_payout::LastCooldownClaim;
use late_core::models::reward::{
    BROGUE_ESCAPE_REWARD_KEY, BROGUE_MASTERY_REWARD_KEY, DARKROOM_BEACON_REWARD_KEY,
    DARKROOM_ESCAPE_REWARD_KEY, DCSS_ORB_REWARD_KEY, DCSS_WIN_REWARD_KEY,
    DEADCHANNEL_OLD_SIGNAL_REWARD_KEY, GREENDRAGON_DRAGON_REWARD_KEY,
    LATEANIA_ARCHDEMON_REWARD_KEY, LATEANIA_FRONTIER_KING_REWARD_KEY,
    LATEANIA_KAETHYR_ASCENDANT_REWARD_KEY, LATEANIA_SUNDERING_DEEP_REWARD_KEY,
    NETHACK_AMULET_REWARD_KEY, NETHACK_ASCENSION_REWARD_KEY, RewardTemplate,
};

/// The milestones a landing shows a timer for: every Games hub payout that
/// sits behind a cooldown window. Tron's five-minute window is not one; its
/// table says nothing about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CooldownReward {
    LateaniaArchdemon,
    LateaniaFrontierKing,
    LateaniaSunderingDeep,
    LateaniaKaethyrAscendant,
    NethackAmulet,
    NethackAscension,
    DcssOrb,
    DcssWin,
    BrogueEscape,
    BrogueMastery,
    DarkroomEscape,
    DarkroomBeacon,
    GreendragonDragon,
    OldSignal,
}

impl CooldownReward {
    pub const ALL: [Self; 14] = [
        Self::LateaniaArchdemon,
        Self::LateaniaFrontierKing,
        Self::LateaniaSunderingDeep,
        Self::LateaniaKaethyrAscendant,
        Self::NethackAmulet,
        Self::NethackAscension,
        Self::DcssOrb,
        Self::DcssWin,
        Self::BrogueEscape,
        Self::BrogueMastery,
        Self::DarkroomEscape,
        Self::DarkroomBeacon,
        Self::GreendragonDragon,
        Self::OldSignal,
    ];

    pub const fn reward_key(self) -> &'static str {
        match self {
            Self::LateaniaArchdemon => LATEANIA_ARCHDEMON_REWARD_KEY,
            Self::LateaniaFrontierKing => LATEANIA_FRONTIER_KING_REWARD_KEY,
            Self::LateaniaSunderingDeep => LATEANIA_SUNDERING_DEEP_REWARD_KEY,
            Self::LateaniaKaethyrAscendant => LATEANIA_KAETHYR_ASCENDANT_REWARD_KEY,
            Self::NethackAmulet => NETHACK_AMULET_REWARD_KEY,
            Self::NethackAscension => NETHACK_ASCENSION_REWARD_KEY,
            Self::DcssOrb => DCSS_ORB_REWARD_KEY,
            Self::DcssWin => DCSS_WIN_REWARD_KEY,
            Self::BrogueEscape => BROGUE_ESCAPE_REWARD_KEY,
            Self::BrogueMastery => BROGUE_MASTERY_REWARD_KEY,
            Self::DarkroomEscape => DARKROOM_ESCAPE_REWARD_KEY,
            Self::DarkroomBeacon => DARKROOM_BEACON_REWARD_KEY,
            Self::GreendragonDragon => GREENDRAGON_DRAGON_REWARD_KEY,
            Self::OldSignal => DEADCHANNEL_OLD_SIGNAL_REWARD_KEY,
        }
    }
}

/// When each [`CooldownReward`] can pay this account next. A milestone that
/// never paid has no entry, and is open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CooldownTimers {
    opens_at: HashMap<CooldownReward, DateTime<Utc>>,
}

impl CooldownTimers {
    /// Match each reward's template to the account's newest claim of the same
    /// game and payout kind. Every reward needs an active `cooldown`
    /// template: one missing is a seed bug, and the grant would fail on it
    /// too.
    pub fn from_rows(
        templates: &[RewardTemplate],
        claims: &[LastCooldownClaim],
    ) -> anyhow::Result<Self> {
        let mut opens_at = HashMap::new();
        for reward in CooldownReward::ALL {
            let key = reward.reward_key();
            let template = templates
                .iter()
                .find(|template| template.key == key)
                .with_context(|| format!("no active reward template {key:?}"))?;
            let window = chrono::Duration::from_std(template.cooldown()?)
                .with_context(|| format!("cooldown of {key:?} out of range"))?;
            let (game, payout_kind) = (template.game()?, template.payout_kind()?);
            let claim = claims
                .iter()
                .find(|claim| claim.game == game && claim.payout_kind == payout_kind);
            if let Some(claim) = claim {
                opens_at.insert(reward, claim.paid_at + window);
            }
        }
        Ok(Self { opens_at })
    }

    pub fn window(&self, reward: CooldownReward, now: DateTime<Utc>) -> PayoutWindow {
        match self.opens_at.get(&reward) {
            Some(opens_at) if *opens_at > now => PayoutWindow::Closed {
                remaining: *opens_at - now,
            },
            Some(_) | None => PayoutWindow::Open,
        }
    }
}

/// The timers read at one moment: what a frame draws from.
#[derive(Clone, Copy, Debug)]
pub struct CooldownView<'a> {
    pub timers: &'a CooldownTimers,
    pub now: DateTime<Utc>,
}

impl CooldownView<'_> {
    pub fn window(&self, reward: CooldownReward) -> PayoutWindow {
        self.timers.window(reward, self.now)
    }
}

/// Whether a milestone would pay this account right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PayoutWindow {
    Open,
    Closed { remaining: chrono::Duration },
}

impl PayoutWindow {
    /// What a landing prints after the reward: `pays now`, or how long until
    /// it does, to the minute and rounded up, so a closed window never reads
    /// as zero.
    pub fn label(self) -> String {
        let remaining = match self {
            Self::Open => return "pays now".to_string(),
            Self::Closed { remaining } => remaining,
        };
        let minutes = (remaining.num_seconds() + 59) / 60;
        let (days, hours, minutes) = (minutes / 1440, minutes % 1440 / 60, minutes % 60);
        let left = match (days, hours) {
            (0, 0) => format!("{minutes}m"),
            (0, _) => format!("{hours}h {minutes}m"),
            _ => format!("{days}d {hours}h"),
        };
        format!("pays again in {left}")
    }
}
