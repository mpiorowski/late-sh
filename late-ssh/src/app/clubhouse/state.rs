//! Per-session clubhouse view state. The room is derived from presence
//! (`app/presence`, `crowd.rs`): every logged-in session on every replica
//! sits where it picked, walkers carry live positions, and every session
//! draws the same room. This struct owns this session's part of it (its
//! own stand: the spot it picked or the cell it walked to, its last emote
//! and pet), the camera target (your own cell, mirrored from the crowd),
//! animation clock, the derived crowd, door arrival/departure ambience,
//! and the first-visit tutorial state machine. Pure: every `now_ms` is
//! handed in.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};

use late_core::models::chat_message::ChatMessage;
use late_core::models::presence::{ClubhouseStand, Emote, Spot};
use uuid::Uuid;

use crate::app::common::primitives::Screen;
use crate::app::presence::svc::Records;

use super::crowd::{self, Crowd, Own};
use super::drunk::DrunkMap;
use super::map;

/// Look for the always-on bots in the active-users map once a second (15
/// ticks).
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

/// The first-visit tour. `Pending` arms it until the screen is first opened;
/// then the tour is FORCED: while it runs, the input gate in `app/input.rs`
/// (`handle_tour_gate`) swallows everything except Enter, which moves every
/// stop on (`State::tutorial_advance`), and the quit keys. The route walks
/// every top-level page in number order, stops twice more for the features
/// that have no page of their own (the Stations modal on Home, the Lobby
/// modal on The Arcade, each held open for real), has the newcomer play one
/// break at a practice pool table, passes through Zen, ends back
/// in the tavern, and `Done` is persisted once on the homecoming Enter. The
/// bartender is
/// deliberately absent from the route: his comped welcome pour stays a
/// hidden treasure for whoever walks up to the glowing bar after the
/// send-off (see [`State::welcome_pour_due`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tutorial {
    /// Nothing to run (returning user).
    Off,
    /// Armed, fires on the first clubhouse entry this session.
    Pending,
    /// Centered box at the door: what late.sh is.
    Welcome,
    /// On Home: the chat pitch.
    VisitChat,
    /// Still on Home: the real Stations modal, held open under the music
    /// pitch.
    VisitMusic,
    /// On The Arcade: solo games and chips.
    VisitArcade,
    /// Still on The Arcade: the real Lobby modal, held open under the
    /// multiplayer pitch.
    VisitLobby,
    /// At the practice pool table: one break, in this session's memory only,
    /// which has to be played to move on.
    VisitTable,
    /// On the Games hub: the heavy-door pitch.
    VisitGames,
    /// Still on the Games hub: one scripted fight (`fight::Fight`), which has
    /// to be won to move on.
    VisitDungeon,
    /// On the Artboard: the shared canvas.
    VisitArtboard,
    /// On the Profiles page: people and their projects.
    VisitDirectory,
    /// On the Leaderboards: the last page.
    VisitLeaderboard,
    /// On Zen: the tiling page and its chord.
    VisitZen,
    /// Back in the tavern: the send-off box.
    Homecoming,
    Done,
}

/// What the forced tour accepts right now. Enter moves every stop on; at
/// the practice table it (or Space) first has to play the break, and at the
/// dungeon stop it first has to win the fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TourStep {
    Enter,
    Table,
    Fight,
}

/// Where an Enter took the tour, for the input gate to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TourMove {
    /// The next stop is on the page already open.
    Stay,
    Page(Screen),
    /// To the practice pool table.
    Table,
    /// Into Zen, through the same toggle `Ctrl+F` runs.
    Zen,
    /// The tour just ended and should be persisted.
    Finished,
}

/// The real modal a stop holds open under its pitch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TourModal {
    None,
    Stations,
    Lobby,
}

