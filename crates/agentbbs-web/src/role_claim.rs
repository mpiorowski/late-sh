//! External-role → capability bridge (ADR-0054 Q2).
//!
//! AgentBBS's core authorization is capability-based (`Caps`/`Role`/`require`,
//! `agentbbs-core/src/caps.rs`), but the web server historically posted
//! everything at `Role::Agent.caps()` — there was no way for an
//! externally-authenticated host app (e.g. a talent marketplace with
//! `employer`/`admin` roles) to elevate or restrict a caller's capabilities.
//!
//! This module verifies a **signed role claim** the host app attaches to a
//! request, so caps can be resolved per-request from a *verified* external
//! claim. The scheme is HMAC-SHA256 over `"{role}:{exp}"` keyed by a shared
//! secret (`AGENTBBS_ROLE_CLAIM_SECRET`) — the same dependency-free primitive
//! the Slack/WhatsApp inbound bridges use, not a new JWT stack. The host app
//! maps its own roles (employer/admin/…) onto AgentBBS's canonical role names
//! and mints a short-lived signed claim; the server verifies and maps it to a
//! `Role`. A missing/expired/forged claim never elevates — the caller falls
//! back to the default `Role::Agent` (fail-closed).

use agentbbs_core::Role;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Map a canonical role name to a core [`Role`]. The host app is responsible for
/// mapping its own role vocabulary (e.g. `employer`→`moderator`) onto these.
pub fn role_from_str(s: &str) -> Option<Role> {
    match s {
        "guest" => Some(Role::Guest),
        "agent" => Some(Role::Agent),
        "moderator" => Some(Role::Moderator),
        "federator" => Some(Role::Federator),
        "sysop" => Some(Role::Sysop),
        _ => None,
    }
}

/// Verify a signed role claim and return the granted [`Role`], or `None` if the
/// claim is unusable (empty secret, expired, bad hex, wrong signature, or an
/// unknown role name). The signed message is `"{role}:{exp}"`; `exp` is a unix
/// timestamp and `now` is passed in (not read from the clock) for determinism.
pub fn verify_role_claim(
    secret: &str,
    role: &str,
    exp: i64,
    sig_hex: &str,
    now: i64,
) -> Option<Role> {
    if secret.is_empty() || now > exp {
        return None;
    }
    let expected = hex::decode(sig_hex).ok()?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(format!("{role}:{exp}").as_bytes());
    mac.verify_slice(&expected).ok()?;
    role_from_str(role)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentbbs_core::caps::Caps;

    fn sign(secret: &str, role: &str, exp: i64) -> String {
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{role}:{exp}").as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    #[test]
    fn accepts_a_valid_claim_and_maps_the_role() {
        let sig = sign("sekret", "moderator", 2000);
        let role = verify_role_claim("sekret", "moderator", 2000, &sig, 1000);
        assert_eq!(role, Some(Role::Moderator));
        assert!(role.unwrap().caps().contains(Caps::MODERATE));
    }

    #[test]
    fn rejects_expired_wrong_secret_and_tampered() {
        let sig = sign("sekret", "sysop", 2000);
        // expired: now > exp
        assert_eq!(verify_role_claim("sekret", "sysop", 2000, &sig, 2001), None);
        // wrong secret
        assert_eq!(verify_role_claim("other", "sysop", 2000, &sig, 1000), None);
        // tampered role (sig was for sysop, claim says agent)
        assert_eq!(verify_role_claim("sekret", "agent", 2000, &sig, 1000), None);
    }

    #[test]
    fn rejects_empty_secret_and_unknown_role() {
        let sig = sign("", "moderator", 2000);
        assert_eq!(verify_role_claim("", "moderator", 2000, &sig, 1000), None);
        let sig2 = sign("sekret", "wizard", 2000);
        assert_eq!(
            verify_role_claim("sekret", "wizard", 2000, &sig2, 1000),
            None
        );
    }

    #[test]
    fn role_names_map_to_expected_caps() {
        assert_eq!(role_from_str("guest").unwrap().caps(), Caps::READ);
        assert!(role_from_str("sysop").unwrap().caps().contains(Caps::SYSOP));
        assert!(role_from_str("nope").is_none());
    }
}
