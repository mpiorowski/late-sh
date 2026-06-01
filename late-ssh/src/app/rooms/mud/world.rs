// Static world definition for Lateania.
//
// Rooms and mob spawns are immutable data, loaded once into the service. The
// full design targets 200 rooms across nine zones (see the project design docs);
// this is the vertical-slice seed: the hub town of Embergate plus the first
// stretch of the King's Road, with a single hostile mob to prove combat.
//
// Content is deliberately data, not code: the slice hardcodes a small seed via
// `seed_world`, but the shape (rooms keyed by id, exits as a direction map) is
// the same one a future TOML/RON loader will produce.

use std::collections::HashMap;

/// Compass (plus vertical) directions a player can move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dir {
    North,
    South,
    East,
    West,
    Up,
    Down,
}

impl Dir {
    pub fn label(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::South => "south",
            Self::East => "east",
            Self::West => "west",
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Self::North => "n",
            Self::South => "s",
            Self::East => "e",
            Self::West => "w",
            Self::Up => "u",
            Self::Down => "d",
        }
    }
}

pub type RoomId = u32;

/// A single location in the world: a node in the room graph.
#[derive(Clone, Debug)]
pub struct Room {
    pub id: RoomId,
    pub name: &'static str,
    pub desc: &'static str,
    pub zone: &'static str,
    pub exits: HashMap<Dir, RoomId>,
    /// True for towns and other no-combat zones.
    pub safe: bool,
}

/// A mob template that spawns at a home room.
#[derive(Clone, Debug)]
pub struct MobSpawn {
    pub id: u32,
    pub name: &'static str,
    pub home: RoomId,
    pub max_hp: i32,
    pub damage: i32,
    pub xp: i32,
    /// Seconds before a slain mob respawns.
    pub respawn_secs: u64,
}

/// The immutable world: every room plus the mob roster.
#[derive(Clone, Debug)]
pub struct World {
    pub rooms: HashMap<RoomId, Room>,
    pub spawns: Vec<MobSpawn>,
    pub start_room: RoomId,
}

impl World {
    pub fn room(&self, id: RoomId) -> Option<&Room> {
        self.rooms.get(&id)
    }
}

fn room(
    id: RoomId,
    name: &'static str,
    zone: &'static str,
    safe: bool,
    desc: &'static str,
    exits: &[(Dir, RoomId)],
) -> Room {
    Room {
        id,
        name,
        desc,
        zone,
        safe,
        exits: exits.iter().copied().collect(),
    }
}

/// Build the vertical-slice world: Embergate (safe hub) + the King's Road.
pub fn seed_world() -> World {
    let rooms = vec![
        room(
            1,
            "Embergate - Town Square",
            "Embergate",
            true,
            "Lanternlight pools on worn cobbles. The town square of Embergate hums \
             with quiet evening trade. A notice board leans by the well, and roads \
             lead off in every direction.",
            &[(Dir::North, 2), (Dir::East, 3), (Dir::West, 4), (Dir::South, 5)],
        ),
        room(
            2,
            "Embergate - The Gilded Flagon",
            "Embergate",
            true,
            "A warm tavern thick with woodsmoke and laughter. Adventurers swap tall \
             tales over tankards. The square lies back to the south.",
            &[(Dir::South, 1)],
        ),
        room(
            3,
            "Embergate - Market Row",
            "Embergate",
            true,
            "Shuttered stalls line a narrow lane. A smith's forge glows at the far \
             end. The square is back to the west.",
            &[(Dir::West, 1)],
        ),
        room(
            4,
            "Embergate - Temple of the Dawn",
            "Embergate",
            true,
            "Pale columns rise toward a domed ceiling painted with sunrise. Clerics \
             move in hushed procession. The square is back to the east.",
            &[(Dir::East, 1)],
        ),
        room(
            5,
            "Embergate - South Gate",
            "Embergate",
            true,
            "A heavy iron portcullis stands raised. Beyond it the King's Road \
             stretches into open country. The square is north.",
            &[(Dir::North, 1), (Dir::South, 6)],
        ),
        room(
            6,
            "The King's Road - Open Country",
            "King's Road",
            false,
            "The cobbles give way to packed earth. Tall grass whispers on either \
             side and the town wall recedes behind you to the north.",
            &[(Dir::North, 5), (Dir::South, 7)],
        ),
        room(
            7,
            "The King's Road - The Old Milestone",
            "King's Road",
            false,
            "A mossy milestone marks the leagues to far cities. A thin trail forks \
             east into a thicket; the road runs on south.",
            &[(Dir::North, 6), (Dir::East, 8), (Dir::South, 9)],
        ),
        room(
            8,
            "The King's Road - Bramble Thicket",
            "King's Road",
            false,
            "Thorns crowd a dead-end clearing. Something has trampled the grass \
             here recently. The trail back is west.",
            &[(Dir::West, 7)],
        ),
        room(
            9,
            "The King's Road - Ruined Watchtower",
            "King's Road",
            false,
            "A toppled watchtower slumps against the hillside, its stones scorched. \
             The road continues south into a shadowed defile; the way back is north.",
            &[(Dir::North, 7), (Dir::South, 10)],
        ),
        room(
            10,
            "The King's Road - The Defile",
            "King's Road",
            false,
            "Steep banks close in on a gloomy cut in the hills. This is as far as \
             the road has been cleared. The way back is north.",
            &[(Dir::North, 9)],
        ),
    ];

    let spawns = vec![
        MobSpawn {
            id: 1,
            name: "a scrawny goblin",
            home: 6,
            max_hp: 18,
            damage: 3,
            xp: 12,
            respawn_secs: 30,
        },
        MobSpawn {
            id: 2,
            name: "a road bandit",
            home: 8,
            max_hp: 26,
            damage: 5,
            xp: 20,
            respawn_secs: 45,
        },
        MobSpawn {
            id: 3,
            name: "a gaunt wolf",
            home: 9,
            max_hp: 22,
            damage: 4,
            xp: 16,
            respawn_secs: 40,
        },
    ];

    World {
        rooms: rooms.into_iter().map(|r| (r.id, r)).collect(),
        spawns,
        start_room: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_exit_resolves_to_a_real_room() {
        let world = seed_world();
        for room in world.rooms.values() {
            for (dir, target) in &room.exits {
                assert!(
                    world.rooms.contains_key(target),
                    "room {} ({}) has a {} exit to missing room {}",
                    room.id,
                    room.name,
                    dir.label(),
                    target
                );
            }
        }
    }

    #[test]
    fn exits_are_reciprocal_where_expected() {
        // Embergate square (1) <-> south gate (5): going south then north returns.
        let world = seed_world();
        let square = world.room(1).expect("square exists");
        let gate_id = square.exits.get(&Dir::South).copied().expect("south exit");
        let gate = world.room(gate_id).expect("gate exists");
        assert_eq!(gate.exits.get(&Dir::North).copied(), Some(1));
    }

    #[test]
    fn start_room_exists_and_is_safe() {
        let world = seed_world();
        let start = world.room(world.start_room).expect("start room exists");
        assert!(start.safe, "players should spawn in a safe room");
    }

    #[test]
    fn every_room_reachable_from_start() {
        let world = seed_world();
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![world.start_room];
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            if let Some(room) = world.room(id) {
                for target in room.exits.values() {
                    stack.push(*target);
                }
            }
        }
        assert_eq!(
            seen.len(),
            world.rooms.len(),
            "some rooms are unreachable from the start room"
        );
    }
}
