//! The process-global clubhouse lobby: one shared, mutex-guarded map of
//! where every active human is in the Late Lounge. Users who have never
//! walked hold an assigned spot (a random free seat, then a standing spot,
//! then the door stack); the first movement key frees the spot and turns
//! them into a walker with a live position that every session renders.
//! Emotes and the dog-pet flourish live here too so everyone sees them, and
//! so does the dog itself: one shared wanderer that trots between the
//! `map::DOG_WAYPOINTS`, naps at each, and holds still when petted or when
//! a walker steps up close. It advances lazily inside `snapshot()`, rate
//! limited by wall clock, so it only moves while somebody is watching.
//!
//! Single-replica by design: this is an in-process `Arc<Mutex<..>>` like the
//! active-users map. If late-ssh ever runs more than one replica, presence
//! needs to move to a shared channel (Postgres LISTEN/NOTIFY or similar).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use late_core::MutexRecover;
use late_core::models::drinks::{decayed_points, drunk_level};
use uuid::Uuid;

use super::map;

/// How long an emote plays for everyone, in milliseconds.
pub const EMOTE_MS: u128 = 3200;
/// How long the dog stays excited after a pet, in milliseconds.
pub const DOG_PET_MS: u128 = 4000;
/// How often the wandering dog takes one step, in milliseconds.
const DOG_STEP_MS: u128 = 600;
/// Nap length range at a reached waypoint, in seconds.
const DOG_NAP_SECS: (u64, u64) = (6, 20);
/// A walker this close (Chebyshev) makes the dog hold still to be petted.
const DOG_FRIEND_RANGE: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emote {
    Wave,
    Dance,
}

/// Where a not-yet-walking user is parked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    Seat(usize),
    Standing(usize),
    /// Overflow stacked at the door; the index is the queue order.
    Door(usize),
}

impl Spot {
    /// The map cell this spot occupies (door slots cycle a small stagger
    /// pattern so stacked patrons stay tellable-apart).
    pub fn position(&self) -> (u16, u16) {
        match *self {
            Spot::Seat(i) => map::SEATS.get(i).map(|s| (s.x, s.y)).unwrap_or(map::SPAWN),
            Spot::Standing(i) => map::STANDING_SPOTS.get(i).copied().unwrap_or(map::SPAWN),
            Spot::Door(i) => map::DOOR_STACK[i % map::DOOR_STACK.len()],
        }
    }
}

#[derive(Debug, Clone)]
struct Parked {
    username: String,
    spot: Spot,
}

#[derive(Debug, Clone)]
struct Walker {
    username: String,
    x: u16,
    y: u16,
}

/// The one shared dog: a live position plus a tiny errand brain (walk to
/// the waypoint, nap there, pick the next one).
#[derive(Debug)]
struct Dog {
    x: u16,
    y: u16,
    facing_left: bool,
    waypoint: (u16, u16),
    /// Napping at a reached waypoint until this instant.
    rest_until: Option<Instant>,
    last_step: Instant,
}

impl Dog {
    fn new() -> Self {
        Self {
            x: map::DOG_HOME.0,
            y: map::DOG_HOME.1,
            facing_left: false,
            waypoint: map::DOG_HOME,
            rest_until: None,
            last_step: Instant::now(),
        }
    }
}

/// A user's raw buzz as last written to the DB; the effective level decays
/// against wall clock at read time (see `late_core::models::drinks`).
#[derive(Debug, Clone, Copy)]
struct DrunkEntry {
    points: i64,
    last_drink_at: DateTime<Utc>,
}

impl DrunkEntry {
    fn level(&self, now: DateTime<Utc>) -> u8 {
        drunk_level(decayed_points(
            self.points,
            (now - self.last_drink_at).num_seconds(),
        ))
    }
}

#[derive(Debug)]
struct LobbyInner {
    parked: HashMap<Uuid, Parked>,
    walkers: HashMap<Uuid, Walker>,
    emotes: HashMap<Uuid, (Emote, Instant)>,
    /// Not pruned on roster sync: a drinker who logs out keeps tinting their
    /// chat history until the buzz decays or the seed task replaces the map.
    drunk: HashMap<Uuid, DrunkEntry>,
    dog_pet: Option<(String, Instant)>,
    dog: Dog,
    rng: u64,
}

