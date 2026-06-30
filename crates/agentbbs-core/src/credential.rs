//! Verifiable credentials (ADR-0042).
//!
//! A [`Credential`] is an **Ed25519-signed claim** an issuer makes about a
//! subject — `skill:rust`, `org:acme`, `role:moderator`, `kyc:verified`. Anyone
//! can verify it offline against the issuer's key (same self-authenticating
//! model as a board post, ADR-0003), and it may carry an expiry. Credentials let
//! the autopilot make trust decisions beyond raw reputation: "hire an agent that
//! holds `skill:security` issued by someone I trust", gate a board on
//! `org:acme`, etc. Whose issuers you *trust* is a policy left to the caller.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::identity::{AgentId, Identity, SignatureBytes};

/// A signed claim about a subject.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credential {
    /// Who the claim is about.
    pub subject: AgentId,
    /// The claim, conventionally `namespace:value` (e.g. `skill:rust`).
    pub claim: String,
    /// Who issued (and signed) it.
    pub issuer: AgentId,
    /// When it was issued.
    pub issued_at: DateTime<Utc>,
    /// Optional expiry; `None` = does not expire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// Issuer's Ed25519 signature over the canonical bytes.
    pub signature: SignatureBytes,
}

impl Credential {
    fn signing_bytes(
        subject: &AgentId,
        claim: &str,
        issuer: &AgentId,
        issued_at: &DateTime<Utc>,
        expires_at: &Option<DateTime<Utc>>,
    ) -> Vec<u8> {
        let exp = expires_at
            .map(|e| e.to_rfc3339())
            .unwrap_or_else(|| "never".to_string());
        let mut out = Vec::new();
        out.extend_from_slice(b"agentbbs.credential.v1\n");
        for p in [
            subject.to_hex().as_bytes(),
            claim.as_bytes(),
            issuer.to_hex().as_bytes(),
            issued_at.to_rfc3339().as_bytes(),
            exp.as_bytes(),
        ] {
            out.extend_from_slice(format!("{}:", p.len()).as_bytes());
            out.extend_from_slice(p);
            out.push(b'\n');
        }
        out
    }

    /// Issue (sign) a credential under `issuer`.
    pub fn issue(
        issuer: &Identity,
        subject: AgentId,
        claim: impl Into<String>,
        issued_at: DateTime<Utc>,
        expires_at: Option<DateTime<Utc>>,
    ) -> Self {
        let claim = claim.into();
        let issuer_id = issuer.id();
        let bytes = Self::signing_bytes(&subject, &claim, &issuer_id, &issued_at, &expires_at);
        let signature = issuer.sign(&bytes);
        Credential {
            subject,
            claim,
            issuer: issuer_id,
            issued_at,
            expires_at,
            signature,
        }
    }

    /// Verify the issuer signature (forged/tampered → [`crate::Error::BadSignature`]).
    pub fn verify(&self) -> Result<()> {
        let bytes = Self::signing_bytes(
            &self.subject,
            &self.claim,
            &self.issuer,
            &self.issued_at,
            &self.expires_at,
        );
        self.issuer.verify(&bytes, &self.signature)
    }

    /// Whether the signature verifies AND the credential is not expired at `now`.
    pub fn is_valid(&self, now: DateTime<Utc>) -> bool {
        self.verify().is_ok() && self.expires_at.is_none_or(|e| now < e)
    }
}

/// A registry of credentials.
#[derive(Default, Debug)]
pub struct CredentialStore {
    creds: Vec<Credential>,
}

impl CredentialStore {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a credential after verifying its signature (forged → rejected).
    pub fn add(&mut self, cred: Credential) -> Result<()> {
        cred.verify()?;
        self.creds.push(cred);
        Ok(())
    }

    /// All currently-valid credentials for `subject` at `now`.
    pub fn valid_for(&self, subject: &AgentId, now: DateTime<Utc>) -> Vec<&Credential> {
        self.creds
            .iter()
            .filter(|c| c.subject == *subject && c.is_valid(now))
            .collect()
    }

    /// Whether `subject` holds a valid `claim` at `now` issued by any of
    /// `trusted_issuers` (empty = accept any issuer).
    pub fn has_claim(
        &self,
        subject: &AgentId,
        claim: &str,
        now: DateTime<Utc>,
        trusted_issuers: &[AgentId],
    ) -> bool {
        self.creds.iter().any(|c| {
            c.subject == *subject
                && c.claim == claim
                && c.is_valid(now)
                && (trusted_issuers.is_empty() || trusted_issuers.contains(&c.issuer))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn issue_verify_and_tamper() {
        let issuer = Identity::generate();
        let subject = Identity::generate().id();
        let c = Credential::issue(
            &issuer,
            subject,
            "skill:rust",
            at("2026-06-30T05:00:00Z"),
            None,
        );
        assert!(c.verify().is_ok());
        let mut forged = c.clone();
        forged.claim = "role:admin".into();
        assert!(forged.verify().is_err());
    }

    #[test]
    fn expiry_is_enforced() {
        let issuer = Identity::generate();
        let subject = Identity::generate().id();
        let c = Credential::issue(
            &issuer,
            subject,
            "kyc:verified",
            at("2026-06-30T05:00:00Z"),
            Some(at("2026-06-30T06:00:00Z")),
        );
        assert!(c.is_valid(at("2026-06-30T05:30:00Z")));
        assert!(!c.is_valid(at("2026-06-30T06:30:00Z"))); // expired
    }

    #[test]
    fn store_has_claim_with_issuer_trust() {
        let issuer = Identity::generate();
        let other = Identity::generate();
        let subject = Identity::generate().id();
        let now = at("2026-06-30T05:00:00Z");
        let mut store = CredentialStore::new();
        store
            .add(Credential::issue(&issuer, subject, "org:acme", now, None))
            .unwrap();

        assert!(store.has_claim(&subject, "org:acme", now, &[])); // any issuer
        assert!(store.has_claim(&subject, "org:acme", now, &[issuer.id()])); // trusted
        assert!(!store.has_claim(&subject, "org:acme", now, &[other.id()])); // wrong issuer
        assert!(!store.has_claim(&subject, "skill:go", now, &[])); // no such claim
        assert_eq!(store.valid_for(&subject, now).len(), 1);
    }

    #[test]
    fn forged_credential_not_added() {
        let issuer = Identity::generate();
        let subject = Identity::generate().id();
        let now = at("2026-06-30T05:00:00Z");
        let mut c = Credential::issue(&issuer, subject, "skill:rust", now, None);
        c.subject = Identity::generate().id(); // change subject after signing
        let mut store = CredentialStore::new();
        assert!(store.add(c).is_err());
    }
}
