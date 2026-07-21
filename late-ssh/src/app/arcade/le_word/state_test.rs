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
