/// crawl's `MAX_NAME_LENGTH` is 30; names longer than that are rejected at the
/// name prompt, so never pass one to `-name`.
const MAX_NAME_LENGTH: usize = 30;

/// Used when a connection presents an empty or fully-stripped username. Should
/// not happen in practice (late-ssh always sends an account-derived playname),
/// but we never pass an empty `-name` to the child.
const FALLBACK: &str = "late";

/// Sanitize the SSH username into a PTY-safe crawl `-name` playname.
///
/// late-ssh already derives a safe, account-stable name (`late_` + UUID hex) and
/// sends it as the SSH username, so this is defense in depth: keep only ASCII
/// alphanumerics and underscore (both valid in crawl names), and cap at crawl's
/// name limit. Anything else is dropped rather than passed through to the
/// child's argv.
pub fn sanitize(username: &str) -> String {
    let cleaned: String = username
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
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
