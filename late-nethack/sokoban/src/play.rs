// Type a solution into the late.sh window under Hyprland: send each key with
// `hyprctl`, screenshot the window with `grim`, and check the cursor (the
// hero) is where the plan says after every chunk. Stops on the first
// mismatch: a monster in the way, a prompt, a boulder that didn't move.

use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use crate::keys::Step;
use crate::level::Level;
use crate::screen::{CellSize, Grid, PpmError, cursor, parse_ppm};

/// The pacing the player watches at: 5 keys, a beat between them, a second
/// of pause, then the check. They interrupt on a monster.
const CHUNK: usize = 5;
const KEY_GAP: Duration = Duration::from_millis(150);
const PAUSE: Duration = Duration::from_secs(1);
/// A hidden window's screenshot can lag; look a few times before calling it.
const CHECKS: usize = 6;
const CHECK_GAP: Duration = Duration::from_millis(500);

#[derive(Debug)]
pub enum PlayError {
    /// A tool failed to run or exited non-zero: (command, detail).
    Tool(&'static str, String),
    NoWindow(String),
    Screenshot(PpmError),
    NoCursor,
    /// The hero isn't where the plan put them after `sent` keys.
    Lost { sent: usize, want: (usize, usize), at: Option<(i64, i64)> },
    WriteBoard(String),
}

struct Window {
    /// `address:0x...` for `hyprctl` dispatchers.
    address: String,
    /// The foreign toplevel id `grim -T` captures, Hyprland's `stableId`.
    toplevel: String,
}

fn run(cmd: &'static str, args: &[&str]) -> Result<Vec<u8>, PlayError> {
    match Command::new(cmd).args(args).output() {
        Ok(out) if out.status.success() => Ok(out.stdout),
        Ok(out) => Err(PlayError::Tool(cmd, String::from_utf8_lossy(&out.stderr).trim().to_string())),
        Err(e) => Err(PlayError::Tool(cmd, e.to_string())),
    }
}

fn find_window(title: &str) -> Result<Window, PlayError> {
    let out = run("hyprctl", &["clients", "-j"])?;
    let clients: serde_json::Value = match serde_json::from_slice(&out) {
        Ok(v) => v,
        Err(e) => return Err(PlayError::Tool("hyprctl", format!("clients -j: {e}"))),
    };
    let found = clients.as_array().into_iter().flatten().find(|c| c["title"] == title);
    match found.map(|c| (c["address"].as_str(), c["stableId"].as_str())) {
        Some((Some(address), Some(toplevel))) => {
            Ok(Window { address: format!("address:{address}"), toplevel: toplevel.to_string() })
        }
        _ => Err(PlayError::NoWindow(title.to_string())),
    }
}

fn send(w: &Window, key: char) -> Result<(), PlayError> {
    let call = format!(r#"hl.dsp.send_shortcut({{ mods = "", key = "{key}", window = "{}" }})"#, w.address);
    run("hyprctl", &["dispatch", &call]).map(|_| ())
}

fn hero_pixel(w: &Window, cell: CellSize) -> Result<Option<(f64, f64)>, PlayError> {
    let ppm = run("grim", &["-t", "ppm", "-T", &w.toplevel, "-"])?;
    match parse_ppm(&ppm) {
        Ok(img) => Ok(cursor(&img, cell)),
        Err(e) => Err(PlayError::Screenshot(e)),
    }
}

/// The cursor's pixel in the window titled `title`, typing nothing.
pub fn locate(title: &str, cell: CellSize) -> Result<(f64, f64), PlayError> {
    let window = find_window(title)?;
    hero_pixel(&window, cell)?.ok_or(PlayError::NoCursor)
}

/// Type `steps` into the window titled `title`, from `level`'s current hero
/// square. After every verified chunk the board at that point is written to
/// `board_path`, so after a stop it holds the last position known good.
pub fn play(
    level: &Level,
    hero: usize,
    steps: &[Step],
    title: &str,
    cell: CellSize,
    board_path: &str,
) -> Result<(), PlayError> {
    let window = find_window(title)?;
    let at = hero_pixel(&window, cell)?.ok_or(PlayError::NoCursor)?;
    let grid = Grid::calibrate(cell, at, level.pos[hero]);
    let mut sent = 0;
    for chunk in steps.chunks(CHUNK) {
        for step in chunk {
            send(&window, step.key)?;
            sleep(KEY_GAP);
        }
        sent += chunk.len();
        sleep(PAUSE);
        let after = chunk[chunk.len() - 1].after;
        let want = level.pos[after.hero];
        let mut seen = None;
        for _ in 0..CHECKS {
            seen = hero_pixel(&window, cell)?.map(|px| grid.cell_at(px));
            if seen == Some((want.0 as i64, want.1 as i64)) {
                break;
            }
            sleep(CHECK_GAP);
        }
        if seen != Some((want.0 as i64, want.1 as i64)) {
            return Err(PlayError::Lost { sent, want, at: seen });
        }
        if let Err(e) = std::fs::write(board_path, level.render(&after) + "\n") {
            return Err(PlayError::WriteBoard(e.to_string()));
        }
        let keys: String = chunk.iter().map(|s| s.key).collect();
        println!("{sent:4}/{} {keys:5} ok {want:?}", steps.len());
    }
    Ok(())
}
