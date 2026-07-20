use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

/// Write the per-session DOOR32.SYS dropfile and return the game-relative
/// directory to pass via `/P`.
///
/// DOOR32.SYS is how a BBS hands a door its session: comm type, user identity,
/// time budget. We are the "BBS" here, so each SSH session gets its own
/// dropfile directory (`DROP/<node>/`) inside the game dir, freshly rewritten
/// on every launch. Comm type 0 (Local) is the load-bearing choice: the game
/// then talks to its controlling terminal (our PTY) with no socket or FOSSIL
/// layer, and skips the interactive "enter your name" local logon because the
/// identity comes from the file.
///
/// The playname is already sanitized (`playname::sanitize`): single token, no
/// whitespace/newlines, so it can't split into a first/last name pair or break
/// the line-oriented format.
pub fn write_door32(game_dir: &str, node: u16, playname: &str) -> Result<String> {
    let rel = format!("DROP/{node}/");
    let dir = Path::new(game_dir).join("DROP").join(node.to_string());
    fs::create_dir_all(&dir).with_context(|| format!("creating dropfile dir {}", dir.display()))?;
    // Field order per the DOOR32.SYS standard (and the game's load_door32):
    // comm type, comm/socket handle, baud, BBSID, user record #, real name,
    // handle/alias, access level, time left (minutes), emulation (1=ANSI),
    // node number. The game keys the player record on the uppercased real
    // name; time is per-session and generous (the game's own daily turn
    // limits are what actually bound play).
    let contents = format!("0\n0\n38400\nlate.sh\n1\n{playname}\n{playname}\n100\n999\n1\n{node}\n");
    let path = dir.join("door32.sys");
    fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))?;
    Ok(rel)
}