/// One rendered person, however they are placed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presence {
    pub user_id: Uuid,
    pub username: String,
    pub placement: Placement,
    pub emote: Option<Emote>,
    /// Current drunk level 0 (sober) through 4 (wasted), already decayed.
    pub drunk_level: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    Seated(usize),
    Standing(usize),
    /// Door-stack slot index (see `map::DOOR_STACK`); slots repeat once the
    /// stack overflows, and the renderer adds a `+N` label for the pile.
    Door(usize),
    Walking(u16, u16),
}

impl Placement {
    pub fn position(&self) -> (u16, u16) {
        match *self {
            Placement::Seated(i) => Spot::Seat(i).position(),
            Placement::Standing(i) => Spot::Standing(i).position(),
            Placement::Door(i) => Spot::Door(i).position(),
            Placement::Walking(x, y) => (x, y),
        }
    }
}

/// The dog as a render view: where it is and how the tail should behave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DogView {
    /// The body-center cell of the `(ᴥ)` sprite.
    pub x: u16,
    pub y: u16,
    /// The tail trails the walking direction.
    pub facing_left: bool,
    /// Napping at a waypoint: slow tail, the occasional `z`.
    pub resting: bool,
}

impl Default for DogView {
    fn default() -> Self {
        Self {
            x: map::DOG_HOME.0,
            y: map::DOG_HOME.1,
            facing_left: false,
            resting: true,
        }
    }
}

/// Everything a session needs to draw the crowd, cloned out per tick.
#[derive(Debug, Clone, Default)]
pub struct LobbySnapshot {
    pub people: Vec<Presence>,
    /// How many door-stack patrons exceed the distinct render slots.
    pub door_overflow: usize,
    /// Milliseconds since the dog was last petted, with the petter's name,
    /// while inside the excitement window.
    pub dog_pet: Option<(String, u128)>,
    pub dog: DogView,
}

impl LobbySnapshot {
    pub fn headcount(&self) -> usize {
        self.people.len()
    }

    pub fn find(&self, user_id: Uuid) -> Option<&Presence> {
        self.people.iter().find(|p| p.user_id == user_id)
    }
}

#[derive(Clone, Debug)]
pub struct SharedLobby {
    inner: Arc<Mutex<LobbyInner>>,
}

impl Default for SharedLobby {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedLobby {
    pub fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self::with_seed(seed)
    }

