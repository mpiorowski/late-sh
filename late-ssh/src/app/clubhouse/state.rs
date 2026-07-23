//! Per-session clubhouse view state. The crowd itself lives in the shared
//! [`lobby`](super::lobby): every active human holds a seat until their
//! first step frees it, walkers carry live positions, and every session
//! renders the same room. This struct owns the session-local bits: the
//! camera target (your own cell, mirrored from the lobby), animation clock,
//! the latest lobby snapshot, door arrival/departure ambience, and the
//! first-visit tutorial state machine.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};

use late_core::models::chat_message::ChatMessage;
use uuid::Uuid;

use super::lobby::{Emote, LobbySnapshot, SharedLobby};
use super::map;

/// Refresh the roster from the active-users map once a second (15 ticks).
const ROSTER_REFRESH_TICKS: u64 = 15;
/// How long a door ambience line lingers, in ticks (~5s).
const DOOR_EVENT_TICKS: u64 = 75;
/// How many ambience lines can stack by the door.
const DOOR_EVENT_MAX: usize = 4;
/// How long a bartender banner line holds when nothing waits behind it
/// (~14s, same reading budget the banner always had).
const BANNER_FULL_TICKS: u64 = 212;
/// Minimum hold per line while more are queued (~6s): long enough to read
/// three sanitized lines, short enough that a busy bar keeps moving.
const BANNER_QUEUE_DWELL_TICKS: u64 = 90;
/// Lines older than this never enqueue, so returning to the screen (or
/// connecting fresh) replays only the recent beat, not the night's backlog.
const BANNER_ENQUEUE_MAX_AGE_MS: i64 = 15_000;
/// Waiting lines beyond this drop oldest-first; nobody wants the answer to
/// a question from a minute ago crawling through the banner.
const BANNER_QUEUE_MAX: usize = 8;

/// A live human from the active-users map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occupant {
    pub user_id: Uuid,
    pub username: String,
}

/// A clickable person from the last render, in absolute terminal cells.
/// Published by the renderer (which only holds `&State`) so a mouse click
/// can be resolved back to a user and open their profile, the same view as
/// `/profile <name>`.
#[derive(Debug, Clone)]
pub struct ClubhouseHit {
    pub user_id: Uuid,
    pub username: String,
    pub x0: u16,
    pub y0: u16,
    pub x1: u16,
    pub y1: u16,
}

impl ClubhouseHit {
    fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
}

/// `* name slipped in` / `* name headed out`, shown near the door.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoorEvent {
    pub username: String,
    pub arrived: bool,
    pub until_tick: u64,
}

/// Where a banner line's text comes from. `Lounge` lines are his real #lounge
/// messages, resolved against the tail at draw time; `Local` lines are client
/// side only (the tutorial welcome), so nobody else in the tavern sees them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BannerLine {
    Lounge(Uuid),
    Local(String),
}

/// The bartender line currently pinned in the banner.
#[derive(Debug, Clone)]
struct BannerEntry {
    line: BannerLine,
    shown_tick: u64,
}

/// The first-visit walkthrough. `Pending` arms it until the screen is first
/// opened; it ends by walking up to the bartender (no Esc skip, so a stray
/// keypress can't cut it short), and `Done` is persisted once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tutorial {
    /// Nothing to run (returning user).
    Off,
    /// Armed, fires on the first clubhouse entry this session.
    Pending,
    /// Box over your head at the door: how to walk, go see the bartender.
    Welcome,
    /// Walking; a hint points at the bar until you reach it.
    GoToBar,
    /// At the bar: the chat lesson popup.
    BarLesson,
    /// Last box: the landmarks and Ctrl+O, then you're on your own.
    SendOff,
    Done,
}

