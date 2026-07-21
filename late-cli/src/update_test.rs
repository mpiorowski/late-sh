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
