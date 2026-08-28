// Per-session client wrapper for a Lateania world.
//
// One State per session. Holds a cached snapshot drained from the service's
// watch channel each tick, plus local-only UI state: which side panel is open
// (room / character / abilities / inventory / shop) and a selection cursor for
// list panels. All real actions delegate to the service's *_task methods; this
// struct never blocks and never mutates world truth.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use tokio::sync::watch;
use uuid::Uuid;

use super::classes::Class;
use super::svc::{LateaniaService, MudSnapshot, PlayerView, empty_player_view};
use super::world::Dir;
use super::world::RoomId;
use super::worldmap::{Coord, MapCamera, Route};

/// Lines moved per `[` / `]` press when scrolling a text panel.
const SCROLL_STEP: usize = 3;

/// Where the player has marked they're going, resolved against where they are
/// standing now. Rendered as one line under the room's exits: the exits say
/// what is available, this says which of them to take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heading {
    /// Standing in the marked room.
    Arrived(&'static str),
    /// The marked room, and the next exit to take toward it.
    Toward(&'static str, Route),
    /// Marked, but no walk over ground the player knows reaches it from here.
    Unreachable(&'static str),
}

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
    /// The quest journal: the active starter step, accepted bounties, the Long
    /// Road, and (once open) the Frontier zone quests. A list panel: Enter on
    /// a row tracks its target on the compass/map.
    Quests,
    /// Adventurers in the room: select one and press Enter to auto-follow them.
    Follow,
    /// The companion vendor at a capital Stable: select a beast and Enter to buy
    /// it; `x` feeds (heals/raises) the companion you already have.
    Stable,
    /// The Animal Taming panel: the tameable wild beasts roaming this room, each
    /// with its required Taming level and your odds. Select one and Enter to
    /// attempt the tame. Opened with `q` where a tameable beast is present.
    Taming,
    /// The housing ledger: buy a deed at the clerk, or (inside a home you own)
    /// buy and place a furnishing. `Enter` activates the selected row.
    Housing,
    /// The appearance/bio builder: pick a field with the cursor, `Enter` cycles
    /// its option forward and `x` cycles back.
    Appearance,
    /// The crafting panel at a station: select a recipe and `Enter` to make it.
    Crafting,
    /// The waystone fast-travel menu: pick a destination and `Enter` to step
    /// through to it.
    Portal,
    /// The whole-world atlas: exploration progress per region (read-only,
    /// scrollable with `[` / `]`). Toggled with `m`.
    Map,
    /// The leaderboard: top adventurers currently online, by level, pvp
    /// kills, and gold (read-only, scrollable with `[` / `]`). Toggled
    /// with `!` (not `?`, which late.sh reserves globally for a cross-door
    /// help overlay).
    Leaderboard,
    /// A quest board's postings: ready-to-claim counter-bounties and bounties
    /// still open to accept, in one explicit picker. Opened by choosing the
    /// board feature in the Examine panel (there's no key left to spare for
    /// a dedicated binding - every letter and the sensible symbols are
    /// already taken).
    Board,
}

/// A combat action a player can trigger by clicking its on-screen chip, mapping
/// one-to-one to a key: [`ClickAction::Attack`] is space/x, [`ClickAction::Quaff`]
/// is Q, [`ClickAction::Flee`] is z, and [`ClickAction::Ability`] is the digit of
/// that action-bar slot. The mouse handler resolves a click to one of these and
/// then calls the very same method the key would.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickAction {
    Attack,
    Quaff,
    Flee,
    Ability(u8),
    /// Lock onto the foe with this spawn id (a click on its roster row).
    AttackMob(u32),
    /// Lock onto a hostile adventurer (a click on their roster row in a
    /// `pvp` room's "Adventurers here" list).
    AttackPlayer(Uuid),
}

/// The first recorded chip whose rect contains cell `(x, y)`. Pure so the click
/// geometry can be unit-tested without standing up a whole `State`.
fn hit_at(hits: &[(Rect, ClickAction)], x: u16, y: u16) -> Option<ClickAction> {
    hits.iter()
        .find(|(r, _)| {
            x >= r.x
                && x < r.x.saturating_add(r.width)
                && y >= r.y
                && y < r.y.saturating_add(r.height)
        })
        .map(|(_, action)| *action)
}