    /// Deterministic seat draws for tests.
    pub fn with_seed(seed: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(LobbyInner {
                parked: HashMap::new(),
                walkers: HashMap::new(),
                emotes: HashMap::new(),
                drunk: HashMap::new(),
                dog_pet: None,
                dog: Dog::new(),
                rng: if seed == 0 {
                    0xA409_3822_299F_31D0
                } else {
                    seed
                },
            })),
        }
    }

    /// Reconcile the lobby with the live human roster: drop everyone who
    /// disconnected, keep usernames fresh, seat newcomers (random free seat,
    /// then standing room, then the door stack), and let door-stack patrons
    /// take seats that free up.
    pub fn sync(&self, roster: &[(Uuid, String)]) {
        let mut inner = self.inner.lock_recover();

        inner
            .parked
            .retain(|id, _| roster.iter().any(|(rid, _)| rid == id));
        inner
            .walkers
            .retain(|id, _| roster.iter().any(|(rid, _)| rid == id));
        inner
            .emotes
            .retain(|id, _| roster.iter().any(|(rid, _)| rid == id));

        for (id, name) in roster {
            if let Some(parked) = inner.parked.get_mut(id) {
                parked.username = name.clone();
            } else if let Some(walker) = inner.walkers.get_mut(id) {
                walker.username = name.clone();
            }
        }

        // Newcomers, sorted by name so one sync is deterministic regardless
        // of map iteration order.
        let mut newcomers: Vec<&(Uuid, String)> = roster
            .iter()
            .filter(|(id, _)| !inner.parked.contains_key(id) && !inner.walkers.contains_key(id))
            .collect();
        newcomers.sort_by_key(|(_, name)| name.to_lowercase());
        for (id, name) in newcomers {
            let spot = inner.draw_spot();
            inner.parked.insert(
                *id,
                Parked {
                    username: name.clone(),
                    spot,
                },
            );
        }

        inner.promote_door_stack();
    }

    /// Ensure this user exists as a walker and try one step. Their parked
    /// spot (if any) is freed on the first call. Returns the position after
    /// the (possibly blocked) step.
    pub fn walk(&self, user_id: Uuid, username: &str, dx: i32, dy: i32) -> (u16, u16) {
        let mut inner = self.inner.lock_recover();
        let (mut x, mut y) = match inner.walkers.get(&user_id) {
            Some(w) => (w.x, w.y),
            None => inner
                .parked
                .remove(&user_id)
                .map(|p| p.spot.position())
                .unwrap_or(map::SPAWN),
        };
        let nx = x.saturating_add_signed(dx as i16);
        let ny = y.saturating_add_signed(dy as i16);
        if map::walkable(nx, ny) {
            (x, y) = (nx, ny);
        }
        inner.walkers.insert(
            user_id,
            Walker {
                username: username.to_string(),
                x,
                y,
            },
        );
        (x, y)
    }

    /// Drop this user at an exact cell as a walker (tutorial door spawn).
    pub fn place(&self, user_id: Uuid, username: &str, x: u16, y: u16) {
        let mut inner = self.inner.lock_recover();
        inner.parked.remove(&user_id);
        inner.walkers.insert(
            user_id,
            Walker {
                username: username.to_string(),
                x,
                y,
            },
        );
    }

    /// This user's current cell, if the lobby knows them.
    pub fn position_of(&self, user_id: Uuid) -> Option<(u16, u16)> {
        let inner = self.inner.lock_recover();
        if let Some(w) = inner.walkers.get(&user_id) {
            return Some((w.x, w.y));
        }
        inner.parked.get(&user_id).map(|p| p.spot.position())
    }

    pub fn emote(&self, user_id: Uuid, emote: Emote) {
        let mut inner = self.inner.lock_recover();
        inner.emotes.insert(user_id, (emote, Instant::now()));
    }

    pub fn pet_dog(&self, username: &str) {
        let mut inner = self.inner.lock_recover();
        inner.dog_pet = Some((username.to_string(), Instant::now()));
    }

    /// Record a fresh pour so the buyer's glow updates instantly, ahead of
    /// the next DB seed pass.
    pub fn record_drink(&self, user_id: Uuid, points: i64, last_drink_at: DateTime<Utc>) {
        let mut inner = self.inner.lock_recover();
        inner.drunk.insert(
            user_id,
            DrunkEntry {
                points,
                last_drink_at,
            },
        );
    }

    /// Wholesale replace the drunk map from a DB seed pass. Passing only
    /// still-active rows also prunes users who sobered up or were deleted.
    pub fn set_drunk_states(&self, entries: Vec<(Uuid, i64, DateTime<Utc>)>) {
        let mut inner = self.inner.lock_recover();
        inner.drunk = entries
            .into_iter()
            .map(|(user_id, points, last_drink_at)| {
                (
                    user_id,
                    DrunkEntry {
                        points,
                        last_drink_at,
                    },
                )
            })
            .collect();
    }

    /// Current drunk levels for everyone the lobby knows about, omitting the
    /// sober. Chat author labels read this instead of the DB.
    pub fn drunk_levels(&self) -> HashMap<Uuid, u8> {
        let inner = self.inner.lock_recover();
        let now = Utc::now();
        inner
            .drunk
            .iter()
            .filter_map(|(id, entry)| {
                let level = entry.level(now);
                (level > 0).then_some((*id, level))
            })
            .collect()
    }

    /// Clone out the render view. Seated people come out in seat order so
    /// draw order is stable frame to frame. Also nudges the shared dog: the
    /// step is wall-clock rate-limited, so however many sessions snapshot,
    /// the dog trots at one speed.
    pub fn snapshot(&self) -> LobbySnapshot {
        let mut inner = self.inner.lock_recover();
        let now = Instant::now();
        inner.step_dog(now);
        let now_utc = Utc::now();
        let emote_of = |id: &Uuid| {
            inner
                .emotes
                .get(id)
                .filter(|(_, at)| now.duration_since(*at).as_millis() < EMOTE_MS)
                .map(|(emote, _)| *emote)
        };
        let drunk_of = |id: &Uuid| {
            inner
                .drunk
                .get(id)
                .map(|entry| entry.level(now_utc))
                .unwrap_or(0)
        };

        let mut people: Vec<Presence> =
            Vec::with_capacity(inner.parked.len() + inner.walkers.len());
        let mut door_count = 0usize;
        for (id, parked) in inner.parked.iter() {
            let placement = match parked.spot {
                Spot::Seat(i) => Placement::Seated(i),
                Spot::Standing(i) => Placement::Standing(i),
                Spot::Door(i) => {
                    door_count += 1;
                    Placement::Door(i)
                }
            };
            people.push(Presence {
                user_id: *id,
                username: parked.username.clone(),
                placement,
                emote: emote_of(id),
                drunk_level: drunk_of(id),
            });
        }
        for (id, walker) in inner.walkers.iter() {
            people.push(Presence {
                user_id: *id,
                username: walker.username.clone(),
                placement: Placement::Walking(walker.x, walker.y),
                emote: emote_of(id),
                drunk_level: drunk_of(id),
            });
        }
        // Stable order: seats, standing, door, walkers; each by index/name.
        people.sort_by(|a, b| {
            placement_rank(&a.placement)
                .cmp(&placement_rank(&b.placement))
                .then_with(|| a.username.to_lowercase().cmp(&b.username.to_lowercase()))
        });

        let dog_pet = inner.dog_pet.as_ref().and_then(|(name, at)| {
            let elapsed = now.duration_since(*at).as_millis();
            (elapsed < DOG_PET_MS).then(|| (name.clone(), elapsed))
        });

        LobbySnapshot {
            people,
            door_overflow: door_count.saturating_sub(map::DOOR_STACK.len()),
            dog_pet,
            dog: DogView {
                x: inner.dog.x,
                y: inner.dog.y,
                facing_left: inner.dog.facing_left,
                resting: inner.dog.rest_until.is_some(),
            },
        }
    }
}

