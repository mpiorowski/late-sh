/// The arcade handle's own cap (`late_core::models::arcade_handle`). Usurper
/// keys the player record by the dropfile's uppercased "real name", a Pascal
/// short string with plenty of headroom, so the handle cap is the binding one.
const MAX_NAME_LENGTH: usize = 20;

/// Used when a connection presents an empty or fully-stripped username. Should
/// not happen in practice (late-ssh always sends the account's arcade handle),
/// but we never write an empty name into the dropfile.
const FALLBACK: &str = "late";

/// Sanitize the SSH username into the player name written to DOOR32.SYS.
///
/// late-ssh already sends a validated arcade handle, so this is defense in
/// depth: keep only ASCII alphanumerics and underscore, cap the length, and
/// drop everything else. Crucially this can never produce whitespace or
/// newlines: the name lands inside a line-oriented dropfile, and a space would
/// split it into first/last name (changing the identity the game keys on).
pub(crate) fn sanitize(username: &str) -> String {
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
