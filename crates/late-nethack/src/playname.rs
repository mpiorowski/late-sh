/// NetHack's `PL_NSIZ` is 32, leaving 31 usable characters for the `-u` name.
const PL_NSIZ_USABLE: usize = 31;

/// Used when a connection presents an empty or fully-stripped username. Should
/// not happen in practice (late-ssh always sends an account-derived playname),
/// but we never pass an empty `-u` to the child.
const FALLBACK: &str = "late";

/// Sanitize the SSH username into a PTY-safe NetHack `-u` playname.
///
/// late-ssh already derives a safe, account-stable name (`late_` + UUID hex) and
/// sends it as the SSH username, so this is defense in depth: keep only ASCII
/// alphanumerics and underscore, and cap at `PL_NSIZ`. Anything else is dropped
/// rather than passed through to the child's argv.
pub fn sanitize(username: &str) -> String {
    let cleaned: String = username
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(PL_NSIZ_USABLE)
        .collect();
    if cleaned.is_empty() {
        FALLBACK.to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_alphanumerics_and_underscore() {
        assert_eq!(sanitize("late_9f3c1122"), "late_9f3c1122");
    }

    #[test]
    fn strips_punctuation_and_shell_metachars() {
        assert_eq!(sanitize("bob; rm -rf /"), "bobrmrf");
        assert_eq!(sanitize("a b\tc"), "abc");
    }

    #[test]
    fn caps_at_pl_nsiz() {
        let name = sanitize(&"x".repeat(100));
        assert_eq!(name.len(), PL_NSIZ_USABLE);
    }

    #[test]
    fn empty_falls_back() {
        assert_eq!(sanitize(""), FALLBACK);
        assert_eq!(sanitize("!@#$"), FALLBACK);
    }
}
