use super::*;

fn c(rank: CardRank, suit: CardSuit) -> PlayingCard {
    PlayingCard { rank, suit }
}

fn uid(value: u128) -> Uuid {
    Uuid::from_u128(value)
}

fn seat_player(state: &mut SharedState, index: usize, user: Uuid, cards: Vec<PlayingCard>) {
    state.seats[index] = Some(user);
    state.balances[index] = 1_000;
    state.hole_cards[index] = cards;
}

fn credits_by_user(settlements: Vec<PokerSettlement>) -> HashMap<Uuid, i64> {
    settlements
        .into_iter()
        .map(|settlement| (settlement.user_id, settlement.credit))
        .collect()
}

#[test]
fn ace_low_straight_is_scored_as_five_high() {
    let hand = evaluate_best_hand(&[
        c(CardRank::Ace, CardSuit::Spades),
        c(CardRank::Number(2), CardSuit::Hearts),
        c(CardRank::Number(3), CardSuit::Clubs),
        c(CardRank::Number(4), CardSuit::Diamonds),
        c(CardRank::Number(5), CardSuit::Spades),
        c(CardRank::King, CardSuit::Hearts),
        c(CardRank::Queen, CardSuit::Hearts),
    ]);

    assert_eq!(hand.value.category, 4);
    assert_eq!(hand.value.ranks[0], 5);
}

#[test]
fn full_house_beats_flush() {
    let full_house = evaluate_best_hand(&[
        c(CardRank::Ace, CardSuit::Spades),
        c(CardRank::Ace, CardSuit::Hearts),
        c(CardRank::Ace, CardSuit::Clubs),
        c(CardRank::King, CardSuit::Diamonds),
        c(CardRank::King, CardSuit::Spades),
    ]);
    let flush = evaluate_best_hand(&[
        c(CardRank::Ace, CardSuit::Hearts),
        c(CardRank::Number(9), CardSuit::Hearts),
        c(CardRank::Number(7), CardSuit::Hearts),
        c(CardRank::Number(4), CardSuit::Hearts),
        c(CardRank::Number(2), CardSuit::Hearts),
    ]);

    assert!(full_house.value > flush.value);
}

#[test]
fn side_pots_pay_short_all_in_and_side_winner() {
    let mut state = SharedState::new(uid(100));
    seat_player(
        &mut state,
        0,
        uid(1),
        vec![
            c(CardRank::Ace, CardSuit::Spades),
            c(CardRank::Ace, CardSuit::Hearts),
        ],
    );
    seat_player(
        &mut state,
        1,
        uid(2),
        vec![
            c(CardRank::King, CardSuit::Spades),
            c(CardRank::King, CardSuit::Hearts),
        ],
    );
    seat_player(
        &mut state,
        2,
        uid(3),
        vec![
            c(CardRank::Queen, CardSuit::Spades),
            c(CardRank::Queen, CardSuit::Hearts),
        ],
    );
    state.community = vec![
        c(CardRank::Number(2), CardSuit::Clubs),
        c(CardRank::Number(4), CardSuit::Diamonds),
        c(CardRank::Number(7), CardSuit::Clubs),
        c(CardRank::Number(9), CardSuit::Diamonds),
        c(CardRank::Jack, CardSuit::Clubs),
    ];
    state.committed = [50, 100, 100, 0];

    let settlements = state.finish_showdown();
    let credits = credits_by_user(settlements);

    assert_eq!(credits.get(&uid(1)), Some(&150));
    assert_eq!(credits.get(&uid(2)), Some(&100));
    assert_eq!(credits.get(&uid(3)), Some(&0));
}

