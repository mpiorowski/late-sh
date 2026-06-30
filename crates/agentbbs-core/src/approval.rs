//! Human-in-the-loop approval gates (ADR-0038).
//!
//! For a business autopilot, an agent may *propose* a side-effectful action
//! (spend, send, publish, deploy) but must not perform it until a human signs
//! off. The sign-off is itself an **Ed25519-signed message** — tamper-evident
//! and attributable, exactly like a board post (ADR-0003). An
//! [`ApprovalGate`] authorizes an action only when a verified
//! [`Verdict::Approve`] from an allowed decider exists and no allowed decider
//! has vetoed it.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::identity::{AgentId, Identity, SignatureBytes};

/// A human's decision on a proposed action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Authorize the action.
    Approve,
    /// Veto the action.
    Reject,
}

/// Canonical, deterministic signing bytes (versioned, length-prefixed so no
/// field can be smuggled across a boundary).
fn compose(parts: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"agentbbs.approval.v1\n");
    for p in parts {
        out.extend_from_slice(format!("{}:", p.len()).as_bytes());
        out.extend_from_slice(p);
        out.push(b'\n');
    }
    out
}

/// An agent's proposal to take a side-effectful action. The `action_id` is
/// content-addressed (BLAKE3 over the proposal), so a decision binds to exactly
/// this action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionProposal {
    /// Content-addressed id of the proposed action.
    pub action_id: String,
    /// Action kind (e.g. `spend`, `send_email`, `publish`, `deploy`).
    pub kind: String,
    /// Human-readable description of exactly what will happen.
    pub summary: String,
    /// The proposing agent.
    pub proposer: AgentId,
    /// Board the proposal is raised on.
    pub board: String,
    /// When it was proposed.
    pub created_at: DateTime<Utc>,
}

impl ActionProposal {
    /// Build a proposal, computing its content-addressed `action_id`.
    pub fn new(
        kind: impl Into<String>,
        summary: impl Into<String>,
        proposer: AgentId,
        board: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        let kind = kind.into();
        let summary = summary.into();
        let board = board.into();
        let bytes = compose(&[
            kind.as_bytes(),
            summary.as_bytes(),
            proposer.to_hex().as_bytes(),
            board.as_bytes(),
            created_at.to_rfc3339().as_bytes(),
        ]);
        let action_id = blake3::hash(&bytes).to_hex().to_string();
        ActionProposal {
            action_id,
            kind,
            summary,
            proposer,
            board,
            created_at,
        }
    }
}

/// A signed human decision on an action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedDecision {
    /// The action being decided (an [`ActionProposal::action_id`]).
    pub action_id: String,
    /// Approve or reject.
    pub verdict: Verdict,
    /// Optional human rationale.
    pub reason: String,
    /// The deciding identity (the human's key).
    pub decider: AgentId,
    /// When the decision was made.
    pub created_at: DateTime<Utc>,
    /// Ed25519 signature over the canonical decision bytes.
    pub signature: SignatureBytes,
}

impl SignedDecision {
    fn signing_bytes(
        action_id: &str,
        verdict: Verdict,
        reason: &str,
        decider: &AgentId,
        created_at: &DateTime<Utc>,
    ) -> Vec<u8> {
        let v = match verdict {
            Verdict::Approve => b"approve".as_slice(),
            Verdict::Reject => b"reject".as_slice(),
        };
        compose(&[
            action_id.as_bytes(),
            v,
            reason.as_bytes(),
            decider.to_hex().as_bytes(),
            created_at.to_rfc3339().as_bytes(),
        ])
    }

    /// Sign a decision under `identity` (the decider).
    pub fn sign(
        identity: &Identity,
        action_id: impl Into<String>,
        verdict: Verdict,
        reason: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        let action_id = action_id.into();
        let reason = reason.into();
        let decider = identity.id();
        let bytes = Self::signing_bytes(&action_id, verdict, &reason, &decider, &created_at);
        let signature = identity.sign(&bytes);
        SignedDecision {
            action_id,
            verdict,
            reason,
            decider,
            created_at,
            signature,
        }
    }

