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
#[path = "battleship_test.rs"]
mod battleship_test;

