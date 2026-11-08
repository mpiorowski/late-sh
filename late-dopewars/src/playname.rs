/// dopewars' `PlayerName` dialogue accepts a length-limited, mostly-free-form
/// name; cap well under anything that would overflow a `.sco` record.
const MAX_NAME_LENGTH: usize = 32;

/// Used when a connection presents an empty or fully-stripped username. Should
/// not happen in practice (late-ssh always sends a sanitized playname), but we
/// never pass an empty `-P` to the child.
const FALLBACK: &str = "drifter";

/// Sanitize the SSH username into a PTY-safe dopewars `-P` playname.
///
/// late-ssh already sends a safe handle (`dopewars_playname`), so this is defense
/// in depth: keep only characters that are valid in a late.sh username, cap the
/// length, and never pass an empty name through to the child's argv.
pub(crate) fn sanitize(username: &str) -> String {
    let cleaned: String = username
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        .take(MAX_NAME_LENGTH)
        .collect();
    if cleaned.is_empty() {
        FALLBACK.to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
#[path = "playname_test.rs"]
mod playname_test;
