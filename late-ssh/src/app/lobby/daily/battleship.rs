//! Battleship rules for daily correspondence matches. Pure state + logic,
//! no I/O: the service persists `DailyBattleshipState` as the match's
//! `state` JSON the same way chess persists `DailyChessState`.
//!
//! v1 rules: both fleets are placed randomly at claim time (a placement
//! phase would cost the match a whole correspondence day), players
//! alternate single shots on a 10x10 grid, and a hit fires again. Sink all
//! five ships to win.

use anyhow::{Context, Result, bail, ensure};
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub const GRID: usize = 10;
pub const CELLS: usize = GRID * GRID;
/// Classic fleet: carrier, battleship, cruiser, submarine, destroyer.
pub const FLEET_LENGTHS: [usize; 5] = [5, 4, 3, 3, 2];
const STATE_VERSION: u8 = 1;

pub fn ship_name(len: usize) -> &'static str {
    match len {
        5 => "carrier",
        4 => "battleship",
        3 => "cruiser",
        2 => "destroyer",
        _ => "ship",
    }
}

/// `0 -> A1`, `99 -> J10`: column letter + 1-based row.
pub fn cell_label(cell: usize) -> String {
    let col = (b'A' + (cell % GRID) as u8) as char;
    format!("{col}{}", cell / GRID + 1)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DailyBattleshipState {
    pub version: u8,
    #[serde(default)]
    pub revision: u64,
    /// `[challenger, claimer]` at creation; always resolve by user id.
    pub sides: [BattleshipSide; 2],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BattleshipSide {
    pub user_id: Uuid,
    pub ships: Vec<Ship>,
    /// Shots this side fired at the other, in order.
    pub shots: Vec<Shot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ship {
    /// Contiguous grid cells (one row or one column).
    pub cells: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shot {
    pub cell: u8,
    pub hit: bool,
    pub at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShotOutcome {
    pub hit: bool,
    /// Length of the ship this shot finished off, if any.
    pub sunk_len: Option<usize>,
    pub fleet_sunk: bool,
}

impl ShotOutcome {
    /// `D7 miss` / `D7 hit` / `D7 hit, carrier sunk` — the move-feed label.
    pub fn label(&self, cell: usize) -> String {
        let spot = cell_label(cell);
        match (self.hit, self.sunk_len) {
            (false, _) => format!("{spot} miss"),
            (true, None) => format!("{spot} hit"),
            (true, Some(len)) => format!("{spot} hit, {} sunk", ship_name(len)),
        }
    }
}

impl DailyBattleshipState {
    pub fn new(challenger: Uuid, claimer: Uuid) -> Self {
        let mut rng = rand::thread_rng();
        Self {
            version: STATE_VERSION,
            revision: 0,
            sides: [
                BattleshipSide {
                    user_id: challenger,
                    ships: random_fleet(&mut rng),
                    shots: Vec::new(),
                },
                BattleshipSide {
                    user_id: claimer,
                    ships: random_fleet(&mut rng),
                    shots: Vec::new(),
                },
            ],
        }
    }

    pub fn parse(value: &Value) -> Result<Self> {
        let state: Self =
            serde_json::from_value(value.clone()).context("corrupt daily match state")?;
        ensure!(
            state.version == STATE_VERSION,
            "unsupported daily battleship state version: {}",
            state.version
        );
        Ok(state)
    }

    pub fn side_index_of(&self, user_id: Uuid) -> Option<usize> {
        self.sides.iter().position(|side| side.user_id == user_id)
    }

    pub fn side(&self, index: usize) -> &BattleshipSide {
        &self.sides[index]
    }

    pub fn opponent_index(index: usize) -> usize {
        1 - index
    }

    /// Has `shooter` already fired at `cell`?
    pub fn already_shot(&self, shooter: usize, cell: usize) -> bool {
        self.sides[shooter]
            .shots
            .iter()
            .any(|shot| shot.cell as usize == cell)
    }

    /// Fire one shot. Validates bounds and repeats; the caller owns turn
    /// order and match status.
    pub fn apply_shot(
        &mut self,
        shooter: usize,
        cell: usize,
        at: DateTime<Utc>,
    ) -> Result<ShotOutcome> {
        ensure!(cell < CELLS, "that square is off the grid");
        if self.already_shot(shooter, cell) {
            bail!("you already fired at {}", cell_label(cell));
        }
        let target = Self::opponent_index(shooter);
        let hit = self.sides[target]
            .ships
            .iter()
            .any(|ship| ship.cells.contains(&(cell as u8)));
        self.sides[shooter].shots.push(Shot {
            cell: cell as u8,
            hit,
            at,
        });
        let sunk_len = hit
            .then(|| {
                self.sides[target]
                    .ships
                    .iter()
                    .find(|ship| ship.cells.contains(&(cell as u8)))
                    .filter(|ship| self.ship_sunk(shooter, ship))
                    .map(|ship| ship.cells.len())
            })
            .flatten();
        Ok(ShotOutcome {
            hit,
            sunk_len,
            fleet_sunk: self.fleet_sunk_by(shooter),
        })
    }

    /// All of `ship`'s cells are in `shooter`'s hit list.
    pub fn ship_sunk(&self, shooter: usize, ship: &Ship) -> bool {
        ship.cells.iter().all(|cell| {
            self.sides[shooter]
                .shots
                .iter()
                .any(|shot| shot.hit && shot.cell == *cell)
        })
    }

    /// Ships of the side `shooter` targets that still have unhit cells.
    pub fn ships_afloat_against(&self, shooter: usize) -> usize {
        let target = Self::opponent_index(shooter);
        self.sides[target]
            .ships
            .iter()
            .filter(|ship| !self.ship_sunk(shooter, ship))
            .count()
    }

    pub fn fleet_sunk_by(&self, shooter: usize) -> bool {
        self.ships_afloat_against(shooter) == 0
    }

    pub fn shot_count(&self) -> usize {
        self.sides.iter().map(|side| side.shots.len()).sum()
    }
}

/// Random legal fleet: each ship on one row or column, no overlaps
/// (touching is allowed, as in the classic rules).
fn random_fleet(rng: &mut impl Rng) -> Vec<Ship> {
    'fleet: loop {
        let mut occupied = [false; CELLS];
        let mut ships = Vec::with_capacity(FLEET_LENGTHS.len());
        for len in FLEET_LENGTHS {
            let mut placed = false;
            for _ in 0..1000 {
                let horizontal = rng.gen_bool(0.5);
                let (max_col, max_row) = if horizontal {
                    (GRID - len, GRID - 1)
                } else {
                    (GRID - 1, GRID - len)
                };
                let col = rng.gen_range(0..=max_col);
                let row = rng.gen_range(0..=max_row);
                let step = if horizontal { 1 } else { GRID };
                let start = row * GRID + col;
                let cells: Vec<u8> = (0..len).map(|i| (start + i * step) as u8).collect();
                if cells.iter().any(|cell| occupied[*cell as usize]) {
                    continue;
                }
                for cell in &cells {
                    occupied[*cell as usize] = true;
                }
                ships.push(Ship { cells });
                placed = true;
                break;
            }
            if !placed {
                // Statistically unreachable on a 10x10 board; restart clean
                // rather than return a short fleet.
                continue 'fleet;
            }
        }
        return ships;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(challenger_ships: Vec<Ship>, claimer_ships: Vec<Ship>) -> DailyBattleshipState {
        DailyBattleshipState {
            version: STATE_VERSION,
            revision: 0,
            sides: [
                BattleshipSide {
                    user_id: Uuid::from_u128(1),
                    ships: challenger_ships,
                    shots: Vec::new(),
                },
                BattleshipSide {
                    user_id: Uuid::from_u128(2),
                    ships: claimer_ships,
                    shots: Vec::new(),
                },
            ],
        }
    }

    fn ship(cells: &[u8]) -> Ship {
        Ship {
            cells: cells.to_vec(),
        }
    }

    #[test]
    fn random_fleet_is_legal() {
        let mut rng = rand::thread_rng();
        for _ in 0..50 {
            let fleet = random_fleet(&mut rng);
            let mut lens: Vec<usize> = fleet.iter().map(|ship| ship.cells.len()).collect();
            lens.sort_unstable();
            assert_eq!(lens, vec![2, 3, 3, 4, 5]);

            let mut seen = [false; CELLS];
            for ship in &fleet {
                let step = ship.cells[1] - ship.cells[0];
                assert!(step == 1 || step as usize == GRID, "ship must be a line");
                for pair in ship.cells.windows(2) {
                    assert_eq!(pair[1] - pair[0], step, "ship must be contiguous");
                }
                if step == 1 {
                    let row = ship.cells[0] as usize / GRID;
                    assert!(
                        ship.cells.iter().all(|c| *c as usize / GRID == row),
                        "horizontal ship must not wrap rows"
                    );
                }
                for cell in &ship.cells {
                    assert!((*cell as usize) < CELLS);
                    assert!(!seen[*cell as usize], "ships must not overlap");
                    seen[*cell as usize] = true;
                }
            }
        }
    }

    #[test]
    fn shots_hit_miss_and_reject_repeats() {
        let mut state = state_with(vec![ship(&[0, 1])], vec![ship(&[10, 20])]);
        let now = Utc::now();

        // Challenger (side 0) fires at claimer's ship at cell 10.
        let outcome = state.apply_shot(0, 10, now).unwrap();
        assert!(outcome.hit);
        assert_eq!(outcome.sunk_len, None);
        assert!(!outcome.fleet_sunk);

        let miss = state.apply_shot(0, 55, now).unwrap();
        assert!(!miss.hit);

        let repeat = state.apply_shot(0, 10, now);
        assert!(repeat.unwrap_err().to_string().contains("already fired"));

        // Finishing the only ship sinks it and the fleet.
        let kill = state.apply_shot(0, 20, now).unwrap();
        assert_eq!(kill.sunk_len, Some(2));
        assert!(kill.fleet_sunk);
        assert_eq!(kill.label(20), "A3 hit, destroyer sunk");
    }

    #[test]
    fn sides_track_shots_independently() {
        let mut state = state_with(vec![ship(&[0])], vec![ship(&[0])]);
        let now = Utc::now();
        state.apply_shot(0, 5, now).unwrap();
        // The claimer may fire at a cell the challenger already tried.
        let outcome = state.apply_shot(1, 5, now).unwrap();
        assert!(!outcome.hit);
        assert_eq!(state.shot_count(), 2);
    }

    #[test]
    fn cell_labels_are_battleship_coordinates() {
        assert_eq!(cell_label(0), "A1");
        assert_eq!(cell_label(9), "J1");
        assert_eq!(cell_label(90), "A10");
        assert_eq!(cell_label(99), "J10");
    }

    #[test]
    fn state_round_trips_through_json() {
        let state = DailyBattleshipState::new(Uuid::from_u128(7), Uuid::from_u128(8));
        let value = serde_json::to_value(&state).unwrap();
        let parsed = DailyBattleshipState::parse(&value).unwrap();
        assert_eq!(parsed.sides[0].user_id, Uuid::from_u128(7));
        assert_eq!(parsed.sides[1].ships.len(), FLEET_LENGTHS.len());
    }
}
