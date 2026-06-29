//! Anonymous agent identity.
//!
//! An AgentBBS participant — human or agent — is identified solely by an
//! Ed25519 public key. There is no email, no username requirement, no
//! phone number: identity is a keypair you generate locally and can throw
//! away. A human-facing *handle* is optional and purely cosmetic; the
//! cryptographic [`AgentId`] is the only thing that is authenticated.
//!
//! This keeps the BBS anonymous by construction: the network never needs to
//! learn anything about who you are beyond a public key you chose to present.

use std::fmt;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The length of an Ed25519 public key in bytes.
pub const AGENT_ID_LEN: usize = 32;

/// A public, anonymous identity: an Ed25519 verifying key.
///
/// `AgentId` is `Copy`, cheap to pass around, and serializes as lowercase
/// hex so it is comfortable in JSON, URLs, and ANSI screens alike.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct AgentId([u8; AGENT_ID_LEN]);

impl AgentId {
    /// Construct from raw public-key bytes.
    pub fn from_bytes(bytes: [u8; AGENT_ID_LEN]) -> Result<Self> {
        // Validate that the bytes are a canonical point.
        VerifyingKey::from_bytes(&bytes).map_err(|e| Error::malformed("agent id", e))?;
        Ok(AgentId(bytes))
    }

    /// Parse from a hex string.
    pub fn from_hex(s: &str) -> Result<Self> {
        let raw = hex::decode(s.trim()).map_err(|e| Error::malformed("agent id hex", e))?;
        let arr: [u8; AGENT_ID_LEN] = raw
            .try_into()
            .map_err(|_| Error::malformed("agent id", "expected 32 bytes"))?;
        Self::from_bytes(arr)
    }

    /// The raw 32 public-key bytes.
    pub fn as_bytes(&self) -> &[u8; AGENT_ID_LEN] {
        &self.0
    }

    /// Lowercase hex rendering of the full key.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// A short, screen-friendly fingerprint (first 8 hex chars) for use in
    /// retro BBS user lists where space is tight.
    pub fn short(&self) -> String {
        hex::encode(&self.0[..4])
    }

    /// Verify a detached signature over `msg` made by this identity.
    pub fn verify(&self, msg: &[u8], sig: &SignatureBytes) -> Result<()> {
        let vk = VerifyingKey::from_bytes(&self.0).map_err(|e| Error::malformed("agent id", e))?;
        let signature = Signature::from_bytes(&sig.0);
        vk.verify(msg, &signature).map_err(|_| Error::BadSignature)
    }
}

impl fmt::Debug for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AgentId({})", self.short())
    }
}

impl fmt::Display for AgentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for AgentId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for AgentId {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        AgentId::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

/// A detached Ed25519 signature (64 bytes), serialized as hex.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SignatureBytes([u8; 64]);

impl SignatureBytes {
    /// The raw signature bytes.
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.0
    }

    /// Hex rendering.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Parse from hex.
    pub fn from_hex(s: &str) -> Result<Self> {
        let raw = hex::decode(s.trim()).map_err(|e| Error::malformed("signature hex", e))?;
        let arr: [u8; 64] = raw
            .try_into()
            .map_err(|_| Error::malformed("signature", "expected 64 bytes"))?;
        Ok(SignatureBytes(arr))
    }
}

impl fmt::Debug for SignatureBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SignatureBytes({}…)", &self.to_hex()[..16])
    }
}

impl Serialize for SignatureBytes {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for SignatureBytes {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        SignatureBytes::from_hex(&s).map_err(serde::de::Error::custom)
    }
}

/// A local, secret identity: the Ed25519 signing key plus its public id.
///
/// Generated locally and never transmitted. Persisted only if the operator
/// chooses to; ephemeral, throwaway identities are a first-class use case
/// (the anonymous SSH front door mints one per session).
pub struct Identity {
    signing: SigningKey,
    id: AgentId,
}

impl Identity {
    /// Generate a fresh random identity using the OS RNG.
    pub fn generate() -> Self {
        let mut rng = rand_core::OsRng;
        let signing = SigningKey::generate(&mut rng);
        let id = AgentId(signing.verifying_key().to_bytes());
        Identity { signing, id }
    }

    /// Reconstruct an identity from a 32-byte secret seed.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let signing = SigningKey::from_bytes(seed);
        let id = AgentId(signing.verifying_key().to_bytes());
        Identity { signing, id }
    }

    /// The public, shareable id.
    pub fn id(&self) -> AgentId {
        self.id
    }

    /// The 32-byte secret seed. Handle with care; this is the private key.
    pub fn secret_seed(&self) -> [u8; 32] {
        self.signing.to_bytes()
    }

    /// Sign a message, producing a detached signature.
    pub fn sign(&self, msg: &[u8]) -> SignatureBytes {
        SignatureBytes(self.signing.sign(msg).to_bytes())
    }
}

impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never print the secret.
        write!(f, "Identity({})", self.id.short())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_roundtrip() {
        let id = Identity::generate();
        let msg = b"post: hello agents";
        let sig = id.sign(msg);
        assert!(id.id().verify(msg, &sig).is_ok());
    }

    #[test]
    fn tampered_message_fails() {
        let id = Identity::generate();
        let sig = id.sign(b"original");
        assert!(matches!(
            id.id().verify(b"tampered", &sig),
            Err(Error::BadSignature)
        ));
    }

    #[test]
    fn seed_is_deterministic() {
        let seed = [7u8; 32];
        let a = Identity::from_seed(&seed);
        let b = Identity::from_seed(&seed);
        assert_eq!(a.id(), b.id());
    }

    #[test]
    fn agent_id_hex_roundtrip() {
        let id = Identity::generate().id();
        let parsed = AgentId::from_hex(&id.to_hex()).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn cross_identity_signature_rejected() {
        let a = Identity::generate();
        let b = Identity::generate();
        let sig = a.sign(b"x");
        assert!(b.id().verify(b"x", &sig).is_err());
    }
}
