use anyhow::{Context, Result};
use late_core::nonogram::{
    NonogramPack, NonogramPackIndex, NonogramPackIndexEntry, NonogramPuzzle, derive_clues,
};
use std::collections::BTreeMap;
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() -> Result<()> {
    let args = Args::parse(env::args().skip(1).collect())?;
    fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("failed to create {}", args.output_dir.display()))?;
    cleanup_validation_dir(&args.output_dir)?;

    let mut index = NonogramPackIndex {
        version: 1,
        packs: Vec::new(),
    };

    for (width, height) in &args.sizes {
        let pack = build_pack(
            *width,
            *height,
            args.count_per_size,
            args.seed_base,
            if args.skip_number_loom {
                None
            } else {
                Some(args.number_loom_bin.as_str())
            },
            &args.output_dir,
        )?;
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
    cleanup_validation_dir(&args.output_dir)?;
    Ok(())
}

#[derive(Debug)]
struct Args {
    output_dir: PathBuf,
    sizes: Vec<(u16, u16)>,
    count_per_size: usize,
    number_loom_bin: String,
    skip_number_loom: bool,
    seed_base: u64,
}

impl Args {
    fn parse(args: Vec<String>) -> Result<Self> {
        let mut output_dir = PathBuf::from("late-ssh/assets/nonograms");
        let mut sizes = Vec::new();
        let mut count_per_size = 100usize;
        let mut number_loom_bin = "number-loom".to_string();
        let mut skip_number_loom = false;
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
                "--number-loom-bin" => {
                    idx += 1;
                    number_loom_bin = require_value(&args, idx, "--number-loom-bin")?.to_string();
                }
                "--skip-number-loom" => skip_number_loom = true,
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
            number_loom_bin,
            skip_number_loom,
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
        "Usage: gen_nonograms [--size WxH] [--count-per-size N] [--output-dir DIR] [--number-loom-bin BIN] [--skip-number-loom]"
    );
}

fn build_pack(
    width: u16,
    height: u16,
    count: usize,
    seed_base: u64,
    number_loom_bin: Option<&str>,
    output_dir: &Path,
) -> Result<NonogramPack> {
    let size_key = format!("{width}x{height}");
    let profile = difficulty_profile(width, height);
    let mut puzzles = Vec::with_capacity(count);
    let mut attempt = 0usize;
    let mut rejected = 0usize;
    let mut warning_counts = BTreeMap::new();
    let max_attempts = count.saturating_mul(500).max(count);

    while puzzles.len() < count && attempt < max_attempts {
        let seed = seed_base
            .wrapping_add((u64::from(width) << 32) ^ (u64::from(height) << 16))
            .wrapping_add(attempt as u64);
        let Some(puzzle) =
            build_puzzle_with_profile(width, height, seed, &size_key, attempt, &profile)
        else {
            rejected = rejected.saturating_add(1);
            attempt = attempt.saturating_add(1);
            continue;
        };

        let accepted = if let Some(bin) = number_loom_bin {
            validate_puzzle_with_number_loom(bin, &puzzle, output_dir, &mut warning_counts)?
        } else {
            true
        };

        if accepted {
            puzzles.push(NonogramPuzzle {
                id: format!("{size_key}-{:06}", puzzles.len()),
                ..puzzle
            });
        } else {
            rejected = rejected.saturating_add(1);
        }

        attempt = attempt.saturating_add(1);
    }

    if puzzles.len() < count {
        anyhow::bail!(
            "only accepted {} of {count} requested puzzles for {size_key} after {attempt} attempts",
            puzzles.len()
        );
    }

    println!(
        "Built pack {size_key}: accepted {}, rejected {}",
        puzzles.len(),
        rejected
    );
    print_warning_summary(&size_key, &warning_counts);

    Ok(NonogramPack {
        size_key,
        width,
        height,
        puzzles,
    })
}

fn build_puzzle_with_profile(
    width: u16,
    height: u16,
    seed: u64,
    size_key: &str,
    attempt: usize,
    profile: &DifficultyProfile,
) -> Option<NonogramPuzzle> {
    let solution = generate_solution(width as usize, height as usize, seed, profile);
    if !matches_profile(&solution, profile) {
        return None;
    }

    let (row_clues, col_clues) = derive_clues(&solution);

    Some(NonogramPuzzle {
        id: format!("{size_key}-candidate-{attempt:06}"),
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

fn validate_puzzle_with_number_loom(
    bin: &str,
    puzzle: &NonogramPuzzle,
    output_dir: &Path,
    warning_counts: &mut BTreeMap<String, usize>,
) -> Result<bool> {
    let validation_dir = validation_dir(output_dir);
    fs::create_dir_all(&validation_dir)
        .with_context(|| format!("failed to create {}", validation_dir.display()))?;

    let char_grid_path = validation_dir.join(format!("{}.txt", puzzle.id));
    let html_path = validation_dir.join(format!("{}.html", puzzle.id));
    fs::write(&char_grid_path, render_char_grid(&puzzle.solution))
        .with_context(|| format!("failed to write {}", char_grid_path.display()))?;

    let output = Command::new(bin)
        .arg(&char_grid_path)
        .arg(&html_path)
        .arg("--input-format")
        .arg("char-grid")
        .arg("--output-format")
        .arg("html")
        .output()
        .with_context(|| format!("failed to execute `{bin}`"))?;

    collect_warnings(&String::from_utf8_lossy(&output.stdout), warning_counts);
    collect_warnings(&String::from_utf8_lossy(&output.stderr), warning_counts);

    Ok(output.status.success())
}

fn validation_dir(output_dir: &Path) -> PathBuf {
    output_dir.join(".number-loom-validation")
}

fn cleanup_validation_dir(output_dir: &Path) -> Result<()> {
    let validation_dir = validation_dir(output_dir);
    if validation_dir.exists() {
        fs::remove_dir_all(&validation_dir)
            .with_context(|| format!("failed to remove {}", validation_dir.display()))?;
    }
    Ok(())
}

fn collect_warnings(output: &str, warning_counts: &mut BTreeMap<String, usize>) {
    for line in output.lines() {
        let line = line.trim();
        if !line.starts_with("Warning:") {
            continue;
        }

        let warning = line.trim_start_matches("Warning:").trim();
        if warning.eq_ignore_ascii_case("missing author") {
            continue;
        }

        *warning_counts.entry(warning.to_string()).or_default() += 1;
    }
}

fn print_warning_summary(size_key: &str, warning_counts: &BTreeMap<String, usize>) {
    if warning_counts.is_empty() {
        return;
    }

    println!("Warnings for {size_key}:");
    for (warning, count) in warning_counts {
        println!("  {count}x {warning}");
    }
}

fn render_char_grid(solution: &[Vec<u8>]) -> String {
    let mut out = String::new();
    for row in solution {
        for cell in row {
            out.push(if *cell == 1 { '#' } else { '.' });
        }
        out.push('\n');
    }
    out
}

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
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
