use crate::app::activity::event::ActivityEvent;
use crate::app::arcade::le_word::input::{handle_arrow, handle_key};
use crate::app::arcade::le_word::state::*;
use crate::app::arcade::le_word::svc::LeWordService;
use crate::test_helpers::new_test_db;
use late_core::models::le_word::{DailyWin, DailyWord, Game};
use late_core::test_utils::create_test_user;
use tokio::sync::broadcast;

#[test]
fn score_guess_handles_duplicate_letters() {
    assert_eq!(
        score_guess("allee", "apple"),
        [
            LetterScore::Correct,
            LetterScore::Present,
            LetterScore::Absent,
            LetterScore::Absent,
            LetterScore::Correct,
        ]
    );
    assert_eq!(
        score_guess("sassy", "abyss"),
        [
            LetterScore::Present,
            LetterScore::Present,
            LetterScore::Absent,
            LetterScore::Correct,
            LetterScore::Present,
        ]
    );
}

#[test]
fn score_guess_matches_shade_screenshot_case() {
    assert_eq!(
        score_guess("wormy", "shade"),
        [
            LetterScore::Absent,
            LetterScore::Absent,
            LetterScore::Absent,
            LetterScore::Absent,
            LetterScore::Absent,
        ]
    );
    assert_eq!(
        score_guess("adieu", "shade"),
        [
            LetterScore::Present,
            LetterScore::Present,
            LetterScore::Absent,
            LetterScore::Present,
            LetterScore::Absent,
        ]
    );
    assert_eq!(
        score_guess("adeem", "shade"),
        [
            LetterScore::Present,
            LetterScore::Present,
            LetterScore::Present,
            LetterScore::Absent,
            LetterScore::Absent,
        ]
    );
    assert_eq!(
        score_guess("house", "shade"),
        [
            LetterScore::Present,
            LetterScore::Absent,
            LetterScore::Absent,
            LetterScore::Present,
            LetterScore::Correct,
        ]
    );
}

#[test]
fn score_letter_from_guesses_keeps_best_keyboard_hint() {
    let guesses = vec!["allee".to_string(), "sassy".to_string()];

    assert_eq!(
        score_letter_from_guesses(&guesses, "apple", 'a'),
        Some(LetterScore::Correct)
    );
    assert_eq!(
        score_letter_from_guesses(&guesses, "apple", 'l'),
        Some(LetterScore::Present)
    );
    assert_eq!(
        score_letter_from_guesses(&guesses, "apple", 's'),
        Some(LetterScore::Absent)
    );
    assert_eq!(score_letter_from_guesses(&guesses, "apple", 'z'), None);
}

#[tokio::test]
async fn replay_restores_the_in_progress_daily_board() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "le-word-state-modes").await;
    let (activity_tx, _) = broadcast::channel::<ActivityEvent>(8);
    let svc = LeWordService::new(test_db.db.clone(), activity_tx);
    let today = svc.today();
    let daily_word = DailyWord {
        id: uuid::Uuid::now_v7(),
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        puzzle_date: today,
        answer_word: "hunch".to_string(),
    };
    let mut state = State::new(user.id, svc, Some(daily_word), Vec::new());
    state.current_guess = "glass".to_string();

    state.show_replay();

    assert_eq!(state.mode, Mode::Replay);
    assert!(!state.is_daily_active());
    assert_ne!(state.answer, "hunch");
    assert!(state.current_guess.is_empty());
    let first_replay_answer = state.answer.clone();

    state.new_replay();

    assert_ne!(state.answer, first_replay_answer);
    state.show_daily();
    assert_eq!(state.mode, Mode::Daily);
    assert!(state.is_daily_active());
    assert_eq!(state.answer, "hunch");
    assert_eq!(state.current_guess, "glass");
}

