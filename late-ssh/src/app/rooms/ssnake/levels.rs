use std::sync::{Arc, LazyLock};

use anyhow::{Context, Result, bail};

pub const MAX_WIDTH: usize = 63;
pub const MAX_HEIGHT: usize = 36;

const LEVEL_SOURCES: [&str; 20] = [
    include_str!("../../../../assets/ssnake_levels/level_01.txt"),
    include_str!("../../../../assets/ssnake_levels/level_02.txt"),
    include_str!("../../../../assets/ssnake_levels/level_03.txt"),
    include_str!("../../../../assets/ssnake_levels/level_04.txt"),
    include_str!("../../../../assets/ssnake_levels/level_05.txt"),
    include_str!("../../../../assets/ssnake_levels/level_06.txt"),
    include_str!("../../../../assets/ssnake_levels/level_07.txt"),
    include_str!("../../../../assets/ssnake_levels/level_08.txt"),
    include_str!("../../../../assets/ssnake_levels/level_09.txt"),
    include_str!("../../../../assets/ssnake_levels/level_10.txt"),
    include_str!("../../../../assets/ssnake_levels/level_11.txt"),
    include_str!("../../../../assets/ssnake_levels/level_12.txt"),
    include_str!("../../../../assets/ssnake_levels/level_13.txt"),
    include_str!("../../../../assets/ssnake_levels/level_14.txt"),
    include_str!("../../../../assets/ssnake_levels/level_15.txt"),
    include_str!("../../../../assets/ssnake_levels/level_16.txt"),
    include_str!("../../../../assets/ssnake_levels/level_17.txt"),
    include_str!("../../../../assets/ssnake_levels/level_18.txt"),
    include_str!("../../../../assets/ssnake_levels/level_19.txt"),
    include_str!("../../../../assets/ssnake_levels/level_20.txt"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Empty,
    Wall,
    /// An open gap in the border row/column: floor for collisions, rendered
    /// as a tunnel hint. Snakes wrap around the arena through it.
    Warp,
}

#[derive(Clone, Debug)]
pub struct SsnakeLevel {
    pub name: String,
    pub lives: i32,
    pub points_needed: i32,
    pub lives_bonus: i32,
    pub points_bonus: i64,
    pub tick_millis: u64,
    pub initial_length: i32,
    pub growth_factor: i32,
    pub width: usize,
    pub height: usize,
    cells: Vec<Cell>,
}

impl SsnakeLevel {
    pub fn cell(&self, x: usize, y: usize) -> Cell {
        self.cells[y * self.width + x]
    }

    pub fn is_wall(&self, x: usize, y: usize) -> bool {
        self.cell(x, y) == Cell::Wall
    }
}

/// All levels that parsed cleanly. Bad files are dropped with a warning so a
/// broken level edit never takes the whole room down; the unit tests below
/// keep the shipped set honest.
pub static LEVELS: LazyLock<Vec<Arc<SsnakeLevel>>> = LazyLock::new(|| {
    LEVEL_SOURCES
        .iter()
        .enumerate()
        .filter_map(|(index, source)| match parse_level(source) {
            Ok(level) => Some(Arc::new(level)),
            Err(error) => {
                tracing::error!(?error, index, "skipping unparseable ssnake level");
                None
            }
        })
        .collect()
});

/// Walled empty arena for deterministic game-logic tests.
#[cfg(test)]
pub fn open_test_arena(width: usize, height: usize) -> SsnakeLevel {
    let mut cells = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            if x == 0 || y == 0 || x == width - 1 || y == height - 1 {
                cells.push(Cell::Wall);
            } else {
                cells.push(Cell::Empty);
            }
        }
    }
    SsnakeLevel {
        name: "Test Arena".to_string(),
        lives: 3,
        points_needed: 5,
        lives_bonus: 1,
        points_bonus: 100,
        tick_millis: 100,
        initial_length: 4,
        growth_factor: 3,
        width,
        height,
        cells,
    }
}

fn parse_level(source: &str) -> Result<SsnakeLevel> {
    let mut name = None;
    let mut lives = None;
    let mut points_needed = None;
    let mut lives_bonus = None;
    let mut points_bonus = None;
    let mut tick_millis = None;
    let mut initial_length = None;
    let mut growth_factor = None;

    let mut lines = source.lines();
    for line in lines.by_ref() {
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let (key, value) = line
            .split_once(':')
            .with_context(|| format!("header line without ':': {line}"))?;
        let value = value.trim();
        match key.trim() {
            "name" => name = Some(value.to_string()),
            "lives" => lives = Some(value.parse().context("bad lives")?),
            "points-needed" => points_needed = Some(value.parse().context("bad points-needed")?),
            "lives-bonus" => lives_bonus = Some(value.parse().context("bad lives-bonus")?),
            "points-bonus" => points_bonus = Some(value.parse().context("bad points-bonus")?),
            "tick-millis" => tick_millis = Some(value.parse().context("bad tick-millis")?),
            "initial-length" => {
                initial_length = Some(value.parse().context("bad initial-length")?);
            }
            "growth-factor" => growth_factor = Some(value.parse().context("bad growth-factor")?),
            other => bail!("unknown header key: {other}"),
        }
    }

    let mut cells = Vec::new();
    let mut width = 0usize;
    let mut height = 0usize;
    for line in lines {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let row: Vec<Cell> = line
            .chars()
            .map(|ch| match ch {
                '.' => Ok(Cell::Empty),
                '#' => Ok(Cell::Wall),
                '~' => Ok(Cell::Warp),
                other => Err(anyhow::anyhow!("unknown map char: {other:?}")),
            })
            .collect::<Result<_>>()?;
        if width == 0 {
            width = row.len();
        } else if row.len() != width {
            bail!("ragged map row: expected {width} cells, got {}", row.len());
        }
        cells.extend(row);
        height += 1;
    }

    if width == 0 || height == 0 {
        bail!("level has no map rows");
    }
    if width > MAX_WIDTH || height > MAX_HEIGHT {
        bail!("map {width}x{height} exceeds {MAX_WIDTH}x{MAX_HEIGHT}");
    }
    let lives = lives.context("missing lives")?;
    let points_needed = points_needed.context("missing points-needed")?;
    if lives < 0 || points_needed < 1 {
        bail!("lives must be >= 0 and points-needed >= 1");
    }

    Ok(SsnakeLevel {
        name: name.context("missing name")?,
        lives,
        points_needed,
        lives_bonus: lives_bonus.context("missing lives-bonus")?,
        points_bonus: points_bonus.context("missing points-bonus")?,
        tick_millis: tick_millis.context("missing tick-millis")?,
        initial_length: initial_length.context("missing initial-length")?,
        growth_factor: growth_factor.context("missing growth-factor")?,
        width,
        height,
        cells,
    })
}

#[cfg(test)]
mod tests {
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
}