fn placement_rank(placement: &Placement) -> (u8, usize) {
    match *placement {
        Placement::Seated(i) => (0, i),
        Placement::Standing(i) => (1, i),
        Placement::Door(i) => (2, i),
        Placement::Walking(..) => (3, 0),
    }
}

impl LobbyInner {
    /// One tick of dog: at most one cell per `DOG_STEP_MS`. It freezes for
    /// a fresh pet and whenever a walker is close (stay for the pat), naps
    /// at reached waypoints, and otherwise steps toward the current
    /// waypoint, larger axis first. Boxed in, it just picks a new errand.
    fn step_dog(&mut self, now: Instant) {
        if now.duration_since(self.dog.last_step).as_millis() < DOG_STEP_MS {
            return;
        }
        self.dog.last_step = now;
        if let Some((_, at)) = &self.dog_pet
            && now.duration_since(*at).as_millis() < DOG_PET_MS
        {
            return;
        }
        let (dx, dy) = (self.dog.x, self.dog.y);
        if self
            .walkers
            .values()
            .any(|w| w.x.abs_diff(dx) <= DOG_FRIEND_RANGE && w.y.abs_diff(dy) <= DOG_FRIEND_RANGE)
        {
            return;
        }
        if let Some(until) = self.dog.rest_until {
            if now < until {
                return;
            }
            self.dog.rest_until = None;
            let pick = self.next_rand(map::DOG_WAYPOINTS.len());
            self.dog.waypoint = map::DOG_WAYPOINTS[pick];
        }
        let (wx, wy) = self.dog.waypoint;
        if (dx, dy) == (wx, wy) {
            let (lo, hi) = DOG_NAP_SECS;
            let nap = lo + self.next_rand((hi - lo) as usize + 1) as u64;
            self.dog.rest_until = Some(now + Duration::from_secs(nap));
            return;
        }
        let step_x = (i32::from(wx) - i32::from(dx)).signum();
        let step_y = (i32::from(wy) - i32::from(dy)).signum();
        let mut order = [(step_x, 0), (0, step_y)];
        if wx.abs_diff(dx) < wy.abs_diff(dy) {
            order.reverse();
        }
        for (sx, sy) in order {
            if sx == 0 && sy == 0 {
                continue;
            }
            let nx = dx.wrapping_add_signed(sx as i16);
            let ny = dy.wrapping_add_signed(sy as i16);
            if map::walkable(nx, ny) {
                if sx != 0 {
                    self.dog.facing_left = sx < 0;
                }
                (self.dog.x, self.dog.y) = (nx, ny);
                return;
            }
        }
        // Both axes blocked: give up on this errand, sniff out another.
        let pick = self.next_rand(map::DOG_WAYPOINTS.len());
        self.dog.waypoint = map::DOG_WAYPOINTS[pick];
    }

