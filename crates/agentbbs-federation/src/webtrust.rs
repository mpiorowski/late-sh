//! Web-of-trust federation (ADR-0043).
//!
//! Transitive peer trust via **Ed25519-signed endorsements**: a node vouches for
//! another as a trustworthy peer, and trust flows from a caller-chosen root set
//! along endorsement edges, bounded by depth. Extends G5 peer discovery (a
//! discovered `Unknown` peer can be auto-promoted when it is reachable within
//! depth N of your trusted set) without any global authority.

use std::collections::{HashMap, VecDeque};

use agentbbs_core::{AgentId, Identity, Result, SignatureBytes};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A signed statement that `endorser` vouches for `subject` as a trusted peer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endorsement {
    /// Who is vouching.
    pub endorser: AgentId,
    /// The peer being vouched for.
    pub subject: AgentId,
    /// When the endorsement was made.
    pub created_at: DateTime<Utc>,
    /// Endorser's signature over the canonical bytes.
    pub signature: SignatureBytes,
}

impl Endorsement {
    fn signing_bytes(endorser: &AgentId, subject: &AgentId, created_at: &DateTime<Utc>) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"agentbbs.endorsement.v1\n");
        for p in [
            endorser.to_hex().as_bytes(),
            subject.to_hex().as_bytes(),
            created_at.to_rfc3339().as_bytes(),
        ] {
            out.extend_from_slice(format!("{}:", p.len()).as_bytes());
            out.extend_from_slice(p);
            out.push(b'\n');
        }
        out
    }

    /// Sign an endorsement under `endorser`.
    pub fn sign(endorser: &Identity, subject: AgentId, created_at: DateTime<Utc>) -> Self {
        let endorser_id = endorser.id();
        let bytes = Self::signing_bytes(&endorser_id, &subject, &created_at);
        let signature = endorser.sign(&bytes);
        Endorsement {
            endorser: endorser_id,
            subject,
            created_at,
            signature,
        }
    }

    /// Verify the endorser signature (forged/tampered → `BadSignature`).
    pub fn verify(&self) -> Result<()> {
        let bytes = Self::signing_bytes(&self.endorser, &self.subject, &self.created_at);
        self.endorser.verify(&bytes, &self.signature)
    }
}

/// A directed web of verified endorsements.
#[derive(Default, Debug)]
pub struct WebOfTrust {
    // endorser -> subjects they vouch for
    edges: HashMap<String, Vec<AgentId>>,
}

impl WebOfTrust {
    /// An empty web.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an endorsement after verifying its signature (forged → rejected).
    pub fn add(&mut self, e: Endorsement) -> Result<()> {
        e.verify()?;
        let list = self.edges.entry(e.endorser.to_hex()).or_default();
        if !list.contains(&e.subject) {
            list.push(e.subject);
        }
        Ok(())
    }

    /// BFS from `roots` along endorsement edges, returning each reachable node
    /// with its minimum trust depth (1 = directly endorsed by a root). Roots
    /// themselves are not included. `max_depth` bounds the walk (0 = nothing).
    pub fn trusted_from(&self, roots: &[AgentId], max_depth: u32) -> HashMap<String, u32> {
        let mut depth: HashMap<String, u32> = HashMap::new();
        let mut queue: VecDeque<(AgentId, u32)> = VecDeque::new();
        let root_set: std::collections::HashSet<String> =
            roots.iter().map(|r| r.to_hex()).collect();
        for r in roots {
            queue.push_back((*r, 0));
        }
        while let Some((node, d)) = queue.pop_front() {
            if d >= max_depth {
                continue;
            }
            if let Some(subjects) = self.edges.get(&node.to_hex()) {
                for s in subjects {
                    let hex = s.to_hex();
                    if root_set.contains(&hex) {
                        continue;
                    }
                    let nd = d + 1;
                    if depth.get(&hex).is_none_or(|&old| nd < old) {
                        depth.insert(hex, nd);
                        queue.push_back((*s, nd));
                    }
                }
            }
        }
        depth
    }

    /// Whether `node` is trusted from `roots` within `max_depth`.
    pub fn is_trusted(&self, node: &AgentId, roots: &[AgentId], max_depth: u32) -> bool {
        self.trusted_from(roots, max_depth)
            .contains_key(&node.to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-06-30T05:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn sign_verify_and_tamper() {
        let a = Identity::generate();
        let b = Identity::generate().id();
        let e = Endorsement::sign(&a, b, now());
        assert!(e.verify().is_ok());
        let mut forged = e.clone();
        forged.subject = Identity::generate().id();
        assert!(forged.verify().is_err());
    }

    #[test]
    fn transitive_trust_within_depth() {
        let a = Identity::generate();
        let b = Identity::generate();
        let c = Identity::generate();
        let mut wot = WebOfTrust::new();
        wot.add(Endorsement::sign(&a, b.id(), now())).unwrap(); // a → b
        wot.add(Endorsement::sign(&b, c.id(), now())).unwrap(); // b → c

        let roots = [a.id()];
        // depth 1: only b
        assert!(wot.is_trusted(&b.id(), &roots, 1));
        assert!(!wot.is_trusted(&c.id(), &roots, 1));
        // depth 2: b and c
        assert!(wot.is_trusted(&c.id(), &roots, 2));
        let d = wot.trusted_from(&roots, 2);
        assert_eq!(d.get(&b.id().to_hex()), Some(&1));
        assert_eq!(d.get(&c.id().to_hex()), Some(&2));
    }

    #[test]
    fn unrooted_nodes_are_untrusted() {
        let a = Identity::generate();
        let b = Identity::generate();
        let stranger = Identity::generate();
        let mut wot = WebOfTrust::new();
        wot.add(Endorsement::sign(&stranger, b.id(), now()))
            .unwrap(); // stranger → b
                       // From root {a}, nothing is reachable (a endorses nobody).
        assert!(!wot.is_trusted(&b.id(), &[a.id()], 5));
    }

    #[test]
    fn forged_endorsement_not_added() {
        let a = Identity::generate();
        let b = Identity::generate().id();
        let mut e = Endorsement::sign(&a, b, now());
        e.subject = Identity::generate().id(); // change after signing
        let mut wot = WebOfTrust::new();
        assert!(wot.add(e).is_err());
    }
}
