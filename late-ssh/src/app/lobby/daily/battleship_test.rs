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
