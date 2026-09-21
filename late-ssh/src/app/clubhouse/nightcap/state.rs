//! Nightcap's per-session state: which seat (if any) this user holds, the
//! latest shared snapshot for rendering, the drink menu, the one order in
//! flight, and the roster-refresh cadence. Pure: the chips move in
//! `svc.rs`, whose outcome comes back through `outcome_sender` and lands
//! here on the next tick. See `lobby.rs` for the actual shared seat state.

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use uuid::Uuid;

use crate::app::common::primitives::thousands;
use crate::app::games::chips::svc::RoundRefusal;

use super::lobby::{SEAT_COUNT, SeatChange, SeatView, SharedSeats};

/// How often (in `tick` calls) the live roster is reconciled against the
/// shared seats, mirroring the Clubhouse's own cadence.
const ROSTER_REFRESH_TICKS: u64 = 20;

/// The house menu: four fixed pours, priced across the same 100..1000 band
/// the tavern's bartender quotes, so a drink here buys exactly the buzz it
/// would at the counter. No haggling and nobody to ask; that is the point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drink {
    HouseBeer,
    WhiskeyNeat,
    OldFashioned,
    TopShelf,
}

impl Drink {
    pub const MENU: [Drink; 4] = [
        Drink::HouseBeer,
        Drink::WhiskeyNeat,
        Drink::OldFashioned,
        Drink::TopShelf,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Drink::HouseBeer => "house beer",
            Drink::WhiskeyNeat => "whiskey neat",
            Drink::OldFashioned => "old fashioned",
            Drink::TopShelf => "top shelf",
        }
    }

    pub fn price(self) -> i64 {
        match self {
            Drink::HouseBeer => 100,
            Drink::WhiskeyNeat => 250,
            Drink::OldFashioned => 500,
            Drink::TopShelf => 1_000,
        }
    }
}

/// What a seated patron asked the house for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Drink(Drink),
    /// A round for every other stool, at the tavern's per-patron price.
    Round,
}

/// How an order settled, as `svc.rs` reports it back. Plain data: the
/// failure has already been logged where it happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderOutcome {
    Poured {
        drink: Drink,
        balance: i64,
    },
    /// Somebody's round covered it; `remaining` is what is still banked.
    Comped {
        drink: Drink,
        remaining: i64,
    },
    /// The chip floor refused the pour. Nothing charged.
    Bounced {
        drink: Drink,
    },
    RoundBought {
        patrons: i64,
        total: i64,
        balance: i64,
    },
    RoundRefused(RoundRefusal),
    Failed,
}

pub struct State {
    lobby: Option<SharedSeats>,
    user_id: Uuid,
    username: String,
    anim_tick: u64,
    last_roster_tick: u64,
    force_roster_refresh: bool,
    snapshot: [Option<SeatView>; SEAT_COUNT],
    menu_open: bool,
    order_in_flight: bool,
    outcome_tx: UnboundedSender<OrderOutcome>,
    outcome_rx: UnboundedReceiver<OrderOutcome>,
    /// Flavor line shown in the footer after the last seat/drink action.
    pub last_message: Option<String>,
}

impl State {
    pub fn new(lobby: Option<SharedSeats>, user_id: Uuid, username: String) -> Self {
        let (outcome_tx, outcome_rx) = unbounded_channel();
        Self {
            lobby,
            user_id,
            username,
            anim_tick: 0,
            last_roster_tick: 0,
            force_roster_refresh: true,
            snapshot: std::array::from_fn(|_| None),
            menu_open: false,
            order_in_flight: false,
            outcome_tx,
            outcome_rx,
            last_message: None,
        }
    }

    /// Screen entry hook: refresh the seat snapshot immediately rather than
    /// waiting for the next scheduled roster tick.
    pub fn enter_screen(&mut self) {
        self.force_roster_refresh = true;
    }

    /// Screen exit hook: hand the stool back. Esc is not the only way out
    /// (digits, Tab, `0` all leave), so this hangs off `set_screen` rather
    /// than the Esc path, the same shape the other contextual screens use.
    pub fn leave_screen(&mut self) {
        if let Some(lobby) = &self.lobby {
            lobby.vacate(self.user_id);
        }
        self.menu_open = false;
        self.last_message = None;
        self.refresh_snapshot();
    }

    pub fn tick(&mut self, anim_tick: u64) {
        self.anim_tick = anim_tick;
    }

    pub fn roster_refresh_due(&mut self) -> bool {
        if !self.force_roster_refresh
            && self.anim_tick.wrapping_sub(self.last_roster_tick) < ROSTER_REFRESH_TICKS
        {
            return false;
        }
        self.force_roster_refresh = false;
        self.last_roster_tick = self.anim_tick;
        true
    }

    /// Drop anyone no longer connected from the shared seats, and relabel
    /// the patrons still here from the roster's names.
    pub fn refresh_roster(&mut self, roster: &[(Uuid, String)]) {
        if let Some(lobby) = &self.lobby {
            lobby.sync(roster);
        }
    }

    pub fn refresh_snapshot(&mut self) {
        let Some(lobby) = &self.lobby else {
            return;
        };
        self.snapshot = lobby.snapshot();
    }

