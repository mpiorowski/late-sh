//! Per-session house-table state: which table is open on
//! `Screen::HouseTable` and the client wrapper around its singleton
//! service. `HouseTableClient` is a closed enum over the four runtimes —
//! no trait objects; every delegation is an exhaustive match.

use std::collections::HashSet;

use ratatui::{Frame, layout::Rect};
use uuid::Uuid;

use crate::app::common::primitives::Screen;
use crate::app::lobby::house::{
    registry::HouseTableRegistry, tables::HouseTable, types::InputAction,
};
use crate::app::notify::{Notification, Notifier};
use crate::usernames::UsernameLookup;

pub enum HouseTableClient {
    Poker(crate::app::lobby::house::poker::state::State),
    Blackjack(crate::app::lobby::house::blackjack::state::State),
    Asterion(crate::app::lobby::house::asterion::state::State),
    Tron(Box<crate::app::lobby::house::tron::state::State>),
    Ssnake(Box<crate::app::lobby::house::ssnake::state::State>),
}

impl HouseTableClient {
    pub fn table(&self) -> HouseTable {
        match self {
            Self::Poker(_) => HouseTable::Poker,
            Self::Blackjack(_) => HouseTable::Blackjack,
            Self::Asterion(_) => HouseTable::Asterion,
            Self::Tron(_) => HouseTable::Tron,
            Self::Ssnake(_) => HouseTable::Ssnake,
        }
    }

    /// True when this tick moved visible state (snapshot landed, event
    /// applied, flash expired). The games' server loops go quiet when no
    /// round runs, so an idle table settles clean.
    pub fn tick(&mut self) -> bool {
        match self {
            Self::Poker(state) => state.tick(),
            Self::Blackjack(state) => state.tick(),
            Self::Asterion(state) => state.tick(),
            Self::Tron(state) => state.tick(),
            Self::Ssnake(state) => state.tick(),
        }
    }

    pub fn touch_activity(&self) {
        match self {
            Self::Poker(state) => state.touch_activity(),
            Self::Blackjack(state) => state.touch_activity(),
            Self::Asterion(state) => state.touch_activity(),
            Self::Tron(state) => state.touch_activity(),
            Self::Ssnake(state) => state.touch_activity(),
        }
    }

    pub fn handle_key(&mut self, byte: u8) -> InputAction {
        match self {
            Self::Poker(state) => crate::app::lobby::house::poker::input::handle_key(state, byte),
            Self::Blackjack(state) => {
                // Blackjack's input module predates the shared action enum
                // and only understands Esc, so map q onto it and translate.
                let byte = if matches!(byte, b'q' | b'Q') {
                    0x1B
                } else {
                    byte
                };
                match crate::app::lobby::house::blackjack::input::handle_key(state, byte) {
                    crate::app::lobby::house::blackjack::input::InputAction::Ignored => {
                        InputAction::Ignored
                    }
                    crate::app::lobby::house::blackjack::input::InputAction::Handled => {
                        InputAction::Handled
                    }
                    crate::app::lobby::house::blackjack::input::InputAction::Leave => {
                        InputAction::Leave
                    }
                }
            }
            Self::Asterion(state) => {
                crate::app::lobby::house::asterion::input::handle_key(state, byte)
            }
            Self::Tron(state) => crate::app::lobby::house::tron::input::handle_key(state, byte),
            Self::Ssnake(state) => crate::app::lobby::house::ssnake::input::handle_key(state, byte),
        }
    }

    /// True when the game consumed the arrow; false hands it to embedded
    /// chat message selection, mirroring the rooms-era split.
    pub fn handle_arrow(&mut self, key: u8) -> bool {
        match self {
            Self::Poker(_) => false,
            Self::Blackjack(_) => false,
            Self::Asterion(state) => {
                crate::app::lobby::house::asterion::input::handle_arrow(state, key)
            }
            Self::Tron(state) => crate::app::lobby::house::tron::input::handle_arrow(state, key),
            Self::Ssnake(state) => {
                crate::app::lobby::house::ssnake::input::handle_arrow(state, key)
            }
        }
    }

