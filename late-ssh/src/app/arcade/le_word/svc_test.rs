use super::*;

use crate::app::activity::event::ActivityEvent;
use crate::test_helpers::new_test_db;
use late_core::test_utils::create_test_user;

#[test]
fn supplied_word_pools_are_loaded() {
    assert_eq!(answer_words().len(), 2317);
    assert!(valid_guesses().contains("hunch"));
    assert!(valid_guesses().contains("noire"));
}

#[test]
fn daily_selection_avoids_used_answers() {
    let mut used: HashSet<&str> = answer_words().iter().copied().collect();
    used.remove("hunch");
    for _ in 0..32 {
        assert_eq!(choose_unused_answer(&used).expect("answer"), "hunch");
    }
}

#[test]
fn replay_selection_never_reuses_current_answer() {
    for _ in 0..32 {
        let answer = choose_replay_answer("hunch", None);
        assert_ne!(answer, "hunch");
        assert!(answer_words().contains(&answer));
    }
}

#[test]
fn replay_selection_excludes_the_daily_answer() {
    let answers = ["hunch", "apple", "shade"];
    for _ in 0..32 {
        assert_eq!(
            choose_replay_answer_from(&answers, "hunch", Some("apple")),
            "shade"
        );
    }
}

#[tokio::test]
async fn queued_game_saves_preserve_transition_order() {
    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "le-word-save-order").await;
    let (activity_tx, _) = broadcast::channel::<ActivityEvent>(8);
    let svc = LeWordService::new(test_db.db.clone(), activity_tx);

    svc.save_game_task(GameParams {
        user_id: user.id,
        mode: "replay".to_string(),
        puzzle_date: None,
        answer_word: "hunch".to_string(),
        guesses: serde_json::json!(["glass"]),
        current_guess: String::new(),
        is_game_over: false,
        won: false,
    });
    svc.save_game_task(GameParams {
        user_id: user.id,
        mode: "replay".to_string(),
        puzzle_date: None,
        answer_word: "shade".to_string(),
        guesses: serde_json::json!([]),
        current_guess: String::new(),
        is_game_over: false,
        won: false,
    });
    svc.flush_game_saves().await.expect("flush queued saves");

    let replay = svc
        .load_games(user.id)
        .await
        .expect("load saved games")
        .into_iter()
        .find(|game| game.mode == "replay")
        .expect("saved replay");
    assert_eq!(replay.answer_word, "shade");
    assert!(
        replay
            .guesses
            .as_array()
            .expect("replay guesses")
            .is_empty()
    );
}
