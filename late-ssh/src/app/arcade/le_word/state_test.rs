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

/// Yesterday's word stayed on the board of a session that never reconnected,
/// so guesses went on being scored against it after the day rolled over.
#[test]
fn rolling_over_the_day_clears_yesterdays_word() {
    use crate::app::arcade::le_word::svc::LeWordService;
    use late_core::db::{Db, DbConfig};
    use late_core::models::le_word::DailyWord;
    use uuid::Uuid;

    let (activity_feed, _) = tokio::sync::broadcast::channel(1);
    let svc = LeWordService::new(
        Db::new(&DbConfig::default()).expect("test db pool"),
        activity_feed,
    );
    let today = svc.today();
    let yesterday = today.pred_opt().expect("yesterday");
    let now = chrono::Utc::now();
    let mut state = State::new(
        Uuid::now_v7(),
        svc,
        Some(DailyWord {
            id: Uuid::now_v7(),
            created: now,
            updated: now,
            puzzle_date: yesterday,
            answer_word: "crane".to_string(),
        }),
        None,
    );
    state.guesses.push("slate".to_string());
    assert_eq!(state.puzzle_date, yesterday);

    assert!(state.ensure_current_daily(), "the day should roll over");
    assert_eq!(state.puzzle_date, today);
    assert!(state.guesses.is_empty(), "yesterday's guesses are still up");
    assert!(
        !state.daily_word_loaded,
        "the board must not accept guesses until today's word lands"
    );

    // Same day again: nothing to do.
    assert!(!state.ensure_current_daily());
}
