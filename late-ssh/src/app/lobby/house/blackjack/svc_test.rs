use super::*;
use crate::app::games::cards::{CardRank, CardSuit, PlayingCard};
use crate::app::lobby::house::blackjack::state::MIN_BET;

fn user_id() -> Uuid {
    Uuid::now_v7()
}

fn card(rank: CardRank) -> PlayingCard {
    PlayingCard {
        rank,
        suit: CardSuit::Spades,
    }
}

#[test]
fn settled_balance_for_user_uses_latest_balance() {
    let user = user_id();
    let other = user_id();
    let settled_balances = vec![
        SettledBalance {
            user_id: user,
            new_balance: 750,
        },
        SettledBalance {
            user_id: other,
            new_balance: 1200,
        },
        SettledBalance {
            user_id: user,
            new_balance: 1250,
        },
    ];

    assert_eq!(
        settled_balance_for_user(&settled_balances, user),
        Some(1250)
    );
}

#[test]
fn seats_allow_four_players() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    let users = (0..=MAX_SEATS).map(|_| user_id()).collect::<Vec<_>>();

    for (index, user_id) in users.iter().take(MAX_SEATS).enumerate() {
        assert_eq!(table.sit(*user_id).expect("seat should be open"), index);
    }

    assert!(matches!(
        table.sit(users[MAX_SEATS]),
        Err(SeatFailure::TableFull)
    ));
}

#[test]
fn same_user_cannot_take_two_seats() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    let user_id = user_id();

    assert_eq!(table.sit(user_id).expect("seat should be open"), 0);
    assert!(matches!(
        table.sit(user_id),
        Err(SeatFailure::AlreadySeated)
    ));
}

#[test]
fn betting_seat_cannot_leave_mid_hand() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    let user_id = user_id();
    let seat_index = table.sit(user_id).expect("seat should be open");
    table.seats[seat_index].bet = Some(Bet::new(MIN_BET).unwrap());
    table.phase = Phase::PlayerTurn;

    assert!(matches!(
        table.leave_seat(user_id),
        Err(SeatFailure::CannotLeaveWithBet)
    ));
    assert_eq!(table.user_seat_index(user_id), Some(0));
}

#[test]
fn betting_seat_can_leave_after_settlement() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    let user_id = user_id();
    let seat_index = table.sit(user_id).expect("seat should be open");
    table.seats[seat_index].bet = Some(Bet::new(MIN_BET).unwrap());
    table.seats[seat_index].last_outcome = Some(Outcome::Push);
    table.phase = Phase::Settling;

    assert_eq!(table.leave_seat(user_id).expect("leave should work"), 0);
    assert_eq!(table.user_seat_index(user_id), None);
    assert_eq!(table.phase, Phase::Settling);
}

#[test]
fn deal_requires_at_least_one_bet() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    table.sit(user_id()).expect("seat should be open");

    assert!(matches!(
        table.start_round(),
        Err(ActionFailure::InvalidPhase("at least one bet is required"))
    ));
}

#[test]
fn round_deals_each_betting_seat() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    let user_a = user_id();
    let user_b = user_id();
    let seat_a = table.sit(user_a).expect("seat should be open");
    let seat_b = table.sit(user_b).expect("seat should be open");
    table.seats[seat_a].bet = Some(Bet::new(MIN_BET).unwrap());
    table.seats[seat_b].bet = Some(Bet::new(MIN_BET).unwrap());

    let _ = table.start_round().expect("round should start");

    assert_eq!(table.dealer_hand.len(), 2);
    assert_eq!(table.seats[seat_a].hand.len(), 2);
    assert_eq!(table.seats[seat_b].hand.len(), 2);
    assert!(matches!(
        table.phase,
        Phase::PlayerTurn | Phase::DealerTurn | Phase::Settling
    ));
}

#[test]
fn round_player_count_tracks_betting_seats() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    assert_eq!(table.round_player_count, 0);

    let solo = table.sit(user_id()).expect("seat should be open");
    table.seats[solo].bet = Some(Bet::new(MIN_BET).unwrap());
    table.start_round().expect("round should start");
    // Solo play against the dealer must not earn quest credit.
    assert_eq!(table.round_player_count, 1);

    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    let seat_a = table.sit(user_id()).expect("seat should be open");
    let seat_b = table.sit(user_id()).expect("seat should be open");
    table.seats[seat_a].bet = Some(Bet::new(MIN_BET).unwrap());
    table.seats[seat_b].bet = Some(Bet::new(MIN_BET).unwrap());
    table.start_round().expect("round should start");
    assert_eq!(table.round_player_count, 2);
}

#[test]
fn stand_waits_for_other_unresolved_seats() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    let user_a = user_id();
    let user_b = user_id();
    let seat_a = table.sit(user_a).expect("seat should be open");
    let seat_b = table.sit(user_b).expect("seat should be open");
    table.seats[seat_a].bet = Some(Bet::new(MIN_BET).unwrap());
    table.seats[seat_b].bet = Some(Bet::new(MIN_BET).unwrap());
    table.phase = Phase::PlayerTurn;

    let settlements = table.stand_seat(seat_a).expect("seat can stand");

    assert!(settlements.is_empty());
    assert!(table.seats[seat_a].stood);
    assert_eq!(table.seats[seat_a].last_action, Some(SeatAction::Stand));
    assert!(!table.seats[seat_b].stood);
    assert_eq!(table.phase, Phase::PlayerTurn);
}