    pub fn preferred_game_height(&self, area: Rect) -> u16 {
        match self {
            Self::Poker(_) => {
                let fancy = crate::app::lobby::house::poker::ui::fancy_game_height(area);
                if fancy > 0 {
                    fancy
                } else {
                    area.height.saturating_mul(7) / 10
                }
            }
            Self::Blackjack(_) => {
                let fancy = crate::app::lobby::house::blackjack::ui::fancy_game_height(area);
                if fancy > 0 {
                    fancy
                } else {
                    area.height.saturating_mul(7) / 10
                }
            }
            Self::Asterion(_) => area.height.saturating_mul(7).saturating_div(10).max(1),
            Self::Tron(_) => crate::app::lobby::house::tron::ui::preferred_height(area),
            Self::Ssnake(state) => {
                crate::app::lobby::house::ssnake::ui::preferred_height(state, area)
            }
        }
    }

    pub fn draw(&self, frame: &mut Frame, area: Rect, usernames: &UsernameLookup<'_>) {
        match self {
            Self::Poker(state) => {
                crate::app::lobby::house::poker::ui::draw_game(frame, area, state, usernames);
            }
            Self::Blackjack(state) => {
                crate::app::lobby::house::blackjack::ui::draw_game(
                    frame, area, state, false, usernames,
                );
            }
            Self::Asterion(state) => {
                crate::app::lobby::house::asterion::ui::draw_game(frame, area, state, usernames);
            }
            Self::Tron(state) => {
                crate::app::lobby::house::tron::ui::draw_game(frame, area, state, usernames);
            }
            Self::Ssnake(state) => {
                crate::app::lobby::house::ssnake::ui::draw_game(frame, area, state, usernames);
            }
        }
    }

    /// Asterion frees its hero slot when the client drops, so leaving the
    /// screen must drop it. The seat-holding games keep the client so
    /// re-entering restores chip selection and subscription cursors.
    pub fn drop_on_leave(&self) -> bool {
        match self {
            Self::Poker(_) => false,
            Self::Blackjack(_) => false,
            Self::Asterion(_) => true,
            Self::Tron(_) => false,
            Self::Ssnake(_) => false,
        }
    }

    pub fn chip_balance(&self) -> Option<i64> {
        match self {
            Self::Poker(state) => Some(state.global_balance()),
            Self::Blackjack(state) => Some(state.balance()),
            Self::Asterion(_) => None,
            Self::Tron(_) => None,
            Self::Ssnake(_) => None,
        }
    }

    pub fn can_sync_external_chip_balance(&self) -> bool {
        match self {
            Self::Poker(state) => state.can_sync_external_chip_balance(),
            Self::Blackjack(state) => {
                state.snapshot.phase == crate::app::lobby::house::blackjack::state::Phase::Betting
            }
            Self::Asterion(_) => false,
            Self::Tron(_) => false,
            Self::Ssnake(_) => false,
        }
    }

    pub fn sync_external_chip_balance(&mut self, balance: i64) {
        match self {
            Self::Poker(state) => state.sync_external_chip_balance(balance),
            Self::Blackjack(state) => state.set_balance(balance),
            Self::Asterion(_) => {}
            Self::Tron(_) => {}
            Self::Ssnake(_) => {}
        }
    }
}

/// Per-session house-table UI state. The singleton services are the system
/// of record; this is which table this session has open plus its client.
pub struct HouseState {
    user_id: Uuid,
    registry: HouseTableRegistry,
    /// Table currently open on `Screen::HouseTable`.
    pub open: Option<HouseTable>,
    /// Screen the Lobby modal was sitting on when the table opened; q/Esc
    /// restores it (and reopens the modal).
    pub return_screen: Screen,
    /// Kept across leave for the seat-holding games so re-entering the same
    /// table reuses client state; dropped for Asterion (`drop_on_leave`).
    client: Option<HouseTableClient>,
    /// One-time idempotent chat join fired from `App::tick`.
    pub chat_join_requested: bool,
    /// Off-screen your-turn desktop notify: producer handle plus the set of
    /// tables whose current turn already fired, edge-detected each tick.
    notifier: Notifier,
    turn_notified: HashSet<HouseTable>,
    /// False until the first tick seeds the baseline, so reconnecting
    /// mid-hand never notifies for a turn that was already yours.
    turn_notify_seeded: bool,
}

