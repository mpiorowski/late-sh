//! Nightcap's per-session state: which seat (if any) this user holds, the
//! latest shared snapshots for rendering (seats and wall), the drink menu,
//! the carving field, the one order in flight, and the roster-refresh
//! cadence. Pure: the chips and the carvings move in `svc.rs`, whose
//! outcome comes back through `outcome_sender` and lands here on the next
//! tick. See `lobby.rs` for the shared seats and `wall.rs` for the wall.

use ratatui_textarea::{TextArea, WrapMode};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use uuid::Uuid;

use late_core::models::nightcap_carving::CARVING_MAX_CHARS;

use crate::app::common::composer::new_themed_textarea;
use crate::app::common::primitives::thousands;
use crate::app::games::chips::svc::RoundRefusal;

use super::lobby::{SEAT_COUNT, SeatChange, SeatView, SharedSeats};
use super::wall::{SharedWall, WallSnapshot};

/// How often (in `tick` calls) the live roster is reconciled against the
/// shared seats, mirroring the Clubhouse's own cadence.
const ROSTER_REFRESH_TICKS: u64 = 20;

/// How long the muted TV holds one caption before the next, in world
/// ticks: about half a minute. Slow on purpose; it is a TV in the corner,
/// not a ticker.
const TV_DWELL_TICKS: u64 = 450;

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

/// How something the house was asked for settled, as `svc.rs` reports it
/// back. Plain data: the failure has already been logged where it happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
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
    Carved {
        stool: usize,
    },
    CarveFailed,
}

pub struct State {
    lobby: Option<SharedSeats>,
    wall: Option<SharedWall>,
    user_id: Uuid,
    username: String,
    anim_tick: u64,
    last_roster_tick: u64,
    force_roster_refresh: bool,
    snapshot: [Option<SeatView>; SEAT_COUNT],
    wall_snapshot: WallSnapshot,
    /// The last thing that happened on late.sh, for the TV: "name action".
    last_activity: Option<String>,
    menu_open: bool,
    /// The line being carved into this session's stool, while `c` is open.
    carving: Option<TextArea<'static>>,
    order_in_flight: bool,
    outcome_tx: UnboundedSender<Outcome>,
    outcome_rx: UnboundedReceiver<Outcome>,
    /// Flavor line shown in the footer after the last seat/drink action.
    pub last_message: Option<String>,
}

