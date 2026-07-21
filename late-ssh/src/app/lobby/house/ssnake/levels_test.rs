use super::*;

#[test]
fn all_shipped_levels_parse() {
    for (index, source) in LEVEL_SOURCES.iter().enumerate() {
        let level = parse_level(source)
            .unwrap_or_else(|error| panic!("level {} failed: {error:#}", index + 1));
        assert!(level.width <= MAX_WIDTH);
        assert!(level.height <= MAX_HEIGHT);
        assert!(level.tick_millis >= 60, "level {} too fast", index + 1);
        assert!(
            level
                .cells
                .iter()
                .any(|cell| matches!(cell, Cell::Empty | Cell::Warp)),
            "level {} has no floor",
            index + 1
        );
    }
    assert_eq!(LEVELS.len(), LEVEL_SOURCES.len());
}

#[test]
fn parser_rejects_ragged_rows() {
    let source = "name: X\nlives: 3\npoints-needed: 1\nlives-bonus: 0\npoints-bonus: 0\ntick-millis: 100\ninitial-length: 3\ngrowth-factor: 3\n\n###\n##\n";
    assert!(parse_level(source).is_err());
}

#[test]
fn parser_reads_warp_cells() {
    let source = "name: X\nlives: 3\npoints-needed: 1\nlives-bonus: 0\npoints-bonus: 0\ntick-millis: 100\ninitial-length: 3\ngrowth-factor: 3\n\n#~#\n#.#\n###\n";
    let level = parse_level(source).unwrap();
    assert_eq!(level.cell(1, 0), Cell::Warp);
    assert_eq!(level.cell(1, 1), Cell::Empty);
    assert!(level.is_wall(0, 0));
}
