//! The tavern's crowd, derived from presence (`app/presence`): every
//! logged-in session on every replica, where it sits or walks, its wave or
//! dance, the last pet of the dog, and the dog itself. Pure: no I/O, no
//! clock reads (every `now_ms` is handed in), so every replica holding the
//! same records draws the same room.
//!
//! Nobody hands out seats. Each session picks its own ([`pick_spot`]: a
//! random free seat, else the first free standing spot, else the door) and
//! publishes it. Two sessions that pick the same spot in the same breath
//! are settled here: the earlier claim keeps it (`since_ms`, then the user
//! id), the other is drawn at the door, and its session notices on its
//! next refresh and picks again (`State::settle`). One user with two
//! sessions is one patron: the latest mover's.
//!
//! The dog is a pure function of the wall clock ([`dog_at`]): a fixed
//! errand schedule over `map::DOG_WAYPOINTS`, so every replica draws the
//! same trot with nothing to sync. A pet sets its tail going; it does not
//! stop walking for you.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use late_core::models::presence::{ClubhouseStand, Emote, PresenceRecord, Spot};
use uuid::Uuid;

use super::map;

/// How long an emote plays for everyone, in milliseconds.
pub const EMOTE_MS: i64 = 3200;
/// How long the dog stays excited after a pet, in milliseconds.
pub const DOG_PET_MS: i64 = 4000;
/// How close (Chebyshev) a walker must be to a free seat to sit in it.
const SIT_REACH: u16 = 2;
/// One errand of the dog: trot from one waypoint to the next, then nap
/// there for the rest of the cycle.
const DOG_CYCLE_MS: i64 = 40_000;
/// The dog's pace, slowed to fit a long errand into two thirds of a cycle.
const DOG_STEP_MS: i64 = 600;

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
            Placement::Seated(i) => map::SEATS.get(i).map(|s| (s.x, s.y)).unwrap_or(map::SPAWN),
            Placement::Standing(i) => map::STANDING_SPOTS.get(i).copied().unwrap_or(map::SPAWN),
            Placement::Door(i) => map::DOOR_STACK[i % map::DOOR_STACK.len()],
            Placement::Walking(x, y) => (x, y),
        }
    }
}

/// One rendered person, however they are placed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patron {
    pub user_id: Uuid,
    pub username: String,
    pub placement: Placement,
    pub emote: Option<Emote>,
    /// Current drunk level 0 (sober) through 4 (wasted), already decayed.
    pub drunk_level: u8,
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

/// Everything a session needs to draw the room.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Crowd {
    pub people: Vec<Patron>,
    /// How many door-stack patrons exceed the distinct render slots.
    pub door_overflow: usize,
    /// Milliseconds since the dog was last petted, with the petter's name,
    /// while inside the excitement window.
    pub dog_pet: Option<(String, u128)>,
    pub dog: DogView,
}

impl Crowd {
    pub fn headcount(&self) -> usize {
        self.people.len()
    }

    pub fn find(&self, user_id: Uuid) -> Option<&Patron> {
        self.people.iter().find(|p| p.user_id == user_id)
    }

    /// Whether a seat has somebody on it.
    pub fn seat_taken(&self, seat: usize) -> bool {
        self.people
            .iter()
            .any(|p| p.placement == Placement::Seated(seat))
    }

    /// The nearest free seat within reach of a cell, if any. A seat the
    /// walker is standing on counts (distance 0), so `s` reads as "sit down
    /// right here".
    pub fn free_seat_near(&self, x: u16, y: u16) -> Option<usize> {
        (0..map::SEATS.len())
            .filter(|&i| !self.seat_taken(i))
            .map(|i| {
                let seat = &map::SEATS[i];
                (seat.x.abs_diff(x).max(seat.y.abs_diff(y)), i)
            })
            .filter(|&(distance, _)| distance <= SIT_REACH)
            .min_by_key(|&(distance, i)| (distance, i))
            .map(|(_, i)| i)
    }
}