#[derive(Debug)]
pub struct State {
    pub player_x: u16,
    pub player_y: u16,
    pub anim_tick: u64,
    session_id: Uuid,
    /// This session's part of the room, published through presence.
    own: ClubhouseStand,
    /// Every live presence record, as last copied on the tick.
    records: Records,
    drunk: DrunkMap,
    /// The room as last derived from `records` and `own`.
    pub crowd: Crowd,
    /// Draws a free seat when this session picks one.
    rng: u64,
    user_id: Uuid,
    username: String,
    pub graybeard_online: bool,
    pub bartender_online: bool,
    pub bot_online: bool,
    last_roster_tick: u64,
    force_roster_refresh: bool,
    /// Patrons in the last crowd, for arrival/departure diffs.
    seen: HashSet<Uuid>,
    /// The first crowd only primes `seen`; it must not announce the whole
    /// room as arrivals.
    seen_primed: bool,
    pub door_events: VecDeque<DoorEvent>,
    pub tutorial: Tutorial,
    /// The hidden welcome pour fired this session, so walking back to the
    /// bar doesn't repeat the bartender's scripted welcome. The once-ever
    /// guarantee lives in the DB (`UserDrinks::record_welcome_pour`).
    welcome_pour_claimed: bool,
    /// The dungeon stop's scripted fight.
    pub tour_fight: super::fight::Fight,
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
    /// A session sits down the moment it logs in, wherever it sees a free
    /// seat (or joins this user's other session), so everyone online is in
    /// the room whether or not they ever open the page.
    pub fn new(
        session_id: Uuid,
        user_id: Uuid,
        username: String,
        tutorial_pending: bool,
        records: Records,
        drunk: DrunkMap,
        now_ms: i64,
    ) -> Self {
        let rng = seed_rng(session_id);
        let own = crowd::first_stand(&records, user_id, rng, now_ms);
        let mut state = Self {
            player_x: map::SPAWN.0,
            player_y: map::SPAWN.1,
            anim_tick: 0,
            session_id,
            own,
            records,
            drunk,
            crowd: Crowd::default(),
            rng,
            user_id,
            username,
            graybeard_online: false,
            bartender_online: false,
            bot_online: false,
            last_roster_tick: 0,
            force_roster_refresh: false,
            seen: HashSet::new(),
            seen_primed: false,
            door_events: VecDeque::new(),
            banner_current: None,
            banner_queue: VecDeque::new(),
            banner_watermark: None,
            hit_layout: RefCell::new(Vec::new()),
            welcome_pour_claimed: false,
            tour_fight: super::fight::Fight::new(),
            tutorial: if tutorial_pending {
                Tutorial::Pending
            } else {
                Tutorial::Off
            },
        };
        state.refresh_crowd(now_ms);
        state
    }

    /// Sync the animation clock to the wall-clock world tick (66ms units,
    /// `App::marquee_tick`) and expire door ambience. Called every world
    /// tick. The clock must come from wall time, not a per-call increment:
    /// the adaptive loop ticks sparsely, so counting calls would tie
    /// animation speed to the tick cadence (walking held the hot cadence
    /// and visibly sped the room up 4x).
    pub fn tick(&mut self, wall_tick: u64) {
        self.anim_tick = wall_tick;
        let now = self.anim_tick;
        self.door_events.retain(|e| e.until_tick > now);
    }