/// Whether a leave-confirmation deadline is still live at `now`. Pure so the
/// "press Esc twice to leave" window logic can be unit-tested without
/// standing up a whole `State` (which needs a real service to construct).
fn is_leave_confirm_pending(until: Option<Instant>, now: Instant) -> bool {
    until.is_some_and(|deadline| now < deadline)
}

/// A memoised route: the `(standing in, heading for)` pair it was computed for,
/// and the walk it produced (`None` when no known-ground route exists).
type CachedRoute = ((RoomId, RoomId), Option<Route>);

/// The two pages of the `m` map. `m` cycles closed -> Field -> Lands -> closed,
/// so one key walks the whole map from where your feet are to how the world
/// hangs together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapMode {
    /// The room-level overhead field: your own neighbourhood, one land at a time.
    Field,
    /// The land graph: every country and the roads between them, no bosses and
    /// no gates. The question it answers is "how do I get there".
    Lands,
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
    /// Absolute screen rects of the combat action-bar chips, recorded fresh each
    /// draw so a mouse click can resolve to the same action as its key. Interior-
    /// mutable because the render pass only holds `&State`.
    combat_hits: RefCell<Vec<(Rect, ClickAction)>>,
    /// Category headers the player has folded in the collapsible list panels
    /// (crafting / inventory / shop), by prefixed key (e.g. `"inv:Weapons"`).
    /// Session-only; folds a long list down to its category headers.
    collapsed: std::collections::HashSet<String>,
    joined: bool,
    join_pending: bool,
    join_requested_at: Instant,
    reset_version: u64,
    reset_elsewhere: bool,
    /// The chat line being composed, if the player is typing (Some = compose
    /// mode captures keys). Chat is world-local via the service's `say`, so it
    /// never leaks into late.sh's global feed.
    chat_buffer: Option<String>,
    /// Set by a first Esc press outside of chat: the deadline by which a
    /// confirming second Esc must land to actually leave Lateania (see
    /// `arm_leave_confirm`/`confirm_leave`). A single stray Esc - an easy
    /// slip in a persistent world - must never instantly drop a player out.
    leave_confirm_until: Option<Instant>,
    /// Where the overhead world map (Panel::Map) is looking, relative to the
    /// player. Reset whenever the panel changes, so opening the map always
    /// re-centres on them.
    map_camera: MapCamera,
    /// Which page of the map `m` is showing. Always reopens on the field, so
    /// `m` means the same thing every time it is pressed from the room.
    map_mode: MapMode,
    /// A room the player has marked to travel back to (`x` on the map's
    /// crosshair, or Enter on a journal quest row). Local to the session and
    /// never persisted: it is a note to oneself, not world truth.
    map_dest: Option<RoomId>,
    /// Whether the world map overlays active-quest targets (`!` markers and
    /// border arrows). Toggled with `q` while the map is open; on by default.
    map_quests: bool,
    /// The last route computed, keyed by the (standing in, heading for) pair it
    /// was computed for. A route only changes when one of those two changes, so
    /// caching on that pair keeps the walk off the render path: the panel is
    /// redrawn on every keystroke and every snapshot, but the search runs once
    /// per room actually entered.
    route_cache: RefCell<Option<CachedRoute>>,
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
            combat_hits: RefCell::new(Vec::new()),
            collapsed: std::collections::HashSet::new(),
            joined: true,
            join_pending: true,
            join_requested_at,
            reset_version,
            reset_elsewhere: false,
            chat_buffer: None,
            leave_confirm_until: None,
            map_camera: MapCamera::default(),
            map_mode: MapMode::Field,
            map_dest: None,
            map_quests: true,
            route_cache: RefCell::new(None),
        };
        state.svc.join_task(user_id, session_id);
        state
    }

    /// Returns true when the visible state moved: a world snapshot landed,
    /// a remote reset kicked this session, or the pending join resolved.
    pub fn tick(&mut self) -> bool {
        let mut changed = false;
        if self.snapshot_rx.has_changed().unwrap_or(false) {
            self.snapshot = self.snapshot_rx.borrow_and_update().clone();
            changed = true;
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
            return true;
        }
        if self.snapshot.players.contains_key(&self.user_id) && self.join_pending {
            self.join_pending = false;
            changed = true;
        }
        changed
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
            self.map_camera.recenter();
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
        self.map_camera.recenter();
    }

    /// True when the graphical overhead world map is the active panel.
    pub fn map_open(&self) -> bool {
        self.panel == Panel::Map
    }

    /// Which page of the map is showing.
    pub fn map_mode(&self) -> MapMode {
        self.map_mode
    }

    /// `m`: closed -> the overhead field -> the land graph -> closed. One key
    /// walks the whole map, from the ground under your feet out to how the
    /// countries hang together.
    pub fn cycle_map(&mut self) {
        match (self.panel == Panel::Map, self.map_mode) {
            (false, _) => {
                self.map_mode = MapMode::Field;
                self.set_panel(Panel::Map);
            }
            (true, MapMode::Field) => {
                self.map_mode = MapMode::Lands;
                self.cursor = 0;
                self.list_scroll.set(0);
            }
            (true, MapMode::Lands) => {
                self.map_mode = MapMode::Field;
                self.set_panel(Panel::Room);
            }
        }
    }

    /// Flip between the live-map RPG view and the plain text MUD view. The
    /// preference lives on the character (persisted), so this routes through the
    /// service; the next snapshot carries the new value into the view.
    pub fn toggle_rpg_mode(&mut self) {
        if self.ensure_player_present() {
            self.svc.toggle_rpg_mode_task(self.user_id);
        }
    }

    /// Where the world map is looking, relative to the player.
    pub fn map_camera(&self) -> MapCamera {
        self.map_camera
    }

    /// Where the player's own room sits in the coordinate field, if it has one.
    /// Reads the snapshot directly rather than through `view()`, which clones
    /// the whole PlayerView.
    pub fn player_coord(&self) -> Option<Coord> {
        let room = self.snapshot.players.get(&self.user_id)?.room?;
        super::worldmap::world_coords().get(&room).copied()
    }

    /// Pan the world-map camera (arrow / wasd while the map is open).
    pub fn pan_map(&mut self, dx: i32, dy: i32) {
        let Some(player) = self.player_coord() else {
            return;
        };
        self.map_camera
            .pan(player, super::worldmap::bounds(), dx, dy);
    }

    /// Re-centre the world-map camera on the player (position and level).
    pub fn recenter_map(&mut self) {
        self.map_camera.recenter();
    }

    /// Move the viewed world-map level up (+1) or down (-1).
    pub fn change_map_level(&mut self, delta: i32) {
        let Some(player) = self.player_coord() else {
            return;
        };
        self.map_camera
            .change_level(player, super::worldmap::bounds(), delta);
    }

    /// The room under the map's crosshair, resolved the same way the canvas
    /// resolves it, or None when the cursor sits on blank or fog.
    fn cursor_room(&self) -> Option<RoomId> {
        let player_room = self.snapshot.players.get(&self.user_id)?.room?;
        let at = self.map_camera.center(self.player_coord()?);
        let visited = &self.snapshot.players.get(&self.user_id)?.visited;
        super::worldmap::room_at(super::worldmap::world_coords(), at, visited, player_room)
    }

    /// Mark (or unmark) the room under the map crosshair as where the player is
    /// trying to get to. Marking the room already marked clears it, so one key
    /// both sets and cancels.
    pub fn toggle_map_dest(&mut self) {
        let picked = self.cursor_room();
        self.map_dest = match (picked, self.map_dest) {
            (Some(room), Some(current)) if room == current => None,
            (picked, _) => picked,
        };
        self.route_cache.replace(None);
    }

    /// The room the player marked, for drawing it on the map.
    pub fn dest_room(&self) -> Option<RoomId> {
        self.map_dest
    }

    /// Whether the map overlays active-quest targets.
    pub fn map_quests(&self) -> bool {
        self.map_quests
    }

    /// Flip the map's quest overlay (`q` while the map is open).
    pub fn toggle_map_quests(&mut self) {
        self.map_quests = !self.map_quests;
    }

    /// Track (or untrack) a quest's target room from the journal: Enter on a
    /// row with a target marks it exactly like `x` on the map's crosshair, so
    /// the compass line under the exits starts guiding toward it.
    fn toggle_quest_track(&mut self, target: RoomId) {
        self.map_dest = match self.map_dest {
            Some(current) if current == target => None,
            _ => Some(target),
        };
        self.route_cache.replace(None);
    }

    /// Where the player marked they're going, and how to get there from the
    /// room they're standing in right now. None when nothing is marked.
    pub fn heading(&self) -> Option<Heading> {
        let dest = self.map_dest?;
        let name = super::worldmap::room_name(dest)?;
        let player = self.snapshot.players.get(&self.user_id)?;
        let here = player.room?;
        if here == dest {
            return Some(Heading::Arrived(name));
        }
        let mut cache = self.route_cache.borrow_mut();
        let route = match *cache {
            Some((key, route)) if key == (here, dest) => route,
            _ => {
                let route = super::worldmap::route(here, dest, &player.visited);
                *cache = Some(((here, dest), route));
                route
            }
        };
        Some(match route {
            Some(route) => Heading::Toward(name, route),
            // Marked, reachable once, but no walk over known ground gets there
            // from here now. Say so rather than showing a confident direction.
            None => Heading::Unreachable(name),
        })
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

    /// Crafting rows: collapsible skill headers + the recipes of expanded skills.
    pub fn craft_rows(&self) -> Vec<super::svc::SectionRow> {
        self.view()
            .crafting
            .map(|c| c.rows(&self.collapsed))
            .unwrap_or_default()
    }

    /// Inventory rows: items grouped under collapsible category headers
    /// (Weapons / Armor / Consumables / Valuables).
    pub fn inv_rows(&self) -> Vec<super::svc::SectionRow> {
        let inv = self.view().inventory;
        super::svc::section_rows(
            inv.len(),
            |i| {
                let cat = inv[i].category;
                (format!("inv:{cat}"), cat.to_string())
            },
            &self.collapsed,
        )
    }

    /// Shop rows: stock grouped under the same collapsible category headers.
    pub fn shop_rows(&self) -> Vec<super::svc::SectionRow> {
        let Some(shop) = self.view().shop else {
            return Vec::new();
        };
        super::svc::section_rows(
            shop.entries.len(),
            |i| {
                let cat = shop.entries[i].category;
                (format!("shop:{cat}"), cat.to_string())
            },
            &self.collapsed,
        )
    }

    /// The section rows for whichever collapsible panel is active (else empty).
    fn active_rows(&self) -> Vec<super::svc::SectionRow> {
        match self.panel {
            Panel::Crafting => self.craft_rows(),
            Panel::Inventory => self.inv_rows(),
            Panel::Shop => self.shop_rows(),
            _ => Vec::new(),
        }
    }

    /// Fold or unfold a category header, keeping the cursor on that header so the
    /// view doesn't jump.
    fn toggle_section(&mut self, key: String) {
        use super::svc::SectionRow;
        if !self.collapsed.remove(&key) {
            self.collapsed.insert(key.clone());
        }
        if let Some(i) = self
            .active_rows()
            .iter()
            .position(|r| matches!(r, SectionRow::Header { key: k, .. } if *k == key))
        {
            self.cursor = i;
        }
    }

    /// Current list length for whichever list panel is active (for cursor clamp).
    fn list_len(&self) -> usize {
        match self.panel {
            Panel::Abilities => self.view().abilities.len(),
            Panel::Examine => self.view().features.len(),
            Panel::Titles => self.view().titles.len(),
            Panel::Follow => self.view().occupants.len(),
            Panel::Stable => self.view().stable.map(|s| s.entries.len()).unwrap_or(0),
            Panel::Taming => self.view().taming.map(|t| t.entries.len()).unwrap_or(0),
            Panel::Housing => self.view().housing.map(|h| h.entries.len()).unwrap_or(0),
            Panel::Portal => self.view().portal.map(|p| p.entries.len()).unwrap_or(0),
            Panel::Board => self.view().board.map(|b| b.entries.len()).unwrap_or(0),
            // The journal cursor walks the active quests and then the Long
            // Road's milestones, so the panel's long tail stays reachable.
            Panel::Quests => self.view().quests.len() + self.view().road.len(),
            Panel::Appearance => self.view().appearance.len(),
            // These panels' cursors walk headers + visible items, not the raw list.
            Panel::Inventory | Panel::Shop | Panel::Crafting => self.active_rows().len(),
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

    /// Place an earned attribute point on the `choice`-th score (point screen 1-6).
    pub fn spend_score_point(&mut self, choice: usize) {
        if self.ensure_player_present() {
            self.svc.spend_score_point_task(self.user_id, choice);
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

    /// Work a resource node in the current room (chop/mine/fish/forage/skin).
    pub fn gather(&mut self) {
        if self.ensure_player_present() {
            self.svc.gather_task(self.user_id);
        }
    }

    // ---- Local chat (say) ----------------------------------------------
    //
    // Composing a line captures keystrokes until Enter (send) or Esc (cancel).
    // Sending routes through the service's `say`, which is scope-aware: a
    // leading `/z`/`/zone` reaches everyone in the same named zone, `/w`/
    // `/world` reaches every adventurer in Lateania, and no marker means the
    // room, same as it always has. Whichever scope, this is still world-local
    // chat - it never reaches late.sh's own global feed.

    /// True while the player is typing a chat line (input capture is active).
    pub fn chat_active(&self) -> bool {
        self.chat_buffer.is_some()
    }

    /// How long a first Esc press keeps the "press again to leave" window
    /// open (see `arm_leave_confirm`).
    const LEAVE_CONFIRM_SECS: u64 = 6;

    /// True while a first Esc press is waiting on a confirming second one.
    /// The title bar shows a warning for as long as this is true. Factored
    /// out as a pure function of the deadline so it can be unit-tested
    /// without a live `State` (which needs a real service to construct).
    pub fn leave_confirm_pending(&self) -> bool {
        is_leave_confirm_pending(self.leave_confirm_until, Instant::now())
    }

    /// Arm the leave-confirmation window: called on a first Esc press
    /// outside of chat compose. Any key other than a confirming second Esc
    /// just lets the window lapse on its own.
    pub fn arm_leave_confirm(&mut self) {
        self.leave_confirm_until =
            Some(Instant::now() + Duration::from_secs(Self::LEAVE_CONFIRM_SECS));
    }

    /// Consume the confirmation window: true only if it was armed and still
    /// live, meaning this Esc is the confirming second press that should
    /// actually leave Lateania.
    pub fn confirm_leave(&mut self) -> bool {
        let confirmed = self.leave_confirm_pending();
        self.leave_confirm_until = None;
        confirmed
    }

    /// The line being composed, for the input prompt (None when not composing).
    pub fn chat_text(&self) -> Option<&str> {
        self.chat_buffer.as_deref()
    }

    /// Begin composing a chat line.
    pub fn open_chat(&mut self) {
        if self.chat_buffer.is_none() {
            self.chat_buffer = Some(String::new());
        }
    }

    /// Discard the line being composed.
    pub fn chat_cancel(&mut self) {
        self.chat_buffer = None;
    }

    /// Append a typed character to the chat line (capped so it can't run away).
    pub fn chat_push(&mut self, c: char) {
        if let Some(buf) = self.chat_buffer.as_mut()
            && buf.chars().count() < 200
        {
            buf.push(c);
        }
    }

    /// Delete the last character of the chat line.
    pub fn chat_backspace(&mut self) {
        if let Some(buf) = self.chat_buffer.as_mut() {
            buf.pop();
        }
    }

    /// Send the composed line as local speech, then close compose mode.
    pub fn chat_send(&mut self) {
        if let Some(buf) = self.chat_buffer.take() {
            let msg = buf.trim().to_string();
            if !msg.is_empty() && self.ensure_player_present() {
                self.svc.say_task(self.user_id, msg);
            }
        }
    }

    /// Speak the word of recall: warp back to Embergate's Town Square.
    pub fn recall(&mut self) {
        if self.ensure_player_present() {
            self.svc.recall_task(self.user_id);
        }
    }

    /// Fix a personal waypoint at the current room.
    pub fn set_waypoint(&mut self) {
        if self.ensure_player_present() {
            self.svc.set_waypoint_task(self.user_id);
        }
    }

    /// Warp to the marked personal waypoint, from anywhere.
    pub fn warp_to_waypoint(&mut self) {
        if self.ensure_player_present() {
            self.svc.warp_to_waypoint_task(self.user_id);
        }
    }

    /// Retreat to the nearest safe haven (out of combat only).
    pub fn retreat(&mut self) {
        if self.ensure_player_present() {
            self.svc.retreat_task(self.user_id);
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

    /// Mount or dismount the companion (Wildbound rideable beasts).
    pub fn toggle_mount(&mut self) {
        if self.ensure_player_present() {
            self.svc.toggle_mount_task(self.user_id);
        }
    }

    /// Quaff the best healing potion without leaving the combat view, so you can
    /// keep an eye on both health bars instead of opening the inventory panel.
    pub fn quaff(&mut self) {
        if self.ensure_player_present() {
            self.svc.quaff_task(self.user_id);
        }
    }

    /// Drop last frame's action-bar hit-map. Called at the top of every draw so a
    /// bar that isn't shown this frame (map open, etc.) leaves nothing clickable.
    pub fn clear_combat_hits(&self) {
        self.combat_hits.borrow_mut().clear();
    }

    /// Record the absolute screen rect of one action-bar chip during draw.
    pub fn record_combat_hit(&self, rect: Rect, action: ClickAction) {
        self.combat_hits.borrow_mut().push((rect, action));
    }

    /// The action whose chip covers cell `(x, y)`, if a click landed on one.
    pub fn combat_hit_at(&self, x: u16, y: u16) -> Option<ClickAction> {
        hit_at(&self.combat_hits.borrow(), x, y)
    }

    /// Perform a click-resolved combat action (routes to the same method its key
    /// does). Returns whether a chip was actually hit.
    pub fn click_combat(&mut self, x: u16, y: u16) -> bool {
        let Some(action) = self.combat_hit_at(x, y) else {
            return false;
        };
        match action {
            ClickAction::Attack => self.attack(),
            ClickAction::Quaff => self.quaff(),
            ClickAction::Flee => self.flee(),
            ClickAction::Ability(slot) => self.use_ability(slot),
            ClickAction::AttackMob(mob_id) => self.attack_mob(mob_id),
            ClickAction::AttackPlayer(target_id) => self.attack_player(target_id),
        }
        true
    }

    /// Lock onto a specific foe (a click on its roster row) and start trading
    /// blows; the combat tick carries it from there, same as a plain attack.
    pub fn attack_mob(&mut self, mob_id: u32) {
        if self.ensure_player_present() {
            self.svc.engage_mob_task(self.user_id, mob_id);
        }
    }

    /// Lock onto a hostile adventurer in a `pvp` room (a click on their
    /// roster row) and start duelling; the combat tick carries it from there.
    pub fn attack_player(&mut self, target_id: Uuid) {
        if self.ensure_player_present() {
            self.svc.engage_player_task(self.user_id, target_id);
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

    /// Open the Animal Taming panel (only meaningful where a tameable beast roams).
    pub fn open_taming(&mut self) {
        self.toggle_panel(Panel::Taming);
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
                use super::svc::SectionRow;
                match self.inv_rows().get(self.cursor).cloned() {
                    Some(SectionRow::Header { key, .. }) => self.toggle_section(key),
                    Some(SectionRow::Item { index }) => {
                        if let Some(row) = self.view().inventory.get(index) {
                            match inv_action(row) {
                                InvAction::Unequip => {
                                    self.svc.unequip_task(self.user_id, row.item_id)
                                }
                                InvAction::Equip => self.svc.equip_task(self.user_id, row.item_id),
                                InvAction::Use => self.svc.use_item_task(self.user_id, row.item_id),
                            }
                        }
                    }
                    None => {}
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
                use super::svc::SectionRow;
                match self.shop_rows().get(self.cursor).cloned() {
                    Some(SectionRow::Header { key, .. }) => self.toggle_section(key),
                    Some(SectionRow::Item { index }) => {
                        if let Some(shop) = self.view().shop
                            && let Some(entry) = shop.entries.get(index)
                        {
                            self.svc.buy_task(self.user_id, entry.item_id);
                        }
                    }
                    None => {}
                }
            }
            Panel::Examine => {
                // Every feature reveals its description when looked at, boards
                // included - that's the "look at things" rule. A board then
                // also opens its picker on top, because choosing what to accept
                // or claim must be an explicit decision rather than a blind
                // draw (the same shape as the Portal's look + fast-travel menu).
                let is_board = self
                    .view()
                    .features
                    .get(self.cursor)
                    .is_some_and(|f| f.kind == "board");
                self.svc.interact_task(self.user_id, self.cursor);
                if is_board {
                    self.set_panel(Panel::Board);
                }
            }
            Panel::Board => {
                if let Some(board) = self.view().board
                    && let Some(entry) = board.entries.get(self.cursor)
                {
                    if entry.ready {
                        self.svc.claim_board_task(self.user_id, entry.quest_id);
                    } else {
                        self.svc.accept_board_task(self.user_id, entry.quest_id);
                    }
                }
            }
            Panel::Titles => self.svc.set_active_title_task(self.user_id, self.cursor),
            Panel::Quests => {
                // Track the highlighted quest - or Long Road crown - on the
                // compass/map (or untrack it if it is already the marked
                // destination). Rows without a single meaningful place stay
                // inert.
                let target = {
                    let view = self.view();
                    let quests = view.quests.len();
                    if self.cursor < quests {
                        view.quests.get(self.cursor).and_then(|q| q.target)
                    } else {
                        view.road.get(self.cursor - quests).and_then(|s| s.target)
                    }
                };
                if let Some(target) = target {
                    self.toggle_quest_track(target);
                }
            }
            Panel::Follow => self.follow_selected(),
            Panel::Stable => {
                if let Some(stable) = self.view().stable
                    && let Some(entry) = stable.entries.get(self.cursor)
                {
                    self.svc.buy_pet_task(self.user_id, entry.key.clone());
                }
            }
            Panel::Taming => {
                if let Some(taming) = self.view().taming
                    && let Some(entry) = taming.entries.get(self.cursor)
                {
                    self.svc.tame_task(self.user_id, entry.idx);
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
            Panel::Crafting => {
                use super::svc::SectionRow;
                match self.craft_rows().get(self.cursor).cloned() {
                    // On a skill header: fold or unfold that category.
                    Some(SectionRow::Header { key, .. }) => self.toggle_section(key),
                    // On a recipe: craft it.
                    Some(SectionRow::Item { index }) => {
                        if let Some(cr) = self.view().crafting
                            && let Some(e) = cr.entries.get(index)
                        {
                            self.svc.craft_task(self.user_id, e.recipe);
                        }
                    }
                    None => {}
                }
            }
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
            use super::svc::SectionRow;
            // The cursor walks category headers + items; only an item row sells.
            if let Some(SectionRow::Item { index }) = self.inv_rows().get(self.cursor).cloned()
                && let Some(row) = self.view().inventory.get(index)
            {
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

/// What Enter does to a row of the inventory panel. The panel lists worn gear
/// alongside loose gear, so the row's own state picks the verb.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvAction {
    /// Take off worn gear and put it back in the pack.
    Unequip,
    /// Put on a piece of gear from the pack.
    Equip,
    /// Drink/eat/apply a consumable.
    Use,
}

pub fn inv_action(row: &super::svc::InvView) -> InvAction {
    match (row.equipped, row.slot.is_some()) {
        (true, _) => InvAction::Unequip,
        (false, true) => InvAction::Equip,
        (false, false) => InvAction::Use,
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
