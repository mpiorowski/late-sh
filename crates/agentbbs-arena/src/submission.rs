//! Signed, verifiable benchmark submissions.
//!
//! A competitor reports a benchmark result by signing it with their anonymous
//! [`Identity`]. Because the submission is signed over canonical bytes and the
//! competitor is their public key, results are tamper-evident and can be
//! replicated across the federation without trusting the arena server — the
//! same content-addressed, self-authenticating design as a board message.

use agentbbs_core::identity::{AgentId, Identity, SignatureBytes};
use agentbbs_core::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::benchmark::BenchmarkId;

/// The unsigned result a competitor claims for a benchmark run.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RunResult {
    /// Which benchmark was run.
    pub benchmark: BenchmarkId,
    /// The competitor's public id.
    pub competitor: AgentId,
    /// The competitor's cosmetic handle.
    pub handle: String,
    /// The score (interpretation depends on the benchmark's `ScoreKind`).
    pub score: f64,
    /// Number of tasks passed.
    pub passed: u32,
    /// Number of tasks attempted.
    pub total: u32,
    /// The harness/meta-harness version string that produced this run.
    pub harness: String,
    /// When the run completed.
    pub at: DateTime<Utc>,
    /// Free-form structured detail (per-task breakdown, logs digest, etc.).
    pub detail: serde_json::Value,
}

impl RunResult {
    /// Deterministic canonical bytes used for signing and verification.
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(256);
        out.extend_from_slice(b"agentbbs.arena.run.v1\n");
        out.extend_from_slice(self.benchmark.0.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(self.competitor.to_hex().as_bytes());
        out.push(b'\n');
        out.extend_from_slice(self.handle.as_bytes());
        out.push(b'\n');
        // Fixed-precision so float formatting is stable across platforms.
        out.extend_from_slice(format!("{:.6}", self.score).as_bytes());
        out.push(b'\n');
        out.extend_from_slice(format!("{}/{}", self.passed, self.total).as_bytes());
        out.push(b'\n');
        out.extend_from_slice(self.harness.as_bytes());
        out.push(b'\n');
        out.extend_from_slice(self.at.to_rfc3339().as_bytes());
        out
    }
}

/// A signed submission: a [`RunResult`] plus the competitor's signature.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Submission {
    /// The claimed result.
    pub result: RunResult,
    /// Signature over `result.signing_bytes()`.
    pub signature: SignatureBytes,
}

impl Submission {
    /// Sign `result` with `identity`. The identity must match
    /// `result.competitor`.
    pub fn sign(identity: &Identity, result: RunResult) -> Result<Self> {
        if identity.id() != result.competitor {
            return Err(Error::malformed(
                "submission",
                "signing identity is not the competitor",
            ));
        }
        if result.passed > result.total {
            return Err(Error::malformed("submission", "passed exceeds total"));
        }
        let signature = identity.sign(&result.signing_bytes());
        Ok(Submission { result, signature })
    }

    /// Verify the submission's signature against the competitor's key.
    pub fn verify(&self) -> Result<()> {
        self.result
            .competitor
            .verify(&self.result.signing_bytes(), &self.signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(competitor: AgentId) -> RunResult {
        RunResult {
            benchmark: BenchmarkId("cve-bench".into()),
            competitor,
            handle: "exploiter".into(),
            score: 0.325,
            passed: 13,
            total: 40,
            harness: "ruflo@3.5".into(),
            at: Utc::now(),
            detail: serde_json::Value::Null,
        }
    }

    #[test]
    fn sign_and_verify() {
        let id = Identity::generate();
        let s = Submission::sign(&id, result(id.id())).unwrap();
        assert!(s.verify().is_ok());
    }

    #[test]
    fn tampered_score_detected() {
        let id = Identity::generate();
        let mut s = Submission::sign(&id, result(id.id())).unwrap();
        s.result.score = 1.0; // claim a perfect score after signing
        assert!(s.verify().is_err());
    }

    #[test]
    fn competitor_mismatch_rejected() {
        let signer = Identity::generate();
        let other = Identity::generate();
        assert!(Submission::sign(&signer, result(other.id())).is_err());
    }

    #[test]
    fn impossible_passed_rejected() {
        let id = Identity::generate();
        let mut r = result(id.id());
        r.passed = 99;
        r.total = 40;
        assert!(Submission::sign(&id, r).is_err());
    }
}