#[test]
fn double_down_doubles_bet_draws_once_and_stands() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    let user_id = user_id();
    let seat_index = table.sit(user_id).expect("seat should be open");
    table.seats[seat_index].bet = Some(Bet::new(MIN_BET).unwrap());
    table.seats[seat_index].hand = vec![card(CardRank::Number(10)), card(CardRank::Number(6))];
    table.dealer_hand = vec![card(CardRank::Number(10)), card(CardRank::Number(7))];
    table.shoe = Shoe::from_top(vec![card(CardRank::Number(2))]);
    table.phase = Phase::PlayerTurn;

    let extra_bet = table
        .prepare_double_down(seat_index)
        .expect("double should be available");
    let settlements = table
        .finish_double_down(seat_index, 900)
        .expect("double should finish");

    assert_eq!(extra_bet, MIN_BET);
    assert!(settlements.is_empty());
    assert_eq!(
        table.seats[seat_index].bet.map(Bet::amount),
        Some(MIN_BET * 2)
    );
    assert_eq!(table.seats[seat_index].hand.len(), 3);
    assert!(table.seats[seat_index].stood);
    assert!(!table.seats[seat_index].pending_double);
    assert_eq!(
        table.seats[seat_index].last_action,
        Some(SeatAction::Double)
    );
    assert_eq!(table.phase, Phase::DealerTurn);
}

#[test]
fn double_down_bust_settles_doubled_bet() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    let user_id = user_id();
    let seat_index = table.sit(user_id).expect("seat should be open");
    table.seats[seat_index].bet = Some(Bet::new(MIN_BET).unwrap());
    table.seats[seat_index].hand = vec![card(CardRank::Number(10)), card(CardRank::Number(9))];
    table.dealer_hand = vec![card(CardRank::Number(10)), card(CardRank::Number(7))];
    table.shoe = Shoe::from_top(vec![card(CardRank::Number(5))]);
    table.phase = Phase::PlayerTurn;

    table
        .prepare_double_down(seat_index)
        .expect("double should be available");
    let settlements = table
        .finish_double_down(seat_index, 900)
        .expect("double should finish");

    assert_eq!(settlements.len(), 1);
    assert_eq!(settlements[0].bet, MIN_BET * 2);
    assert_eq!(settlements[0].outcome, Outcome::DealerWin);
    assert_eq!(settlements[0].credit, 0);
    assert_eq!(
        table.seats[seat_index].last_outcome,
        Some(Outcome::DealerWin)
    );
    assert_eq!(table.seats[seat_index].last_net_change, -(MIN_BET * 2));
    assert_eq!(table.phase, Phase::Settling);
}

#[test]
fn dealer_turn_draws_one_card_per_step_before_settlement() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    let user_id = user_id();
    let seat_index = table.sit(user_id).expect("seat should be open");
    table.seats[seat_index].bet = Some(Bet::new(MIN_BET).unwrap());
    table.seats[seat_index].hand = vec![card(CardRank::Number(10)), card(CardRank::Number(7))];
    table.dealer_hand = vec![card(CardRank::Number(10)), card(CardRank::Number(6))];
    table.phase = Phase::PlayerTurn;

    let settlements = table.stand_seat(seat_index).expect("seat can stand");
    let dealer_turn_id = table
        .schedule_dealer_turn_if_needed()
        .expect("dealer turn should be scheduled");

    assert!(settlements.is_empty());
    assert_eq!(table.phase, Phase::DealerTurn);
    assert_eq!(table.dealer_hand.len(), 2);

    let step = table
        .dealer_step(dealer_turn_id)
        .expect("dealer step should match current turn");

    assert!(!step.done);
    assert!(step.settlements.is_empty());
    assert_eq!(table.phase, Phase::DealerTurn);
    assert_eq!(table.dealer_hand.len(), 3);
    assert_eq!(table.seats[seat_index].last_outcome, None);

    let mut final_step = None;
    for _ in 0..10 {
        let step = table
            .dealer_step(dealer_turn_id)
            .expect("dealer turn should still be current");
        if step.done {
            final_step = Some(step);
            break;
        }
    }
    let final_step = final_step.expect("dealer should eventually settle");

    assert_eq!(table.phase, Phase::Settling);
    assert_eq!(final_step.settlements.len(), 1);
    assert!(table.seats[seat_index].last_outcome.is_some());
}

