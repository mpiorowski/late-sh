// NetHack Sokoban solver and autoplayer. Local only: see SOKOBAN.md.
//
//   sokoban board <level.txt> [--flip h|v|hv]   stock map as it shows on screen
//   sokoban solve <board.txt> [--boards]         print the keys, batch by batch
//   sokoban play <board.txt> --window <title> --cell <W>x<H>
//                                                solve, then type it into the game
//   sokoban locate --window <title> --cell <W>x<H>
//                                                where the cursor is, typing nothing

mod keys;
mod level;
mod play;
mod screen;
mod solve;

use std::process::ExitCode;

use crate::level::{Level, State};
use crate::screen::CellSize;

const USAGE: &str = "usage:
  sokoban board <level.txt> [--flip h|v|hv]
  sokoban solve <board.txt> [--boards]
  sokoban play <board.txt> --window <title> --cell <W>x<H>
  sokoban locate --window <title> --cell <W>x<H>";

enum Cmd {
    Board { path: String, flip_h: bool, flip_v: bool },
    Solve { path: String, boards: bool },
    Play { path: String, window: String, cell: CellSize },
    Locate { window: String, cell: CellSize },
}

fn parse_cell(v: &str) -> Option<CellSize> {
    let (w, h) = v.split_once('x')?;
    Some(CellSize { width: w.parse().ok()?, height: h.parse().ok()? })
}

fn parse_args(args: &[String]) -> Result<Cmd, String> {
    let (cmd, rest) = args.split_first().ok_or(USAGE)?;
    let (path, flags) = match cmd.as_str() {
        "locate" => (String::new(), rest),
        _ => {
            let (path, flags) = rest.split_first().ok_or(USAGE)?;
            (path.clone(), flags)
        }
    };
    let mut flip = None;
    let mut boards = false;
    let mut window = None;
    let mut cell = None;
    let mut it = flags.iter();
    while let Some(f) = it.next() {
        match (cmd.as_str(), f.as_str()) {
            ("board", "--flip") => flip = it.next().cloned(),
            ("solve", "--boards") => boards = true,
            ("play" | "locate", "--window") => window = it.next().cloned(),
            ("play" | "locate", "--cell") => cell = Some(it.next().and_then(|v| parse_cell(v)).ok_or("--cell takes <W>x<H> pixels, like 12.952x28")?),
            _ => return Err(format!("unexpected argument {f}\n{USAGE}")),
        }
    }
    match cmd.as_str() {
        "board" => match flip.as_deref() {
            None => Ok(Cmd::Board { path, flip_h: false, flip_v: false }),
            Some("h") => Ok(Cmd::Board { path, flip_h: true, flip_v: false }),
            Some("v") => Ok(Cmd::Board { path, flip_h: false, flip_v: true }),
            Some("hv") | Some("vh") => Ok(Cmd::Board { path, flip_h: true, flip_v: true }),
            Some(other) => Err(format!("--flip takes h, v or hv, got {other}")),
        },
        "solve" => Ok(Cmd::Solve { path, boards }),
        "play" => match (window, cell) {
            (Some(window), Some(cell)) => Ok(Cmd::Play { path, window, cell }),
            _ => Err(format!("play needs --window and --cell\n{USAGE}")),
        },
        "locate" => match (window, cell) {
            (Some(window), Some(cell)) => Ok(Cmd::Locate { window, cell }),
            _ => Err(format!("locate needs --window and --cell\n{USAGE}")),
        },
        _ => Err(USAGE.to_string()),
    }
}

fn load(path: &str) -> Result<(Level, State), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("reading {path}: {e}"))?;
    match level::parse(&text) {
        Ok(p) => Ok(p),
        Err(level::ParseError::NoHoles) => Err(format!("{path}: no open holes left, the level is solved")),
        Err(e) => Err(format!("parsing {path}: {e:?}")),
    }
}

fn solve(level: &Level, start: State) -> Result<Vec<solve::Push>, String> {
    match solve::solve(level, start) {
        Ok(p) => Ok(p),
        Err(solve::SolveError::LimitReached) => Err("no solution found within the search limit".into()),
        Err(solve::SolveError::Unsolvable) => Err("unsolvable from this position".into()),
    }
}

fn run(cmd: Cmd) -> Result<(), String> {
    match cmd {
        Cmd::Board { path, flip_h, flip_v } => {
            let text = std::fs::read_to_string(&path).map_err(|e| format!("reading {path}: {e}"))?;
            println!("{}", level::flip(&text, flip_h, flip_v));
            Ok(())
        }
        Cmd::Solve { path, boards } => {
            let (level, start) = load(&path)?;
            let pushes = solve(&level, start)?;
            let batches = keys::batches(&level, start, &pushes);
            let mut total = 0;
            for (n, b) in batches.iter().enumerate() {
                total += b.keys.len();
                let (x, y) = b.from;
                println!("{:3}  boulder at col {x:2} row {y:2}  {:5} x{:<2}  {}", n + 1, b.dir.word(), b.pushes, b.keys);
                if boards {
                    println!("{}\n", level.render(&b.after));
                }
            }
            println!("\n{} pushes, {} batches, {total} keys", pushes.len(), batches.len());
            Ok(())
        }
        Cmd::Play { path, window, cell } => {
            let (level, start) = load(&path)?;
            let pushes = solve(&level, start)?;
            let steps = keys::steps(&level, start, &pushes);
            // stop short of the first key that walks into a mimic or pushes a
            // boulder onto one: the player kills it, then plays on.
            let touches = |s: &keys::Step| level.mimics.iter().any(|&m| s.after.hero == m || s.after.boulders.has(m));
            let (typed, mimic) = match steps.iter().position(touches) {
                Some(i) => (&steps[..i], Some(i)),
                None => (&steps[..], None),
            };
            println!("{} keys, typing {}", steps.len(), typed.len());
            match play::play(&level, start.hero, typed, &window, cell, &path) {
                Ok(()) => {
                    match mimic {
                        Some(_) => println!("stopped at the mimic: kill it, make its M a . in {path}, play again"),
                        None => println!("done: every hole filled"),
                    }
                    Ok(())
                }
                Err(e) => Err(play_error(e, &path)),
            }
        }
        Cmd::Locate { window, cell } => match play::locate(&window, cell) {
            Ok((x, y)) => {
                println!("cursor at pixel {x:.1},{y:.1}");
                Ok(())
            }
            Err(e) => Err(play_error(e, "")),
        },
    }
}

fn play_error(e: play::PlayError, board: &str) -> String {
    match e {
        play::PlayError::Lost { sent, want, at } => {
            format!("stopped after {sent} keys: hero should be at {want:?}, cursor is at {at:?}; {board} holds the last good board")
        }
        play::PlayError::Tool(cmd, detail) => format!("{cmd} failed: {detail}"),
        play::PlayError::NoWindow(title) => format!("no window titled {title:?} in hyprctl clients"),
        play::PlayError::Screenshot(e) => format!("unreadable screenshot: {e:?}"),
        play::PlayError::NoCursor => "no cursor on screen: is the game in front, without a menu?".into(),
        play::PlayError::WriteBoard(e) => format!("writing {board}: {e}"),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = match parse_args(&args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    match run(cmd) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