#[derive(Debug)]
pub struct State {
    pub player_x: u16,
    pub player_y: u16,
    pub anim_tick: u64,
    lobby: Option<SharedLobby>,
    /// Latest crowd view, cloned from the lobby every tick while on screen.
    pub snapshot: LobbySnapshot,
    user_id: Uuid,
    username: String,
    pub graybeard_online: bool,
    pub bartender_online: bool,
    last_roster_tick: u64,
    force_roster_refresh: bool,
    /// Roster ids from the last refresh, for arrival/departure diffs.
    seen: HashSet<Uuid>,
    /// The first refresh only primes `seen`; it must not announce the whole
    /// room as arrivals.
    seen_primed: bool,
    pub door_events: VecDeque<DoorEvent>,
    pub tutorial: Tutorial,
    /// The bartender banner plays his lines one at a time: the pinned line,
    /// the ids waiting their turn, and the newest `created` already taken
    /// from the tail (so each line enqueues exactly once).
    banner_current: Option<BannerEntry>,
    banner_queue: VecDeque<BannerLine>,
    banner_watermark: Option<chrono::DateTime<chrono::Utc>>,
    /// Clickable avatar/label boxes from the last render, for opening
    /// profiles on click. Interior-mutable so `ui::draw` can publish it
    /// while holding only a shared borrow of this state.
    hit_layout: RefCell<Vec<ClubhouseHit>>,
}

impl State {
    pub fn new(
        lobby: Option<SharedLobby>,
        user_id: Uuid,
        username: String,
        tutorial_pending: bool,
    ) -> Self {
        Self {
            player_x: map::SPAWN.0,
            player_y: map::SPAWN.1,
            anim_tick: 0,
            lobby,
            snapshot: LobbySnapshot::default(),
            user_id,
            username,
            graybeard_online: false,
            bartender_online: false,
            last_roster_tick: 0,
            force_roster_refresh: false,
            seen: HashSet::new(),
            seen_primed: false,
            door_events: VecDeque::new(),
            banner_current: None,
            banner_queue: VecDeque::new(),
            banner_watermark: None,
            hit_layout: RefCell::new(Vec::new()),
            tutorial: if tutorial_pending {
                Tutorial::Pending
            } else {
                Tutorial::Off
            },
        }
    }

    /// Advance the animation clock and expire door ambience. Called every
    /// world tick.
    pub fn tick(&mut self, _on_screen: bool) {
        self.anim_tick = self.anim_tick.wrapping_add(1);
        let now = self.anim_tick;
        self.door_events.retain(|e| e.until_tick > now);
    }

