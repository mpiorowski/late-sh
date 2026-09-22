use super::*;

fn test_state() -> State {
    let db = late_core::db::Db::new(&late_core::db::DbConfig::default()).expect("lazy db");
    State::new(
        Uuid::nil(),
        SolitaireService::new(db, tokio::sync::broadcast::channel(4).0),
        Vec::new(),
    )
}

#[test]
fn reset_confirmation_is_per_action_kind() {
    let mut state = test_state();

    // Two presses of the same key confirm and fire.
    assert!(!state.request_reset(ResetKind::Reset));
    assert!(state.request_reset(ResetKind::Reset));
    assert_eq!(state.reset_pending, None);

    // A press for a different kind re-arms for that kind instead of
    // firing the originally-armed action.
    assert!(!state.request_reset(ResetKind::NewBoard));
    assert!(!state.request_reset(ResetKind::Reset));
    assert_eq!(state.reset_pending, Some(ResetKind::Reset));
    assert!(state.request_reset(ResetKind::Reset));
    assert_eq!(state.reset_pending, None);
}

#[test]
fn seeded_deal_uses_full_deck() {
    let snapshot = snapshot_from_seed(42);
    let count = snapshot.stock.len()
        + snapshot.waste.len()
        + snapshot.foundations.iter().map(Vec::len).sum::<usize>()
        + snapshot.tableau.iter().map(Vec::len).sum::<usize>();
    assert_eq!(count, 52);
    assert_eq!(snapshot.stock.len(), 24);
}

#[test]
fn draw_one_draws_one_card() {
    let mut stock = vec![
        Card {
            suit: Suit::Hearts,
            rank: 1,
        },
        Card {
            suit: Suit::Spades,
            rank: 13,
        },
    ];
    let mut waste = Vec::new();
    assert!(draw_stock_once(&mut stock, &mut waste, 1));
    assert_eq!(stock.len(), 1);
    assert_eq!(waste.len(), 1);
}

#[test]
fn draw_three_draws_up_to_three_cards() {
    let mut stock = vec![
        Card {
            suit: Suit::Hearts,
            rank: 1,
        },
        Card {
            suit: Suit::Spades,
            rank: 13,
        },
        Card {
            suit: Suit::Clubs,
            rank: 7,
        },
        Card {
            suit: Suit::Diamonds,
            rank: 10,
        },
    ];
    let mut waste = Vec::new();
    assert!(draw_stock_once(&mut stock, &mut waste, 3));
    assert_eq!(stock.len(), 1);
    assert_eq!(waste.len(), 3);
    assert_eq!(waste.last().map(|card| card.rank), Some(13));
}

#[test]
fn moving_from_tableau_reveals_next_card() {
    let mut state = test_state();
    state.tableau[0] = vec![TableauCard {
        card: Card {
            suit: Suit::Clubs,
            rank: 8,
        },
        face_up: true,
    }];
    state.tableau[1] = vec![
        TableauCard {
            card: Card {
                suit: Suit::Hearts,
                rank: 8,
            },
            face_up: false,
        },
        TableauCard {
            card: Card {
                suit: Suit::Hearts,
                rank: 7,
            },
            face_up: true,
        },
    ];

    assert!(state.try_move(Selection::Tableau { col: 1, row: 1 }, Focus::Tableau(0, 0)));
    assert!(state.tableau[1][0].face_up);
}

#[test]
fn ace_can_move_to_matching_foundation() {
    let mut state = test_state();
    state.waste = vec![Card {
        suit: Suit::Spades,
        rank: 1,
    }];
    assert!(state.try_move(Selection::Waste, Focus::Foundation(3)));
}

fn complete_foundations() -> [Vec<Card>; 4] {
    let suits = [Suit::Hearts, Suit::Diamonds, Suit::Clubs, Suit::Spades];
    array::from_fn(|i| {
        (1..=13)
            .map(|rank| Card {
                suit: suits[i],
                rank,
            })
            .collect()
    })
}