/// This session's own part of the room, laid over the records so its own
/// moves draw before they come back from the presence task.
#[derive(Debug, Clone, Copy)]
pub struct Own<'a> {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub username: &'a str,
    pub stand: ClubhouseStand,
}

#[derive(Clone, Copy)]
struct Entry<'a> {
    session_id: Uuid,
    user_id: Uuid,
    username: &'a str,
    stand: ClubhouseStand,
}

fn entries<'a>(records: &'a [PresenceRecord], own: &Own<'a>) -> Vec<Entry<'a>> {
    records
        .iter()
        .filter(|record| record.session_id != own.session_id)
        .map(|record| Entry {
            session_id: record.session_id,
            user_id: record.user_id,
            username: record.username.as_str(),
            stand: record.clubhouse,
        })
        .chain(std::iter::once(Entry {
            session_id: own.session_id,
            user_id: own.user_id,
            username: own.username,
            stand: own.stand,
        }))
        .collect()
}

/// A parked spot as a contested key; `None` for the door and for walkers,
/// which nobody contests, and for an index this map does not have (a
/// record from a replica on another map), which is drawn at the door.
fn claim(spot: Spot) -> Option<Spot> {
    match spot {
        Spot::Seat { index } if usize::from(index) < map::SEATS.len() => Some(spot),
        Spot::Standing { index } if usize::from(index) < map::STANDING_SPOTS.len() => Some(spot),
        Spot::Seat { .. } | Spot::Standing { .. } | Spot::Door | Spot::Walking { .. } => None,
    }
}

/// The room as every replica draws it from the same records.
pub fn crowd(
    records: &[PresenceRecord],
    own: &Own<'_>,
    drunk: &HashMap<Uuid, u8>,
    now_ms: i64,
) -> Crowd {
    let entries = entries(records, own);

    // One patron per user: the latest mover; their newest emote.
    let mut chosen: HashMap<Uuid, Entry> = HashMap::new();
    let mut emotes: HashMap<Uuid, (Emote, i64)> = HashMap::new();
    for entry in &entries {
        match chosen.get(&entry.user_id) {
            Some(kept)
                if (kept.stand.since_ms, kept.session_id)
                    >= (entry.stand.since_ms, entry.session_id) => {}
            Some(_) | None => {
                chosen.insert(entry.user_id, *entry);
            }
        }
        if let Some((emote, at)) = entry.stand.emote {
            match emotes.get(&entry.user_id) {
                Some((_, kept_at)) if *kept_at >= at => {}
                Some(_) | None => {
                    emotes.insert(entry.user_id, (emote, at));
                }
            }
        }
    }

    // A contested spot goes to the earlier claim.
    let mut holders: HashMap<(bool, u16), (i64, Uuid)> = HashMap::new();
    for entry in chosen.values() {
        let key = match claim(entry.stand.spot) {
            Some(Spot::Seat { index }) => (true, index),
            Some(Spot::Standing { index }) => (false, index),
            Some(Spot::Door | Spot::Walking { .. }) | None => continue,
        };
        let claim = (entry.stand.since_ms, entry.user_id);
        match holders.get(&key) {
            Some(held) if *held <= claim => {}
            Some(_) | None => {
                holders.insert(key, claim);
            }
        }
    }

    let mut people = Vec::with_capacity(chosen.len());
    let mut at_door: Vec<(i64, Uuid, &Entry)> = Vec::new();
    for entry in chosen.values() {
        let holds = |key: (bool, u16)| {
            holders.get(&key).map(|(_, user_id)| *user_id) == Some(entry.user_id)
        };
        let placement = match claim(entry.stand.spot) {
            Some(Spot::Seat { index }) if holds((true, index)) => {
                Some(Placement::Seated(usize::from(index)))
            }
            Some(Spot::Standing { index }) if holds((false, index)) => {
                Some(Placement::Standing(usize::from(index)))
            }
            _ => match entry.stand.spot {
                Spot::Walking { x, y } => Some(Placement::Walking(x, y)),
                Spot::Seat { .. } | Spot::Standing { .. } | Spot::Door => None,
            },
        };
        match placement {
            Some(placement) => people.push(patron(entry, placement, &emotes, drunk, now_ms)),
            None => at_door.push((entry.stand.since_ms, entry.user_id, entry)),
        }
    }
    at_door.sort_by_key(|(since, user_id, _)| (*since, *user_id));
    let door_count = at_door.len();
    for (slot, (_, _, entry)) in at_door.into_iter().enumerate() {
        people.push(patron(entry, Placement::Door(slot), &emotes, drunk, now_ms));
    }
    // Stable order: seats, standing, door, walkers; each by index/name.
    people.sort_by(|a, b| {
        placement_rank(&a.placement)
            .cmp(&placement_rank(&b.placement))
            .then_with(|| a.username.to_lowercase().cmp(&b.username.to_lowercase()))
            .then_with(|| a.user_id.cmp(&b.user_id))
    });

    let dog_pet = entries
        .iter()
        .filter_map(|entry| entry.stand.petted_dog_at_ms.map(|at| (at, entry.username)))
        .max_by_key(|(at, _)| *at)
        .and_then(|(at, username)| {
            let elapsed = now_ms - at;
            (0..DOG_PET_MS)
                .contains(&elapsed)
                .then(|| (username.to_string(), elapsed as u128))
        });

    Crowd {
        people,
        door_overflow: door_count.saturating_sub(map::DOOR_STACK.len()),
        dog_pet,
        dog: dog_at(now_ms),
    }
}

