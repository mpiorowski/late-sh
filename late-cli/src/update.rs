//! Best-effort "a newer release is available" check shown once before the SSH
//! session starts. Fail-open by design: any network, status, or parse error is
//! logged at debug and ignored, so a slow or unreachable distribution host
//! never blocks `late` from connecting. On Unix the nag also offers `u` to run
//! the installer over this binary and restart into the new one.

use std::{
    convert::Infallible,
    io::IsTerminal,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use tracing::{debug, info, warn};

/// Distribution host serving the published `VERSION` file and installers.
/// Matches the installer default and its `LATE_INSTALL_BASE_URL` override, so a
/// custom mirror works for both the installer and this check.
const DEFAULT_DIST_BASE_URL: &str = "https://cli.late.sh";
/// Nix flake app, pointed at the GitHub repo rather than the dist host.
const NIX_INSTALL_COMMAND: &str = "nix run github:mpiorowski/late-sh#late";
/// Keep the network probe short; it runs before connect on every release build.
const CHECK_TIMEOUT: Duration = Duration::from_secs(2);
/// How long the "please update" nag stays on screen before connecting anyway.
const NAG_PAUSE: Duration = Duration::from_secs(5);

/// Probe for a newer release and, if one is published, print a short nag and
/// pause for [`NAG_PAUSE`] before returning. Always returns; never errors.
/// No-op on unstamped local/dev builds and when disabled via env.
pub(crate) async fn check_for_update() {
    // Only stamped release builds carry the release tag; source/dev builds
    // embed the Cargo.toml fallback and must never nag.
    if crate::config::VERSION == env!("CARGO_PKG_VERSION") {
        return;
    }
    if disabled() {
        debug!("update check disabled via LATE_NO_UPDATE_CHECK");
        return;
    }
    let base = dist_base_url();
    let Some(latest) = fetch_latest_version(&base).await else {
        return;
    };
    if !is_outdated(crate::config::VERSION, &latest) {
        return;
    }
    // The path is read before anything replaces the file: on Linux it comes
    // from /proc/self/exe, which names the old, unlinked file afterwards.
    let offer = match (self_update_script(&base), std::env::current_exe()) {
        (Some(script), Ok(exe)) if std::io::stdin().is_terminal() => Some((script, exe)),
        (Some(_), Err(err)) => {
            debug!(error = %err, "update check: no path to this binary, not offering an update");
            None
        }
        _ => None,
    };
    for line in nag_lines(crate::config::VERSION, &latest, &base, offer.is_some()) {
        eprintln!("{line}");
    }
    let Some((script, exe)) = offer else {
        tokio::time::sleep(NAG_PAUSE).await;
        return;
    };
    let answer = match tokio::task::spawn_blocking(|| wait_for_answer(NAG_PAUSE)).await {
        Ok(Ok(answer)) => answer,
        Ok(Err(err)) => {
            debug!(error = %err, "update check: could not read a key, continuing");
            NagAnswer::Continue
        }
        Err(err) => {
            debug!(error = %err, "update check: key wait task failed, continuing");
            NagAnswer::Continue
        }
    };
    match answer {
        NagAnswer::Continue => {}
        NagAnswer::UpdateNow => {
            // Success replaces this process, so only a failure comes back.
            let Err(err) = update_and_restart(&script, &exe);
            warn!(error = ?err, "self update failed");
            eprintln!("late: update failed: {err:#}");
            eprintln!("late: run it yourself: {script}");
            eprintln!(
                "late: connecting with {} for now...",
                crate::config::VERSION
            );
        }
    }
}

/// How the wait under the nag ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NagAnswer {
    UpdateNow,
    Continue,
}

/// What a key pressed under the nag means: `u` updates, any other key skips
/// the wait. Releases and repeats mean nothing (Windows reports both).
fn answer_for(key: KeyEvent) -> Option<NagAnswer> {
    match (key.kind, key.code) {
        (KeyEventKind::Press, KeyCode::Char('u' | 'U')) => Some(NagAnswer::UpdateNow),
        (KeyEventKind::Press, _) => Some(NagAnswer::Continue),
        (KeyEventKind::Release | KeyEventKind::Repeat, _) => None,
    }
}

/// Wait up to `timeout` for a key, in raw mode so one key answers without
/// Enter. Raw mode is off again before this returns, whatever happened.
fn wait_for_answer(timeout: Duration) -> Result<NagAnswer> {
    crossterm::terminal::enable_raw_mode().context("entering raw mode")?;
    let answer = read_answer(timeout);
    let restored = crossterm::terminal::disable_raw_mode().context("leaving raw mode");
    let answer = answer?;
    restored?;
    Ok(answer)
}

fn read_answer(timeout: Duration) -> Result<NagAnswer> {
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || !event::poll(left).context("polling for a key")? {
            return Ok(NagAnswer::Continue);
        }
        if let Event::Key(key) = event::read().context("reading a key")?
            && let Some(answer) = answer_for(key)
        {
            return Ok(answer);
        }
    }
}

/// The shell line that installs the latest release, for the builds that can
/// replace their own binary while it runs. Windows cannot: a running
/// `late.exe` is locked, so `install.ps1` would fail to copy over it.
fn self_update_script(base: &str) -> Option<String> {
    match cfg!(unix) {
        true => Some(shell_install_command(base)),
        false => None,
    }
}

