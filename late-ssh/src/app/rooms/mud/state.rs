// Per-session client wrapper for a Lateania world.
//
// One State per session. Holds a cached snapshot drained from the service's
// watch channel each tick, plus local-only UI state: which side panel is open
// (room / character / abilities / inventory / shop) and a selection cursor for
// list panels. All real actions delegate to the service's *_task methods; this
// struct never blocks and never mutates world truth.

use tokio::sync::watch;
use uuid::Uuid;

use super::classes::Class;
use super::svc::{MudService, MudSnapshot, PlayerView, empty_player_view};
use super::world::Dir;

/// Which side panel the session is looking at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Room,
    Character,
    Abilities,
    Inventory,
    Shop,
}

pub struct State {
    user_id: Uuid,
    snapshot: MudSnapshot,
    svc: MudService,
    snapshot_rx: watch::Receiver<MudSnapshot>,
    panel: Panel,
    /// Selection cursor for the inventory/shop list panels.
    cursor: usize,
}

impl State {
    pub fn new(svc: MudService, user_id: Uuid) -> Self {
        let snapshot_rx = svc.subscribe_state();
        let snapshot = snapshot_rx.borrow().clone();
        let state = Self {
            user_id,
            snapshot,
            svc,
            snapshot_rx,
            panel: Panel::Room,
            cursor: 0,
        };
        state.svc.join_task(user_id);
        state
    }

    pub fn room_id(&self) -> Uuid {
        self.svc.room_id()
    }

    pub fn is_self(&self, user_id: Uuid) -> bool {
        self.user_id == user_id
    }

    pub fn tick(&mut self) {
        if self.snapshot_rx.has_changed().unwrap_or(false) {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
        }
    }

    pub fn touch_activity(&self) {
        self.svc.touch_activity_task(self.user_id);
    }

    pub fn view(&self) -> PlayerView {
        self.snapshot
            .players
            .get(&self.user_id)
            .cloned()
            .unwrap_or_else(empty_player_view)
    }

    pub fn player_count(&self) -> usize {
        self.snapshot.players.values().filter(|p| p.joined).count()
    }

    pub fn panel(&self) -> Panel {
        self.panel
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_panel(&mut self, panel: Panel) {
        if self.panel != panel {
            self.panel = panel;
            self.cursor = 0;
        }
    }

    pub fn toggle_panel(&mut self, panel: Panel) {
        if self.panel == panel {
            self.panel = Panel::Room;
        } else {
            self.panel = panel;
        }
        self.cursor = 0;
    }

    /// Current list length for whichever list panel is active (for cursor clamp).
    fn list_len(&self) -> usize {
        match self.panel {
            Panel::Inventory => self.view().inventory.len(),
            Panel::Shop => self.view().shop.map(|s| s.entries.len()).unwrap_or(0),
            _ => 0,
        }
    }

    pub fn cursor_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn cursor_down(&mut self) {
        let len = self.list_len();
        if len > 0 && self.cursor + 1 < len {
            self.cursor += 1;
        }
    }

    // ---- Actions --------------------------------------------------------

    pub fn choose_class(&self, class: Class) {
        self.svc.choose_class_task(self.user_id, class);
    }

    pub fn go(&self, dir: Dir) {
        self.svc.move_task(self.user_id, dir);
    }

    pub fn look(&self) {
        self.svc.look_task(self.user_id);
    }

    pub fn attack(&self) {
        self.svc.attack_task(self.user_id);
    }

    pub fn use_ability(&self, slot: u8) {
        self.svc.ability_task(self.user_id, slot);
    }

    pub fn flee(&self) {
        self.svc.flee_task(self.user_id);
    }

    pub fn leave_world(&self) {
        self.svc.leave_task(self.user_id);
    }

    /// Context action on the selected list row (equip/use in inventory, buy in shop).
    pub fn activate_selection(&self) {
        match self.panel {
            Panel::Inventory => {
                let view = self.view();
                if let Some(row) = view.inventory.get(self.cursor) {
                    if row.slot.is_some() {
                        self.svc.equip_task(self.user_id, row.item_id);
                    } else {
                        self.svc.use_item_task(self.user_id, row.item_id);
                    }
                }
            }
            Panel::Shop => {
                if let Some(shop) = self.view().shop {
                    if let Some(entry) = shop.entries.get(self.cursor) {
                        self.svc.buy_task(self.user_id, entry.item_id);
                    }
                }
            }
            _ => {}
        }
    }

    /// Secondary action: sell the selected inventory row at a shop.
    pub fn sell_selection(&self) {
        if self.panel == Panel::Inventory {
            let view = self.view();
            if let Some(row) = view.inventory.get(self.cursor) {
                self.svc.sell_task(self.user_id, row.item_id);
            }
        }
    }
}
