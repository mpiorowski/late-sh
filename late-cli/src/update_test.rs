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
    let text = nag_lines("v0.0.1-cli", "v0.33.5-cli", "https://cli.late.sh", false).join("\n");
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
    let lines = nag_lines("v1-cli", "v2-cli", "https://cli.late.sh", true);
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

/// The nag offers `u` only where this build can replace itself.
#[test]
fn nag_offers_the_update_key_only_when_it_can_run() {
    let offered = nag_lines("v1-cli", "v2-cli", "https://cli.late.sh", true).join("\n");
    assert!(offered.contains("press u to update now"), "{offered}");
    let plain = nag_lines("v1-cli", "v2-cli", "https://cli.late.sh", false).join("\n");
    assert!(!plain.contains("press u"), "{plain}");
}

/// `u` updates, any other key skips the wait, and a key's release or
/// repeat answers nothing.
#[test]
fn a_key_under_the_nag_answers_it() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    let key = |code, kind| KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind,
        state: KeyEventState::NONE,
    };
    assert_eq!(
        answer_for(key(KeyCode::Char('u'), KeyEventKind::Press)),
        Some(NagAnswer::UpdateNow)
    );
    assert_eq!(
        answer_for(key(KeyCode::Char('U'), KeyEventKind::Press)),
        Some(NagAnswer::UpdateNow)
    );
    assert_eq!(
        answer_for(key(KeyCode::Enter, KeyEventKind::Press)),
        Some(NagAnswer::Continue)
    );
    assert_eq!(
        answer_for(key(KeyCode::Char('u'), KeyEventKind::Release)),
        None
    );
}

/// Raw mode turns Ctrl+C into a key, so the nag answers it as the
/// interrupt it was meant as: quit, never connect.
#[test]
fn ctrl_c_under_the_nag_quits() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    let ctrl_c = KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    };
    assert_eq!(answer_for(ctrl_c), Some(NagAnswer::Quit));
    let plain_c = KeyEvent {
        modifiers: KeyModifiers::NONE,
        ..ctrl_c
    };
    assert_eq!(answer_for(plain_c), Some(NagAnswer::Continue));
}

/// Unix builds run the same installer line the nag prints, from the
/// configured host; Windows cannot replace its running binary.
#[test]
fn the_self_update_runs_the_printed_installer_on_unix_only() {
    let script = self_update_script("https://mirror.example");
    match cfg!(unix) {
        true => assert_eq!(
            script.as_deref(),
            Some("curl -fsSL https://mirror.example/install.sh | bash")
        ),
        false => assert_eq!(script, None),
    }
}
