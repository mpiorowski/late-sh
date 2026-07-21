use super::*;

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
