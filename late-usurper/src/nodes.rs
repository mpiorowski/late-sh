use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

/// Allocator for the game's node numbers. Usurper's multinode model gives each
/// concurrent session a distinct node (its dropfile directory and `/N` flag);
/// two sessions on the same node would clobber each other's dropfile and
/// confuse the game's online tracking, so the numbers are leased here and
/// returned when the session's bridge ends.
#[derive(Clone)]
pub struct Nodes {
    inner: Arc<Mutex<BTreeSet<u16>>>,
    max: u16,
}

impl Nodes {
    pub fn new(max: u16) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BTreeSet::new())),
            max: max.max(1),
        }
    }

    /// Lease the lowest free node number, or `None` when the door is full.
    /// The lease frees itself on drop.
    pub fn acquire(&self) -> Option<NodeLease> {
        let mut used = self.inner.lock().expect("nodes mutex");
        let free = (1..=self.max).find(|n| !used.contains(n))?;
        used.insert(free);
        Some(NodeLease {
            number: free,
            pool: self.inner.clone(),
        })
    }
}

/// A leased node number; returns itself to the pool on drop. RAII, so a bridge
/// that ends on any path (clean exit, teardown, panic) frees its node.
pub struct NodeLease {
    number: u16,
    pool: Arc<Mutex<BTreeSet<u16>>>,
}

impl NodeLease {
    pub fn number(&self) -> u16 {
        self.number
    }
}

impl Drop for NodeLease {
    fn drop(&mut self) {
        self.pool.lock().expect("nodes mutex").remove(&self.number);
    }
}
