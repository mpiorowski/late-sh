//! Best-effort "a newer release is available" check shown once before the SSH
//! session starts. Fail-open by design: any network, status, or parse error is
//! logged at debug and ignored, so a slow or unreachable distribution host
//! never blocks `late` from connecting.

use std::time::Duration;

use tracing::debug;

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
    for line in nag_lines(crate::config::VERSION, &latest, &base) {
        eprintln!("{line}");
    }
    tokio::time::sleep(NAG_PAUSE).await;
}

/// Per-platform install commands, defined in one place so the nag and any
/// future help text stay in sync. `{base}` is the distribution host.
fn install_methods(base: &str) -> [(&'static str, String); 4] {
    let shell = format!("curl -fsSL {base}/install.sh | bash");
    [
        ("linux", shell.clone()),
        ("macos", shell),
        ("windows", format!("irm {base}/install.ps1 | iex")),
        ("nixos", NIX_INSTALL_COMMAND.to_string()),
    ]
}

/// Build the multi-line "update available" nag with the install commands
/// aligned in a left-justified column so they all start at the same place.
fn nag_lines(current: &str, latest: &str, base: &str) -> Vec<String> {
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
mod tests {
    use super::*;

    #[test]
    fn outdated_detects_newer_release() {
        assert!(is_outdated("v0.27.11-cli", "v0.28.0-cli"));
        assert!(is_outdated("v0.27.11-cli", "v0.27.12-cli"));
        assert!(is_outdated("v0.9.0-cli", "v0.10.0-cli"));
    }

    #[test]
    fn not_outdated_when_same_or_newer() {
        assert!(!is_outdated("v0.28.0-cli", "v0.28.0-cli"));
        assert!(!is_outdated("v0.28.1-cli", "v0.28.0-cli"));
        assert!(!is_outdated("v1.0.0-cli", "v0.28.0-cli"));
    }

    #[test]
    fn outdated_falls_back_to_inequality_for_unparseable() {
        // Prerelease suffix isn't cleanly numeric, so inequality wins.
        assert!(is_outdated("v0.28.0-rc1-cli", "v0.28.0-cli"));
        assert!(!is_outdated("weird", "weird"));
    }

    #[test]
    fn sanitize_extracts_first_clean_line() {
        assert_eq!(
            sanitize_version("v0.28.0-cli\n"),
            Some("v0.28.0-cli".to_string())
        );
        assert_eq!(
            sanitize_version("  v0.28.0-cli  \nextra"),
            Some("v0.28.0-cli".to_string())
        );
    }

    #[test]
    fn nag_lists_each_platform_with_base_url() {
        let text = nag_lines("v0.0.1-cli", "v0.33.5-cli", "https://cli.late.sh").join("\n");
        assert!(text.contains("v0.0.1-cli -> v0.33.5-cli"));
        for label in ["linux", "macos", "windows", "nixos"] {
            assert!(text.contains(label), "missing platform: {label}");
        }
        assert!(text.contains("https://cli.late.sh/install.sh"));
        assert!(text.contains("https://cli.late.sh/install.ps1"));
        assert!(text.contains("nix run github:mpiorowski/late-sh#late"));
    }

    #[test]
    fn nag_install_commands_align_to_one_column() {
        let lines = nag_lines("v1-cli", "v2-cli", "https://cli.late.sh");
        // Every command should begin at the same column. Match on command
        // prefixes that don't also appear in a label (e.g. "nix" would collide
        // with the "nixos" label, so match the fuller "nix run").
        let starts: Vec<usize> = lines
            .iter()
            .filter_map(|line| {
                ["curl", "irm", "nix run"]
                    .iter()
                    .find_map(|cmd| line.find(cmd))
            })
            .collect();
        assert_eq!(starts.len(), 4);
        assert!(starts.iter().all(|&start| start == starts[0]));
    }

    #[test]
    fn sanitize_rejects_html_and_empty() {
        assert_eq!(sanitize_version("<!DOCTYPE html>"), None);
        assert_eq!(sanitize_version(""), None);
        assert_eq!(sanitize_version("   \n"), None);
    }
}
