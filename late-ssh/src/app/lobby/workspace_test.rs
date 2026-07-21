use super::*;

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

#[test]
fn from_home_enters_first_board() {
    assert_eq!(
        next_workspace(&[id(1), id(2)], &[], &[], GameWorkspace::Dashboard),
        GameWorkspace::DailyBoard(id(1))
    );
}

#[test]
fn from_home_with_no_stops_stays_home() {
    assert_eq!(
        next_workspace(&[], &[], &[], GameWorkspace::Dashboard),
        GameWorkspace::Dashboard
    );
}

#[test]
fn advances_through_boards_then_wraps_home() {
    let ids = [id(1), id(2)];
    assert_eq!(
        next_workspace(&ids, &[], &[], GameWorkspace::DailyBoard(id(1))),
        GameWorkspace::DailyBoard(id(2))
    );
    assert_eq!(
        next_workspace(&ids, &[], &[], GameWorkspace::DailyBoard(id(2))),
        GameWorkspace::Dashboard
    );
}

#[test]
fn board_no_longer_my_turn_restarts_from_front() {
    // Just moved on match 1: it left the my-turn list, so the next hop
    // goes to the front of what's still waiting.
    assert_eq!(
        next_workspace(&[id(2), id(3)], &[], &[], GameWorkspace::DailyBoard(id(1))),
        GameWorkspace::DailyBoard(id(2))
    );
}

#[test]
fn last_board_gone_and_queue_empty_lands_home() {
    assert_eq!(
        next_workspace(&[], &[], &[], GameWorkspace::DailyBoard(id(1))),
        GameWorkspace::Dashboard
    );
}

#[test]
fn seated_tables_slot_after_your_turn_boards() {
    let tables = [HouseTable::Poker, HouseTable::Tron];
    assert_eq!(
        next_workspace(&[id(1)], &tables, &[], GameWorkspace::DailyBoard(id(1))),
        GameWorkspace::HouseTable(HouseTable::Poker)
    );
    assert_eq!(
        next_workspace(
            &[id(1)],
            &tables,
            &[],
            GameWorkspace::HouseTable(HouseTable::Poker)
        ),
        GameWorkspace::HouseTable(HouseTable::Tron)
    );
    assert_eq!(
        next_workspace(
            &[id(1)],
            &tables,
            &[],
            GameWorkspace::HouseTable(HouseTable::Tron)
        ),
        GameWorkspace::Dashboard
    );
}

#[test]
fn tables_only_cycle_works_without_boards() {
    let tables = [HouseTable::Blackjack];
    assert_eq!(
        next_workspace(&[], &tables, &[], GameWorkspace::Dashboard),
        GameWorkspace::HouseTable(HouseTable::Blackjack)
    );
    assert_eq!(
        next_workspace(
            &[],
            &tables,
            &[],
            GameWorkspace::HouseTable(HouseTable::Blackjack)
        ),
        GameWorkspace::Dashboard
    );
}

#[test]
fn lost_seat_restarts_from_front() {
    assert_eq!(
        next_workspace(
            &[id(1)],
            &[HouseTable::Tron],
            &[],
            GameWorkspace::HouseTable(HouseTable::Poker)
        ),
        GameWorkspace::DailyBoard(id(1))
    );
}

#[test]
fn arcade_stops_slot_after_house_tables() {
    let tables = [HouseTable::Poker];
    let arcade = [ArcadeStop::Sudoku, ArcadeStop::Solitaire];
    assert_eq!(
        next_workspace(
            &[],
            &tables,
            &arcade,
            GameWorkspace::HouseTable(HouseTable::Poker)
        ),
        GameWorkspace::Arcade(ArcadeStop::Sudoku)
    );
    assert_eq!(
        next_workspace(
            &[],
            &tables,
            &arcade,
            GameWorkspace::Arcade(ArcadeStop::Sudoku)
        ),
        GameWorkspace::Arcade(ArcadeStop::Solitaire)
    );
    assert_eq!(
        next_workspace(
            &[],
            &tables,
            &arcade,
            GameWorkspace::Arcade(ArcadeStop::Solitaire)
        ),
        GameWorkspace::Dashboard
    );
}

#[test]
fn arcade_only_cycle_works_without_lobby_stops() {
    let arcade = [ArcadeStop::LeWord];
    assert_eq!(
        next_workspace(&[], &[], &arcade, GameWorkspace::Dashboard),
        GameWorkspace::Arcade(ArcadeStop::LeWord)
    );
    assert_eq!(
        next_workspace(&[], &[], &arcade, GameWorkspace::Arcade(ArcadeStop::LeWord)),
        GameWorkspace::Dashboard
    );
}

#[test]
fn solved_arcade_stop_restarts_from_front() {
    // Just solved the sudoku: it left the unfinished list, so the next
    // hop goes back to the front of what's still waiting.
    assert_eq!(
        next_workspace(
            &[id(1)],
            &[],
            &[ArcadeStop::Nonogram],
            GameWorkspace::Arcade(ArcadeStop::Sudoku)
        ),
        GameWorkspace::DailyBoard(id(1))
    );
}
