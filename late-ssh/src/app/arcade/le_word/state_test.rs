use crate::app::arcade::le_word::state::*;

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

/// A board on the daily word `answer` for `puzzle_date`, behind a service
/// whose pool never connects.
pub(crate) fn state_for(puzzle_date: chrono::NaiveDate, answer: &str) -> State {
    use crate::app::arcade::le_word::svc::LeWordService;
    use late_core::db::{Db, DbConfig};
    use late_core::models::le_word::DailyWord;
    use uuid::Uuid;

    let (activity_feed, _) = tokio::sync::broadcast::channel(1);
    let svc = LeWordService::new(
        Db::new(&DbConfig::default()).expect("test db pool"),
        activity_feed,
    );
    let now = chrono::Utc::now();
    State::new(
        Uuid::now_v7(),
        svc,
        Some(Today {
            word: DailyWord {
                id: Uuid::now_v7(),
                created: now,
                updated: now,
                puzzle_date,
                answer_word: answer.to_string(),
            },
            saved: None,
        }),
    )
}

fn yesterdays_state() -> (State, chrono::NaiveDate, chrono::NaiveDate) {
    let today = chrono::Utc::now().date_naive();
    let yesterday = today.pred_opt().expect("yesterday");
    (state_for(yesterday, "crane"), today, yesterday)
}

fn todays_word(today: chrono::NaiveDate) -> late_core::models::le_word::DailyWord {
    let now = chrono::Utc::now();
    late_core::models::le_word::DailyWord {
        id: uuid::Uuid::now_v7(),
        created: now,
        updated: now,
        puzzle_date: today,
        answer_word: "slate".to_string(),
    }
}

/// A round of `guesses` on today's word, as another session saved it.
fn saved_round(today: chrono::NaiveDate, guesses: &[&str]) -> late_core::models::le_word::Game {
    let now = chrono::Utc::now();
    late_core::models::le_word::Game {
        id: uuid::Uuid::now_v7(),
        created: now,
        updated: now,
        user_id: uuid::Uuid::now_v7(),
        puzzle_date: today,
        answer_word: "slate".to_string(),
        guesses: serde_json::json!(guesses),
        current_guess: String::new(),
        is_game_over: false,
        won: false,
    }
}

/// Yesterday's word stayed on the board of a session that never reconnected,
/// so guesses went on being scored against it after the day rolled over.
/// The rollover now clears it at once, and today's word lands with the
/// round the player already has on it.
#[test]
fn rolling_over_the_day_clears_yesterdays_word() {
    let (mut state, today, yesterday) = yesterdays_state();
    state.guesses.push("slate".to_string());
    assert_eq!(state.puzzle_date, yesterday);

    assert!(state.ensure_current_daily(), "the day should roll over");
    assert!(state.guesses.is_empty(), "yesterday's guesses are still up");
    assert!(
        !state.daily_word_loaded,
        "the board must not accept guesses until today's word lands"
    );
    assert!(!state.ensure_current_daily(), "nothing left to clear");

    let (tx, rx) = tokio::sync::oneshot::channel();
    state.load = DailyLoad::Loading(rx);
    tx.send(Some(Today {
        word: todays_word(today),
        saved: Some(saved_round(today, &["crane"])),
    }))
    .expect("deliver word");
    assert!(state.poll_load());
    assert_eq!(
        (
            state.puzzle_date,
            state.daily_word_loaded,
            state.guesses.clone(),
            state.message.as_str()
        ),
        (today, true, vec!["crane".to_string()], "Keep going.")
    );
    assert!(!state.ensure_current_daily());
}

/// A failed rollover load used to disable Le Word for the rest of the
/// session. The date only advances when the word lands, and a load that
/// brings nothing says so and waits before the next.
#[test]
fn a_failed_load_says_so_and_keeps_yesterdays_date() {
    let (mut state, _today, yesterday) = yesterdays_state();
    assert!(state.ensure_current_daily());

    let (tx, rx) = tokio::sync::oneshot::channel::<Option<Today>>();
    state.load = DailyLoad::Loading(rx);
    drop(tx);
    assert!(state.poll_load(), "the dead load should be noticed");
    assert_eq!(
        (
            state.puzzle_date,
            state.daily_word_loaded,
            state.message.as_str()
        ),
        (yesterday, false, "Le Word is unavailable. Retrying soon.")
    );
    assert!(!state.poll_load(), "inside the wait nothing changes");
}

/// A session whose connect-time load failed kept Le Word unavailable until
/// it reconnected: only the rollover reloaded, and the date had not moved.
/// The board now loads again once the wait passes, with the player's round.
#[tokio::test]
async fn a_failed_connect_time_load_retries_with_the_saved_round() {
    use late_core::{models::le_word::GameParams, test_utils::create_test_user};

    use crate::app::arcade::le_word::svc::LeWordService;
    use crate::test_helpers::new_test_db;

    let test_db = new_test_db().await;
    let user = create_test_user(&test_db.db, "le-word-retry").await;
    let (activity_feed, _) = tokio::sync::broadcast::channel(8);
    let svc = LeWordService::new(test_db.db.clone(), activity_feed);
    let word = svc.ensure_daily_word().await.expect("daily word");
    let client = test_db.db.get().await.expect("db client");
    late_core::models::le_word::Game::upsert(
        &client,
        GameParams {
            user_id: user.id,
            puzzle_date: word.puzzle_date,
            answer_word: word.answer_word.clone(),
            guesses: serde_json::json!(["crane"]),
            current_guess: String::new(),
            is_game_over: false,
            won: false,
        },
    )
    .await
    .expect("another session's round");

    let mut state = State::new(user.id, svc, None);
    assert!(!state.daily_word_loaded);
    state.load = DailyLoad::Failed {
        retry_after: std::time::Instant::now(),
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !state.daily_word_loaded {
        assert!(
            std::time::Instant::now() < deadline,
            "the retry never landed"
        );
        state.poll_load();
        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    }
    assert_eq!(
        (
            state.puzzle_date,
            state.answer.clone(),
            state.guesses.clone()
        ),
        (
            word.puzzle_date,
            word.answer_word,
            vec!["crane".to_string()]
        )
    );
}
