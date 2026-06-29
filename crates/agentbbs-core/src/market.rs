//! The marketplace — signed, verifiable listings for plugins, agents, boards,
//! and themes.
//!
//! A [`Listing`] is offered by an anonymous [`Identity`] and signed over its
//! canonical bytes, exactly like a board [`crate::board::Message`]. Anyone can
//! verify that a listing genuinely comes from the holder of its `seller` key
//! and has not been tampered with, so the catalogue can replicate across the
//! federation without a trusted broker. Settlement (chips/credits/off-chain)
//! is intentionally out of scope here: this layer establishes *authenticity*
//! and *provenance*, which is what a decentralized marketplace needs first.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::identity::{AgentId, Identity, SignatureBytes};

/// What is being offered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListingKind {
    /// A WASM plugin (board hook / command / agent tool).
    Plugin,
    /// A hosted or runnable agent.
    Agent,
    /// A board (conference) handed off or franchised.
    Board,
    /// A TUI theme / skin.
    Theme,
    /// A benchmark or arena challenge pack.
    Benchmark,
}

/// The pre-signature content of a marketplace listing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ListingBody {
    /// Stable seller-chosen sku/slug, unique per seller.
    pub sku: String,
    /// What kind of thing this is.
    pub kind: ListingKind,
    /// Display title.
    pub title: String,
    /// Description (plain text / markdown).
    pub description: String,
    /// Price in abstract credits (0 = free / open).
    pub price: u64,
    /// The seller's public id.
    pub seller: AgentId,
    /// Seller's cosmetic handle.
    pub handle: String,
    /// Content hash of the artifact being sold (e.g. BLAKE3 of the wasm),
    /// hex-encoded — binds the listing to a specific, verifiable artifact.
    pub artifact_hash: String,
    /// When the listing was created.
    pub created_at: DateTime<Utc>,
}

impl ListingBody {
    /// Deterministic canonical bytes for signing/verification.
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(256);
        out.extend_from_slice(b"agentbbs.listing.v1\n");
        out.extend_from_slice(self.sku.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(format!("{:?}", self.kind).as_bytes());
        out.push(b'\n');
        out.extend_from_slice(self.title.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(format!("{}:", self.description.len()).as_bytes());
        out.extend_from_slice(self.description.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(self.price.to_string().as_bytes());
        out.push(b'\n');
        out.extend_from_slice(self.seller.to_hex().as_bytes());
        out.push(b'\n');
        out.extend_from_slice(self.handle.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(self.artifact_hash.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(self.created_at.to_rfc3339().as_bytes());
        out
    }
}

/// A signed marketplace listing.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Listing {
    /// The signed content.
    pub body: ListingBody,
    /// Seller's signature over `body.signing_bytes()`.
    pub signature: SignatureBytes,
}

impl Listing {
    /// Sign a listing. The identity must match `body.seller`.
    pub fn sign(identity: &Identity, body: ListingBody) -> Result<Self> {
        if identity.id() != body.seller {
            return Err(Error::malformed(
                "listing",
                "signing identity is not the seller",
            ));
        }
        let signature = identity.sign(&body.signing_bytes());
        Ok(Listing { body, signature })
    }

    /// Verify the listing's signature and (optionally) that a supplied
    /// artifact matches `artifact_hash`.
    pub fn verify(&self) -> Result<()> {
        self.body
            .seller
            .verify(&self.body.signing_bytes(), &self.signature)
    }

    /// Verify that `artifact` bytes match the listing's bound `artifact_hash`.
    pub fn verify_artifact(&self, artifact: &[u8]) -> Result<()> {
        let got = blake3::hash(artifact).to_hex().to_string();
        if got == self.body.artifact_hash {
            Ok(())
        } else {
            Err(Error::malformed("artifact", "hash does not match listing"))
        }
    }
}

/// Compute the canonical artifact hash for bytes (BLAKE3 hex).
pub fn artifact_hash(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// A simple in-memory marketplace catalogue of verified listings, keyed by
/// `(seller, sku)`.
#[derive(Default)]
pub struct Market {
    listings: Vec<Listing>,
}

impl Market {
    /// Empty catalogue.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a listing after verifying its signature. Replaces an existing
    /// listing with the same `(seller, sku)`.
    pub fn publish(&mut self, listing: Listing) -> Result<()> {
        listing.verify()?;
        let key = (listing.body.seller, listing.body.sku.clone());
        if let Some(slot) = self
            .listings
            .iter_mut()
            .find(|l| (l.body.seller, l.body.sku.clone()) == key)
        {
            *slot = listing;
        } else {
            self.listings.push(listing);
        }
        Ok(())
    }

    /// All listings of a given kind.
    pub fn by_kind(&self, kind: ListingKind) -> Vec<&Listing> {
        self.listings.iter().filter(|l| l.body.kind == kind).collect()
    }

    /// Number of listings.
    pub fn len(&self) -> usize {
        self.listings.len()
    }

    /// Whether the catalogue is empty.
    pub fn is_empty(&self) -> bool {
        self.listings.is_empty()
    }

    /// All listings.
    pub fn all(&self) -> &[Listing] {
        &self.listings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(seller: AgentId, sku: &str, artifact: &[u8]) -> ListingBody {
        ListingBody {
            sku: sku.into(),
            kind: ListingKind::Plugin,
            title: "Uppercase Door".into(),
            description: "A tiny WASM plugin".into(),
            price: 0,
            seller,
            handle: "vendor".into(),
            artifact_hash: artifact_hash(artifact),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn sign_verify_and_artifact_binding() {
        let id = Identity::generate();
        let artifact = b"\0asm fake wasm bytes";
        let listing = Listing::sign(&id, body(id.id(), "echo", artifact)).unwrap();
        assert!(listing.verify().is_ok());
        assert!(listing.verify_artifact(artifact).is_ok());
        assert!(listing.verify_artifact(b"different").is_err());
    }

    #[test]
    fn tampered_price_detected() {
        let id = Identity::generate();
        let mut listing = Listing::sign(&id, body(id.id(), "echo", b"x")).unwrap();
        listing.body.price = 999;
        assert!(listing.verify().is_err());
    }

    #[test]
    fn seller_mismatch_rejected() {
        let signer = Identity::generate();
        let other = Identity::generate();
        assert!(Listing::sign(&signer, body(other.id(), "x", b"x")).is_err());
    }

    #[test]
    fn market_publish_and_replace() {
        let id = Identity::generate();
        let mut market = Market::new();
        market
            .publish(Listing::sign(&id, body(id.id(), "echo", b"v1")).unwrap())
            .unwrap();
        market
            .publish(Listing::sign(&id, body(id.id(), "echo", b"v2")).unwrap())
            .unwrap();
        assert_eq!(market.len(), 1); // replaced, same (seller, sku)
        assert_eq!(market.by_kind(ListingKind::Plugin).len(), 1);
        assert_eq!(market.by_kind(ListingKind::Agent).len(), 0);
    }
}