#[tokio::test]
async fn saved_replay_board_is_restored_after_reconnect() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "le-word-replay-restore").await;
    let (activity_tx, _) = broadcast::channel::<ActivityEvent>(8);
    let svc = LeWordService::new(test_db.db.clone(), activity_tx);
    let today = svc.today();
    let daily_word = DailyWord {
        id: uuid::Uuid::now_v7(),
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        puzzle_date: today,
        answer_word: "hunch".to_string(),
    };
    let replay_game = Game {
        id: uuid::Uuid::now_v7(),
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        user_id: user.id,
        mode: "replay".to_string(),
        puzzle_date: None,
        answer_word: "apple".to_string(),
        guesses: serde_json::json!(["shade"]),
        current_guess: "cl".to_string(),
        is_game_over: false,
        won: false,
    };
    let mut state = State::new(user.id, svc, Some(daily_word), vec![replay_game]);

    state.show_replay();

    assert_eq!(state.mode, Mode::Replay);
    assert_eq!(state.answer, "apple");
    assert_eq!(state.guesses, vec!["shade"]);
    assert_eq!(state.current_guess, "cl");
}

#[tokio::test]
async fn saved_replay_rotates_if_its_answer_matches_todays_daily() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "le-word-replay-daily-collision").await;
    let (activity_tx, _) = broadcast::channel::<ActivityEvent>(8);
    let svc = LeWordService::new(test_db.db.clone(), activity_tx);
    let today = svc.today();
    let daily_word = DailyWord {
        id: uuid::Uuid::now_v7(),
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        puzzle_date: today,
        answer_word: "hunch".to_string(),
    };
    let replay_game = Game {
        id: uuid::Uuid::now_v7(),
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        user_id: user.id,
        mode: "replay".to_string(),
        puzzle_date: None,
        answer_word: "hunch".to_string(),
        guesses: serde_json::json!(["hunch"]),
        current_guess: String::new(),
        is_game_over: true,
        won: true,
    };
    let mut state = State::new(user.id, svc, Some(daily_word), vec![replay_game]);

    state.show_replay();

    assert_eq!(state.mode, Mode::Replay);
    assert_ne!(state.answer, "hunch");
    assert!(state.guesses.is_empty());
    assert!(!state.is_game_over);
}

#[tokio::test]
async fn random_replay_requires_a_double_press() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "le-word-replay-confirm").await;
    let (activity_tx, _) = broadcast::channel::<ActivityEvent>(8);
    let svc = LeWordService::new(test_db.db.clone(), activity_tx);
    let today = svc.today();
    let daily_word = DailyWord {
        id: uuid::Uuid::now_v7(),
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        puzzle_date: today,
        answer_word: "hunch".to_string(),
    };
    let mut state = State::new(user.id, svc, Some(daily_word), Vec::new());

    assert!(handle_key(&mut state, b'0'));
    assert_eq!(state.mode, Mode::Daily);
    assert!(state.reset_pending);

    assert!(handle_arrow(&mut state, b'A'));
    assert!(!state.reset_pending);
    assert!(state.message.is_empty());

    assert!(handle_key(&mut state, b'0'));
    assert!(state.reset_pending);
    assert!(handle_key(&mut state, b'0'));
    assert_eq!(state.mode, Mode::Replay);
    assert!(!state.reset_pending);
    assert_ne!(state.answer, "hunch");
}

#[tokio::test]
async fn replay_win_does_not_record_a_daily_win() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "le-word-replay-no-reward").await;
    let (activity_tx, mut activity_rx) = broadcast::channel::<ActivityEvent>(8);
    let svc = LeWordService::new(test_db.db.clone(), activity_tx);
    let today = svc.today();
    let daily_word = DailyWord {
        id: uuid::Uuid::now_v7(),
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        puzzle_date: today,
        answer_word: "hunch".to_string(),
    };
    let mut state = State::new(user.id, svc, Some(daily_word), Vec::new());
    state.show_replay();
    state.current_guess = state.answer.clone();

    assert!(state.submit_guess());
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = test_db.db.get().await.expect("client");
    assert!(
        !DailyWin::has_won_today(&client, user.id, today)
            .await
            .expect("daily win lookup")
    );
    assert!(matches!(
        activity_rx.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));
}