#[test]
fn short_all_in_raise_does_not_reopen_prior_actor_raises() {
    let mut state = SharedState::new(uid(100));
    seat_player(
        &mut state,
        0,
        uid(1),
        vec![
            c(CardRank::Ace, CardSuit::Spades),
            c(CardRank::Ace, CardSuit::Hearts),
        ],
    );
    seat_player(
        &mut state,
        1,
        uid(2),
        vec![
            c(CardRank::King, CardSuit::Spades),
            c(CardRank::King, CardSuit::Hearts),
        ],
    );
    seat_player(
        &mut state,
        2,
        uid(3),
        vec![
            c(CardRank::Queen, CardSuit::Spades),
            c(CardRank::Queen, CardSuit::Hearts),
        ],
    );
    state.phase = PokerPhase::PreFlop;
    state.active_seat = Some(2);
    state.current_bet = 100;
    state.min_raise = 100;
    state.committed = [100, 100, 0, 0];
    state.street_bet = [100, 100, 0, 0];
    state.acted_this_street = [true, true, false, false];
    state.balances[2] = 150;

    let short_all_in = match state.all_in(uid(3)) {
        ActionOutcome::Commit(request) => request,
        _ => panic!("short all-in should commit chips"),
    };
    assert_eq!(short_all_in.amount, 150);

    let settlements = state.apply_commit_success(short_all_in, 0);

    assert!(settlements.is_empty());
    assert_eq!(state.current_bet, 150);
    assert_eq!(state.min_raise, 100);
    assert_eq!(state.active_seat, Some(0));
    assert!(state.acted_this_street[0]);
    assert!(state.acted_this_street[1]);
    assert!(!state.can_raise(0));

    assert!(matches!(
        state.bet_or_raise(uid(1), 100),
        ActionOutcome::None
    ));
    let call = match state.call_or_check(uid(1)) {
        ActionOutcome::Commit(request) => request,
        _ => panic!("prior actor should still be able to call the extra chips"),
    };
    assert_eq!(call.amount, 50);
}

#[test]
fn four_way_all_in_side_pots_pay_each_level() {
    let mut state = SharedState::new(uid(100));
    seat_player(
        &mut state,
        0,
        uid(1),
        vec![
            c(CardRank::Ace, CardSuit::Spades),
            c(CardRank::Ace, CardSuit::Hearts),
        ],
    );
    seat_player(
        &mut state,
        1,
        uid(2),
        vec![
            c(CardRank::King, CardSuit::Spades),
            c(CardRank::King, CardSuit::Hearts),
        ],
    );
    seat_player(
        &mut state,
        2,
        uid(3),
        vec![
            c(CardRank::Queen, CardSuit::Spades),
            c(CardRank::Queen, CardSuit::Hearts),
        ],
    );
    seat_player(
        &mut state,
        3,
        uid(4),
        vec![
            c(CardRank::Jack, CardSuit::Spades),
            c(CardRank::Jack, CardSuit::Hearts),
        ],
    );
    state.community = vec![
        c(CardRank::Number(2), CardSuit::Clubs),
        c(CardRank::Number(4), CardSuit::Diamonds),
        c(CardRank::Number(7), CardSuit::Clubs),
        c(CardRank::Number(9), CardSuit::Diamonds),
        c(CardRank::Number(10), CardSuit::Clubs),
    ];
    state.committed = [25, 50, 100, 200];

    let credits = credits_by_user(state.finish_showdown());

    assert_eq!(credits.get(&uid(1)), Some(&100));
    assert_eq!(credits.get(&uid(2)), Some(&75));
    assert_eq!(credits.get(&uid(3)), Some(&100));
    assert_eq!(credits.get(&uid(4)), Some(&100));
}

#[test]
fn tied_side_pot_splits_only_among_eligible_players() {
    let mut state = SharedState::new(uid(100));
    seat_player(
        &mut state,
        0,
        uid(1),
        vec![
            c(CardRank::Ace, CardSuit::Spades),
            c(CardRank::King, CardSuit::Hearts),
        ],
    );
    seat_player(
        &mut state,
        1,
        uid(2),
        vec![
            c(CardRank::Queen, CardSuit::Spades),
            c(CardRank::Jack, CardSuit::Hearts),
        ],
    );
    seat_player(
        &mut state,
        2,
        uid(3),
        vec![
            c(CardRank::Number(10), CardSuit::Spades),
            c(CardRank::Number(9), CardSuit::Hearts),
        ],
    );
    state.community = vec![
        c(CardRank::Number(2), CardSuit::Clubs),
        c(CardRank::Number(3), CardSuit::Diamonds),
        c(CardRank::Number(4), CardSuit::Clubs),
        c(CardRank::Number(5), CardSuit::Diamonds),
        c(CardRank::Number(6), CardSuit::Clubs),
    ];
    state.committed = [50, 100, 100, 0];

    let credits = credits_by_user(state.finish_showdown());

    assert_eq!(credits.get(&uid(1)), Some(&50));
    assert_eq!(credits.get(&uid(2)), Some(&100));
    assert_eq!(credits.get(&uid(3)), Some(&100));
}

