/// Cap for the sanitized playname. The playname becomes a directory name under
/// the playground (`players/<playname>`), so the bound is filesystem hygiene,
/// not a game limit; late-ssh's arcade handles are at most 20 chars already.
const MAX_NAME_LENGTH: usize = 30;

/// Used when a connection presents an empty or fully-stripped username. Should
/// not happen in practice (late-ssh always sends the account's arcade handle),
/// but we never build a player directory from an empty name.
const FALLBACK: &str = "late";

/// Sanitize the SSH username into a filesystem-safe playname.
///
/// late-ssh already sends a validated arcade handle (`[A-Za-z][A-Za-z0-9_]*`)
/// as the SSH username, so this is defense in depth: keep only ASCII
/// alphanumerics and underscore, and cap the length. Anything else is dropped
/// rather than turned into a path component: in particular `/` and `.` can
/// never survive, so a hostile username cannot traverse out of the playground.
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
