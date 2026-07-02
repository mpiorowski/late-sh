//! Per-session clubhouse view state: your avatar, the camera target, and the
//! crowd. Occupants are live humans from the active-users map (bots stay out,
//! except the bartender and graybeard who have fixed staff spots). Arrival
//! order is preserved so people keep their chairs; when the room is over
//! capacity the whole crowd rotates one place every so often so everyone in
//! the door queue eventually gets a seat.

use uuid::Uuid;

use super::map;

/// Refresh the roster from the active-users map once a second (15 ticks).
const ROSTER_REFRESH_TICKS: u64 = 15;
/// With an overflow queue, rotate seating every ~25s so nobody waits forever.
const ROTATION_TICKS: u32 = 375;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occupant {
    pub user_id: Uuid,
    pub username: String,
}

#[derive(Debug)]
pub struct State {
    pub player_x: u16,
    pub player_y: u16,
    pub anim_tick: u64,
    /// Live humans (minus this session's user), in arrival order.
    present: Vec<Occupant>,
    rotation_offset: usize,
    ticks_until_rotate: u32,
    pub graybeard_online: bool,
    pub bartender_online: bool,
    last_roster_tick: u64,
}

impl Default for State {
    fn default() -> Self {
        Self {
            player_x: map::SPAWN.0,
            player_y: map::SPAWN.1,
            anim_tick: 0,
            present: Vec::new(),
            rotation_offset: 0,
            ticks_until_rotate: ROTATION_TICKS,
            graybeard_online: false,
            bartender_online: false,
            last_roster_tick: 0,
        }
    }
}

impl State {
    /// Seats plus standing spots: how many occupants are visible at once.
    fn visible_capacity() -> usize {
        map::SEATS.len() + map::STANDING_SPOTS.len()
    }

    /// Advance animation and (while the screen is visible) the seat-rotation
    /// clock. Called every world tick.
    pub fn tick(&mut self, on_screen: bool) {
        self.anim_tick = self.anim_tick.wrapping_add(1);
        if !on_screen {
            return;
        }
        if self.present.len() <= Self::visible_capacity() {
            self.rotation_offset = 0;
            self.ticks_until_rotate = ROTATION_TICKS;
            return;
        }
        self.ticks_until_rotate = self.ticks_until_rotate.saturating_sub(1);
        if self.ticks_until_rotate == 0 {
            self.rotation_offset = (self.rotation_offset + 1) % self.present.len().max(1);
            self.ticks_until_rotate = ROTATION_TICKS;
        }
    }

    pub fn roster_refresh_due(&mut self) -> bool {
        if self.anim_tick.wrapping_sub(self.last_roster_tick) < ROSTER_REFRESH_TICKS {
            return false;
        }
        self.last_roster_tick = self.anim_tick;
        true
    }

    /// Merge a fresh roster while preserving arrival order: dropped users
    /// free their spots, newcomers append at the back (sorted by name so a
    /// single refresh is deterministic regardless of map iteration order).
    pub fn update_roster(&mut self, mut roster: Vec<Occupant>) {
        self.present
            .retain(|old| roster.iter().any(|new| new.user_id == old.user_id));
        // Keep renamed users current without moving them.
        for old in &mut self.present {
            if let Some(new) = roster.iter().find(|new| new.user_id == old.user_id) {
                old.username = new.username.clone();
            }
        }
        roster.retain(|new| !self.present.iter().any(|old| old.user_id == new.user_id));
        roster.sort_by(|a, b| a.username.to_lowercase().cmp(&b.username.to_lowercase()));
        self.present.extend(roster);
        if self.rotation_offset >= self.present.len() {
            self.rotation_offset = 0;
        }
    }

    fn rotated(&self, index: usize) -> Option<&Occupant> {
        if self.present.is_empty() {
            return None;
        }
        self.present
            .get((index + self.rotation_offset) % self.present.len())
            .filter(|_| index < self.present.len())
    }

    /// Occupied seats: `(seat, occupant)` pairs in seat order.
    pub fn seated(&self) -> impl Iterator<Item = (&'static map::Seat, &Occupant)> {
        map::SEATS
            .iter()
            .enumerate()
            .filter_map(|(i, seat)| self.rotated(i).map(|who| (seat, who)))
    }

    /// Occupied standing spots near the door.
    pub fn standing(&self) -> impl Iterator<Item = ((u16, u16), &Occupant)> {
        map::STANDING_SPOTS
            .iter()
            .enumerate()
            .filter_map(|(i, &spot)| self.rotated(map::SEATS.len() + i).map(|who| (spot, who)))
    }

