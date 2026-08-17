// The pushed per-player rc file: decode + validation for the one env request
// late-ssh sends before the shell (`LATE_DOOR_RC_B64`). The DB copy on the
// late.sh side is the source of truth; what lands here is an ephemeral
// materialization for the child. The env var name and the 16KB cap are
// duplicated in late-ssh (`app/door/rc.rs`) and the other rc-taking host;
// keep the copies in sync, like the identity derivations.

use base64::Engine as _;

/// SSH env variable carrying the base64 rc. An empty decoded value means
/// "clear the per-player file".
pub(crate) const RC_ENV_VAR: &str = "LATE_DOOR_RC_B64";

/// Decoded size cap, mirroring late-ssh's paste boundary.
pub(crate) const MAX_RC_BYTES: usize = 16 * 1024;

/// Decode and validate a pushed rc env value. Refusals are tagged for the
/// caller's log line; a refused rc simply leaves the child on defaults.
pub(crate) fn decode_rc(value: &str) -> Result<String, &'static str> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| "invalid base64")?;
    if bytes.len() > MAX_RC_BYTES {
        return Err("rc exceeds size cap");
    }
    let text = String::from_utf8(bytes).map_err(|_| "rc is not utf-8")?;
    if text.contains('\0') {
        return Err("rc contains NUL");
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b64(s: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(s)
    }

    #[test]
    fn decodes_plain_rc_and_empty_clear() {
        assert_eq!(
            decode_rc(&b64(b"OPTIONS=color\n")).unwrap(),
            "OPTIONS=color\n"
        );
        assert_eq!(decode_rc(&b64(b"")).unwrap(), "");
    }

    #[test]
    fn rejects_bad_base64_oversize_nul_and_non_utf8() {
        assert!(decode_rc("not base64!!").is_err());
        assert!(decode_rc(&b64(&vec![b'x'; MAX_RC_BYTES + 1])).is_err());
        assert!(decode_rc(&b64(b"OPT\0IONS")).is_err());
        assert!(decode_rc(&b64(&[0xff, 0xfe])).is_err());
    }
}
