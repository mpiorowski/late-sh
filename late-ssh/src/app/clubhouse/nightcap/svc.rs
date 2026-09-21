//! Nightcap orchestration: the one place the bar touches chips, the drunk
//! map, and telemetry. `state.rs` decides whether an order may be placed;
//! this module places it off-thread and reports the outcome back over the
//! session's channel, where `State::drain_outcomes` prints it.
//!
//! Every pour runs through the same rails the tavern's `@bartender` uses
//! (`ChipService::buy_drink`, `cash_round_drink`, `buy_round`, then
//! `SharedLobby::record_drink`), so a drink here is exactly as drunk as a
//! drink at the counter and a credit from a round bought in either room can
//! be cashed in either room. Only the ordering differs: a fixed menu, no
//! conversation.

use std::time::Duration;

use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use late_core::db::Db;
use late_core::models::artboard_piece::ArtboardPiece;
use late_core::models::article::Article;
use late_core::models::chips::UserChips;
use late_core::models::drink_round::ROUND_PRICE_PER_PATRON;
use late_core::models::nightcap_carving::Carving;
use late_core::shutdown::CancellationToken;

use crate::app::clubhouse::lobby::SharedLobby;
use crate::app::games::chips::svc::{ChipService, RoundError};
use crate::metrics;

use super::lobby::SharedSeats;
use super::state::{Order, Outcome};
use super::wall::{SharedWall, TAB_BOARD_SIZE, WallSnapshot};

/// How often the wall is re-read for the whole process. The TV, the tab
/// board and the carvings all move slowly; a carve shows at once anyway.
pub const WALL_REFRESH_INTERVAL: Duration = Duration::from_secs(300);

/// The house's process-global handle: the DB and the shared wall. Built in
/// `main.rs`, threaded into sessions like the seats, and the only thing at
/// this bar that can read or write a table.
#[derive(Clone)]
pub struct NightcapHouse {
    db: Db,
    wall: SharedWall,
}

impl NightcapHouse {
    pub fn new(db: Db, wall: SharedWall) -> Self {
        Self { db, wall }
    }

    pub fn wall(&self) -> SharedWall {
        self.wall.clone()
    }

    /// Re-read the wall now and then every [`WALL_REFRESH_INTERVAL`] until
    /// shutdown. One task per process.
    pub fn spawn_wall_refresh_task(
        self,
        shutdown: CancellationToken,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(WALL_REFRESH_INTERVAL);
            tracing::info!("nightcap wall refresher started");
            loop {
                tokio::select! {
                    _ = shutdown.cancelled() => {
                        tracing::info!("nightcap wall refresher shutting down");
                        break;
                    }
                    _ = interval.tick() => {
                        if let Err(error) = self.refresh_wall().await {
                            tracing::warn!(error = ?error, "failed to refresh the nightcap wall");
                        }
                    }
                }
            }
        })
    }

    async fn refresh_wall(&self) -> anyhow::Result<()> {
        let client = self.db.get().await?;
        let headline = Article::list_recent(&client, 1)
            .await?
            .into_iter()
            .next()
            .map(|article| article.title);
        let newest_piece = ArtboardPiece::newest_hung(&client).await?;
        let tab = UserChips::top_round_buyers(&client, TAB_BOARD_SIZE).await?;
        let mut carvings: [Option<Carving>; super::lobby::SEAT_COUNT] =
            std::array::from_fn(|_| None);
        for carving in Carving::list(&client).await? {
            if let Some(slot) = carvings.get_mut(carving.stool as usize) {
                *slot = Some(carving);
            }
        }
        self.wall.set(WallSnapshot {
            headline,
            newest_piece,
            tab,
            carvings,
        });
        Ok(())
    }

    /// Carve a seated patron's line into their stool. Fire-and-forget from
    /// the input path; the outcome arrives on `outcome_tx`, and the wall is
    /// updated in place so every session sees the line at once.
    pub fn spawn_carve(
        self,
        user_id: Uuid,
        stool: usize,
        body: String,
        outcome_tx: UnboundedSender<Outcome>,
    ) {
        tokio::spawn(async move {
            let outcome = match self.carve(user_id, stool, &body).await {
                Ok(carving) => {
                    tracing::info!(user_id = %user_id, stool, body = %carving.body, "nightcap stool carved");
                    self.wall.set_carving(carving);
                    Outcome::Carved { stool }
                }
                Err(error) => {
                    tracing::error!(error = ?error, user_id = %user_id, stool, "nightcap carve failed");
                    Outcome::CarveFailed
                }
            };
            let _ = outcome_tx.send(outcome);
        });
    }

    async fn carve(&self, user_id: Uuid, stool: usize, body: &str) -> anyhow::Result<Carving> {
        let client = self.db.get().await?;
        Carving::carve(&client, stool as i16, user_id, body).await
    }
}

/// How a single-drink order settled, for the counter's label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NightcapOrderResult {
    Poured,
    Comped,
    Bounced,
    Failed,
}

