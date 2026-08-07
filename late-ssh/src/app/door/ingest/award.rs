//! Chip payout + profile badge grants for door-game milestones landed by the
//! log pipe. The shared door-award sink the plan calls for: the NetHack
//! screen-scrape sink (`door/nethack/award.rs`) migrates onto this roster
//! when its ingestion phase moves badges to the xlogfile.
//!
//! Same dedup story as NetHack's: once per account for life, enforced twice
//! over — the lifetime reward template claim and the `NOT EXISTS` profile
//! award insert — so replays (cursor resets, backfill, re-wins) are no-ops.
//! Backfilled historical wins DO grant (owner decision): it is the same
//! achievement, and the idempotence makes it safe.
//!
//! This sink only moves chips and badges. Feed events stay with the ingest
//! service, which gates them on insert freshness and recency instead.

use late_core::db::Db;
use late_core::models::chips::ChipMove;
use late_core::models::profile_award::{
    DCSS_ORB_AWARD_CATEGORY, DCSS_WIN_AWARD_CATEGORY, award_badge, grant_unique_milestone_award,
};
use late_core::models::reward::{DCSS_ORB_REWARD_KEY, DCSS_WIN_REWARD_KEY};
use uuid::Uuid;

use crate::app::games::chips::svc::ChipService;

/// The badge-paying door milestones, one variant per lifetime badge pair
/// member. Phase 2 adds the NetHack pair, Phase 3 Brogue's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorBadge {
    /// DCSS: picked up the Orb of Zot (10k, `DCO`).
    DcssOrb,
    /// DCSS: escaped with the Orb (20k, `DCW`).
    DcssWin,
}

impl DoorBadge {
    const fn reward_key(self) -> &'static str {
        match self {
            Self::DcssOrb => DCSS_ORB_REWARD_KEY,
            Self::DcssWin => DCSS_WIN_REWARD_KEY,
        }
    }

    const fn chip_move(self) -> ChipMove {
        match self {
            Self::DcssOrb => ChipMove::DcssOrbFound,
            Self::DcssWin => ChipMove::DcssOrbEscape,
        }
    }

    const fn award_category(self) -> &'static str {
        match self {
            Self::DcssOrb => DCSS_ORB_AWARD_CATEGORY,
            Self::DcssWin => DCSS_WIN_AWARD_CATEGORY,
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

    /// Grant the chips + badge for a milestone. A win implies the Orb pickup
    /// (crawl will not let you win without it), so it back-grants the Orb in
    /// case the milestone stream missed it; the dedup guards make the
    /// redundant grant a no-op.
    pub fn grant(&self, user_id: Uuid, badge: DoorBadge) {
        match badge {
            DoorBadge::DcssOrb => self.spawn_grant(user_id, DoorBadge::DcssOrb),
            DoorBadge::DcssWin => {
                self.spawn_grant(user_id, DoorBadge::DcssOrb);
                self.spawn_grant(user_id, DoorBadge::DcssWin);
            }
        }
    }

    fn spawn_grant(&self, user_id: Uuid, badge: DoorBadge) {
        let chip_svc = self.chip_svc.clone();
        let db = self.db.clone();
        tokio::spawn(async move {
            let grant = match chip_svc
                .credit_lifetime_reward_template(user_id, badge.reward_key(), badge.chip_move())
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
            // Already claimed on a prior line/session — nothing more to do.
            if !grant.credited {
                return;
            }

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
