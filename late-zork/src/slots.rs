use std::path::{Path, PathBuf};
use std::time::Duration;

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
    let mut editions = Vec::with_capacity(3);
    for edition in Edition::ALL {
        let home = directory(data_dir, account, edition);
        let autosave = inspect(bin, story_dir, &home, edition, false).await;
        let manual = inspect(bin, story_dir, &home, edition, true).await;
        editions.push(EditionSlots {
            edition,
            autosave,
            manual,
        });
    }
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