#[test]
fn fold_win_does_not_reveal_winner_cards() {
    let mut state = SharedState::new(uid(100));
    seat_player(
        &mut state,
        0,
        uid(1),
        vec![
            c(CardRank::Ace, CardSuit::Spades),
            c(CardRank::Ace, CardSuit::Hearts),
        ],
    );
    seat_player(
        &mut state,
        1,
        uid(2),
        vec![
            c(CardRank::King, CardSuit::Spades),
            c(CardRank::King, CardSuit::Hearts),
        ],
    );
    state.committed = [20, 20, 0, 0];
    state.folded[1] = true;

    let settlements = state.finish_by_fold(0);

    assert_eq!(settlements.len(), 2);
    assert!(state.seat_snapshot(0).revealed_cards.is_none());
    assert_eq!(state.winners, vec![0]);
}

#[test]
fn configured_blinds_drive_forced_commits() {
    let mut state = SharedState::new_with_settings(
        uid(100),
        PokerTableSettings {
            pace: Default::default(),
            small_blind: 50,
            starting_stack: 1_000,
        },
    );
    state.seats[0] = Some(uid(1));
    state.seats[1] = Some(uid(2));
    state.balances[0] = 1_000;
    state.balances[1] = 1_000;

    let requests = state.start_hand(uid(1));
    let amounts = requests
        .iter()
        .map(|request| request.amount)
        .collect::<Vec<_>>();

    assert_eq!(amounts, vec![50, 100]);
    assert_eq!(state.public_snapshot().small_blind, 50);
    assert_eq!(state.public_snapshot().big_blind, 100);
    assert_eq!(state.min_raise, 100);
}

#[test]
fn sit_uses_fixed_starting_stack_instead_of_global_balance() {
    let mut state = SharedState::new_with_settings(
        uid(100),
        PokerTableSettings {
            starting_stack: 5_000,
            ..Default::default()
        },
    );

    let rich_seat = state.sit(uid(1), 10_000);
    let short_seat = state.sit(uid(2), 1_000);

    assert_eq!(rich_seat, Some(0));
    assert_eq!(state.balances[0], 5_000);
    assert_eq!(state.global_balances.get(&uid(1)), Some(&10_000));
    assert_eq!(short_seat, None);
    assert_eq!(state.seats[1], None);
    assert!(state.status_message.contains("Need 5000 chips"));
}

#[test]
fn committed_chips_reduce_table_stack_not_to_global_balance() {
    let mut state = SharedState::new(uid(100));
    state.seats[0] = Some(uid(1));
    state.balances[0] = 1_000;
    let request = state.set_pending_commit(0, CommitKind::BetRaise, 100, 100, 100);

    let settlements = state.apply_commit_success(request, 9_900);

    assert!(settlements.is_empty());
    assert_eq!(state.balances[0], 900);
    assert_eq!(state.global_balances.get(&uid(1)), Some(&9_900));
    assert_eq!(state.committed[0], 100);
}

#[test]
fn external_balance_drop_clamps_seated_stack_before_commit() {
    let mut state = SharedState::new(uid(100));
    seat_player(
        &mut state,
        0,
        uid(1),
        vec![
            c(CardRank::Ace, CardSuit::Spades),
            c(CardRank::King, CardSuit::Spades),
        ],
    );
    seat_player(
        &mut state,
        1,
        uid(2),
        vec![
            c(CardRank::Queen, CardSuit::Hearts),
            c(CardRank::Jack, CardSuit::Hearts),
        ],
    );
    state.phase = PokerPhase::Flop;
    state.active_seat = Some(0);

    assert!(state.sync_balance(uid(1), 300));
    assert_eq!(state.global_balances.get(&uid(1)), Some(&300));
    assert_eq!(state.balances[0], 300);

    let request = match state.all_in(uid(1)) {
        ActionOutcome::Commit(request) => request,
        _ => panic!("all-in should commit the clamped stack"),
    };

    assert_eq!(request.amount, 300);
    let settlements = state.apply_commit_success(request, 0);
    assert!(settlements.is_empty());
    assert_eq!(state.balances[0], 0);
    assert_eq!(state.committed[0], 300);
    assert_eq!(state.global_balances.get(&uid(1)), Some(&0));
}

