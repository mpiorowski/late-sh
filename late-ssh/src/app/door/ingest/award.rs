//! Chip payout + profile badge grants for door-game milestones landed by the
//! log pipe. The shared door-award sink for all three external roguelike
//! doors (devdocs/PLAN-ROGUELIKE-BOARDS.md): DCSS, NetHack, Brogue.
//!
//! Chips repeat, badges do not (SHOP.md Phase 6). A win is the same 20+ hours
//! the second time, so it pays the full amount again, gated two ways at once
//! by `credit_run_cooldown_reward_template`: once per ingested log line, and
//! at most once per the template's 7-day window per account. The line key is
//! what makes a replay safe (the ingest grants on every sighting, fresh or
//! replayed, precisely so a crash between insert and grant heals), and the
//! window is what stops a lucky week paying four times.
//! The profile badge stays once per account for life: the `NOT EXISTS` insert
//! runs on every sighting, credited or not, so a badge lost to a crash heals
//! on the next one.
//! Backfilled historical wins DO grant (owner decision): it is the same
//! achievement, and the two gates make it safe.
//!
//! One line, one milestone. A win line grants the win and nothing else: it
//! never back-grants the Orb or Amulet pickup "in case the milestone stream
//! missed it". A pickup that never landed is an ingest bug and shows up as a
//! missing badge; paying it off the win line would hide the bug and, once the
//! pickup's 7-day window had passed, pay the pickup a second time.
//!
//! This sink only moves chips and badges. Feed events stay with the ingest
//! service, which gates them on insert freshness and recency instead.

use late_core::db::Db;
use late_core::models::chips::ChipMove;
use late_core::models::profile_award::{
    BROGUE_ESCAPE_AWARD_CATEGORY, BROGUE_MASTERY_AWARD_CATEGORY, DCSS_ORB_AWARD_CATEGORY,
    DCSS_WIN_AWARD_CATEGORY, NETHACK_AMULET_AWARD_CATEGORY, NETHACK_ASCENSION_AWARD_CATEGORY,
    award_badge, grant_unique_milestone_award,
};
use late_core::models::reward::{
    BROGUE_ESCAPE_REWARD_KEY, BROGUE_MASTERY_REWARD_KEY, DCSS_ORB_REWARD_KEY, DCSS_WIN_REWARD_KEY,
    NETHACK_AMULET_REWARD_KEY, NETHACK_ASCENSION_REWARD_KEY,
};
use uuid::Uuid;

use crate::app::games::chips::svc::ChipService;

/// Which ingested log line a grant is paying for: the natural key of the
/// `door_runs` / `door_milestones` row it landed as, minus the game (the
/// payout is already scoped to one). This is the run identity the repeat
/// payout is keyed on, so re-reading a log from offset 0 pays nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoorLineKey {
    pub source_file: String,
    pub source_offset: i64,
}

impl DoorLineKey {
    /// The `game_payout_claims` event key. One line, one claim.
    fn event_key(&self) -> String {
        format!("{}:{}", self.source_file, self.source_offset)
    }
}

/// The badge-paying door milestones, one variant per lifetime badge pair
/// member.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorBadge {
    /// DCSS: picked up the Orb of Zot (20k, `DCO`).
    DcssOrb,
    /// DCSS: escaped with the Orb (50k, `DCW`).
    DcssWin,
    /// NetHack: acquired the Amulet of Yendor (20k, `NHA`).
    NethackAmulet,
    /// NetHack: ascended (50k, `NHY`).
    NethackAscension,
    /// Brogue: escaped the Dungeons of Doom (20k, `BRE`).
    BrogueEscape,
    /// Brogue: mastered the Dungeons of Doom, the super-victory (50k, `BRM`).
    BrogueMastery,
}

impl DoorBadge {
    const fn reward_key(self) -> &'static str {
        match self {
            Self::DcssOrb => DCSS_ORB_REWARD_KEY,
            Self::DcssWin => DCSS_WIN_REWARD_KEY,
            Self::NethackAmulet => NETHACK_AMULET_REWARD_KEY,
            Self::NethackAscension => NETHACK_ASCENSION_REWARD_KEY,
            Self::BrogueEscape => BROGUE_ESCAPE_REWARD_KEY,
            Self::BrogueMastery => BROGUE_MASTERY_REWARD_KEY,
        }
    }

    const fn chip_move(self) -> ChipMove {
        match self {
            Self::DcssOrb => ChipMove::DcssOrbFound,
            Self::DcssWin => ChipMove::DcssOrbEscape,
            Self::NethackAmulet => ChipMove::NethackAmuletAcquired,
            Self::NethackAscension => ChipMove::NethackAscension,
            Self::BrogueEscape => ChipMove::BrogueEscape,
            Self::BrogueMastery => ChipMove::BrogueMastery,
        }
    }

    const fn award_category(self) -> &'static str {
        match self {
            Self::DcssOrb => DCSS_ORB_AWARD_CATEGORY,
            Self::DcssWin => DCSS_WIN_AWARD_CATEGORY,
            Self::NethackAmulet => NETHACK_AMULET_AWARD_CATEGORY,
            Self::NethackAscension => NETHACK_ASCENSION_AWARD_CATEGORY,
            Self::BrogueEscape => BROGUE_ESCAPE_AWARD_CATEGORY,
            Self::BrogueMastery => BROGUE_MASTERY_AWARD_CATEGORY,
        }
    }
}

/// Fire-and-forget grant sink. Cheap to clone (handles all the way down).
#[derive(Clone)]
pub struct DoorAwards {
    chip_svc: ChipService,
    db: Db,
}

impl DoorAwards {
    pub fn new(chip_svc: ChipService, db: Db) -> Self {
        Self { chip_svc, db }
    }

    /// Grant the chips + badge for the one milestone `line` landed as. Every
    /// badge grants only itself (see the module doc): a DCSS or NetHack win
    /// does not pay the pickup it implies, and a Brogue Mastery does not pay
    /// the Escape it never passed through.
    pub fn grant(&self, user_id: Uuid, badge: DoorBadge, line: &DoorLineKey) {
        let chip_svc = self.chip_svc.clone();
        let db = self.db.clone();
        let event_key = line.event_key();
        tokio::spawn(async move {
            let grant = match chip_svc
                .credit_run_cooldown_reward_template(
                    user_id,
                    badge.reward_key(),
                    &event_key,
                    badge.chip_move(),
                )
                .await
            {
                Ok(grant) => grant,
                Err(error) => {
                    tracing::error!(
                        ?error,
                        user_id = %user_id,
                        badge = badge.reward_key(),
                        "failed to credit door milestone chips"
                    );
                    return;
                }
            };
            // The badge insert runs on every sighting, credited or not: it is
            // NOT EXISTS-idempotent on its own, and gating it on the chip
            // claim being fresh would lose the badge forever when the 7-day
            // window (or a replayed line) suppresses the chips.
            let code = award_badge(badge.award_category(), 1);
            match db.get().await {
                Ok(client) => {
                    if let Err(error) = grant_unique_milestone_award(
                        &client,
                        user_id,
                        badge.award_category(),
                        grant.amount,
                    )
                    .await
                    {
                        tracing::error!(
                            ?error,
                            user_id = %user_id,
                            badge = %code,
                            "failed to grant door profile award badge"
                        );
                    }
                }
                Err(error) => {
                    tracing::error!(
                        ?error,
                        user_id = %user_id,
                        badge = %code,
                        "no db client for door profile award badge"
                    );
                }
            }
        });
    }
}
