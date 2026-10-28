//! Builds the Chess Puzzle pack from the Lichess puzzle dump
//! (`lichess_db_puzzle.csv.zst` from database.lichess.org, CC0):
//!
//! ```sh
//! zstd -dc lichess_db_puzzle.csv.zst \
//!   | cargo run -p late-core --bin gen_chess_puzzles -- /dev/stdin
//! ```
//!
//! Writes `late-ssh/assets/chess_puzzles/{easy,medium,hard}.json`, one
//! puzzle per line. A tier keeps the `PER_TIER` well-tested puzzles in its
//! rating band whose ids hash lowest under `SEED`, so the pick does not
//! depend on the dump's row order and mostly survives a newer dump. Every
//! kept puzzle is replayed with cozy-chess: the FEN parses and each move is
//! legal in turn.

use std::{
    cmp::Ordering,
    collections::BinaryHeap,
    env, fs,
    io::{BufRead, BufReader},
    path::PathBuf,
};

use anyhow::{Context, Result, bail};
use cozy_chess::{Board, Piece, util::parse_uci_move};
use late_core::{
    chess_puzzle::{ChessPuzzle, Theme, rating_range},
    models::chips::Difficulty,
};

const HEADER: &str =
    "PuzzleId,FEN,Moves,Rating,RatingDeviation,Popularity,NbPlays,Themes,GameUrl,OpeningTags";
const PER_TIER: usize = 2000;
const SEED: u64 = 0x6c61_7465_2d73_6831;
const MIN_POPULARITY: i32 = 90;
const MIN_PLAYS: u32 = 1000;
const MAX_RATING_DEVIATION: u16 = 80;
/// A line is the setup move, then the player and the opponent in turn,
/// ending on the player: twice the player's moves in all.
const MAX_PLAYER_MOVES: usize = 4;
const TIERS: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard];

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let Some(csv_path) = args.next() else {
        bail!("usage: gen_chess_puzzles <lichess_db_puzzle.csv> [output dir]");
    };
    let output_dir = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "late-ssh/assets/chess_puzzles".to_string()),
    );
    let file = fs::File::open(&csv_path).with_context(|| format!("opening {csv_path}"))?;
    let mut lines = BufReader::new(file).lines();
    let header = lines.next().context("the dump is empty")??;
    if !header.starts_with(HEADER) {
        bail!("unexpected dump header: {header}");
    }

    let mut kept: [BinaryHeap<Candidate>; 3] = Default::default();
    let mut rejected = 0usize;
    for line in lines {
        let line = line.context("reading the dump")?;
        let Some(row) = Row::parse(&line) else {
            bail!("malformed dump row: {line}");
        };
        let Some(tier) = row.tier() else {
            continue;
        };
        let heap = &mut kept[tier];
        let key = splitmix64(fnv1a(row.id) ^ SEED);
        if heap.len() == PER_TIER && heap.peek().is_some_and(|top| top.key <= key) {
            continue;
        }
        let Some(puzzle) = row.replay() else {
            rejected += 1;
            continue;
        };
        heap.push(Candidate { key, puzzle });
        if heap.len() > PER_TIER {
            heap.pop();
        }
    }

    fs::create_dir_all(&output_dir)
        .with_context(|| format!("creating {}", output_dir.display()))?;
    for (tier, heap) in TIERS.into_iter().zip(kept) {
        let mut puzzles: Vec<ChessPuzzle> =
            heap.into_iter().map(|candidate| candidate.puzzle).collect();
        if puzzles.len() < PER_TIER {
            bail!(
                "{} has {} puzzles, wanted {PER_TIER}",
                tier.key(),
                puzzles.len()
            );
        }
        puzzles.sort_by(|a, b| a.id.cmp(&b.id));
        let lines = puzzles
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<Vec<_>, _>>()?;
        let path = output_dir.join(format!("{}.json", tier.key()));
        fs::write(&path, format!("[\n{}\n]\n", lines.join(",\n")))
            .with_context(|| format!("writing {}", path.display()))?;
        println!("{}: {} puzzles", path.display(), puzzles.len());
    }
    println!("{rejected} candidate rows failed the replay");
    Ok(())
}

struct Row<'a> {
    id: &'a str,
    fen: &'a str,
    moves: Vec<&'a str>,
    rating: u16,
    rating_deviation: u16,
    popularity: i32,
    plays: u32,
    themes: &'a str,
}

impl<'a> Row<'a> {
    fn parse(line: &'a str) -> Option<Self> {
        let mut fields = line.split(',');
        Some(Self {
            id: fields.next()?,
            fen: fields.next()?,
            moves: fields.next()?.split(' ').collect(),
            rating: fields.next()?.parse().ok()?,
            rating_deviation: fields.next()?.parse().ok()?,
            popularity: fields.next()?.parse().ok()?,
            plays: fields.next()?.parse().ok()?,
            themes: fields.next()?,
        })
    }

    /// The tier this row is a candidate for, by its numbers alone.
    fn tier(&self) -> Option<usize> {
        let well_tested = self.popularity >= MIN_POPULARITY
            && self.plays >= MIN_PLAYS
            && self.rating_deviation <= MAX_RATING_DEVIATION;
        let short = self.moves.len().is_multiple_of(2)
            && self.moves.len() >= 2
            && self.moves.len() <= 2 * MAX_PLAYER_MOVES;
        if !(well_tested && short) {
            return None;
        }
        TIERS.iter().position(|&tier| {
            let (low, high) = rating_range(tier);
            (low..=high).contains(&self.rating)
        })
    }

    /// The puzzle with its moves in cozy-chess's spelling, or `None` when
    /// the FEN does not parse, a move is illegal, or a move underpromotes
    /// (the board promotes to a queen, so such a line cannot be played).
    fn replay(&self) -> Option<ChessPuzzle> {
        let mut board = Board::from_fen(self.fen, false).ok()?;
        let mut moves = Vec::with_capacity(self.moves.len());
        for uci in &self.moves {
            let mv = parse_uci_move(&board, uci).ok()?;
            if mv.promotion.is_some_and(|piece| piece != Piece::Queen) {
                return None;
            }
            board.try_play(mv).ok()?;
            moves.push(mv.to_string());
        }
        Some(ChessPuzzle {
            id: self.id.to_string(),
            fen: self.fen.to_string(),
            moves,
            rating: self.rating,
            theme: Theme::from_lichess_tags(self.themes),
        })
    }
}

/// A kept puzzle, ordered by its hash so the heap's top is the one to drop.
struct Candidate {
    key: u64,
    puzzle: ChessPuzzle,
}

impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key
            .cmp(&other.key)
            .then_with(|| self.puzzle.id.cmp(&other.puzzle.id))
    }
}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Candidate {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Candidate {}

fn fnv1a(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

fn splitmix64(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}