fn patron(
    entry: &Entry<'_>,
    placement: Placement,
    emotes: &HashMap<Uuid, (Emote, i64)>,
    drunk: &HashMap<Uuid, u8>,
    now_ms: i64,
) -> Patron {
    Patron {
        user_id: entry.user_id,
        username: entry.username.to_string(),
        placement,
        emote: emotes
            .get(&entry.user_id)
            .filter(|(_, at)| (0..EMOTE_MS).contains(&(now_ms - at)))
            .map(|(emote, _)| *emote),
        drunk_level: drunk.get(&entry.user_id).copied().unwrap_or(0),
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

/// Where a session sits down: a random free seat (by `rng`), else the
/// first free standing spot, else the door. Free means no other user's
/// record claims it; this user's other sessions do not count, so a second
/// device never crowds out the first.
pub fn pick_spot(records: &[PresenceRecord], user_id: Uuid, rng: u64) -> Spot {
    let mut seats = HashSet::new();
    let mut standing = HashSet::new();
    for record in records.iter().filter(|record| record.user_id != user_id) {
        match record.clubhouse.spot {
            Spot::Seat { index } => {
                seats.insert(usize::from(index));
            }
            Spot::Standing { index } => {
                standing.insert(usize::from(index));
            }
            Spot::Door | Spot::Walking { .. } => {}
        }
    }
    let free: Vec<usize> = (0..map::SEATS.len())
        .filter(|i| !seats.contains(i))
        .collect();
    if !free.is_empty() {
        return Spot::Seat {
            index: free[(rng % free.len() as u64) as usize] as u16,
        };
    }
    match (0..map::STANDING_SPOTS.len()).find(|i| !standing.contains(i)) {
        Some(index) => Spot::Standing {
            index: index as u16,
        },
        None => Spot::Door,
    }
}

/// A session's first stand in the tavern: where this user already is on
/// another session (a second device joins the first), else a spot of its
/// own.
pub fn first_stand(
    records: &[PresenceRecord],
    user_id: Uuid,
    rng: u64,
    now_ms: i64,
) -> ClubhouseStand {
    let mine = records
        .iter()
        .filter(|record| record.user_id == user_id)
        .max_by_key(|record| (record.clubhouse.since_ms, record.session_id));
    match mine {
        Some(record) => ClubhouseStand {
            spot: record.clubhouse.spot,
            since_ms: record.clubhouse.since_ms,
            emote: None,
            petted_dog_at_ms: None,
        },
        None => ClubhouseStand {
            spot: pick_spot(records, user_id, rng),
            since_ms: now_ms,
            emote: None,
            petted_dog_at_ms: None,
        },
    }
}

/// The dog at a moment: cycle `k` trots from waypoint `k` to waypoint
/// `k + 1` along the shortest open path, then naps there. A cycle whose
/// two waypoints are the same is a long nap.
pub fn dog_at(now_ms: i64) -> DogView {
    let cycle = now_ms.div_euclid(DOG_CYCLE_MS);
    let into = now_ms.rem_euclid(DOG_CYCLE_MS);
    let from = dog_waypoint(cycle);
    let to = dog_waypoint(cycle + 1);
    let path = &dog_paths()[from][to];
    let steps = (path.len() - 1) as i64;
    let facing_left = map::DOG_WAYPOINTS[to].0 < map::DOG_WAYPOINTS[from].0;
    if steps == 0 {
        let (x, y) = path[0];
        return DogView {
            x,
            y,
            facing_left,
            resting: true,
        };
    }
    let step_ms = DOG_STEP_MS.min(DOG_CYCLE_MS * 2 / 3 / steps).max(1);
    let walked = into / step_ms;
    let (x, y) = path[walked.min(steps) as usize];
    DogView {
        x,
        y,
        facing_left,
        resting: walked >= steps,
    }
}

fn dog_waypoint(cycle: i64) -> usize {
    let mut v = cycle as u64 ^ 0x9E37_79B9_7F4A_7C15;
    v ^= v >> 33;
    v = v.wrapping_mul(0xff51_afd7_ed55_8ccd);
    v ^= v >> 33;
    (v % map::DOG_WAYPOINTS.len() as u64) as usize
}

/// The shortest open path between every pair of waypoints, both ends
/// included, found once. `map_test` proves every pair is connected; a pair
/// that is not would fall back to standing at the first waypoint.
fn dog_paths() -> &'static Vec<Vec<Vec<(u16, u16)>>> {
    static PATHS: OnceLock<Vec<Vec<Vec<(u16, u16)>>>> = OnceLock::new();
    PATHS.get_or_init(|| {
        map::DOG_WAYPOINTS
            .iter()
            .map(|&from| {
                map::DOG_WAYPOINTS
                    .iter()
                    .map(|&to| shortest_path(from, to).unwrap_or_else(|| vec![from]))
                    .collect()
            })
            .collect()
    })
}

pub(super) fn shortest_path(from: (u16, u16), to: (u16, u16)) -> Option<Vec<(u16, u16)>> {
    let width = usize::from(map::MAP_W);
    let index = |(x, y): (u16, u16)| usize::from(y) * width + usize::from(x);
    let mut came_from: Vec<Option<(u16, u16)>> =
        vec![None; width * usize::from(map::MAP_H)];
    let mut queue = std::collections::VecDeque::from([from]);
    came_from[index(from)] = Some(from);
    while let Some(cell) = queue.pop_front() {
        if cell == to {
            let mut path = vec![to];
            let mut at = to;
            while at != from {
                at = came_from[index(at)]?;
                path.push(at);
            }
            path.reverse();
            return Some(path);
        }
        let (x, y) = cell;
        for next in [
            (x.wrapping_add(1), y),
            (x.wrapping_sub(1), y),
            (x, y.wrapping_add(1)),
            (x, y.wrapping_sub(1)),
        ] {
            if next.0 < map::MAP_W
                && next.1 < map::MAP_H
                && map::walkable(next.0, next.1)
                && came_from[index(next)].is_none()
            {
                came_from[index(next)] = Some(cell);
                queue.push_back(next);
            }
        }
    }
    None
}

#[cfg(test)]
#[path = "crowd_test.rs"]
mod crowd_test;
