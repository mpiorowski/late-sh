use anyhow::{Context, Result};
use late_core::nonogram::{
    NonogramPack, NonogramPackIndex, NonogramPackIndexEntry, NonogramPuzzle, derive_clues,
    solve_by_lines,
};
use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() -> Result<()> {
    let args = Args::parse(env::args().skip(1).collect())?;
    fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("failed to create {}", args.output_dir.display()))?;

    let mut index = NonogramPackIndex {
        version: 1,
        packs: Vec::new(),
    };

    for (width, height) in &args.sizes {
        let pack = build_pack(*width, *height, args.count_per_size, args.seed_base)?;
        pack.validate()?;

        let file_name = format!("{width}x{height}.json");
        let pack_path = args.output_dir.join(&file_name);
        fs::write(&pack_path, serde_json::to_vec_pretty(&pack)?)
            .with_context(|| format!("failed to write {}", pack_path.display()))?;

        index.packs.push(NonogramPackIndexEntry {
            size_key: pack.size_key.clone(),
            width: *width,
            height: *height,
            puzzle_count: pack.puzzles.len(),
            path: file_name,
        });
    }

    let index_path = args.output_dir.join("index.json");
    fs::write(&index_path, serde_json::to_vec_pretty(&index)?)
        .with_context(|| format!("failed to write {}", index_path.display()))?;

    println!(
        "Wrote {} pack(s) to {}",
        index.packs.len(),
        args.output_dir.display()
    );
    Ok(())
}

#[derive(Debug)]
struct Args {
    output_dir: PathBuf,
    sizes: Vec<(u16, u16)>,
    count_per_size: usize,
    seed_base: u64,
}

impl Args {
    fn parse(args: Vec<String>) -> Result<Self> {
        let mut output_dir = PathBuf::from("late-ssh/assets/nonograms");
        let mut sizes = Vec::new();
        let mut count_per_size = 100usize;
        let mut seed_base = 42u64;

        let mut idx = 0;
        while idx < args.len() {
            match args[idx].as_str() {
                "--output-dir" => {
                    idx += 1;
                    output_dir = PathBuf::from(require_value(&args, idx, "--output-dir")?);
                }
                "--size" => {
                    idx += 1;
                    sizes.push(parse_size(require_value(&args, idx, "--size")?)?);
                }
                "--count-per-size" => {
                    idx += 1;
                    count_per_size = require_value(&args, idx, "--count-per-size")?
                        .parse()
                        .context("invalid --count-per-size")?;
                }
                "--seed-base" => {
                    idx += 1;
                    seed_base = require_value(&args, idx, "--seed-base")?
                        .parse()
                        .context("invalid --seed-base")?;
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                flag => anyhow::bail!("unknown flag: {flag}"),
            }
            idx += 1;
        }

        if sizes.is_empty() {
            sizes = vec![(10, 10), (15, 15), (20, 20)];
        }

        Ok(Self {
            output_dir,
            sizes,
            count_per_size,
            seed_base,
        })
    }
}

fn require_value<'a>(args: &'a [String], idx: usize, flag: &str) -> Result<&'a str> {
    args.get(idx)
        .map(String::as_str)
        .ok_or_else(|| anyhow::anyhow!("missing value for {flag}"))
}

fn parse_size(value: &str) -> Result<(u16, u16)> {
    let (width, height) = value
        .split_once('x')
        .ok_or_else(|| anyhow::anyhow!("size must look like 10x10"))?;
    Ok((
        width.parse().context("invalid width")?,
        height.parse().context("invalid height")?,
    ))
}

fn print_help() {
    println!(
        "Usage: gen_nonograms [--size WxH] [--count-per-size N] [--output-dir DIR] [--seed-base N]"
    );
}

