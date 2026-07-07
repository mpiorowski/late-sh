//! Chip payout + profile badge grants for NetHack milestones.
//!
//! This is the NetHack analogue of the Lateania boss-award path
//! (`door::lateania::svc`): a once-per-account lifetime chip payout plus a
//! rankless profile award badge, fired and forgotten from the screen-scrape in
//! `state.rs`. Account-level dedup is enforced twice over — by the lifetime
//! reward template (`credit_lifetime_reward_template`) and by the `NOT EXISTS`
//! award insert — so re-running across sessions or ticks is harmless.

use late_core::db::Db;
use late_core::models::profile_award::{
    NETHACK_AMULET_AWARD_CATEGORY, NETHACK_ASCENSION_AWARD_CATEGORY, award_badge,
    grant_unique_milestone_award,
};
use late_core::models::reward::{NETHACK_AMULET_REWARD_KEY, NETHACK_ASCENSION_REWARD_KEY};
use uuid::Uuid;

use super::milestone::Milestone;
use crate::app::activity::event::ActivityGame;
use crate::app::activity::publisher::ActivityPublisher;
use crate::app::games::chips::svc::ChipService;

const AMULET_LEDGER_REASON: &str = "nethack_amulet_acquired";
const ASCENSION_LEDGER_REASON: &str = "nethack_ascension";

/// Services needed to mint a NetHack milestone reward. Cheap to clone (each
/// field is itself a handle/clone), held on the per-session door `State`.
#[derive(Clone)]
pub struct NethackAwards {
    chip_svc: ChipService,
    db: Db,
    activity: ActivityPublisher,
}

impl NethackAwards {
    pub fn new(chip_svc: ChipService, db: Db, activity: ActivityPublisher) -> Self {
        Self {
            chip_svc,
            db,
            activity,
        }
    }

    /// Post a non-reward NetHack moment (start, descent, death) to the activity
    /// feed. Visible (category `Game`), no chips or badges.
    pub fn note_event(&self, user_id: Uuid, action: String) {
        self.activity
            .game_event_task(user_id, ActivityGame::Nethack, action);
    }

    /// Grant the chips + badge for a milestone. Ascension implies the Amulet, so
    /// it also back-grants the Amulet award in case the mid-game scrape missed it
    /// (the dedup guards make the redundant grant a no-op if already claimed).
    pub fn grant(&self, user_id: Uuid, milestone: Milestone) {
        match milestone {
            Milestone::Amulet => self.spawn_grant(user_id, Milestone::Amulet),
            Milestone::Ascension => {
                self.spawn_grant(user_id, Milestone::Amulet);
                self.spawn_grant(user_id, Milestone::Ascension);
            }
        }
    }

    fn spawn_grant(&self, user_id: Uuid, milestone: Milestone) {
        let (reward_key, ledger_reason, category, detail) = match milestone {
            Milestone::Amulet => (
                NETHACK_AMULET_REWARD_KEY,
                AMULET_LEDGER_REASON,
                NETHACK_AMULET_AWARD_CATEGORY,
                "acquired the Amulet of Yendor",
            ),
            Milestone::Ascension => (
                NETHACK_ASCENSION_REWARD_KEY,
                ASCENSION_LEDGER_REASON,
                NETHACK_ASCENSION_AWARD_CATEGORY,
                "ascended to Demigod",
            ),
        };
        let chip_svc = self.chip_svc.clone();
        let db = self.db.clone();
        let activity = self.activity.clone();
        tokio::spawn(async move {
            let grant = match chip_svc
                .credit_lifetime_reward_template(user_id, reward_key, ledger_reason)
                .await
            {
                Ok(grant) => grant,
                Err(error) => {
                    tracing::error!(
                        ?error,
                        user_id = %user_id,
                        milestone = reward_key,
                        "failed to credit nethack milestone chips"
                    );
                    return;
                }
            };
            // Already claimed on a prior session/tick — nothing more to do, and
            // no duplicate badge or activity event.
            if !grant.credited {
                return;
            }

            let badge = award_badge(category, 1);
            match db.get().await {
                Ok(client) => {
                    if let Err(error) =
                        grant_unique_milestone_award(&client, user_id, category, grant.amount).await
                    {
                        tracing::error!(
                            ?error,
                            user_id = %user_id,
                            badge = %badge,
                            "failed to grant nethack profile award badge"
                        );
                    }
                }
                Err(error) => {
                    tracing::error!(
                        ?error,
                        user_id = %user_id,
                        badge = %badge,
                        "no db client for nethack profile award badge"
                    );
                }
            }

            // Keep the feed line short: the chips/badge are on the profile, not
            // spelled out in the activity stream.
            activity.game_won_task(
                user_id,
                ActivityGame::Nethack,
                Some(detail.to_string()),
                None,
            );
        });
    }
}
