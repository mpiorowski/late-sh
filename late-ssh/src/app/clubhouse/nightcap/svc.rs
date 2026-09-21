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

use tokio::sync::mpsc::UnboundedSender;
use uuid::Uuid;

use late_core::models::drink_round::ROUND_PRICE_PER_PATRON;

use crate::app::clubhouse::lobby::SharedLobby;
use crate::app::games::chips::svc::{ChipService, RoundError};
use crate::metrics;

use super::lobby::SharedSeats;
use super::state::{Order, OrderOutcome};

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
    outcome_tx: UnboundedSender<OrderOutcome>,
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
) -> OrderOutcome {
    // A banked round credit pays first, as it does at the counter.
    match chip_service.cash_round_drink(user_id).await {
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
            return OrderOutcome::Comped {
                drink,
                remaining: comped.remaining,
            };
        }
        Ok(None) => {}
        Err(error) => {
            metrics::record_nightcap_order(NightcapOrderResult::Failed);
            tracing::error!(error = ?error, user_id = %user_id, "nightcap credit check failed");
            return OrderOutcome::Failed;
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
            OrderOutcome::Poured {
                drink,
                balance: purchase.balance,
            }
        }
        // The floor guard refused the pour. Nothing charged, nothing poured.
        Ok(None) => {
            metrics::record_nightcap_order(NightcapOrderResult::Bounced);
            OrderOutcome::Bounced { drink }
        }
        Err(error) => {
            metrics::record_nightcap_order(NightcapOrderResult::Failed);
            tracing::error!(error = ?error, user_id = %user_id, drink = drink.name(), "nightcap pour failed");
            OrderOutcome::Failed
        }
    }
}

async fn order_round(
    chip_service: &ChipService,
    drunk_lobby: Option<&SharedLobby>,
    seats: &SharedSeats,
    buyer_id: Uuid,
) -> OrderOutcome {
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
            OrderOutcome::RoundBought {
                patrons: purchase.patrons,
                total: purchase.total_chips,
                balance: purchase.balance,
            }
        }
        Err(RoundError::Refused(refusal)) => {
            metrics::record_round_refused(refusal);
            OrderOutcome::RoundRefused(refusal)
        }
        Err(RoundError::Failed(error)) => {
            tracing::error!(error = ?error, user_id = %buyer_id, "nightcap round failed");
            OrderOutcome::Failed
        }
    }
}
