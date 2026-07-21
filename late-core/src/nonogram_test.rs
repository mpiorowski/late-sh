use super::*;

#[test]
fn derives_expected_clues() {
    let solution = vec![
        vec![0, 1, 1, 0, 1],
        vec![1, 1, 0, 0, 0],
        vec![0, 0, 0, 0, 0],
        vec![1, 0, 1, 1, 1],
        vec![0, 0, 1, 0, 0],
    ];

    let (rows, cols) = derive_clues(&solution);
    assert_eq!(rows, vec![vec![2, 1], vec![2], vec![], vec![1, 3], vec![1]]);
    assert_eq!(
        cols,
        vec![vec![1, 1], vec![2], vec![1, 2], vec![1], vec![1, 1]]
    );
}

#[test]
fn daily_selection_is_deterministic() {
    let pack = NonogramPack {
        size_key: "5x5".to_string(),
        width: 5,
        height: 5,
        puzzles: (0..4)
            .map(|idx| NonogramPuzzle {
                id: format!("5x5-{idx:06}"),
                width: 5,
                height: 5,
                row_clues: vec![vec![1]; 5],
                col_clues: vec![vec![1]; 5],
                solution: vec![vec![1, 0, 0, 0, 0]; 5],
                difficulty: "easy".to_string(),
                source: None,
                seed: Some(idx),
            })
            .collect(),
    };

    let date = NaiveDate::from_ymd_opt(2026, 3, 28).expect("date");
    let a = pack.select_for_date(date).expect("puzzle").id.clone();
    let b = pack.select_for_date(date).expect("puzzle").id.clone();
    assert_eq!(a, b);
}