impl HouseState {
    pub(crate) fn new(user_id: Uuid, registry: HouseTableRegistry, notifier: Notifier) -> Self {
        Self {
            user_id,
            registry,
            open: None,
            return_screen: Screen::Dashboard,
            client: None,
            chat_join_requested: false,
            notifier,
            turn_notified: HashSet::new(),
            turn_notify_seeded: false,
        }
    }

    pub fn registry(&self) -> &HouseTableRegistry {
        &self.registry
    }

    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    /// Open a table: reuse the kept client when it's the same table,
    /// otherwise build a fresh one from the singleton service. False when
    /// the runtime failed to spawn (Asterion's maze generation can fail).
    pub fn enter(&mut self, table: HouseTable, return_screen: Screen, chip_balance: i64) -> bool {
        let same_table = self
            .client
            .as_ref()
            .is_some_and(|client| client.table() == table);
        if !same_table {
            let Some(client) = self.registry.enter(table, self.user_id, chip_balance) else {
                return false;
            };
            self.client = Some(client);
        }
        self.open = Some(table);
        self.return_screen = return_screen;
        self.chat_join_requested = false;
        true
    }

    /// Leave the screen. The client survives for seat-holding games so the
    /// player stays seated and can hop back; Asterion drops (frees its
    /// hero slot).
    pub fn close(&mut self) {
        if self
            .client
            .as_ref()
            .is_some_and(HouseTableClient::drop_on_leave)
        {
            self.client = None;
        }
        self.open = None;
        self.chat_join_requested = false;
    }

    /// Returns true when the open client's visible state moved.
    /// `notify_turn_edges` runs regardless: its notifications ride the
    /// notify outbox, which forces frames on its own.
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        if let Some(client) = self.client.as_mut() {
            changed = client.tick();
        }
        self.notify_turn_edges();
        changed
    }

    /// One desktop notification per table that just became this user's turn,
    /// across every table they hold a seat at (not only the open one). Reads
    /// the process-global singletons so it fires while off-screen; runs every
    /// tick regardless of screen, since the terminal window may be unfocused.
    fn notify_turn_edges(&mut self) {
        let awaiting: Vec<HouseTable> = HouseTable::ALL
            .into_iter()
            .filter(|table| self.registry.awaiting_action(*table, self.user_id))
            .collect();
        // First tick only establishes the baseline: a turn already yours on
        // connect must not notify.
        if !self.turn_notify_seeded {
            self.turn_notify_seeded = true;
            self.turn_notified = awaiting.into_iter().collect();
            return;
        }
        // Drop tables whose turn has passed so a turn coming back re-notifies.
        self.turn_notified.retain(|table| awaiting.contains(table));
        for table in awaiting {
            if self.turn_notified.insert(table) {
                self.notifier
                    .push(Notification::house_your_turn(table.display_name()));
            }
        }
    }

    pub fn client(&self) -> Option<&HouseTableClient> {
        self.client.as_ref()
    }

    pub fn client_mut(&mut self) -> Option<&mut HouseTableClient> {
        self.client.as_mut()
    }

    /// Drop the client outright (used when the game backend asks to leave
    /// AND wants its per-session state gone).
    pub fn drop_client(&mut self) {
        self.client = None;
    }

    /// The open table's permanent chat room, for the embedded chat pane and
    /// the visible-room sync. None while no table is open.
    pub fn chat_room_id(&self) -> Option<Uuid> {
        self.open
            .and_then(|table| self.registry.chat_room_id(table))
    }

    /// Tables where this user currently holds a seat, in roster order.
    /// Drives the backtick workspace cycle.
    pub fn my_seated_tables(&self) -> Vec<HouseTable> {
        HouseTable::ALL
            .into_iter()
            .filter(|table| self.registry.is_user_seated(*table, self.user_id))
            .collect()
    }
}
