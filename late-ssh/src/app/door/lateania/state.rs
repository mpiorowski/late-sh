// Per-session client wrapper for a Lateania world.
//
// One State per session. Holds a cached snapshot drained from the service's
// watch channel each tick, plus local-only UI state: which side panel is open
// (room / character / abilities / inventory / shop) and a selection cursor for
// list panels. All real actions delegate to the service's *_task methods; this
// struct never blocks and never mutates world truth.

use std::cell::Cell;
use std::time::{Duration, Instant};

use tokio::sync::watch;
use uuid::Uuid;

use super::classes::Class;
use super::svc::{LateaniaService, MudSnapshot, PlayerView, empty_player_view};
use super::world::Dir;

/// Lines moved per `[` / `]` press when scrolling a text panel.
const SCROLL_STEP: usize = 3;

/// Which side panel the session is looking at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Room,
    Character,
    Abilities,
    Inventory,
    Shop,
    /// Lookable things in the room: select one and press Enter to examine it
    /// (and use it, for a fountain).
    Examine,
    /// Earned titles: select one and press Enter to display it (or clear it).
    Titles,
    /// The quest journal: the Frontier zone quests and their status (read-only).
    Quests,
    /// Adventurers in the room: select one and press Enter to auto-follow them.
    Follow,
    /// The companion vendor at a capital Stable: select a beast and Enter to buy
    /// it; `x` feeds (heals/raises) the companion you already have.
    Stable,
    /// The housing ledger: buy a deed at the clerk, or (inside a home you own)
    /// buy and place a furnishing. `Enter` activates the selected row.
    Housing,
    /// The appearance/bio builder: pick a field with the cursor, `Enter` cycles
    /// its option forward and `x` cycles back.
    Appearance,
    /// The waystone fast-travel menu: pick a destination and `Enter` to step
    /// through to it.
    Portal,
    /// The whole-world atlas: exploration progress per region (read-only,
    /// scrollable with `[` / `]`). Toggled with `m`.
    Map,
}

pub struct State {
    user_id: Uuid,
    session_id: Uuid,
    snapshot: MudSnapshot,
    svc: LateaniaService,
    snapshot_rx: watch::Receiver<MudSnapshot>,
    panel: Panel,
    /// Selection cursor for the inventory/shop list panels.
    cursor: usize,
    /// Line the list view is scrolled to. Interior-mutable so the render pass
    /// (which only holds `&State`) can keep the highlighted row inside a
    /// scroll-off margin. Reset whenever the panel changes.
    list_scroll: Cell<usize>,
    joined: bool,
    join_pending: bool,
    join_requested_at: Instant,
    reset_version: u64,
    reset_elsewhere: bool,
}

impl State {
    pub fn new(svc: LateaniaService, user_id: Uuid) -> Self {
        let session_id = Uuid::now_v7();
        let join_requested_at = Instant::now();
        let snapshot_rx = svc.subscribe_state();
        let snapshot = snapshot_rx.borrow().clone();
        let reset_version = snapshot
            .reset_versions
            .get(&user_id)
            .copied()
            .unwrap_or_default();
        let state = Self {
            user_id,
            session_id,
            snapshot,
            svc,
            snapshot_rx,
            panel: Panel::Room,
            cursor: 0,
            list_scroll: Cell::new(0),
            joined: true,
            join_pending: true,
            join_requested_at,
            reset_version,
            reset_elsewhere: false,
        };
        state.svc.join_task(user_id, session_id);
        state
    }

    pub fn tick(&mut self) {
        if self.snapshot_rx.has_changed().unwrap_or(false) {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
        }
        let reset_version = self
            .snapshot
            .reset_versions
            .get(&self.user_id)
            .copied()
            .unwrap_or_default();
        if reset_version > self.reset_version {
            self.reset_version = reset_version;
            self.joined = false;
            self.join_pending = false;
            self.reset_elsewhere = true;
            return;
        }
        if self.snapshot.players.contains_key(&self.user_id) {
            self.join_pending = false;
        }
    }

    pub fn touch_activity(&mut self) {
        if self.ensure_player_present() {
            self.svc.touch_activity_task(self.user_id);
        }
    }

    pub fn ensure_player_present(&mut self) -> bool {
        if !self.joined {
            return false;
        }
        if self.snapshot.players.contains_key(&self.user_id) {
            self.join_pending = false;
            return true;
        }
        if !self.join_pending || self.join_requested_at.elapsed() >= Duration::from_secs(2) {
            self.join_requested_at = Instant::now();
            self.join_pending = true;
            self.svc.join_task(self.user_id, self.session_id);
        }
        false
    }

