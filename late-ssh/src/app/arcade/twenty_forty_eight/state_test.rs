use super::*;

#[test]
fn shift_and_merge_moves_and_merges_once_per_pair() {
    let mut score = 0;

    // Shift left
    let mut line = [0, 2, 0, 2];
    assert!(shift_and_merge(&mut line, &mut score));
    assert_eq!(line, [4, 0, 0, 0]);
    assert_eq!(score, 4);

    // Merge multiples
    let mut line = [2, 2, 2, 2];
    assert!(shift_and_merge(&mut line, &mut score));
    assert_eq!(line, [4, 4, 0, 0]);

    // Don't merge cascaded
    let mut line = [2, 2, 4, 8];
    assert!(shift_and_merge(&mut line, &mut score));
    assert_eq!(line, [4, 4, 8, 0]);

    // No change
    let mut line = [2, 4, 8, 16];
    assert!(!shift_and_merge(&mut line, &mut score));
    assert_eq!(line, [2, 4, 8, 16]);
}

#[test]
fn shift_and_merge_does_not_chain_merge_in_single_move() {
    let mut score = 0;
    let mut line = [4, 4, 4, 0];

    assert!(shift_and_merge(&mut line, &mut score));
    assert_eq!(line, [8, 4, 0, 0]);
    assert_eq!(score, 8);
}

#[test]
fn shift_and_merge_accumulates_score_for_two_merges() {
    let mut score = 0;
    let mut line = [8, 8, 16, 16];

    assert!(shift_and_merge(&mut line, &mut score));
    assert_eq!(line, [16, 32, 0, 0]);
    assert_eq!(score, 48);
}
