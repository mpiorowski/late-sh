/// Arcade handles are already validated to 3-20 chars, alnum + underscore,
/// starting with a letter (`late_core::models::arcade_handle`). This cap
/// matches that ceiling.
const MAX_LEN: usize = 20;

/// Used when a connection presents an empty or fully-stripped username.
/// Should not happen in practice (late-ssh always sends the account's claimed
/// arcade handle), but bashquest.sh should never see an empty
/// `BASHQUEST_AUTOLOGIN`.
const FALLBACK: &str = "player";

/// Sanitize the SSH username into a value safe to hand bashquest.sh as
/// `BASHQUEST_AUTOLOGIN`.
///
/// late-ssh already derives and sends a validated arcade handle, so this is
/// defense in depth: keep only ASCII alphanumerics and underscore, and cap at
/// `MAX_LEN`. bashquest.sh re-sanitizes on its own side too (`autologin()`),
/// but the host should never hand a hostile string into the child's
/// environment in the first place.
pub(crate) fn sanitize(username: &str) -> String {
    let cleaned: String = username
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(MAX_LEN)
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