#[test]
fn settlement_credit_adds_to_table_stack() {
    let mut state = SharedState::new(uid(100));
    state.seats[0] = Some(uid(1));
    state.balances[0] = 900;

    state.complete_settlements(vec![PokerSettlementUpdate {
        user_id: uid(1),
        credit: 250,
        global_balance: 10_150,
    }]);

    assert_eq!(state.balances[0], 1_150);
    assert_eq!(state.global_balances.get(&uid(1)), Some(&10_150));
}

#[test]
fn player_can_sit_during_active_hand_and_waits_for_next_deal() {
    let mut state = SharedState::new(uid(100));
    seat_player(
        &mut state,
        0,
        uid(1),
        vec![
            c(CardRank::Ace, CardSuit::Spades),
            c(CardRank::King, CardSuit::Spades),
        ],
    );
    seat_player(
        &mut state,
        1,
        uid(2),
        vec![
            c(CardRank::Queen, CardSuit::Hearts),
            c(CardRank::Jack, CardSuit::Hearts),
        ],
    );
    state.phase = PokerPhase::Flop;
    state.active_seat = Some(0);

    let seat = state.sit(uid(3), 1_500);

    assert_eq!(seat, Some(2));
    assert_eq!(state.seats[2], Some(uid(3)));
    assert_eq!(state.balances[2], 1_000);
    assert!(state.hole_cards[2].is_empty());
    assert!(!state.seat_snapshot(2).in_hand);
    assert_eq!(state.active_player_indices(), vec![0, 1]);
    assert_eq!(state.active_seat, Some(0));
    assert!(state.status_message.contains("next hand"));
}

#[test]
fn auto_check_fold_checks_for_free_and_starts_next_countdown() {
    let mut state = SharedState::new(uid(100));
    seat_player(
        &mut state,
        0,
        uid(1),
        vec![
            c(CardRank::Ace, CardSuit::Spades),
            c(CardRank::King, CardSuit::Spades),
        ],
    );
    seat_player(
        &mut state,
        1,
        uid(2),
        vec![
            c(CardRank::Queen, CardSuit::Hearts),
            c(CardRank::Jack, CardSuit::Hearts),
        ],
    );
    state.phase = PokerPhase::Flop;
    state.active_seat = Some(0);
    state.auto_check_fold[0] = true;

    let (settlements, countdown_id) = state.apply_auto_check_folds_and_start_countdown();

    assert!(settlements.is_empty());
    assert_eq!(state.last_action[0], Some(PokerAction::Check));
    assert!(!state.folded[0]);
    assert_eq!(state.active_seat, Some(1));
    assert_eq!(state.action_countdown_seat, Some(1));
    assert!(countdown_id.is_some());
    assert!(state.status_message.contains("auto-checked"));
}

#[test]
fn auto_check_fold_folds_when_call_is_owed() {
    let mut state = SharedState::new(uid(100));
    seat_player(
        &mut state,
        0,
        uid(1),
        vec![
            c(CardRank::Ace, CardSuit::Spades),
            c(CardRank::King, CardSuit::Spades),
        ],
    );
    seat_player(
        &mut state,
        1,
        uid(2),
        vec![
            c(CardRank::Queen, CardSuit::Hearts),
            c(CardRank::Jack, CardSuit::Hearts),
        ],
    );
    state.phase = PokerPhase::Flop;
    state.active_seat = Some(0);
    state.current_bet = 20;
    state.street_bet = [0, 20, 0, 0];
    state.committed = [0, 20, 0, 0];
    state.auto_check_fold[0] = true;

    let (settlements, countdown_id) = state.apply_auto_check_folds_and_start_countdown();
    let credits = credits_by_user(settlements);

    assert!(countdown_id.is_none());
    assert_eq!(state.last_action[0], Some(PokerAction::Fold));
    assert!(state.folded[0]);
    assert_eq!(state.phase, PokerPhase::Showdown);
    assert_eq!(state.winners, vec![1]);
    assert_eq!(credits.get(&uid(2)), Some(&20));
    assert!(state.status_message.contains("auto-folded"));
}