    pub fn snapshot(&self) -> &[Option<SeatView>; SEAT_COUNT] {
        &self.snapshot
    }

    pub fn seats_handle(&self) -> Option<SharedSeats> {
        self.lobby.clone()
    }

    pub fn my_seat(&self) -> Option<usize> {
        self.lobby.as_ref()?.seat_of(self.user_id)
    }

    /// Only the seated speak here. The composer never opens otherwise, and
    /// the footer says why instead of swallowing the key.
    pub fn compose_allowed(&self) -> bool {
        self.my_seat().is_some()
    }

    pub fn note_compose_needs_seat(&mut self) {
        self.last_message = Some("take a seat first.".to_string());
    }

    /// Sit in / stand from the given 0-based seat, reporting what the press
    /// did. A stool someone else holds is the press this room bounces most
    /// often, and saying nothing there is indistinguishable from a key that
    /// never arrived.
    pub fn toggle_seat(&mut self, seat: usize) {
        let Some(lobby) = &self.lobby else {
            return;
        };
        self.last_message = match lobby.toggle_seat(self.user_id, &self.username, seat) {
            // Sitting and standing show themselves: the seat row picks up
            // (or drops) the `(you)` label on the next draw.
            SeatChange::SatDown | SeatChange::StoodUp => None,
            SeatChange::Taken => Some("that stool is taken.".to_string()),
            // Not reachable from the keymap, which only sends `1`-`6`; the
            // variant exists because `SharedSeats` bounds-checks for any
            // caller, not just this one.
            SeatChange::OutOfRange => None,
        };
        // Standing up takes the menu with it: there is no bar to order from.
        if self.my_seat().is_none() {
            self.menu_open = false;
        }
        self.refresh_snapshot();
    }

    pub fn menu_open(&self) -> bool {
        self.menu_open
    }

    /// `d`: open or close the house menu. Needs a stool, like everything
    /// else the house does for you.
    pub fn toggle_menu(&mut self) {
        if self.my_seat().is_none() {
            self.note_compose_needs_seat();
            return;
        }
        self.menu_open = !self.menu_open;
        self.last_message = None;
    }

    /// Close the menu if it is open. Returns whether there was one to close,
    /// so Esc can peel it before leaving the room.
    pub fn close_menu(&mut self) -> bool {
        let was_open = self.menu_open;
        self.menu_open = false;
        was_open
    }

    /// Hand an order to the house. `Some` means the caller should spawn it
    /// (`svc::spawn_order`); `None` means the footer already said why not.
    /// One order at a time: the chips move off-thread and a second press
    /// before the first settles would double-charge.
    pub fn pick(&mut self, order: Order) -> Option<Order> {
        if self.my_seat().is_none() || self.lobby.is_none() {
            self.note_compose_needs_seat();
            return None;
        }
        if self.order_in_flight {
            self.last_message = Some("the house is on it.".to_string());
            return None;
        }
        self.order_in_flight = true;
        self.menu_open = false;
        self.last_message = Some(match order {
            Order::Drink(drink) => format!("you order the {}.", drink.name()),
            Order::Round => "you ask for a round.".to_string(),
        });
        Some(order)
    }

    pub fn outcome_sender(&self) -> UnboundedSender<OrderOutcome> {
        self.outcome_tx.clone()
    }

    /// Pull every settled order off the channel; each tick call.
    pub fn drain_outcomes(&mut self) {
        while let Ok(outcome) = self.outcome_rx.try_recv() {
            self.apply_outcome(outcome);
        }
    }

    pub fn apply_outcome(&mut self, outcome: OrderOutcome) {
        self.order_in_flight = false;
        self.last_message = Some(match outcome {
            OrderOutcome::Poured { drink, balance } => format!(
                "{}, {} chips. {} left on the tab.",
                drink.name(),
                thousands(drink.price()),
                thousands(balance)
            ),
            OrderOutcome::Comped {
                drink,
                remaining: 0,
            } => {
                format!("{}, on somebody's round.", drink.name())
            }
            OrderOutcome::Comped { drink, remaining } => format!(
                "{}, on somebody's round. {remaining} more waiting.",
                drink.name()
            ),
            OrderOutcome::Bounced { drink } => {
                format!("your tab won't cover the {}.", drink.name())
            }
            OrderOutcome::RoundBought {
                patrons,
                total,
                balance,
            } => format!(
                "a round for {patrons}, {} chips. {} left on the tab.",
                thousands(total),
                thousands(balance)
            ),
            OrderOutcome::RoundRefused(RoundRefusal::EmptyHouse) => {
                "nobody else on a stool to buy for.".to_string()
            }
            OrderOutcome::RoundRefused(RoundRefusal::AllHolding) => {
                "everyone here still has a drink coming.".to_string()
            }
            OrderOutcome::RoundRefused(RoundRefusal::InsufficientChips { patrons, total }) => {
                format!(
                    "a round for {patrons} runs {} chips. not tonight.",
                    thousands(total)
                )
            }
            OrderOutcome::Failed => "the tap sputtered. try again.".to_string(),
        });
        self.refresh_snapshot();
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