    pub fn view(&self) -> PlayerView {
        self.snapshot
            .players
            .get(&self.user_id)
            .cloned()
            .unwrap_or_else(empty_player_view)
    }

    pub fn reset_elsewhere(&self) -> bool {
        self.reset_elsewhere
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
            self.list_scroll.set(0);
        }
    }

    pub fn toggle_panel(&mut self, panel: Panel) {
        if self.panel == panel {
            self.panel = Panel::Room;
        } else {
            self.panel = panel;
        }
        self.cursor = 0;
        self.list_scroll.set(0);
    }

    /// Current list scroll offset (first visible line).
    pub fn list_scroll(&self) -> usize {
        self.list_scroll.get()
    }

    /// Store the list scroll offset chosen by the render pass.
    pub fn set_list_scroll(&self, off: usize) {
        self.list_scroll.set(off);
    }

    /// Manual scroll for cursor-less text panels (`[` / `]`). List panels
    /// auto-follow their cursor and re-clamp this on the next render, so these
    /// only have a lasting effect on text panels. The render pass clamps the
    /// value to the content, so growing it past the end is harmless.
    pub fn scroll_text_up(&mut self) {
        let cur = self.list_scroll.get();
        self.list_scroll.set(cur.saturating_sub(SCROLL_STEP));
    }

    pub fn scroll_text_down(&mut self) {
        let cur = self.list_scroll.get();
        self.list_scroll.set(cur + SCROLL_STEP);
    }

    /// Current list length for whichever list panel is active (for cursor clamp).
    fn list_len(&self) -> usize {
        match self.panel {
            Panel::Inventory => self.view().inventory.len(),
            Panel::Abilities => self.view().abilities.len(),
            Panel::Shop => self.view().shop.map(|s| s.entries.len()).unwrap_or(0),
            Panel::Examine => self.view().features.len(),
            Panel::Titles => self.view().titles.len(),
            Panel::Follow => self.view().occupants.len(),
            Panel::Stable => self.view().stable.map(|s| s.entries.len()).unwrap_or(0),
            Panel::Housing => self.view().housing.map(|h| h.entries.len()).unwrap_or(0),
            Panel::Portal => self.view().portal.map(|p| p.entries.len()).unwrap_or(0),
            Panel::Appearance => self.view().appearance.len(),
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

    // ---- Class selection cursor ----------------------------------------

    /// The highlighted class on the selection screen (reuses `cursor`, which is
    /// unused before a class is chosen). Clamped into range.
    pub fn class_cursor(&self) -> usize {
        self.cursor.min(Class::ALL.len() - 1)
    }

    pub fn class_cursor_up(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn class_cursor_down(&mut self) {
        if self.cursor + 1 < Class::ALL.len() {
            self.cursor += 1;
        }
    }

    pub fn choose_class_at_cursor(&mut self) {
        self.choose_class(Class::ALL[self.class_cursor()]);
    }

    // ---- Actions --------------------------------------------------------

    pub fn choose_class(&mut self, class: Class) {
        if self.ensure_player_present() {
            self.svc.choose_class_task(self.user_id, class);
        }
    }

    /// Commit one of the two offered archetype paths (0-based) at level 10.
    pub fn choose_archetype(&mut self, choice: usize) {
        if self.ensure_player_present() {
            self.svc.choose_archetype_task(self.user_id, choice);
        }
    }

    pub fn go(&mut self, dir: Dir) {
        if self.ensure_player_present() {
            self.svc.move_task(self.user_id, dir);
        }
    }

    pub fn look(&mut self) {
        if self.ensure_player_present() {
            self.svc.look_task(self.user_id);
        }
    }

    /// Speak the word of recall: warp back to Embergate's Town Square.
    pub fn recall(&mut self) {
        if self.ensure_player_present() {
            self.svc.recall_task(self.user_id);
        }
    }

    /// Open the Follow panel to pick which adventurer to follow.
    pub fn follow(&mut self) {
        self.toggle_panel(Panel::Follow);
    }

    /// Follow (or stop following) the adventurer highlighted in the Follow panel.
    pub fn follow_selected(&mut self) {
        if !self.ensure_player_present() {
            return;
        }
        if let Some(target) = self.view().occupants.get(self.cursor).map(|o| o.user_id) {
            self.svc.follow_to_task(self.user_id, target);
        }
    }

    /// Stop following whoever is currently being followed.
    pub fn stop_follow(&mut self) {
        if self.ensure_player_present() {
            self.svc.stop_follow_task(self.user_id);
        }
    }

    /// Re-roll ability scores on the selection screen (before choosing a class).
    pub fn reroll(&mut self) {
        if self.ensure_player_present() {
            self.svc.reroll_task(self.user_id);
        }
    }

    /// Examine the selected lookable feature in the room.
    pub fn examine_selection(&mut self) {
        if self.panel == Panel::Examine && self.ensure_player_present() {
            self.svc.interact_task(self.user_id, self.cursor);
        }
    }

    pub fn attack(&mut self) {
        if self.ensure_player_present() {
            self.svc.attack_task(self.user_id);
        }
    }

    pub fn use_ability(&mut self, slot: u8) {
        if self.ensure_player_present() {
            self.svc.ability_task(self.user_id, slot);
        }
    }

    pub fn flee(&mut self) {
        if self.ensure_player_present() {
            self.svc.flee_task(self.user_id);
        }
    }

    /// Release a fallen spirit to the temple instead of waiting for a rez.
    pub fn release(&mut self) {
        if self.ensure_player_present() {
            self.svc.release_task(self.user_id);
        }
    }

    /// Cast the Resurrection rite on the nearest corpse in the room.
    pub fn resurrect(&mut self) {
        if self.ensure_player_present() {
            self.svc.resurrect_task(self.user_id);
        }
    }

    /// Feed and tend the player's companion at the Stable.
    pub fn feed_pet(&mut self) {
        if self.ensure_player_present() {
            self.svc.feed_pet_task(self.user_id);
        }
    }

    pub fn leave_world(&mut self) {
        self.close_session();
    }

    fn close_session(&mut self) {
        if self.joined {
            self.joined = false;
            self.svc.leave_task(self.user_id, self.session_id);
        }
    }

    /// Context action on the selected list row (equip/use in inventory, buy in shop).
    pub fn activate_selection(&mut self) {
        if !self.ensure_player_present() {
            return;
        }
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
            Panel::Abilities => {
                // Cast the highlighted ability; this is how slots past the 1-9
                // hotbar (deep rosters, capstones) are reached.
                if let Some(a) = self.view().abilities.get(self.cursor) {
                    let slot = a.slot;
                    self.svc.ability_task(self.user_id, slot);
                }
            }
            Panel::Shop => {
                if let Some(shop) = self.view().shop
                    && let Some(entry) = shop.entries.get(self.cursor)
                {
                    self.svc.buy_task(self.user_id, entry.item_id);
                }
            }
            Panel::Examine => self.svc.interact_task(self.user_id, self.cursor),
            Panel::Titles => self.svc.set_active_title_task(self.user_id, self.cursor),
            Panel::Follow => self.follow_selected(),
            Panel::Stable => {
                if let Some(stable) = self.view().stable
                    && let Some(entry) = stable.entries.get(self.cursor)
                {
                    self.svc.buy_pet_task(self.user_id, entry.key.clone());
                }
            }
            Panel::Housing => {
                if let Some(housing) = self.view().housing {
                    if housing.furnish {
                        if let Some(entry) = housing.entries.get(self.cursor) {
                            self.svc.buy_furniture_task(self.user_id, entry.key.clone());
                        }
                    } else {
                        // Deed rows are the tiers in order, so the cursor is the plot.
                        self.svc.buy_deed_task(self.user_id, self.cursor);
                    }
                }
            }
            Panel::Portal => {
                if let Some(portal) = self.view().portal
                    && let Some((_, room, _)) = portal.entries.get(self.cursor)
                {
                    self.svc.travel_task(self.user_id, *room);
                    self.panel = Panel::Room;
                }
            }
            Panel::Appearance => self.cycle_appearance(1),
            _ => {}
        }
    }

    /// Open the waystone fast-travel menu (only meaningful on a portal).
    pub fn open_portal(&mut self) {
        self.toggle_panel(Panel::Portal);
    }

    /// Cycle the highlighted appearance field forward (+1) or back (-1).
    pub fn cycle_appearance(&mut self, delta: i8) {
        if self.ensure_player_present() {
            self.svc
                .cycle_appearance_task(self.user_id, self.cursor, delta);
        }
    }

    /// Open the appearance/bio builder.
    pub fn open_appearance(&mut self) {
        self.toggle_panel(Panel::Appearance);
    }

    /// Secondary action: sell the selected inventory row at a shop.
    pub fn sell_selection(&mut self) {
        if !self.ensure_player_present() {
            return;
        }
        if self.panel == Panel::Inventory {
            let view = self.view();
            if let Some(row) = view.inventory.get(self.cursor) {
                self.svc.sell_task(self.user_id, row.item_id);
            }
        }
    }

    /// Batch-sell from the inventory panel (all / common / non-upgrades).
    pub fn sell_batch(&mut self, kind: super::svc::SellBatch) {
        if self.ensure_player_present() {
            self.svc.sell_batch_task(self.user_id, kind);
        }
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.close_session();
    }
}
