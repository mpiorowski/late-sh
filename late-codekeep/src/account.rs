const PREFIX: &str = "late_";
const HEX_LEN: usize = 24;

/// Accept only the stable account label produced by late-ssh. The label becomes
/// a directory name below the CodeKeep data root, so fail closed on any drift.
pub(crate) fn sanitize(user: &str) -> Option<String> {
    let hex = user.strip_prefix(PREFIX)?;
    if hex.len() != HEX_LEN || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!("{PREFIX}{}", hex.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_client_label() {
        assert_eq!(
            sanitize("late_112233445566778899aabbcc").as_deref(),
            Some("late_112233445566778899aabbcc")
        );
    }

    #[test]
    fn rejects_paths_and_wrong_shapes() {
        assert_eq!(sanitize("../../late_112233445566778899aabbcc"), None);
        assert_eq!(sanitize("late_short"), None);
        assert_eq!(sanitize("late_112233445566778899aabbcg"), None);
    }
}
