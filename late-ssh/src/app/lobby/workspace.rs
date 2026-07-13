//! The backtick workspace cycle: Home chat -> each daily board waiting on
//! your move -> each house table you're seated at -> back to Home chat. The
//! one key that spans both Lobby game domains.

use uuid::Uuid;

use crate::app::{
    common::primitives::{Banner, Screen},
    lobby::house::tables::HouseTable,
    state::App,
};

/// One stop on the backtick cycle: Home chat, a daily board where it's your
/// move, or a house table where you hold a seat. The cycle is lobby games
/// only — rooms and Arcade dropped out when correspondence play became the
/// front door.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GameWorkspace {
    Dashboard,
    DailyBoard(Uuid),
    HouseTable(HouseTable),
}

/// Backtick: hop Home chat -> each match waiting on your move (nearest
/// deadline first) -> each house table you're seated at (roster order) ->
/// back to Home chat.
pub(crate) fn cycle_game_workspace(app: &mut App) -> bool {
    let current = match app.screen {
        Screen::Dashboard => GameWorkspace::Dashboard,
        Screen::DailyMatch => match app.daily.board.as_ref() {
            Some(board) => GameWorkspace::DailyBoard(board.match_id),
            None => GameWorkspace::Dashboard,
        },
        Screen::HouseTable => match app.house.open {
            Some(table) => GameWorkspace::HouseTable(table),
            None => GameWorkspace::Dashboard,
        },
        _ => return false,
    };
    let my_turn_ids: Vec<Uuid> = app
        .daily
        .my_turn_matches()
        .iter()
        .map(|item| item.id)
        .collect();
    let seated_tables = app.house.my_seated_tables();
    // Preserve where the first stop in the hop chain was opened from so
    // `q`/`Esc` still returns there after any number of backtick hops.
    let return_screen = match app.screen {
        Screen::DailyMatch => app
            .daily
            .board
            .as_ref()
            .map(|board| board.return_screen)
            .unwrap_or(Screen::Dashboard),
        Screen::HouseTable => app.house.return_screen,
        _ => Screen::Dashboard,
    };
    match next_workspace(&my_turn_ids, &seated_tables, current) {
        GameWorkspace::Dashboard => {
            match app.screen {
                Screen::Dashboard => {
                    app.banner = Some(Banner::error("No games waiting on you."));
                }
                // Wrap back to Home chat, no modal: this is the chat half of
                // the toggle, not a lobby visit.
                Screen::HouseTable => {
                    crate::app::lobby::house::input::leave_table(app, Screen::Dashboard);
                }
                _ => {
                    crate::app::lobby::daily::board_input::leave_board(app, Screen::Dashboard);
                }
            }
            true
        }
        GameWorkspace::DailyBoard(match_id) => {
            let Some(item) = app
                .daily
                .my_turn_matches()
                .into_iter()
                .find(|item| item.id == match_id)
                .cloned()
            else {
                return true;
            };
            app.daily.open_board(&item, return_screen);
            app.set_screen(Screen::DailyMatch);
            true
        }
        GameWorkspace::HouseTable(table) => {
            if app.house.enter(table, return_screen, app.chip_balance) {
                app.set_screen(Screen::HouseTable);
            }
            true
        }
    }
}

/// The stop after `current` in `[Home, boards..., tables...]`. A current
/// stop missing from the list (the turn just passed, the seat was lost)
/// restarts from the front so the hop chain keeps draining the queue
/// instead of bailing home early.
fn next_workspace(
    my_turn_ids: &[Uuid],
    seated_tables: &[HouseTable],
    current: GameWorkspace,
) -> GameWorkspace {
    let stops: Vec<GameWorkspace> = my_turn_ids
        .iter()
        .copied()
        .map(GameWorkspace::DailyBoard)
        .chain(seated_tables.iter().copied().map(GameWorkspace::HouseTable))
        .collect();
    let next = match current {
        GameWorkspace::Dashboard => stops.first(),
        current => match stops.iter().position(|stop| *stop == current) {
            Some(index) => stops.get(index + 1),
            None => stops.first(),
        },
    };
    next.copied().unwrap_or(GameWorkspace::Dashboard)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn from_home_enters_first_board() {
        assert_eq!(
            next_workspace(&[id(1), id(2)], &[], GameWorkspace::Dashboard),
            GameWorkspace::DailyBoard(id(1))
        );
    }

    #[test]
    fn from_home_with_no_stops_stays_home() {
        assert_eq!(
            next_workspace(&[], &[], GameWorkspace::Dashboard),
            GameWorkspace::Dashboard
        );
    }

    #[test]
    fn advances_through_boards_then_wraps_home() {
        let ids = [id(1), id(2)];
        assert_eq!(
            next_workspace(&ids, &[], GameWorkspace::DailyBoard(id(1))),
            GameWorkspace::DailyBoard(id(2))
        );
        assert_eq!(
            next_workspace(&ids, &[], GameWorkspace::DailyBoard(id(2))),
            GameWorkspace::Dashboard
        );
    }

    #[test]
    fn board_no_longer_my_turn_restarts_from_front() {
        // Just moved on match 1: it left the my-turn list, so the next hop
        // goes to the front of what's still waiting.
        assert_eq!(
            next_workspace(&[id(2), id(3)], &[], GameWorkspace::DailyBoard(id(1))),
            GameWorkspace::DailyBoard(id(2))
        );
    }

    #[test]
    fn last_board_gone_and_queue_empty_lands_home() {
        assert_eq!(
            next_workspace(&[], &[], GameWorkspace::DailyBoard(id(1))),
            GameWorkspace::Dashboard
        );
    }

    #[test]
    fn seated_tables_slot_after_your_turn_boards() {
        let tables = [HouseTable::Poker, HouseTable::Tron];
        assert_eq!(
            next_workspace(&[id(1)], &tables, GameWorkspace::DailyBoard(id(1))),
            GameWorkspace::HouseTable(HouseTable::Poker)
        );
        assert_eq!(
            next_workspace(
                &[id(1)],
                &tables,
                GameWorkspace::HouseTable(HouseTable::Poker)
            ),
            GameWorkspace::HouseTable(HouseTable::Tron)
        );
        assert_eq!(
            next_workspace(
                &[id(1)],
                &tables,
                GameWorkspace::HouseTable(HouseTable::Tron)
            ),
            GameWorkspace::Dashboard
        );
    }

    #[test]
    fn tables_only_cycle_works_without_boards() {
        let tables = [HouseTable::Blackjack];
        assert_eq!(
            next_workspace(&[], &tables, GameWorkspace::Dashboard),
            GameWorkspace::HouseTable(HouseTable::Blackjack)
        );
        assert_eq!(
            next_workspace(
                &[],
                &tables,
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
                GameWorkspace::HouseTable(HouseTable::Poker)
            ),
            GameWorkspace::DailyBoard(id(1))
        );
    }
}
