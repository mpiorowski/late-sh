use super::*;

#[test]
fn duplicate_detection_uses_only_visible_grid_rules() {
    let mut grid = [[0u8; 9]; 9];
    grid[0][0] = 5;
    grid[0][8] = 5;
    assert!(cell_has_duplicate(&grid, 0, 0));
    assert!(cell_has_duplicate(&grid, 0, 8));

    grid[0][8] = 0;
    grid[8][0] = 5;
    assert!(cell_has_duplicate(&grid, 0, 0));

    grid[8][0] = 0;
    grid[2][2] = 5;
    assert!(cell_has_duplicate(&grid, 0, 0));
}

#[test]
fn duplicate_detection_does_not_mark_non_conflicting_guess() {
    let mut grid = [[0u8; 9]; 9];
    grid[0][0] = 5;
    grid[1][2] = 6;
    grid[4][4] = 5;

    assert!(!cell_has_duplicate(&grid, 0, 0));
}

#[test]
fn every_digit_has_a_distinct_colour() {
    let colours: Vec<Color> = (1..=9).map(digit_color).collect();
    for (i, a) in colours.iter().enumerate() {
        for b in colours.iter().skip(i + 1) {
            assert_ne!(a, b, "digit colours must all differ");
        }
        assert!(
            matches!(a, Color::Rgb(..)),
            "each digit should map to an explicit RGB colour"
        );
    }
}

#[test]
fn dim_darkens_rgb_and_passes_other_colours_through() {
    assert_eq!(dim(Color::Rgb(200, 100, 50), 50), Color::Rgb(100, 50, 25));
    assert_eq!(dim(Color::Reset, 50), Color::Reset);
}