#[test]
fn action_timeout_checks_when_nothing_is_owed() {
    let mut state = SharedState::new(uid(100));
    seat_player(
        &mut state,
        0,
        uid(1),
        vec![
            c(CardRank::Ace, CardSuit::Spades),
            c(CardRank::King, CardSuit::Spades),
        ],
    );
    seat_player(
        &mut state,
        1,
        uid(2),
        vec![
            c(CardRank::Queen, CardSuit::Hearts),
            c(CardRank::Jack, CardSuit::Hearts),
        ],
    );
    state.phase = PokerPhase::Flop;
    state.active_seat = Some(0);
    let countdown_id = state.start_action_countdown_if_needed().unwrap();
    state.action_deadline = Some(Instant::now() - Duration::from_secs(1));

    let settlements = state.timeout_active_action(countdown_id).unwrap();

    assert!(settlements.is_empty());
    assert_eq!(state.last_action[0], Some(PokerAction::Check));
    assert!(!state.folded[0]);
    assert_eq!(state.missed_actions[0], 1);
    assert_eq!(state.active_seat, Some(1));
}

#[test]
fn action_timeout_folds_when_call_is_owed() {
    let mut state = SharedState::new(uid(100));
    seat_player(
        &mut state,
        0,
        uid(1),
        vec![
            c(CardRank::Ace, CardSuit::Spades),
            c(CardRank::King, CardSuit::Spades),
        ],
    );
    seat_player(
        &mut state,
        1,
        uid(2),
        vec![
            c(CardRank::Queen, CardSuit::Hearts),
            c(CardRank::Jack, CardSuit::Hearts),
        ],
    );
    state.phase = PokerPhase::Flop;
    state.active_seat = Some(0);
    state.current_bet = 20;
    state.street_bet = [0, 20, 0, 0];
    state.committed = [0, 20, 0, 0];
    let countdown_id = state.start_action_countdown_if_needed().unwrap();
    state.action_deadline = Some(Instant::now() - Duration::from_secs(1));

    let settlements = state.timeout_active_action(countdown_id).unwrap();
    let credits = credits_by_user(settlements);

    assert!(state.folded[0]);
    assert_eq!(state.last_action[0], Some(PokerAction::Fold));
    assert_eq!(state.phase, PokerPhase::Showdown);
    assert_eq!(state.winners, vec![1]);
    assert_eq!(credits.get(&uid(2)), Some(&20));
}

#[test]
fn third_missed_action_marks_player_to_leave_before_next_hand() {
    let mut state = SharedState::new(uid(100));
    seat_player(
        &mut state,
        0,
        uid(1),
        vec![
            c(CardRank::Ace, CardSuit::Spades),
            c(CardRank::King, CardSuit::Spades),
        ],
    );
    seat_player(
        &mut state,
        1,
        uid(2),
        vec![
            c(CardRank::Queen, CardSuit::Hearts),
            c(CardRank::Jack, CardSuit::Hearts),
        ],
    );
    state.phase = PokerPhase::Flop;
    state.missed_actions[0] = MAX_MISSED_ACTIONS - 1;
    state.active_seat = Some(0);
    let countdown_id = state.start_action_countdown_if_needed().unwrap();
    state.action_deadline = Some(Instant::now() - Duration::from_secs(1));

    let _ = state.timeout_active_action(countdown_id);

    assert!(state.leave_after_hand[0]);
    state.phase = PokerPhase::Showdown;
    state.settlement_pending = false;
    let _ = state.start_hand(uid(1));
    assert_eq!(state.seats[0], None);
}

#[test]
fn manual_action_resets_missed_action_count() {
    let mut state = SharedState::new(uid(100));
    seat_player(
        &mut state,
        0,
        uid(1),
        vec![
            c(CardRank::Ace, CardSuit::Spades),
            c(CardRank::King, CardSuit::Spades),
        ],
    );
    seat_player(
        &mut state,
        1,
        uid(2),
        vec![
            c(CardRank::Queen, CardSuit::Hearts),
            c(CardRank::Jack, CardSuit::Hearts),
        ],
    );
    state.phase = PokerPhase::Flop;
    state.active_seat = Some(0);
    state.missed_actions[0] = 2;

    let _ = state.call_or_check(uid(1));

    assert_eq!(state.missed_actions[0], 0);
    assert_eq!(state.last_action[0], Some(PokerAction::Check));
}
