use std::sync::{Arc, LazyLock};

use anyhow::{Context, Result, bail};

pub const MAX_WIDTH: usize = 63;
pub const MAX_HEIGHT: usize = 36;

const LEVEL_SOURCES: [&str; 55] = [
    include_str!("../../../../../assets/ssnake_levels/level_01.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_02.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_03.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_04.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_05.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_06.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_07.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_08.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_09.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_10.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_11.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_12.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_13.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_14.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_15.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_16.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_17.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_18.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_19.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_20.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_21.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_22.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_23.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_24.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_25.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_26.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_27.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_28.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_29.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_30.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_31.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_32.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_33.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_34.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_35.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_36.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_37.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_38.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_39.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_40.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_41.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_42.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_43.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_44.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_45.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_46.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_47.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_48.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_49.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_50.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_51.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_52.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_53.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_54.txt"),
    include_str!("../../../../../assets/ssnake_levels/level_55.txt"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    Empty,
    Wall,
    /// An open gap in the border row/column: floor for collisions, rendered
    /// as a tunnel hint. Snakes wrap around the arena through it.
    Warp,
}

/// One arena. The original `.LEV` files also carried `lives`, `lives-bonus`
/// and `points-bonus`; the perpetual arena has no lives and pays chips from
/// its own constants, so those keys were dropped from the assets rather than
/// parsed and ignored.
#[derive(Clone, Debug)]
pub struct SsnakeLevel {
    pub name: String,
    /// Food on this arena; eating the last one clears it and reshuffles.
    pub points_needed: i32,
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
        points_needed: 5,
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
    let mut points_needed = None;
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
            "points-needed" => points_needed = Some(value.parse().context("bad points-needed")?),
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
    let points_needed = points_needed.context("missing points-needed")?;
    if points_needed < 1 {
        bail!("points-needed must be >= 1");
    }

    Ok(SsnakeLevel {
        name: name.context("missing name")?,
        points_needed,
        tick_millis: tick_millis.context("missing tick-millis")?,
        initial_length: initial_length.context("missing initial-length")?,
        growth_factor: growth_factor.context("missing growth-factor")?,
        width,
        height,
        cells,
    })
}

#[cfg(test)]
#[path = "levels_test.rs"]
mod levels_test;
