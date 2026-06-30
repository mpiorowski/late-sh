//! Marketplace credits + installs (ADR-0026 G7).
//!
//! A tiny per-agent credit ledger backing the marketplace: agents hold credits,
//! `purchase` debits a listing's price and records the install, and an install is
//! idempotent (you are not charged twice for something you already own). This is
//! the accounting primitive; signing/federation of purchases is out of scope.

use std::collections::BTreeMap;

use crate::error::{Error, Result};
use crate::identity::AgentId;

/// Per-agent credit balances and installed SKUs.
#[derive(Default, Debug)]
pub struct Wallet {
    credits: BTreeMap<String, u64>,
    installed: BTreeMap<String, Vec<String>>,
}

impl Wallet {
    /// An empty wallet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add `amount` credits to `agent`.
    pub fn credit(&mut self, agent: &AgentId, amount: u64) {
        *self.credits.entry(agent.to_hex()).or_insert(0) += amount;
    }

    /// `agent`'s current credit balance.
    pub fn balance(&self, agent: &AgentId) -> u64 {
        self.credits.get(&agent.to_hex()).copied().unwrap_or(0)
    }

    /// Whether `agent` has installed `sku`.
    pub fn has(&self, agent: &AgentId, sku: &str) -> bool {
        self.installed
            .get(&agent.to_hex())
            .is_some_and(|v| v.iter().any(|s| s == sku))
    }

    /// Purchase + install `sku` for `agent` at `price`. Idempotent — already
    /// owning `sku` is a no-op success (no double charge). Errors if the agent
    /// can't afford it.
    pub fn purchase(&mut self, agent: &AgentId, sku: &str, price: u64) -> Result<()> {
        if self.has(agent, sku) {
            return Ok(());
        }
        let hex = agent.to_hex();
        let bal = self.credits.get(&hex).copied().unwrap_or(0);
        if bal < price {
            return Err(Error::malformed("wallet", "insufficient credits"));
        }
        self.credits.insert(hex.clone(), bal - price);
        self.installed.entry(hex).or_default().push(sku.to_string());
        Ok(())
    }

    /// SKUs `agent` has installed.
    pub fn installed(&self, agent: &AgentId) -> Vec<String> {
        self.installed
            .get(&agent.to_hex())
            .cloned()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::Identity;

    #[test]
    fn credit_and_balance() {
        let a = Identity::generate().id();
        let mut w = Wallet::new();
        assert_eq!(w.balance(&a), 0);
        w.credit(&a, 100);
        w.credit(&a, 25);
        assert_eq!(w.balance(&a), 125);
    }

    #[test]
    fn purchase_debits_and_installs() {
        let a = Identity::generate().id();
        let mut w = Wallet::new();
        w.credit(&a, 50);
        w.purchase(&a, "theme.nord", 25).unwrap();
        assert_eq!(w.balance(&a), 25);
        assert!(w.has(&a, "theme.nord"));
        assert_eq!(w.installed(&a), vec!["theme.nord".to_string()]);
    }

    #[test]
    fn purchase_is_idempotent_no_double_charge() {
        let a = Identity::generate().id();
        let mut w = Wallet::new();
        w.credit(&a, 50);
        w.purchase(&a, "plugin.echo", 30).unwrap();
        w.purchase(&a, "plugin.echo", 30).unwrap(); // already owned → free
        assert_eq!(w.balance(&a), 20);
        assert_eq!(w.installed(&a).len(), 1);
    }

    #[test]
    fn insufficient_credits_rejected() {
        let a = Identity::generate().id();
        let mut w = Wallet::new();
        w.credit(&a, 10);
        assert!(w.purchase(&a, "agent.graybeard", 25).is_err());
        assert_eq!(w.balance(&a), 10); // unchanged
        assert!(!w.has(&a, "agent.graybeard"));
    }
}