/// Draws candidates until the pack is full. A candidate is kept only when
/// line logic alone finishes it (`solve_by_lines`): that is a puzzle a
/// player can solve without guessing, and its answer is the only one.
fn build_pack(width: u16, height: u16, count: usize, seed_base: u64) -> Result<NonogramPack> {
    let size_key = format!("{width}x{height}");
    let profile = difficulty_profile(width, height);
    let mut puzzles: Vec<NonogramPuzzle> = Vec::with_capacity(count);
    let mut attempt = 0usize;
    let mut off_profile = 0usize;
    let mut needs_a_guess = 0usize;
    let mut duplicates = 0usize;
    let max_attempts = count.saturating_mul(500).max(count);

    while puzzles.len() < count && attempt < max_attempts {
        let seed = seed_base
            .wrapping_add((u64::from(width) << 32) ^ (u64::from(height) << 16))
            .wrapping_add(attempt as u64);
        attempt = attempt.saturating_add(1);
        let Some(puzzle) = build_puzzle_with_profile(width, height, seed, &size_key, &profile)
        else {
            off_profile += 1;
            continue;
        };
        if solve_by_lines(&puzzle.row_clues, &puzzle.col_clues).as_ref() != Some(&puzzle.solution) {
            needs_a_guess += 1;
            continue;
        }
        if puzzles.iter().any(|kept| kept.id == puzzle.id) {
            duplicates += 1;
            continue;
        }
        puzzles.push(puzzle);
    }

    if puzzles.len() < count {
        anyhow::bail!(
            "only accepted {} of {count} requested puzzles for {size_key} after {attempt} attempts \
             ({off_profile} off profile, {needs_a_guess} needing a guess, {duplicates} duplicates)",
            puzzles.len()
        );
    }

    println!(
        "Built pack {size_key}: accepted {}, rejected {off_profile} off profile, {needs_a_guess} needing a guess, {duplicates} duplicates",
        puzzles.len()
    );

    Ok(NonogramPack {
        size_key,
        width,
        height,
        puzzles,
    })
}

/// A puzzle's id names its picture: the size and a digest of the solution.
/// Saved games find their puzzle by id, so a regenerated pack must never
/// hand an old id to a different picture; a picture kept across a
/// regeneration keeps its id, and its saves still load.
fn puzzle_id(size_key: &str, solution: &[Vec<u8>]) -> String {
    let mut hasher = Sha256::new();
    for row in solution {
        hasher.update(row);
    }
    format!("{size_key}-{}", hex::encode(&hasher.finalize()[..6]))
}

fn build_puzzle_with_profile(
    width: u16,
    height: u16,
    seed: u64,
    size_key: &str,
    profile: &DifficultyProfile,
) -> Option<NonogramPuzzle> {
    let solution = generate_solution(width as usize, height as usize, seed, profile);
    if !matches_profile(&solution, profile) {
        return None;
    }

    let (row_clues, col_clues) = derive_clues(&solution);

    Some(NonogramPuzzle {
        id: puzzle_id(size_key, &solution),
        width,
        height,
        row_clues,
        col_clues,
        solution,
        difficulty: profile.label.to_string(),
        source: Some("late-tools".to_string()),
        seed: Some(seed),
    })
}

fn generate_solution(
    width: usize,
    height: usize,
    seed: u64,
    profile: &DifficultyProfile,
) -> Vec<Vec<u8>> {
    let mut rng = XorShift64::new(seed);
    let mut grid = vec![vec![0u8; width]; height];
    let density_span = (profile.max_density - profile.min_density).max(0.01);
    let target_density = profile.min_density + rng.next_f32() * density_span;

    for row in 0..height {
        for col in 0..width {
            if !profile.symmetry.should_generate(width, height, row, col) {
                continue;
            }

            let mut threshold = target_density;
            if profile.diagonal_bias > 0.0 && (row == col || row + col + 1 == width) {
                threshold += profile.diagonal_bias;
            }
            if profile.center_bias > 0.0 {
                let row_center = (height as f32 - 1.0) / 2.0;
                let col_center = (width as f32 - 1.0) / 2.0;
                let row_dist = ((row as f32 - row_center).abs() / height as f32).min(1.0);
                let col_dist = ((col as f32 - col_center).abs() / width as f32).min(1.0);
                threshold += (1.0 - (row_dist + col_dist)) * profile.center_bias;
            }

            let val = u8::from(rng.next_f32() < threshold.clamp(0.0, 0.92));
            profile.symmetry.apply(&mut grid, row, col, val);
        }
    }

    // Avoid trivial all-empty/all-full boards.
    let filled = grid.iter().flatten().filter(|cell| **cell == 1).count();
    if filled == 0 {
        grid[height / 2][width / 2] = 1;
    } else if filled == width * height {
        grid[height / 2][width / 2] = 0;
    }

    grid
}

