//! The stage-2 name hit on the wire (`app/deadchannel/haunt`): the beat of
//! the haunting the whole room witnesses.
//!
//! There is no table here on purpose. A name hit is a second and a half of
//! theater; the *mark* it spends is a conditional claim on the user row
//! (`User::claim_first_contact_name_hit`), which stays the only source of
//! truth for the caps. This channel carries the beat to whoever is looking
//! at that message right now, and nothing else: nothing to store, nothing
//! to sweep, and a replica that boots mid-hit misses it exactly like a
//! person who was not looking.
//!
//! The seed travels with the beat so every witness swaps the same
//! characters, in the same two waves, as the person being haunted.

use anyhow::{Context, Result};
use tokio_postgres::Client;
use uuid::Uuid;

/// Cross-process channel for one corrupted author label. The payload is
/// self-contained (`<message>:<room>:<user>:<seed>`), so a listener paints
/// without a lookup: there is no row to look up.
pub const DEADCHANNEL_NAME_HIT_CHANNEL: &str = "deadchannel_name_hit";

/// One name hit as it travels between replicas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NameHitSignal {
    /// The message whose author label corrupts.
    pub message_id: Uuid,
    /// The room it was sent in, so a witness can ignore a beat for a room
    /// they are not in.
    pub room_id: Uuid,
    /// Who is being haunted. Logs only: their own session declines the
    /// beat by recognising the message, not the user, so a second device
    /// of theirs witnesses it like anyone else.
    pub user_id: Uuid,
    /// The wave base for `glitched_name`: the same swaps everywhere.
    pub seed: u64,
}

impl NameHitSignal {
    pub fn to_payload(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.message_id, self.room_id, self.user_id, self.seed
        )
    }
}

pub async fn listen_for_name_hits(client: &Client) -> Result<()> {
    client
        .batch_execute(&format!("LISTEN {DEADCHANNEL_NAME_HIT_CHANNEL};"))
        .await?;
    Ok(())
}

/// Fire the beat at every replica, this one included: the publisher's
/// pooled connection is not the listener's, so the local sessions hear it
/// over the same wire everyone else does. One path, one place to look.
pub async fn notify_name_hit(client: &Client, signal: &NameHitSignal) -> Result<()> {
    let payload = signal.to_payload();
    client
        .execute(
            "SELECT pg_notify($1, $2)",
            &[&DEADCHANNEL_NAME_HIT_CHANNEL, &payload],
        )
        .await
        .context("notifying deadchannel name hit")?;
    Ok(())
}

/// The beat carried by a [`DEADCHANNEL_NAME_HIT_CHANNEL`] payload. `None`
/// for anything this module did not write.
pub fn parse_name_hit_payload(payload: &str) -> Option<NameHitSignal> {
    let mut parts = payload.split(':');
    let message_id = parts.next()?.parse().ok()?;
    let room_id = parts.next()?.parse().ok()?;
    let user_id = parts.next()?.parse().ok()?;
    let seed = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(NameHitSignal {
        message_id,
        room_id,
        user_id,
        seed,
    })
}

#[cfg(test)]
#[path = "deadchannel_name_hit_test.rs"]
mod deadchannel_name_hit_test;