/// Run the installer into the directory `exe` lives in, so this binary is
/// the one replaced wherever it was installed, then restart into it.
fn update_and_restart(script: &str, exe: &Path) -> Result<Infallible> {
    let dir = exe
        .parent()
        .context("this binary's path has no directory")?;
    info!(dir = %dir.display(), "running the installer");
    eprintln!();
    let status = std::process::Command::new("bash")
        .arg("-c")
        .arg(script)
        .env("LATE_INSTALL_DIR", dir)
        .status()
        .context("starting bash for the installer")?;
    if !status.success() {
        anyhow::bail!("installer exited with {status}");
    }
    eprintln!();
    eprintln!("late: updated, restarting...");
    restart(exe)
}

/// Replace this process with the new binary on the same arguments. The
/// check is off in the new one: a host whose `VERSION` runs ahead of its
/// download would otherwise offer the same update forever.
#[cfg(unix)]
fn restart(exe: &Path) -> Result<Infallible> {
    use std::os::unix::process::CommandExt;
    let err = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env("LATE_NO_UPDATE_CHECK", "1")
        .exec();
    Err(err).with_context(|| format!("restarting {}", exe.display()))
}

#[cfg(not(unix))]
fn restart(exe: &Path) -> Result<Infallible> {
    anyhow::bail!("restarting {} is unix-only", exe.display())
}

/// The curl installer line for Linux and macOS: the nag prints it and the
/// `u` self update runs it, so the two cannot drift apart.
fn shell_install_command(base: &str) -> String {
    format!("curl -fsSL {base}/install.sh | bash")
}

/// Per-platform install commands, defined in one place so the nag and any
/// future help text stay in sync. `{base}` is the distribution host.
fn install_methods(base: &str) -> [(&'static str, String); 4] {
    let shell = shell_install_command(base);
    [
        ("linux", shell.clone()),
        ("macos", shell),
        ("windows", format!("irm {base}/install.ps1 | iex")),
        ("nixos", NIX_INSTALL_COMMAND.to_string()),
    ]
}

/// Build the multi-line "update available" nag with the install commands
/// aligned in a left-justified column so they all start at the same place.
fn nag_lines(current: &str, latest: &str, base: &str, offers_update: bool) -> Vec<String> {
    let methods = install_methods(base);
    let label_width = methods
        .iter()
        .map(|(label, _)| label.len())
        .max()
        .unwrap_or(0);

    let mut lines = vec![
        String::new(),
        format!("late: a new version is available  {current} -> {latest}"),
        String::new(),
    ];
    for (label, command) in &methods {
        lines.push(format!("  {label:<width$}  {command}", width = label_width));
    }
    lines.push(String::new());
    if offers_update {
        lines.push("press u to update now and restart, any other key to go on".to_string());
    }
    lines.push(format!(
        "continuing in {}s (set LATE_NO_UPDATE_CHECK=1 to skip)...",
        NAG_PAUSE.as_secs()
    ));
    lines
}

fn disabled() -> bool {
    matches!(std::env::var("LATE_NO_UPDATE_CHECK"), Ok(value) if !value.trim().is_empty() && value != "0")
}

fn dist_base_url() -> String {
    let base = std::env::var("LATE_INSTALL_BASE_URL")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_DIST_BASE_URL.to_string());
    base.trim_end_matches('/').to_string()
}

async fn fetch_latest_version(base: &str) -> Option<String> {
    let url = format!("{base}/VERSION");
    let client = reqwest::Client::builder()
        .timeout(CHECK_TIMEOUT)
        .build()
        .inspect_err(|err| debug!(error = %err, "update check: client build failed"))
        .ok()?;
    let response = client
        .get(&url)
        .send()
        .await
        .inspect_err(|err| debug!(error = %err, "update check: request failed"))
        .ok()?;
    if !response.status().is_success() {
        debug!(status = %response.status(), "update check: non-success status");
        return None;
    }
    let body = response
        .text()
        .await
        .inspect_err(|err| debug!(error = %err, "update check: read body failed"))
        .ok()?;
    sanitize_version(&body)
}

/// Pull a single plausible version token from the fetched body. Guards against
/// a misconfigured host returning HTML or junk instead of the `VERSION` file.
fn sanitize_version(body: &str) -> Option<String> {
    let line = body.lines().next().unwrap_or("").trim();
    if line.is_empty() || line.len() > 64 {
        return None;
    }
    let valid = line
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'));
    valid.then(|| line.to_string())
}

/// True when `latest` is a strictly newer release than `current`. Compares the
/// numeric dotted core (after stripping a leading `v` and a trailing `-cli`);
/// falls back to plain inequality when either side isn't cleanly numeric, so an
/// unparseable scheme still nags rather than silently going stale.
fn is_outdated(current: &str, latest: &str) -> bool {
    match (numeric_core(current), numeric_core(latest)) {
        (Some(current), Some(latest)) => latest > current,
        _ => current != latest,
    }
}

fn numeric_core(version: &str) -> Option<Vec<u64>> {
    let mut core = version.trim();
    core = core.strip_prefix('v').unwrap_or(core);
    core = core.strip_suffix("-cli").unwrap_or(core);
    core.split('.').map(|part| part.parse().ok()).collect()
}

#[cfg(test)]
#[path = "update_test.rs"]
mod update_test;
