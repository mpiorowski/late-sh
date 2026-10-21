use late_core::models::deadchannel_spawn::{Spawn, SpawnHit, SpawnKind, SpawnOutcome};
use uuid::Uuid;

use super::slipped_line;
use crate::app::deadchannel::lounge::state::{hits_needed, lifetime_secs};

fn slipped(kind: SpawnKind, hitters: &[&str]) -> Spawn {
    Spawn {
        id: Uuid::now_v7(),
        kind,
        hits_needed: hits_needed(kind),
        hits: hitters
            .iter()
            .map(|name| SpawnHit {
                user_id: Uuid::now_v7(),
                username: name.to_string(),
                at_ms: 0,
            })
            .collect(),
        spawned_at_ms: 1_000,
        expires_at_ms: 1_000 + lifetime_secs(kind) * 1_000,
        ended_at_ms: Some(1_000 + lifetime_secs(kind) * 1_000),
        outcome: Some(SpawnOutcome::Slipped),
    }
}

#[test]
fn a_spawn_nobody_hit_slips_away_unsaid() {
    assert_eq!(slipped_line(&slipped(SpawnKind::Flicker, &[])), None);
    assert_eq!(slipped_line(&slipped(SpawnKind::Howler, &[])), None);
}

#[test]
fn a_howler_hit_short_of_three_names_who_hit_it() {
    assert_eq!(
        slipped_line(&slipped(SpawnKind::Howler, &["tom", "dax"])).as_deref(),
        Some("a howler slipped past the lounge. @tom and @dax hit it; it needed three.")
    );
}
