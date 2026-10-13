use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::future::join_all;
use tokio::process::Command;

use crate::protocol::{Catalogue, Edition, EditionSlots, Slot};

pub(crate) fn directory(root: &str, account: &str, edition: Edition) -> PathBuf {
    Path::new(root).join(account).join(edition.key())
}

pub(crate) async fn list(
    bin: &str,
    story_dir: &str,
    data_dir: &str,
    account: &str,
    active: Option<Edition>,
) -> Catalogue {
    // All six inspections run at once: each is a bounded interpreter run
    // (up to two seconds), and the client's catalogue budget is fifteen.
    let editions = join_all(Edition::ALL.into_iter().map(|edition| async move {
        let home = directory(data_dir, account, edition);
        let (autosave, manual) = tokio::join!(
            inspect(bin, story_dir, &home, edition, false),
            inspect(bin, story_dir, &home, edition, true),
        );
        EditionSlots {
            edition,
            autosave,
            manual,
        }
    }))
    .await;
    Catalogue { editions, active }
}

async fn inspect(bin: &str, story_dir: &str, home: &Path, edition: Edition, manual: bool) -> Slot {
    match tokio::fs::try_exists(home).await {
        Ok(false) => return Slot::missing(),
        Ok(true) => {}
        Err(_) => return Slot::unavailable(),
    }
    let mut command = Command::new(bin);
    command
        .env_clear()
        .current_dir(home)
        .env("HOME", home)
        .env(
            "LATE_FROTZ_DOOR",
            if manual {
                "inspect-manual"
            } else {
                "inspect-auto"
            },
        )
        .arg(Path::new(story_dir).join(format!("{}.z3", edition.key())))
        .kill_on_drop(true);
    match tokio::time::timeout(Duration::from_secs(2), command.output()).await {
        Ok(Ok(output)) if output.status.success() && output.stdout.len() <= 2048 => {
            serde_json::from_slice(&output.stdout).unwrap_or_else(|_| Slot::unavailable())
        }
        _ => Slot::unavailable(),
    }
}