/// Place a seated patron's order. Fire-and-forget from the input path; the
/// outcome arrives on `outcome_tx`. `drunk_lobby` is the tavern's shared
/// presence map, which carries drunk state for both rooms.
pub fn spawn_order(
    chip_service: ChipService,
    drunk_lobby: Option<SharedLobby>,
    seats: SharedSeats,
    user_id: Uuid,
    order: Order,
    outcome_tx: UnboundedSender<Outcome>,
) {
    tokio::spawn(async move {
        let outcome = match order {
            Order::Drink(drink) => {
                order_drink(&chip_service, drunk_lobby.as_ref(), &seats, user_id, drink).await
            }
            Order::Round => order_round(&chip_service, drunk_lobby.as_ref(), &seats, user_id).await,
        };
        // A closed receiver means the session is gone; the chips have
        // already moved and were logged above, nothing to add.
        let _ = outcome_tx.send(outcome);
    });
}

async fn order_drink(
    chip_service: &ChipService,
    drunk_lobby: Option<&SharedLobby>,
    seats: &SharedSeats,
    user_id: Uuid,
    drink: super::state::Drink,
) -> Outcome {
    // A banked round credit pays first, as it does at the counter, but only
    // for the house measure it bought (`Drink::on_the_round`): a priced
    // pick is debited as ordered and the credit stays banked.
    let credit = if drink.on_the_round() {
        chip_service.cash_round_drink(user_id).await
    } else {
        Ok(None)
    };
    match credit {
        Ok(Some(comped)) => {
            if let Some(lobby) = drunk_lobby {
                lobby.record_drink(user_id, comped.drunk_points, comped.last_drink_at);
            }
            seats.record_pour(user_id);
            metrics::record_round_drink_cashed();
            metrics::record_nightcap_order(NightcapOrderResult::Comped);
            tracing::info!(
                user_id = %user_id,
                drink = drink.name(),
                round_id = %comped.round_id,
                remaining = comped.remaining,
                "nightcap poured against a round credit"
            );
            return Outcome::Comped {
                drink,
                remaining: comped.remaining,
            };
        }
        Ok(None) => {}
        Err(error) => {
            metrics::record_nightcap_order(NightcapOrderResult::Failed);
            tracing::error!(error = ?error, user_id = %user_id, "nightcap credit check failed");
            return Outcome::Failed;
        }
    }

    match chip_service
        .buy_drink(user_id, drink.price(), drink.name())
        .await
    {
        Ok(Some(purchase)) => {
            if let Some(lobby) = drunk_lobby {
                lobby.record_drink(user_id, purchase.drunk_points, purchase.last_drink_at);
            }
            seats.record_pour(user_id);
            metrics::record_nightcap_order(NightcapOrderResult::Poured);
            tracing::info!(
                user_id = %user_id,
                drink = drink.name(),
                price = drink.price(),
                new_balance = purchase.balance,
                "nightcap poured a drink"
            );
            Outcome::Poured {
                drink,
                balance: purchase.balance,
            }
        }
        // The floor guard refused the pour. Nothing charged, nothing poured.
        Ok(None) => {
            metrics::record_nightcap_order(NightcapOrderResult::Bounced);
            Outcome::Bounced { drink }
        }
        Err(error) => {
            metrics::record_nightcap_order(NightcapOrderResult::Failed);
            tracing::error!(error = ?error, user_id = %user_id, drink = drink.name(), "nightcap pour failed");
            Outcome::Failed
        }
    }
}

async fn order_round(
    chip_service: &ChipService,
    drunk_lobby: Option<&SharedLobby>,
    seats: &SharedSeats,
    buyer_id: Uuid,
) -> Outcome {
    // A round here is for the stools, not for everyone online: the buyer
    // can see exactly who they are buying for.
    let patrons = seats.seated_ids_excluding(buyer_id);
    match chip_service
        .buy_round(buyer_id, ROUND_PRICE_PER_PATRON, &patrons)
        .await
    {
        Ok(purchase) => {
            if let Some(lobby) = drunk_lobby {
                lobby.record_drink(buyer_id, purchase.drunk_points, purchase.last_drink_at);
            }
            seats.record_pour(buyer_id);
            metrics::record_round_bought(purchase.patrons, purchase.total_chips);
            tracing::info!(
                user_id = %buyer_id,
                round_id = %purchase.round_id,
                patrons = purchase.patrons,
                total_chips = purchase.total_chips,
                new_balance = purchase.balance,
                "nightcap patron bought the stools a round"
            );
            Outcome::RoundBought {
                patrons: purchase.patrons,
                total: purchase.total_chips,
                balance: purchase.balance,
            }
        }
        Err(RoundError::Refused(refusal)) => {
            metrics::record_round_refused(refusal);
            Outcome::RoundRefused(refusal)
        }
        Err(RoundError::Failed(error)) => {
            tracing::error!(error = ?error, user_id = %buyer_id, "nightcap round failed");
            Outcome::Failed
        }
    }
}