#[test]
fn win_animation_triggers_on_game_won() {
    let mut state = test_state();
    state.mode = Mode::Personal;
    state.foundations = complete_foundations();
    assert!(!state.is_game_over);
    assert!(state.win_anim.is_none());

    state.check_for_win();
    assert!(state.is_game_over);
    assert!(state.win_anim.is_some());

    let anim = state.win_anim.as_ref().unwrap();
    assert!(anim.active);
    assert!(!anim.completed);
    assert!(anim.current_card.is_some());
    // 52 total cards: 1 active bouncing, 51 pending
    assert_eq!(anim.pending_cards.len(), 51);
    assert_eq!(anim.unpeeled_counts.iter().sum::<usize>(), 51);
}

#[test]
fn win_animation_steps_physics_and_stamps_canvas() {
    let foundations = complete_foundations();
    let mut anim = WinAnimationState::new(&foundations, 12345);
    let initial_card = anim.current_card.clone().expect("initial card");

    let stamped_before: usize = anim
        .stamp_canvas
        .iter()
        .map(|row| row.iter().filter(|c| c.is_some()).count())
        .sum();
    assert_eq!(stamped_before, 0);

    assert!(anim.step());

    let stamped_after: usize = anim
        .stamp_canvas
        .iter()
        .map(|row| row.iter().filter(|c| c.is_some()).count())
        .sum();
    assert!(stamped_after > 0, "canvas has stamped cells");

    let stepped_card = anim.current_card.as_ref().expect("card still active");
    assert_ne!(stepped_card.x, initial_card.x);
    assert_ne!(stepped_card.y, initial_card.y);
}

#[test]
fn win_animation_floor_bounce_restitution() {
    let foundations = complete_foundations();
    let mut anim = WinAnimationState::new(&foundations, 12345);
    let view_h = (anim.viewport_height.load(Ordering::Relaxed) as usize)
        .min(BOARD_HEIGHT)
        .max(CARD_HEIGHT + 2);
    let floor_y = (view_h.saturating_sub(CARD_HEIGHT)) as f32;

    anim.current_card = Some(BouncingCard {
        card: Card {
            suit: Suit::Hearts,
            rank: 13,
        },
        x: 30.0,
        y: floor_y - 0.5,
        vx: 1.0,
        vy: 3.0,
    });

    assert!(anim.step());
    let card = anim.current_card.as_ref().expect("bouncing card");
    assert!(
        card.vy < 0.0,
        "card rebounded with negative vy on floor hit"
    );
    assert_eq!(card.y, floor_y);
}

#[test]
fn win_animation_dismissal_on_input() {
    let mut state = test_state();
    state.mode = Mode::Personal;
    state.foundations = complete_foundations();
    state.check_for_win();
    assert!(state.win_anim.as_ref().is_some_and(|a| !a.completed));

    // Pressing Space dismisses the animation immediately
    let handled = crate::app::arcade::solitaire::input::handle_key(&mut state, b' ');
    assert!(handled);
    assert!(state.win_anim.as_ref().is_some_and(|a| a.completed));
}

#[test]
fn foundation_top_reflects_unpeeled_cards() {
    let mut state = test_state();
    state.mode = Mode::Personal;
    state.foundations = complete_foundations();
    state.check_for_win();

    let anim = state.win_anim.as_mut().unwrap();
    // First card peeled off foundation 0 (King of Hearts)
    // So foundation 0 now has 12 cards left (Queen of Hearts on top)
    assert_eq!(anim.unpeeled_counts[0], 12);
    assert_eq!(
        state.foundation_top(0),
        Some(Card {
            suit: Suit::Hearts,
            rank: 12,
        })
    );
}

#[test]
fn win_animation_eventually_completes_when_all_cards_exit() {
    let mut state = test_state();
    state.mode = Mode::Personal;
    state.foundations = complete_foundations();
    state.check_for_win();

    let anim = state.win_anim.as_mut().unwrap();
    anim.pending_cards.clear();
    if let Some(card) = &mut anim.current_card {
        card.x = (BOARD_WIDTH + 10) as f32;
    }

    assert!(anim.step());
    assert!(anim.completed);
    assert!(anim.current_card.is_none());
    assert!(!state.tick());
}
