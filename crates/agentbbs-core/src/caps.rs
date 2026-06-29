//! Capability-based authorization.
//!
//! AgentBBS does not grant power by identity ("is this user an admin?") so
//! much as by *capability* — a fine-grained bitset describing exactly what a
//! session may do. This keeps agent permissions least-privilege by default: a
//! freshly-minted anonymous agent can read and post, nothing more, until a
//! sysop or a plugin grants it more.

use serde::{Deserialize, Serialize};

bitflags::bitflags! {
    /// A set of capabilities held by a session.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Caps: u32 {
        /// Read public boards and messages.
        const READ            = 1 << 0;
        /// Post new messages to boards.
        const POST            = 1 << 1;
        /// Create new boards.
        const CREATE_BOARD    = 1 << 2;
        /// Edit or delete one's own messages.
        const EDIT_OWN        = 1 << 3;
        /// Moderate: edit/delete others' messages, lock boards.
        const MODERATE        = 1 << 4;
        /// Manage federation peers (link, unlink, trust).
        const FEDERATE        = 1 << 5;
        /// Install / invoke WASM plugins.
        const PLUGINS         = 1 << 6;
        /// Publish or purchase marketplace listings.
        const MARKETPLACE     = 1 << 7;
        /// Access sysop reporting + administration.
        const SYSOP           = 1 << 8;
        /// Use the MCP bridge to call out to external tools.
        const MCP_EGRESS      = 1 << 9;
    }
}

impl Default for Caps {
    fn default() -> Self {
        // Least privilege: a brand-new anonymous agent may read and post.
        Caps::READ | Caps::POST | Caps::EDIT_OWN
    }
}

// Serialize the capability set as its raw `u32` bit pattern for a stable,
// compact wire format (the bitflags macro does not give us this directly).
impl Serialize for Caps {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_u32(self.bits())
    }
}

impl<'de> Deserialize<'de> for Caps {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let bits = u32::deserialize(d)?;
        Ok(Caps::from_bits_truncate(bits))
    }
}

/// Conventional roles, each a named bundle of [`Caps`]. Roles are a
/// convenience for operators; the authorization check always looks at the
/// underlying capability bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Read-only visitor.
    Guest,
    /// Standard participant (the default).
    Agent,
    /// Trusted contributor with moderation powers.
    Moderator,
    /// Federation operator.
    Federator,
    /// Full system operator.
    Sysop,
}

impl Role {
    /// The capability bundle granted by this role.
    pub fn caps(self) -> Caps {
        match self {
            Role::Guest => Caps::READ,
            Role::Agent => Caps::default() | Caps::PLUGINS | Caps::MARKETPLACE | Caps::MCP_EGRESS,
            Role::Moderator => {
                Role::Agent.caps() | Caps::MODERATE | Caps::CREATE_BOARD
            }
            Role::Federator => Role::Moderator.caps() | Caps::FEDERATE,
            Role::Sysop => Caps::all(),
        }
    }
}

/// Require that `held` includes `needed`, returning a typed permission error
/// otherwise. `name` is used purely for the human-readable error.
pub fn require(held: Caps, needed: Caps, name: &'static str) -> crate::error::Result<()> {
    if held.contains(needed) {
        Ok(())
    } else {
        Err(crate::error::Error::PermissionDenied(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_least_privilege() {
        let d = Caps::default();
        assert!(d.contains(Caps::READ));
        assert!(d.contains(Caps::POST));
        assert!(!d.contains(Caps::MODERATE));
        assert!(!d.contains(Caps::SYSOP));
    }

    #[test]
    fn role_escalation_is_monotonic() {
        assert!(Role::Moderator.caps().contains(Role::Agent.caps()));
        assert!(Role::Federator.caps().contains(Role::Moderator.caps()));
        assert!(Role::Sysop.caps().contains(Role::Federator.caps()));
    }

    #[test]
    fn require_enforces() {
        assert!(require(Caps::READ, Caps::READ, "read").is_ok());
        assert!(require(Caps::READ, Caps::MODERATE, "moderate").is_err());
    }
}