fn matches_profile(solution: &[Vec<u8>], profile: &DifficultyProfile) -> bool {
    let density = filled_density(solution);
    if density < profile.min_density || density > profile.max_density {
        return false;
    }

    let width = solution.first().map_or(0, Vec::len);
    let height = solution.len();
    let filled = solution.iter().flatten().filter(|cell| **cell == 1).count();
    if filled < profile.min_filled || filled > profile.max_filled.max(width * height) {
        return false;
    }

    let (row_clues, col_clues) = derive_clues(solution);
    let total_runs = row_clues
        .iter()
        .chain(col_clues.iter())
        .map(Vec::len)
        .sum::<usize>();
    let max_line_runs = row_clues
        .iter()
        .chain(col_clues.iter())
        .map(Vec::len)
        .max()
        .unwrap_or(0);
    let long_runs = row_clues
        .iter()
        .chain(col_clues.iter())
        .flat_map(|line| line.iter())
        .filter(|&&run| usize::from(run) >= profile.min_long_run)
        .count();

    total_runs >= profile.min_total_runs
        && total_runs <= profile.max_total_runs
        && max_line_runs >= profile.min_max_line_runs
        && max_line_runs <= profile.max_max_line_runs
        && long_runs >= profile.min_long_runs
}

#[derive(Clone, Copy)]
struct DifficultyProfile {
    label: &'static str,
    min_density: f32,
    max_density: f32,
    min_filled: usize,
    max_filled: usize,
    min_total_runs: usize,
    max_total_runs: usize,
    min_max_line_runs: usize,
    max_max_line_runs: usize,
    min_long_run: usize,
    min_long_runs: usize,
    diagonal_bias: f32,
    center_bias: f32,
    symmetry: Symmetry,
}

#[derive(Clone, Copy)]
enum Symmetry {
    Vertical,
    None,
}

impl Symmetry {
    fn should_generate(&self, width: usize, _height: usize, _row: usize, col: usize) -> bool {
        match self {
            Symmetry::Vertical => col <= width - 1 - col,
            Symmetry::None => true,
        }
    }

    fn apply(&self, grid: &mut [Vec<u8>], row: usize, col: usize, val: u8) {
        let width = grid.first().map_or(0, Vec::len);
        let coords = match self {
            Symmetry::Vertical => vec![(row, col), (row, width - 1 - col)],
            Symmetry::None => vec![(row, col)],
        };

        for (r, c) in coords {
            grid[r][c] = val;
        }
    }
}

fn difficulty_profile(width: u16, height: u16) -> DifficultyProfile {
    match (width, height) {
        (10, 10) => DifficultyProfile {
            label: "easy",
            min_density: 0.32,
            max_density: 0.52,
            min_filled: 28,
            max_filled: 58,
            min_total_runs: 28,
            max_total_runs: 60,
            min_max_line_runs: 2,
            max_max_line_runs: 4,
            min_long_run: 3,
            min_long_runs: 4,
            diagonal_bias: 0.12,
            center_bias: 0.05,
            symmetry: Symmetry::Vertical,
        },
        (15, 15) => DifficultyProfile {
            label: "medium",
            min_density: 0.34,
            max_density: 0.54,
            min_filled: 70,
            max_filled: 130,
            min_total_runs: 55,
            max_total_runs: 120,
            min_max_line_runs: 3,
            max_max_line_runs: 5,
            min_long_run: 4,
            min_long_runs: 6,
            diagonal_bias: 0.04,
            center_bias: 0.02,
            symmetry: Symmetry::Vertical,
        },
        _ => DifficultyProfile {
            label: "hard",
            min_density: 0.34,
            max_density: 0.58,
            min_filled: 120,
            max_filled: 260,
            min_total_runs: 60,
            max_total_runs: 250,
            min_max_line_runs: 3,
            max_max_line_runs: 8,
            min_long_run: 4,
            min_long_runs: 6,
            diagonal_bias: 0.01,
            center_bias: 0.0,
            symmetry: Symmetry::None,
        },
    }
}

fn filled_density(solution: &[Vec<u8>]) -> f32 {
    let total = solution.iter().map(Vec::len).sum::<usize>() as f32;
    let filled = solution.iter().flatten().filter(|cell| **cell == 1).count() as f32;
    filled / total
}

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    /// Candidate seeds are consecutive integers, and xorshift's first draws
    /// from seeds that differ only in their low bits are nearly identical:
    /// every candidate of a pack drew the same target density. One
    /// splitmix64 round spreads each seed over the whole state first.
    fn new(seed: u64) -> Self {
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        Self { state: z.max(1) }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u64() as f64 / u64::MAX as f64) as f32
    }
}