    /// How many people are queued outside the visible room.
    pub fn door_count(&self) -> usize {
        self.present.len().saturating_sub(Self::visible_capacity())
    }

    /// Everyone here, including you.
    pub fn headcount(&self) -> usize {
        self.present.len() + 1
    }

    /// Try to walk one step; furniture, walls, and other patrons block.
    pub fn try_move(&mut self, dx: i32, dy: i32) -> bool {
        let nx = self.player_x.saturating_add_signed(dx as i16);
        let ny = self.player_y.saturating_add_signed(dy as i16);
        if !map::walkable(nx, ny) {
            return false;
        }
        self.player_x = nx;
        self.player_y = ny;
        true
    }

    /// The prop within reach of the player, if any.
    pub fn nearby(&self) -> Option<map::Interactive> {
        map::nearest_interactive(self.player_x, self.player_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn occupant(n: u128, name: &str) -> Occupant {
        Occupant {
            user_id: Uuid::from_u128(n),
            username: name.to_string(),
        }
    }

    #[test]
    fn roster_preserves_arrival_order_across_refreshes() {
        let mut state = State::default();
        state.update_roster(vec![occupant(1, "zoe"), occupant(2, "alice")]);
        // Sorted on first arrival: alice then zoe.
        let first: Vec<_> = state
            .seated()
            .map(|(_, who)| who.username.clone())
            .collect();
        assert_eq!(first, vec!["alice", "zoe"]);

        // A newcomer lands after existing patrons, never reshuffling them.
        state.update_roster(vec![
            occupant(1, "zoe"),
            occupant(2, "alice"),
            occupant(3, "bob"),
        ]);
        let second: Vec<_> = state
            .seated()
            .map(|(_, who)| who.username.clone())
            .collect();
        assert_eq!(second, vec!["alice", "zoe", "bob"]);
    }

    #[test]
    fn roster_frees_seats_when_users_leave() {
        let mut state = State::default();
        state.update_roster(vec![occupant(1, "a"), occupant(2, "b"), occupant(3, "c")]);
        state.update_roster(vec![occupant(1, "a"), occupant(3, "c")]);
        let names: Vec<_> = state
            .seated()
            .map(|(_, who)| who.username.clone())
            .collect();
        assert_eq!(names, vec!["a", "c"]);
    }

    #[test]
    fn overflow_goes_to_standing_then_the_door() {
        let mut state = State::default();
        let total = State::visible_capacity() + 3;
        let roster: Vec<Occupant> = (0..total)
            .map(|i| occupant(i as u128 + 1, &format!("user{i:02}")))
            .collect();
        state.update_roster(roster);
        assert_eq!(state.seated().count(), map::SEATS.len());
        assert_eq!(state.standing().count(), map::STANDING_SPOTS.len());
        assert_eq!(state.door_count(), 3);
    }

    #[test]
    fn rotation_only_happens_with_an_overflow_queue() {
        let mut state = State::default();
        state.update_roster(vec![occupant(1, "a"), occupant(2, "b")]);
        for _ in 0..(ROTATION_TICKS as u64 * 2) {
            state.tick(true);
        }
        assert_eq!(state.rotation_offset, 0);

        let total = State::visible_capacity() + 1;
        let roster: Vec<Occupant> = (0..total)
            .map(|i| occupant(i as u128 + 1, &format!("user{i:02}")))
            .collect();
        state.update_roster(roster);
        for _ in 0..ROTATION_TICKS {
            state.tick(true);
        }
        assert_eq!(state.rotation_offset, 1);
    }

    #[test]
    fn movement_respects_collision() {
        let mut state = State::default();
        assert_eq!((state.player_x, state.player_y), map::SPAWN);
        assert!(state.try_move(0, 1));
        // Walk down into the bottom wall until blocked.
        for _ in 0..40 {
            state.try_move(0, 1);
        }
        assert_eq!(state.player_y, map::MAP_H - 2);
        assert!(!state.try_move(0, 1));
    }

    #[test]
    fn renames_update_labels_in_place() {
        let mut state = State::default();
        state.update_roster(vec![occupant(1, "old-name")]);
        state.update_roster(vec![occupant(1, "new-name")]);
        let names: Vec<_> = state
            .seated()
            .map(|(_, who)| who.username.clone())
            .collect();
        assert_eq!(names, vec!["new-name"]);
    }
}
