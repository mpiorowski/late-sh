// The morgue tree: where crawl writes a game's dump, and the one layout the
// public DCSS tooling can address.
//
// Both ecosystem consumers build a game's morgue URL the same way, from the
// xlog line alone, and neither is configurable:
//   dcss-stats  `${morgueUrl}/${name}/morgue-${name}-${end}.txt`  (getMorgueUrl)
//   Sequell     `path + '/' + player + '/morgue-' + player + ...` (MorgueFilename)
// So the dump has to sit in a directory named for the player. crawl's own
// default is flat (`morgue_dir = _get_save_path("morgue/")`, initfile.cc), which
// is why the host passes `-morgue <root>/<playname>`: every published link
// resolves, or none of them do.
//
// [`migrate_flat`] carries the dumps written before that flag existed into the
// same shape, so the backfill links too.

use std::fs;
use std::path::Path;

/// Everything crawl writes lives under this subdirectory of the child `HOME`
/// (`config.data_dir`). Mirrors `publish.rs`'s constant of the same name: the
/// two modules address one tree from opposite ends, this one writing it and
/// that one serving it.
const CRAWL_SUBDIR: &str = ".crawl";

/// Morgue dumps: one file per finished game, plus the `#` character dumps.
const MORGUE_SUBDIR: &str = "morgue";

/// The morgue root, `$HOME/.crawl/morgue`. This is crawl's own default for the
/// build (SAVEDIR `~/.crawl` + `morgue/`) and the directory `publish.rs` serves
/// at `/crawl/morgue/`; passing `-morgue` moves a player's dumps one level
/// below it, never outside it.
fn root(data_dir: &str) -> String {
    format!(
        "{}/{CRAWL_SUBDIR}/{MORGUE_SUBDIR}",
        data_dir.trim_end_matches('/')
    )
}

/// The per-player morgue directory passed as crawl's `-morgue`. Keyed by the
/// (already sanitized, `[A-Za-z0-9_]`) playname, which is exactly the path
/// segment the ecosystem's fetchers ask for.
pub(crate) fn player_dir(data_dir: &str, playname: &str) -> String {
    format!("{}/{playname}", root(data_dir))
}

/// Move dumps sitting directly under the morgue root into their player's
/// directory. Games finished before the host passed `-morgue` landed flat,
/// where every public link 404s; this is the one-time carry-over, and it is
/// idempotent because a migrated root holds only directories.
///
/// Runs at boot before either listener binds (`main.rs`), which is what makes
/// it safe: no crawl child can be writing a dump into the tree, and no fetcher
/// can observe it half-moved. Each file moves with a rename inside one
/// filesystem, so an interrupted boot leaves every dump either fully at the old
/// path or fully at the new one, and the next boot finishes the job.
///
/// Best-effort by design: a dump that will not move costs one dead link on
/// someone else's website, which is not worth failing the pod (and the door)
/// over. Returns how many files moved.
pub(crate) fn migrate_flat(data_dir: &str) -> usize {
    let root = root(data_dir);
    let root = Path::new(&root);
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        // No morgue directory yet: a fresh volume, nothing to carry over.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return 0,
        Err(e) => {
            tracing::warn!(path = %root.display(), error = ?e, "could not read morgue root; skipping migration");
            return 0;
        }
    };

    let mut moved = 0;
    let mut left = 0;
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                tracing::warn!(path = %root.display(), error = ?e, "could not walk morgue root");
                break;
            }
        };
        // Already-migrated players. Nothing else in this tree is a directory.
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let name = entry.file_name();
        let Some(owner) = name.to_str().and_then(owner_of) else {
            left += 1;
            continue;
        };

        let dir = root.join(owner);
        if let Err(e) = fs::create_dir_all(&dir) {
            tracing::warn!(path = %dir.display(), error = ?e, "could not create player morgue dir");
            left += 1;
            continue;
        }
        let to = dir.join(&name);
        // A dump already at the destination is the authoritative one: it was
        // written by crawl itself under `-morgue`, so never clobber it.
        if to.exists() {
            left += 1;
            continue;
        }
        match fs::rename(entry.path(), &to) {
            Ok(()) => moved += 1,
            Err(e) => {
                tracing::warn!(from = %entry.path().display(), to = %to.display(), error = ?e, "could not move morgue dump");
                left += 1;
            }
        }
    }

    if moved > 0 || left > 0 {
        tracing::info!(
            moved,
            left,
            "carried flat morgue dumps into per-player directories"
        );
    }
    moved
}

/// The player a file at the morgue root belongs to, or `None` for a name crawl
/// does not write there.
///
/// Two shapes land in this directory:
/// - `morgue-<playname>-<YYYYMMDD>-<HHMMSS>.txt`, a finished game, and the
///   `.lst` stash list beside it.
/// - `<playname>.txt`, an in-game `#` character dump, and its `.lst`.
///
/// Splitting the first shape on its dashes works precisely because a playname
/// cannot contain one (`playname::sanitize` keeps `[A-Za-z0-9_]`) while the
/// timestamp always does, so the first dash after the prefix ends the name. The
/// same rule then vets the result, which is what keeps a hand-made file from
/// naming a directory anywhere but here.
fn owner_of(file_name: &str) -> Option<&str> {
    let stem = file_name
        .strip_suffix(".txt")
        .or_else(|| file_name.strip_suffix(".lst"))?;
    let owner = match stem.strip_prefix("morgue-") {
        Some(rest) => rest.split('-').next()?,
        None => stem,
    };
    let playname_shaped = !owner.is_empty()
        && owner
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_');
    playname_shaped.then_some(owner)
}

#[cfg(test)]
#[path = "morgue_test.rs"]
mod morgue_test;