    /// Screen entry hook: redraw the room from what arrived while the
    /// screen was away, look for the bots now and, on the very first visit
    /// ever, start the tutorial at the door.
    pub fn enter_screen(&mut self, now_ms: i64) {
        self.force_roster_refresh = true;
        self.refresh_crowd(now_ms);
        if self.tutorial == Tutorial::Pending {
            self.tutorial = Tutorial::Welcome;
            self.place(map::SPAWN, now_ms);
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

    /// Take the latest presence records: follow this user's newer stand on
    /// another device, pick again after losing a contested spot or when a
    /// seat frees up for someone at the door. Cheap on purpose, since every
    /// session runs it on every change anywhere: the room itself is
    /// redrawn by `refresh_crowd`, on the screen only.
    pub fn set_records(&mut self, records: Records, now_ms: i64) {
        self.records = records;
        self.settle(now_ms);
    }

    /// The name this session's own patron is drawn with, before the record
    /// comes back: the live profile name, so a rename reaches it.
    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn set_username(&mut self, username: &str) {
        if self.username != username {
            self.username = username.to_string();
        }
    }

    fn settle(&mut self, now_ms: i64) {
        let newer_elsewhere = self
            .records
            .iter()
            .filter(|record| record.user_id == self.user_id && record.session_id != self.session_id)
            .map(|record| record.clubhouse)
            .filter(|stand| stand.since_ms > self.own.since_ms)
            .max_by_key(|stand| stand.since_ms);
        if let Some(stand) = newer_elsewhere {
            self.own.spot = stand.spot;
            self.own.since_ms = stand.since_ms;
        }
        let lost = crowd::lost_spot(&self.records, &self.own());
        let waiting = self.own.spot == Spot::Door;
        if lost || waiting {
            let rng = self.next_rand();
            let spot = crowd::pick_spot(&self.records, self.user_id, rng);
            if spot != self.own.spot {
                self.own.spot = spot;
                self.own.since_ms = now_ms;
            }
        }
    }

    /// Redraw the room from the records and this session's own stand, note
    /// who came and went, and mirror our own cell for the camera. Called
    /// on every own change, on entering the screen, and every world tick
    /// while the screen is visible (the dog and the emotes run on the
    /// clock); never while the screen is away, where nothing reads the
    /// room. Whoever came and went meanwhile is announced on the return.
    pub fn refresh_crowd(&mut self, now_ms: i64) {
        let now = chrono::DateTime::from_timestamp_millis(now_ms).unwrap_or_default();
        let levels = self.drunk.levels(now);
        let next = crowd::crowd(&self.records, &self.own(), &levels, now_ms);

        let ids: HashSet<Uuid> = next.people.iter().map(|p| p.user_id).collect();
        if self.seen_primed {
            for who in &next.people {
                if !self.seen.contains(&who.user_id) && who.user_id != self.user_id {
                    self.push_door_event(who.username.clone(), true);
                }
            }
            // Departures need the old names; look them up in the previous
            // crowd before it is replaced.
            let departed: Vec<String> = self
                .seen
                .difference(&ids)
                .filter_map(|gone| self.crowd.find(*gone))
                .map(|p| p.username.clone())
                .collect();
            for name in departed {
                self.push_door_event(name, false);
            }
        }
        self.seen = ids;
        self.seen_primed = true;

        if let Some(own) = next.find(self.user_id) {
            let (x, y) = own.placement.position();
            self.player_x = x;
            self.player_y = y;
        }
        self.crowd = next;
    }

    fn own(&self) -> Own<'_> {
        Own {
            session_id: self.session_id,
            user_id: self.user_id,
            username: &self.username,
            stand: self.own,
        }
    }

    /// This session's part of its presence record.
    pub fn stand(&self) -> ClubhouseStand {
        self.own
    }

    fn next_rand(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
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

    /// Stand on the back-door mat (the `n` shortcut to Nightcap), for the
    /// whole room too, so the rest of it sees you head out back instead of
    /// vanishing from your seat.
    pub fn step_to_back_door(&mut self, now_ms: i64) {
        self.place(map::BACK_DOOR_MAT, now_ms);
    }

    /// Stand at an exact cell as a walker (the tutorial's door, the back
    /// door).
    fn place(&mut self, (x, y): (u16, u16), now_ms: i64) {
        self.own.spot = Spot::Walking { x, y };
        self.own.since_ms = now_ms;
        self.player_x = x;
        self.player_y = y;
        self.refresh_crowd(now_ms);
    }

    /// Try to walk one step. The first step stands you up off your seat,
    /// even into a wall.
    pub fn walk(&mut self, dx: i32, dy: i32, now_ms: i64) {
        let (mut x, mut y) = (self.player_x, self.player_y);
        let nx = x.saturating_add_signed(dx as i16);
        let ny = y.saturating_add_signed(dy as i16);
        if map::walkable(nx, ny) {
            (x, y) = (nx, ny);
        }
        self.place((x, y), now_ms);
    }

    /// Take the nearest free seat within reach, standing back up on the next
    /// step. Mirrors our own cell to the seat so the camera follows. Returns
    /// true when we sat (not walking, or no seat close by, is a no-op).
    pub fn sit(&mut self, now_ms: i64) -> bool {
        let Spot::Walking { x, y } = self.own.spot else {
            return false;
        };
        let Some(seat) = self.crowd.free_seat_near(x, y) else {
            return false;
        };
        self.own.spot = Spot::Seat { index: seat as u16 };
        self.own.since_ms = now_ms;
        self.refresh_crowd(now_ms);
        true
    }

    pub fn emote(&mut self, emote: Emote, now_ms: i64) {
        self.own.emote = Some((emote, now_ms));
        self.refresh_crowd(now_ms);
    }

    pub fn pet_dog(&mut self, now_ms: i64) {
        self.own.petted_dog_at_ms = Some(now_ms);
        self.refresh_crowd(now_ms);
    }

    /// The prop within reach of the player, if any. The dog wanders, so
    /// its live cell comes from the crowd.
    pub fn nearby(&self) -> Option<map::Interactive> {
        let dog = (self.crowd.dog.x, self.crowd.dog.y);
        map::nearest_interactive(self.player_x, self.player_y, dog)
    }

    /// Everyone in the room, this session's user included.
    pub fn headcount(&self) -> usize {
        self.crowd.headcount().max(1)
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

    /// The process's drunk map. Lets an off-thread task (the welcome pour)
    /// push a glow update after its DB write.
    pub fn drunk_handle(&self) -> DrunkMap {
        self.drunk.clone()
    }

    /// Current drunk levels. Chat author labels tint from this, so it must
    /// not hit the DB.
    pub fn drunk_levels(&self, now: chrono::DateTime<chrono::Utc>) -> HashMap<Uuid, u8> {
        self.drunk.levels(now)
    }

    /// True when no tour stands between the newcomer and the rest of the
    /// app: none was ever due, or it has been walked. `Pending` (armed,
    /// not yet at the door) is not settled, unlike `tutorial_forced_step`,
    /// which only reports a step that is capturing keys right now. The
    /// daily paper waits on this so it never opens over the walkthrough.
    pub fn tutorial_settled(&self) -> bool {
        matches!(self.tutorial, Tutorial::Off | Tutorial::Done)
    }

    /// The single input the forced tour accepts right now, or `None` when
    /// input is free (no tour, or the tour is done). The gate in
    /// `app/input.rs` swallows everything else while this is `Some`.
    pub fn tutorial_forced_step(&self) -> Option<TourStep> {
        match self.tutorial {
            Tutorial::Off | Tutorial::Pending | Tutorial::Done => None,
            Tutorial::VisitTable => Some(TourStep::Table),
            Tutorial::VisitDungeon => Some(TourStep::Fight),
            Tutorial::Welcome
            | Tutorial::VisitChat
            | Tutorial::VisitMusic
            | Tutorial::VisitArcade
            | Tutorial::VisitLobby
            | Tutorial::VisitGames
            | Tutorial::VisitArtboard
            | Tutorial::VisitDirectory
            | Tutorial::VisitLeaderboard
            | Tutorial::VisitZen
            | Tutorial::Homecoming => Some(TourStep::Enter),
        }
    }

    /// The modal the current stop holds open, which the input gate keeps in
    /// step after every key.
    pub fn tour_modal(&self) -> TourModal {
        match self.tutorial {
            Tutorial::VisitMusic => TourModal::Stations,
            Tutorial::VisitLobby => TourModal::Lobby,
            Tutorial::Off
            | Tutorial::Pending
            | Tutorial::Welcome
            | Tutorial::VisitChat
            | Tutorial::VisitArcade
            | Tutorial::VisitTable
            | Tutorial::VisitGames
            | Tutorial::VisitDungeon
            | Tutorial::VisitArtboard
            | Tutorial::VisitDirectory
            | Tutorial::VisitLeaderboard
            | Tutorial::VisitZen
            | Tutorial::Homecoming
            | Tutorial::Done => TourModal::None,
        }
    }

    /// The hidden treasure: the bartender comps a welcome pour the first
    /// time the newcomer walks up to the counter. Walking only unlocks
    /// after the homecoming Enter (the gate swallows movement mid-tour), so
    /// in practice this fires after the send-off. Returns true exactly once
    /// per session; the once-ever guarantee is the DB insert behind the comp.
    pub fn welcome_pour_due(&mut self) -> bool {
        if self.tutorial != Tutorial::Off
            && !self.welcome_pour_claimed
            && self.nearby() == Some(map::Interactive::Bartender)
        {
            self.welcome_pour_claimed = true;
            return true;
        }
        false
    }

    /// The bar sign pulses once the tour has come home and the welcome pour
    /// is still unclaimed: the only pointer at the hidden treasure.
    pub fn bar_glow(&self) -> bool {
        matches!(self.tutorial, Tutorial::Homecoming | Tutorial::Done) && !self.welcome_pour_claimed
    }

    /// Enter at a tour stop: move to the next one and say where it lives.
    /// The tour never reads the screen back, so nothing but this can move it.
    pub fn tutorial_advance(&mut self) -> TourMove {
        let (next, went) = match self.tutorial {
            Tutorial::Off | Tutorial::Pending | Tutorial::Done => return TourMove::Stay,
            Tutorial::Welcome => (Tutorial::VisitChat, TourMove::Page(Screen::Dashboard)),
            Tutorial::VisitChat => (Tutorial::VisitMusic, TourMove::Stay),
            Tutorial::VisitMusic => (Tutorial::VisitArcade, TourMove::Page(Screen::Arcade)),
            Tutorial::VisitArcade => (Tutorial::VisitLobby, TourMove::Stay),
            Tutorial::VisitLobby => (Tutorial::VisitTable, TourMove::Table),
            Tutorial::VisitTable => (Tutorial::VisitGames, TourMove::Page(Screen::Games)),
            Tutorial::VisitGames => (Tutorial::VisitDungeon, TourMove::Stay),
            Tutorial::VisitDungeon => (Tutorial::VisitArtboard, TourMove::Page(Screen::Artboard)),
            Tutorial::VisitArtboard => (Tutorial::VisitDirectory, TourMove::Page(Screen::Profiles)),
            Tutorial::VisitDirectory => (
                Tutorial::VisitLeaderboard,
                TourMove::Page(Screen::Leaderboard),
            ),
            Tutorial::VisitLeaderboard => (Tutorial::VisitZen, TourMove::Zen),
            Tutorial::VisitZen => (Tutorial::Homecoming, TourMove::Page(Screen::Clubhouse)),
            Tutorial::Homecoming => (Tutorial::Done, TourMove::Finished),
        };
        self.tutorial = next;
        went
    }
}

/// A per-session seed for seat draws, so two sessions arriving together
/// draw different seats.
fn seed_rng(session_id: Uuid) -> u64 {
    let (high, low) = session_id.as_u64_pair();
    match high ^ low.rotate_left(17) {
        0 => 0xA409_3822_299F_31D0,
        seed => seed,
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod state_test;