impl State {
    pub fn new(
        lobby: Option<SharedSeats>,
        wall: Option<SharedWall>,
        user_id: Uuid,
        username: String,
    ) -> Self {
        let (outcome_tx, outcome_rx) = unbounded_channel();
        Self {
            lobby,
            wall,
            user_id,
            username,
            anim_tick: 0,
            last_roster_tick: 0,
            force_roster_refresh: true,
            snapshot: std::array::from_fn(|_| None),
            wall_snapshot: WallSnapshot::default(),
            last_activity: None,
            menu_open: false,
            carving: None,
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
        self.carving = None;
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
        if let Some(lobby) = &self.lobby {
            self.snapshot = lobby.snapshot();
        }
        if let Some(wall) = &self.wall {
            self.wall_snapshot = wall.snapshot();
        }
    }

    pub fn snapshot(&self) -> &[Option<SeatView>; SEAT_COUNT] {
        &self.snapshot
    }

    pub fn wall(&self) -> &WallSnapshot {
        &self.wall_snapshot
    }

    pub fn seats_handle(&self) -> Option<SharedSeats> {
        self.lobby.clone()
    }

    pub fn my_seat(&self) -> Option<usize> {
        self.lobby.as_ref()?.seat_of(self.user_id)
    }

    /// Something happened on late.sh; the TV may show it next.
    pub fn note_activity(&mut self, username: &str, action: &str) {
        self.last_activity = Some(format!("{username} {action}"));
    }

    pub fn last_activity(&self) -> Option<&str> {
        self.last_activity.as_deref()
    }

    /// Which of `n` captions the TV is showing right now. The renderer
    /// assembles the captions it has (some sources are empty some nights)
    /// and asks for the one to draw.
    pub fn tv_pick(&self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        ((self.anim_tick / TV_DWELL_TICKS) % n as u64) as usize
    }

    /// Only the seated speak here. The composer never opens otherwise, and
    /// the footer says why instead of swallowing the key.
    pub fn compose_allowed(&self) -> bool {
        self.my_seat().is_some()
    }

    pub fn note_compose_needs_seat(&mut self) {
        self.last_message = Some("take a seat first.".to_string());
    }

    /// Seated, but the chat snapshot has not carried the room in yet (the
    /// first second of a session). Say so; "take a seat" would be a lie.
    pub fn note_room_not_loaded(&mut self) {
        self.last_message = Some("the bar is still opening up. try again.".to_string());
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
        // Standing up takes the menu and the knife with it: there is no bar
        // to order from and no stool to carve.
        if self.my_seat().is_none() {
            self.menu_open = false;
            self.carving = None;
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

    /// `c`: open the carving field for this session's stool. Needs a stool;
    /// the line goes into the wood under you, nowhere else.
    pub fn start_carving(&mut self) {
        if self.my_seat().is_none() {
            self.note_compose_needs_seat();
            return;
        }
        self.menu_open = false;
        self.last_message = None;
        self.carving = Some(new_themed_textarea(
            "carve a line into the stool",
            WrapMode::None,
            true,
        ));
    }

    pub fn carving_mut(&mut self) -> Option<&mut TextArea<'static>> {
        self.carving.as_mut()
    }

    /// The draft as typed, for the footer while the knife is out.
    pub fn carving_text(&self) -> Option<String> {
        self.carving.as_ref().map(|field| field.lines().join(""))
    }

    /// Put the knife down without carving. Returns whether it was out, so
    /// Esc can peel it before leaving the room.
    pub fn cancel_carving(&mut self) -> bool {
        let was_out = self.carving.is_some();
        self.carving = None;
        was_out
    }

    /// Enter on the carving field: hand the line to the house. `Some` means
    /// the caller should spawn it (`NightcapHouse::spawn_carve`); `None`
    /// means there was nothing to carve or no stool under the knife.
    pub fn take_carving(&mut self) -> Option<(usize, String)> {
        let field = self.carving.take()?;
        let body: String = field
            .lines()
            .join("")
            .trim()
            .chars()
            .take(CARVING_MAX_CHARS)
            .collect();
        if body.is_empty() {
            self.last_message = Some("nothing to carve.".to_string());
            return None;
        }
        let Some(stool) = self.my_seat() else {
            self.note_compose_needs_seat();
            return None;
        };
        self.last_message = Some("you carve it into the wood.".to_string());
        Some((stool, body))
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

    pub fn outcome_sender(&self) -> UnboundedSender<Outcome> {
        self.outcome_tx.clone()
    }

    /// Pull every settled outcome off the channel; each tick call.
    pub fn drain_outcomes(&mut self) {
        while let Ok(outcome) = self.outcome_rx.try_recv() {
            self.apply_outcome(outcome);
        }
    }

    pub fn apply_outcome(&mut self, outcome: Outcome) {
        // A carve is not an order: it never held the one-order-at-a-time
        // slot, so it must not release one that a pour still holds.
        if !matches!(outcome, Outcome::Carved { .. } | Outcome::CarveFailed) {
            self.order_in_flight = false;
        }
        self.last_message = Some(match outcome {
            Outcome::Poured { drink, balance } => format!(
                "{}, {} chips. {} left on the tab.",
                drink.name(),
                thousands(drink.price()),
                thousands(balance)
            ),
            Outcome::Comped {
                drink,
                remaining: 0,
            } => {
                format!("{}, on somebody's round.", drink.name())
            }
            Outcome::Comped { drink, remaining } => format!(
                "{}, on somebody's round. {remaining} more waiting.",
                drink.name()
            ),
            Outcome::Bounced { drink } => {
                format!("your tab won't cover the {}.", drink.name())
            }
            Outcome::RoundBought {
                patrons,
                total,
                balance,
            } => format!(
                "a round for {patrons}, {} chips. {} left on the tab.",
                thousands(total),
                thousands(balance)
            ),
            Outcome::RoundRefused(RoundRefusal::EmptyHouse) => {
                "nobody else on a stool to buy for.".to_string()
            }
            Outcome::RoundRefused(RoundRefusal::AllHolding) => {
                "everyone here still has a drink coming.".to_string()
            }
            Outcome::RoundRefused(RoundRefusal::InsufficientChips { patrons, total }) => {
                format!(
                    "a round for {patrons} runs {} chips. not tonight.",
                    thousands(total)
                )
            }
            Outcome::Failed => "the tap sputtered. try again.".to_string(),
            Outcome::Carved { stool } => format!("carved into stool {}.", stool + 1),
            Outcome::CarveFailed => "the knife slipped. try again.".to_string(),
        });
        self.refresh_snapshot();
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