    fn next_rand(&mut self, upper: usize) -> usize {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        if upper <= 1 { 0 } else { (x as usize) % upper }
    }

    fn spot_taken(&self, spot: Spot) -> bool {
        self.parked.values().any(|p| p.spot == spot)
    }

    /// A random free seat, else the first free standing spot, else the next
    /// door-stack slot.
    fn draw_spot(&mut self) -> Spot {
        let free_seats: Vec<usize> = (0..map::SEATS.len())
            .filter(|&i| !self.spot_taken(Spot::Seat(i)))
            .collect();
        if !free_seats.is_empty() {
            let pick = self.next_rand(free_seats.len());
            return Spot::Seat(free_seats[pick]);
        }
        if let Some(i) =
            (0..map::STANDING_SPOTS.len()).find(|&i| !self.spot_taken(Spot::Standing(i)))
        {
            return Spot::Standing(i);
        }
        let next = (0..)
            .find(|&i| !self.spot_taken(Spot::Door(i)))
            .unwrap_or(0);
        Spot::Door(next)
    }

    /// The door stack drains into freed seats/standing spots, oldest first.
    fn promote_door_stack(&mut self) {
        loop {
            let Some((&id, _)) = self
                .parked
                .iter()
                .filter(|(_, p)| matches!(p.spot, Spot::Door(_)))
                .min_by_key(|(_, p)| match p.spot {
                    Spot::Door(i) => i,
                    _ => usize::MAX,
                })
            else {
                return;
            };
            let seat = (0..map::SEATS.len())
                .map(Spot::Seat)
                .chain((0..map::STANDING_SPOTS.len()).map(Spot::Standing))
                .find(|&s| !self.spot_taken(s));
            let Some(seat) = seat else {
                return;
            };
            if let Some(parked) = self.parked.get_mut(&id) {
                parked.spot = seat;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(n: u128) -> (Uuid, String) {
        (Uuid::from_u128(n), format!("user{n:03}"))
    }

    fn roster(n: usize) -> Vec<(Uuid, String)> {
        (1..=n as u128).map(user).collect()
    }

    #[test]
    fn sync_seats_everyone_without_collisions() {
        let lobby = SharedLobby::with_seed(7);
        lobby.sync(&roster(20));
        let snap = lobby.snapshot();
        assert_eq!(snap.headcount(), 20);
        let mut cells: Vec<(u16, u16)> =
            snap.people.iter().map(|p| p.placement.position()).collect();
        cells.sort_unstable();
        cells.dedup();
        assert_eq!(cells.len(), 20, "two patrons share a spot");
        assert!(
            snap.people
                .iter()
                .all(|p| matches!(p.placement, Placement::Seated(_)))
        );
    }

    #[test]
    fn overflow_fills_standing_then_stacks_at_the_door() {
        let lobby = SharedLobby::with_seed(7);
        let total = map::SEATS.len() + map::STANDING_SPOTS.len() + map::DOOR_STACK.len() + 3;
        lobby.sync(&roster(total));
        let snap = lobby.snapshot();
        assert_eq!(snap.headcount(), total);
        let seated = snap
            .people
            .iter()
            .filter(|p| matches!(p.placement, Placement::Seated(_)))
            .count();
        let standing = snap
            .people
            .iter()
            .filter(|p| matches!(p.placement, Placement::Standing(_)))
            .count();
        let door = snap
            .people
            .iter()
            .filter(|p| matches!(p.placement, Placement::Door(_)))
            .count();
        assert_eq!(seated, map::SEATS.len());
        assert_eq!(standing, map::STANDING_SPOTS.len());
        assert_eq!(door, map::DOOR_STACK.len() + 3);
        assert_eq!(snap.door_overflow, 3);
    }

    #[test]
    fn first_walk_frees_the_seat_and_moves_from_it() {
        let lobby = SharedLobby::with_seed(7);
        let (id, name) = user(1);
        lobby.sync(&[(id, name.clone())]);
        let seat_pos = lobby.position_of(id).unwrap();

        let walked = lobby.walk(id, &name, 1, 0);
        assert_ne!(walked, seat_pos, "walker did not step off the seat");

        // The freed seat is available to the next arrival.
        let (id2, name2) = user(2);
        lobby.sync(&[(id, name.clone()), (id2, name2.clone())]);
        let snap = lobby.snapshot();
        assert!(matches!(
            snap.find(id).unwrap().placement,
            Placement::Walking(..)
        ));
        assert!(matches!(
            snap.find(id2).unwrap().placement,
            Placement::Seated(_)
        ));
    }

    #[test]
    fn walk_respects_walls() {
        let lobby = SharedLobby::with_seed(7);
        let (id, name) = user(1);
        lobby.place(id, &name, map::SPAWN.0, map::SPAWN.1);
        for _ in 0..60 {
            lobby.walk(id, &name, 0, 1);
        }
        let (_, y) = lobby.position_of(id).unwrap();
        assert_eq!(y, map::MAP_H - 2, "walker escaped the bottom wall");
    }

    #[test]
    fn disconnect_frees_spots_and_walkers() {
        let lobby = SharedLobby::with_seed(7);
        let (a, an) = user(1);
        let (b, bn) = user(2);
        lobby.sync(&[(a, an.clone()), (b, bn.clone())]);
        lobby.walk(b, &bn, 0, -1);
        lobby.sync(&[(a, an)]);
        let snap = lobby.snapshot();
        assert_eq!(snap.headcount(), 1);
        assert!(snap.find(b).is_none());
    }

    #[test]
    fn door_stack_promotes_into_freed_seats() {
        let lobby = SharedLobby::with_seed(7);
        let total = map::SEATS.len() + map::STANDING_SPOTS.len() + 2;
        let full = roster(total);
        lobby.sync(&full);
        let snap = lobby.snapshot();
        let at_door: Vec<Uuid> = snap
            .people
            .iter()
            .filter(|p| matches!(p.placement, Placement::Door(_)))
            .map(|p| p.user_id)
            .collect();
        assert_eq!(at_door.len(), 2);

        // Two seated users leave; the door pair take chairs on next sync.
        let seated: Vec<Uuid> = snap
            .people
            .iter()
            .filter(|p| matches!(p.placement, Placement::Seated(_)))
            .map(|p| p.user_id)
            .take(2)
            .collect();
        let reduced: Vec<(Uuid, String)> = full
            .iter()
            .filter(|(id, _)| !seated.contains(id))
            .cloned()
            .collect();
        lobby.sync(&reduced);
        let snap = lobby.snapshot();
        for id in at_door {
            assert!(
                !matches!(snap.find(id).unwrap().placement, Placement::Door(_)),
                "door patron was not promoted"
            );
        }
    }

    #[test]
    fn renames_apply_in_place() {
        let lobby = SharedLobby::with_seed(7);
        let (id, _) = user(1);
        lobby.sync(&[(id, "old".to_string())]);
        lobby.sync(&[(id, "new".to_string())]);
        assert_eq!(lobby.snapshot().people[0].username, "new");
    }

    /// Rewind the dog's step clock (and expire any nap) so the next
    /// snapshot is guaranteed to advance it.
    fn hurry_the_dog(lobby: &SharedLobby) {
        let mut inner = lobby.inner.lock_recover();
        inner.dog.last_step = Instant::now() - Duration::from_secs(1);
        if inner.dog.rest_until.is_some() {
            inner.dog.rest_until = Some(Instant::now() - Duration::from_millis(1));
        }
    }

    #[test]
    fn the_dog_wanders_the_waypoints_and_stays_on_walkable_floor() {
        let lobby = SharedLobby::with_seed(7);
        let mut visited = std::collections::HashSet::new();
        for _ in 0..600 {
            hurry_the_dog(&lobby);
            let snap = lobby.snapshot();
            assert!(
                map::walkable(snap.dog.x, snap.dog.y),
                "dog stood on a blocked cell at ({}, {})",
                snap.dog.x,
                snap.dog.y
            );
            if snap.dog.resting {
                visited.insert((snap.dog.x, snap.dog.y));
            }
        }
        assert!(
            visited.len() >= 2,
            "dog never made it to a second waypoint: {visited:?}"
        );
        assert!(
            visited.iter().all(|c| map::DOG_WAYPOINTS.contains(c)),
            "dog napped off-waypoint: {visited:?}"
        );
    }

    #[test]
    fn a_fresh_pet_freezes_the_dog() {
        let lobby = SharedLobby::with_seed(7);
        lobby.inner.lock_recover().dog.waypoint = (100, 22);
        lobby.pet_dog("alice");
        for _ in 0..5 {
            hurry_the_dog(&lobby);
            lobby.snapshot();
        }
        let snap = lobby.snapshot();
        assert_eq!((snap.dog.x, snap.dog.y), map::DOG_HOME);
    }

    #[test]
    fn the_dog_waits_when_a_walker_comes_close() {
        let lobby = SharedLobby::with_seed(7);
        lobby.inner.lock_recover().dog.waypoint = (100, 22);
        let (id, name) = user(1);
        lobby.place(id, &name, map::DOG_HOME.0 + 2, map::DOG_HOME.1);
        for _ in 0..5 {
            hurry_the_dog(&lobby);
            lobby.snapshot();
        }
        let snap = lobby.snapshot();
        assert_eq!((snap.dog.x, snap.dog.y), map::DOG_HOME);

        // The friend wanders off; the dog resumes its errand.
        for _ in 0..30 {
            lobby.walk(id, &name, 1, 0);
        }
        hurry_the_dog(&lobby);
        let snap = lobby.snapshot();
        assert_ne!((snap.dog.x, snap.dog.y), map::DOG_HOME);
    }

    #[test]
    fn drunk_levels_decay_and_prune() {
        let lobby = SharedLobby::with_seed(7);
        let (id, name) = user(1);
        lobby.sync(&[(id, name)]);

        let now = Utc::now();
        lobby.record_drink(id, 2_000, now);
        assert_eq!(lobby.snapshot().find(id).unwrap().drunk_level, 3);
        assert_eq!(lobby.drunk_levels().get(&id), Some(&3));

        // A drink from hours ago has partially worn off.
        lobby.record_drink(id, 2_000, now - chrono::Duration::hours(5));
        let level = lobby.snapshot().find(id).unwrap().drunk_level;
        assert_eq!(level, 2, "5h decay of 2000 points should read buzzed");

        // Fully sober entries drop out of the chat-facing map entirely.
        lobby.record_drink(id, 100, now - chrono::Duration::hours(10));
        assert_eq!(lobby.snapshot().find(id).unwrap().drunk_level, 0);
        assert!(lobby.drunk_levels().is_empty());

        // A seed pass replaces everything.
        lobby.record_drink(id, 2_000, now);
        lobby.set_drunk_states(Vec::new());
        assert!(lobby.drunk_levels().is_empty());
    }

    #[test]
    fn emotes_and_dog_pets_show_in_snapshots() {
        let lobby = SharedLobby::with_seed(7);
        let (id, name) = user(1);
        lobby.sync(&[(id, name.clone())]);
        lobby.emote(id, Emote::Wave);
        lobby.pet_dog(&name);
        let snap = lobby.snapshot();
        assert_eq!(snap.find(id).unwrap().emote, Some(Emote::Wave));
        assert_eq!(
            snap.dog_pet.as_ref().map(|(n, _)| n.as_str()),
            Some("user001")
        );
    }
}
