use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

/// Prepare the writable game tree before serving: copy any file missing from
/// `game_dir` out of the image's read-only `seed_dir` template, then sweep the
/// stale-lock artifacts a hard kill can leave behind.
///
/// The copy is strictly fill-in-the-blanks: an existing file is never
/// overwritten, so the shared world (DATA/USERS.DAT and friends) survives both
/// restarts and image upgrades, while a fresh volume gets the full seed (TEXT/
/// screens, DOCS/, USURPER.CFG, USURP.CTL, and the generated world data).
///
/// The sweeps are safe exactly because this runs before any session exists:
/// - `DATA/MAINT.FLG` is the game's maintenance lock; if the host died mid-
///   maintenance it would block every node forever (the door-wide wedge).
/// - `NODE/ONLINERS.DAT` is the who-is-playing table; nobody is playing at
///   boot, and the sysop docs bless deleting it exactly then. Stale entries
///   otherwise show ghost players until the game's own kick-out ages them.
pub fn prepare_game_dir(seed_dir: &str, game_dir: &str) -> Result<()> {
    fs::create_dir_all(game_dir)
        .with_context(|| format!("creating usurper game dir {game_dir}"))?;
    copy_missing(Path::new(seed_dir), Path::new(game_dir))
        .with_context(|| format!("seeding {game_dir} from {seed_dir}"))?;

    for stale in ["DATA/MAINT.FLG", "NODE/ONLINERS.DAT"] {
        let path = Path::new(game_dir).join(stale);
        match fs::remove_file(&path) {
            Ok(()) => tracing::info!(file = %path.display(), "swept stale lock file at boot"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(file = %path.display(), error = ?e, "could not sweep stale lock file"),
        }
    }
    Ok(())
}

fn copy_missing(seed: &Path, target: &Path) -> Result<()> {
    for entry in fs::read_dir(seed).with_context(|| format!("reading {}", seed.display()))? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&to).with_context(|| format!("creating {}", to.display()))?;
            copy_missing(&from, &to)?;
        } else if !to.exists() {
            fs::copy(&from, &to)
                .with_context(|| format!("copying {} -> {}", from.display(), to.display()))?;
        }
    }
    Ok(())
}