    /// Screen entry hook: refresh the crowd immediately and, on the very
    /// first visit ever, start the tutorial at the door.
    pub fn enter_screen(&mut self) {
        self.force_roster_refresh = true;
        if self.tutorial == Tutorial::Pending {
            self.tutorial = Tutorial::Welcome;
            if let Some(lobby) = &self.lobby {
                lobby.place(self.user_id, &self.username, map::SPAWN.0, map::SPAWN.1);
            }
            self.player_x = map::SPAWN.0;
            self.player_y = map::SPAWN.1;
        }
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

    /// Reconcile the shared lobby with a fresh human roster (including this
    /// session's user) and record arrival/departure ambience.
    pub fn refresh_roster(&mut self, roster: Vec<Occupant>) {
        if let Some(own) = roster.iter().find(|o| o.user_id == self.user_id) {
            self.username = own.username.clone();
        }

        let ids: HashSet<Uuid> = roster.iter().map(|o| o.user_id).collect();
        if self.seen_primed {
            for who in &roster {
                if !self.seen.contains(&who.user_id) && who.user_id != self.user_id {
                    self.push_door_event(who.username.clone(), true);
                }
            }
            // Departures need the old names; look them up in the previous
            // snapshot before it is replaced.
            let departed: Vec<String> = self
                .seen
                .difference(&ids)
                .filter_map(|gone| self.snapshot.find(*gone))
                .map(|p| p.username.clone())
                .collect();
            for name in departed {
                self.push_door_event(name, false);
            }
        }
        self.seen = ids;
        self.seen_primed = true;

        if let Some(lobby) = &self.lobby {
            let pairs: Vec<(Uuid, String)> = roster
                .into_iter()
                .map(|o| (o.user_id, o.username))
                .collect();
            lobby.sync(&pairs);
        }
    }

    /// Pull the latest crowd view and mirror our own cell for the camera.
    /// Called every world tick while the screen is visible.
    pub fn refresh_snapshot(&mut self) {
        let Some(lobby) = &self.lobby else {
            return;
        };
        self.snapshot = lobby.snapshot();
        if let Some(own) = self.snapshot.find(self.user_id) {
            let (x, y) = own.placement.position();
            self.player_x = x;
            self.player_y = y;
        }
    }

    /// Feed the newest-first #lounge tail into the bartender banner and
    /// advance it. When several patrons ask him at once, his answers used to
    /// overwrite each other the moment they landed; instead they queue, and
    /// each line holds the banner for a minimum dwell before the next takes
    /// over. Called every world tick while the screen is up.
    pub fn update_bartender_banner(
        &mut self,
        bartender_id: Option<Uuid>,
        lounge_messages: &[ChatMessage],
        now: chrono::DateTime<chrono::Utc>,
    ) {
        let Some(bartender_id) = bartender_id else {
            return;
        };
        // Collect his lines above the watermark (the tail is newest-first,
        // so stop at the first already-seen message), then enqueue them
        // oldest-first so answers play in the order he gave them.
        let mut fresh: Vec<&ChatMessage> = lounge_messages
            .iter()
            .take_while(|m| self.banner_watermark.is_none_or(|w| m.created > w))
            .filter(|m| m.user_id == bartender_id)
            .collect();
        if let Some(newest) = fresh.first() {
            self.banner_watermark = Some(newest.created);
        }
        fresh.reverse();
        for message in fresh {
            let age_ms = now
                .signed_duration_since(message.created)
                .num_milliseconds();
            if age_ms > BANNER_ENQUEUE_MAX_AGE_MS {
                continue;
            }
            self.banner_queue.push_back(BannerLine::Lounge(message.id));
        }
        while self.banner_queue.len() > BANNER_QUEUE_MAX {
            self.banner_queue.pop_front();
        }

        let advance = match &self.banner_current {
            None => true,
            Some(entry) => {
                let shown = self.anim_tick.wrapping_sub(entry.shown_tick);
                shown >= BANNER_FULL_TICKS
                    || (!self.banner_queue.is_empty() && shown >= BANNER_QUEUE_DWELL_TICKS)
            }
        };
        if advance {
            self.banner_current = self.banner_queue.pop_front().map(|line| BannerEntry {
                line,
                shown_tick: self.anim_tick,
            });
        }
    }

    /// Pin a client-side line in the bartender banner, ahead of whatever is
    /// queued: the tutorial welcome is the reason the walker is standing at the
    /// bar, so it must not wait behind another patron's answer.
    pub fn show_local_bartender_line(&mut self, line: String) {
        self.banner_current = Some(BannerEntry {
            line: BannerLine::Local(line),
            shown_tick: self.anim_tick,
        });
    }

    /// The bartender line the banner should render right now.
    pub fn bartender_banner_line(&self) -> Option<&BannerLine> {
        self.banner_current.as_ref().map(|e| &e.line)
    }

    fn push_door_event(&mut self, username: String, arrived: bool) {
        if self.door_events.len() >= DOOR_EVENT_MAX {
            self.door_events.pop_front();
        }
        self.door_events.push_back(DoorEvent {
            username,
            arrived,
            until_tick: self.anim_tick.wrapping_add(DOOR_EVENT_TICKS),
        });
    }

    /// True while an arrival is fresh, so the door sign can glow.
    pub fn door_glow(&self) -> bool {
        self.door_events.iter().any(|e| e.arrived)
    }

    /// Try to walk one step; the first step frees your seat in the shared
    /// lobby. Also advances the tutorial off the welcome box.
    pub fn walk(&mut self, dx: i32, dy: i32) {
        if let Some(lobby) = &self.lobby {
            let (x, y) = lobby.walk(self.user_id, &self.username, dx, dy);
            self.player_x = x;
            self.player_y = y;
        } else {
            // Headless/test sessions still walk locally.
            let nx = self.player_x.saturating_add_signed(dx as i16);
            let ny = self.player_y.saturating_add_signed(dy as i16);
            if map::walkable(nx, ny) {
                self.player_x = nx;
                self.player_y = ny;
            }
        }
        if self.tutorial == Tutorial::Welcome {
            self.tutorial = Tutorial::GoToBar;
        }
    }

    /// Take the nearest free seat within reach, standing back up on the next
    /// step. Mirrors our own cell to the seat so the camera follows. Returns
    /// true when we sat (no lobby, or no seat close by, is a no-op).
    pub fn sit(&mut self) -> bool {
        if let Some(lobby) = &self.lobby
            && let Some((x, y)) = lobby.sit(self.user_id, &self.username)
        {
            self.player_x = x;
            self.player_y = y;
            return true;
        }
        false
    }

    pub fn emote(&self, emote: Emote) {
        if let Some(lobby) = &self.lobby {
            lobby.emote(self.user_id, emote);
        }
    }

    pub fn pet_dog(&self) {
        if let Some(lobby) = &self.lobby {
            lobby.pet_dog(&self.username);
        }
    }

    /// The prop within reach of the player, if any. The dog wanders, so
    /// its live cell comes from the lobby snapshot.
    pub fn nearby(&self) -> Option<map::Interactive> {
        let dog = (self.snapshot.dog.x, self.snapshot.dog.y);
        map::nearest_interactive(self.player_x, self.player_y, dog)
    }

    /// Everyone in the room (the lobby roster includes this session's user
    /// once the first refresh lands).
    pub fn headcount(&self) -> usize {
        self.snapshot.headcount().max(1)
    }

    pub fn own_user_id(&self) -> Uuid {
        self.user_id
    }

    /// Publish the clickable people from a render pass (absolute terminal
    /// cells). Called once per frame from `ui::draw`.
    pub fn set_hit_layout(&self, hits: Vec<ClubhouseHit>) {
        *self.hit_layout.borrow_mut() = hits;
    }

    /// The user under a terminal cell, if a click there landed on someone's
    /// avatar or name label in the last frame.
    pub fn hit_test(&self, x: u16, y: u16) -> Option<(Uuid, String)> {
        self.hit_layout
            .borrow()
            .iter()
            .find(|h| h.contains(x, y))
            .map(|h| (h.user_id, h.username.clone()))
    }

    /// Clone the shared lobby handle, if this session is wired to one. Lets an
    /// off-thread task (the welcome pour) push a glow update after its DB write.
    pub fn lobby_handle(&self) -> Option<SharedLobby> {
        self.lobby.clone()
    }

    /// Current drunk levels from the shared lobby (empty on headless/test
    /// paths). Chat author labels tint from this, so it must not hit the DB.
    pub fn drunk_levels(&self) -> HashMap<Uuid, u8> {
        self.lobby
            .as_ref()
            .map(|lobby| lobby.drunk_levels())
            .unwrap_or_default()
    }

    /// GoToBar -> BarLesson when the player reaches the counter. Returns
    /// true exactly once, so the caller can trigger the bartender greeting.
    pub fn tutorial_reached_bar(&mut self) -> bool {
        if self.tutorial == Tutorial::GoToBar && self.nearby() == Some(map::Interactive::Bartender)
        {
            self.tutorial = Tutorial::BarLesson;
            return true;
        }
        false
    }

    /// Advance past the current tutorial popup (Enter). Returns true when
    /// the tutorial just finished and should be persisted.
    pub fn tutorial_advance(&mut self) -> bool {
        match self.tutorial {
            Tutorial::BarLesson => {
                self.tutorial = Tutorial::SendOff;
                false
            }
            Tutorial::SendOff => {
                self.tutorial = Tutorial::Done;
                true
            }
            _ => false,
        }
    }

    /// True while a tutorial popup wants Enter before anything else.
    pub fn tutorial_capturing_keys(&self) -> bool {
        matches!(self.tutorial, Tutorial::BarLesson | Tutorial::SendOff)
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