#[test]
fn action_timeout_removes_unacted_seats_after_settlement() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    let user_id = user_id();
    let seat_index = table.sit(user_id).expect("seat should be open");
    table.seats[seat_index].bet = Some(Bet::new(MIN_BET).unwrap());
    table.seats[seat_index].hand = vec![card(CardRank::Number(10)), card(CardRank::Number(7))];
    table.dealer_hand = vec![card(CardRank::Number(10)), card(CardRank::Queen)];
    table.phase = Phase::PlayerTurn;

    let settlements = table.auto_stand_remaining();
    let dealer_turn_id = table
        .schedule_dealer_turn_if_needed()
        .expect("dealer turn should be scheduled");

    assert!(settlements.is_empty());
    assert_eq!(
        table.seats[seat_index].last_action,
        Some(SeatAction::MissedAction)
    );
    assert_eq!(table.phase, Phase::DealerTurn);

    let step = table
        .dealer_step(dealer_turn_id)
        .expect("dealer step should match current turn");

    assert!(step.done);
    assert_eq!(step.left_seats, vec![(user_id, seat_index)]);
    assert_eq!(table.user_seat_index(user_id), None);
    assert!(
        table
            .status_message
            .contains("Seat 1 missed the action timer and left.")
    );
}

#[test]
fn seated_player_auto_leaves_after_three_missed_deals() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    let active_user = user_id();
    let idle_user = user_id();
    let active_seat = table.sit(active_user).expect("seat should be open");
    let idle_seat = table.sit(idle_user).expect("seat should be open");

    for missed_deals in 1..MAX_MISSED_DEALS {
        table.seats[active_seat].bet = Some(Bet::new(MIN_BET).unwrap());
        table.start_round().expect("round should start");
        assert_eq!(table.user_seat_index(idle_user), Some(idle_seat));
        assert_eq!(table.seats[idle_seat].missed_deals, missed_deals);
        assert_eq!(
            table.seats[idle_seat].last_action,
            Some(SeatAction::MissedDeal)
        );
        table.reset_to_betting("next hand");
    }

    table.seats[active_seat].bet = Some(Bet::new(MIN_BET).unwrap());
    table.start_round().expect("round should start");

    assert_eq!(table.user_seat_index(idle_user), None);
    assert!(
        table
            .status_message
            .contains("Seat 2 missed 3 deals and left.")
    );
}

#[test]
fn seated_player_auto_leaves_after_five_minutes_idle() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    let user_id = user_id();
    let seat_index = table.sit(user_id).expect("seat should be open");
    let activity_generation = table.record_activity(user_id).expect("seat exists");

    table.seats[seat_index].last_activity =
        Instant::now() - Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS + 1);
    let kick = table
        .kick_inactive_user(user_id, activity_generation)
        .expect("idle seat should be kicked");

    assert_eq!(kick.left_seats, vec![(user_id, seat_index)]);
    assert!(kick.settlements.is_empty());
    assert_eq!(table.user_seat_index(user_id), None);
    assert!(
        table
            .status_message
            .contains("Seat 1 idle for 5m and left.")
    );
}

#[test]
fn seated_player_activity_generation_blocks_stale_idle_kick() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    let user_id = user_id();
    let seat_index = table.sit(user_id).expect("seat should be open");
    let stale_generation = table.record_activity(user_id).expect("seat exists");
    let fresh_generation = table.record_activity(user_id).expect("seat exists");

    table.seats[seat_index].last_activity =
        Instant::now() - Duration::from_secs(SEAT_IDLE_TIMEOUT_SECS + 1);

    assert!(
        table
            .kick_inactive_user(user_id, stale_generation)
            .is_none()
    );
    assert_eq!(table.user_seat_index(user_id), Some(seat_index));
    assert_ne!(stale_generation, fresh_generation);
}

#[test]
fn betting_countdown_starts_once_as_hard_cap() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());

    let first_id = table.ensure_betting_countdown();
    let first_deadline = table.betting_deadline.expect("deadline should be set");
    let second_id = table.ensure_betting_countdown();
    let second_deadline = table.betting_deadline.expect("deadline should be set");

    assert_eq!(first_id, second_id);
    assert_eq!(second_deadline, first_deadline);
    assert!(table.countdown_matches(second_id));
}

#[test]
fn all_seated_bets_ready_when_every_seated_player_has_locked_bet() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    let user_a = user_id();
    let user_b = user_id();
    let seat_a = table.sit(user_a).expect("seat should be open");
    let seat_b = table.sit(user_b).expect("seat should be open");

    table.seats[seat_a].bet = Some(Bet::new(MIN_BET).unwrap());
    assert!(!table.all_seated_bets_ready());

    table.seats[seat_b].bet = Some(Bet::new(MIN_BET).unwrap());
    assert!(table.all_seated_bets_ready());
}

#[test]
fn thrown_stake_chips_are_visible_on_seat_snapshot() {
    let mut table = SharedTableState::new(BlackjackTableSettings::default());
    let user_id = user_id();
    let seat_index = table.sit(user_id).expect("seat should be open");

    table
        .throw_chip(user_id, MIN_BET)
        .expect("chip should be accepted");

    let snapshot = table.snapshot();
    assert_eq!(snapshot.seats[seat_index].stake_chips, vec![MIN_BET]);
    assert_eq!(snapshot.seats[seat_index].bet_amount, None);
}