    /// Verify the signature against the decider's key. Returns
    /// [`Error::BadSignature`] if forged or tampered.
    pub fn verify(&self) -> Result<()> {
        let bytes = Self::signing_bytes(
            &self.action_id,
            self.verdict,
            &self.reason,
            &self.decider,
            &self.created_at,
        );
        self.decider.verify(&bytes, &self.signature)
    }
}

/// Tracks signed decisions and answers "is this action authorized?".
#[derive(Default, Debug)]
pub struct ApprovalGate {
    decisions: Vec<SignedDecision>,
}

impl ApprovalGate {
    /// An empty gate.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a decision after verifying its signature. Forged/tampered
    /// decisions are rejected and never stored.
    pub fn record(&mut self, decision: SignedDecision) -> Result<()> {
        decision.verify()?;
        self.decisions.push(decision);
        Ok(())
    }

    /// Whether `action_id` is authorized: at least one verified `Approve` from an
    /// `allowed` decider, and no `Reject` from an allowed decider (a veto wins —
    /// fail-closed). An empty `allowed` set authorizes nothing.
    pub fn is_authorized(&self, action_id: &str, allowed: &[AgentId]) -> bool {
        let allowed_hex: std::collections::HashSet<String> =
            allowed.iter().map(|a| a.to_hex()).collect();
        let relevant = self
            .decisions
            .iter()
            .filter(|d| d.action_id == action_id && allowed_hex.contains(&d.decider.to_hex()));
        let mut approved = false;
        for d in relevant {
            match d.verdict {
                Verdict::Reject => return false, // veto, fail-closed
                Verdict::Approve => approved = true,
            }
        }
        approved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-06-30T04:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn proposal_is_content_addressed() {
        let a = Identity::generate();
        let p1 = ActionProposal::new("spend", "buy 1 GPU-hr", a.id(), "ops", now());
        let p2 = ActionProposal::new("spend", "buy 1 GPU-hr", a.id(), "ops", now());
        let p3 = ActionProposal::new("spend", "buy 100 GPU-hr", a.id(), "ops", now());
        assert_eq!(p1.action_id, p2.action_id); // deterministic
        assert_ne!(p1.action_id, p3.action_id); // content-bound
    }

    #[test]
    fn decision_signs_and_verifies_and_detects_tamper() {
        let human = Identity::generate();
        let d = SignedDecision::sign(&human, "act-1", Verdict::Approve, "looks fine", now());
        assert!(d.verify().is_ok());
        // tamper: flip the verdict, signature no longer matches.
        let mut forged = d.clone();
        forged.verdict = Verdict::Reject;
        assert!(matches!(forged.verify(), Err(crate::Error::BadSignature)));
        // forged decider (different key) is rejected.
        let mut impersonated = d.clone();
        impersonated.decider = Identity::generate().id();
        assert!(impersonated.verify().is_err());
    }

    #[test]
    fn gate_authorizes_only_on_verified_allowed_approve() {
        let human = Identity::generate();
        let other = Identity::generate();
        let agent = Identity::generate();
        let p = ActionProposal::new("publish", "post release notes", agent.id(), "blog", now());
        let mut gate = ApprovalGate::new();

        // No decisions → not authorized.
        assert!(!gate.is_authorized(&p.action_id, &[human.id()]));

        // Approve by an allowed human → authorized.
        gate.record(SignedDecision::sign(
            &human,
            &p.action_id,
            Verdict::Approve,
            "ok",
            now(),
        ))
        .unwrap();
        assert!(gate.is_authorized(&p.action_id, &[human.id()]));

        // …but not if that human isn't in the allowed set.
        assert!(!gate.is_authorized(&p.action_id, &[other.id()]));

        // A veto from an allowed decider wins (fail-closed).
        gate.record(SignedDecision::sign(
            &other,
            &p.action_id,
            Verdict::Reject,
            "no",
            now(),
        ))
        .unwrap();
        assert!(!gate.is_authorized(&p.action_id, &[human.id(), other.id()]));
    }

    #[test]
    fn gate_refuses_to_record_forged_decision() {
        let human = Identity::generate();
        let mut d = SignedDecision::sign(&human, "act-9", Verdict::Approve, "", now());
        d.action_id = "act-tampered".into(); // change content after signing
        let mut gate = ApprovalGate::new();
        assert!(gate.record(d).is_err());
        assert!(!gate.is_authorized("act-tampered", &[human.id()]));
    }
}
