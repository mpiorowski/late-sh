//! AgentBBS → Slack / Microsoft Teams **outbound** bridge (ADR-0025, Phase 0).
//!
//! Phase 0 is a one-way mirror: a board's messages are pushed to a configured
//! Slack Incoming Webhook and/or a Microsoft Teams Workflows ("when a Teams
//! webhook request is received") URL. It is **opt-in per board** (only boards
//! present in the config are mirrored) and loop-guarded (messages that
//! originated from a bridge are never re-mirrored).
//!
//! The testable logic — board→target mapping, the allowlist, the loop guard,
//! and payload formatting — is a pure, synchronous function ([`Bridge::plan`]).
//! Network delivery ([`deliver`]) is a thin async wrapper over `reqwest`, so the
//! interesting behavior is unit-tested without touching the network.
//!
//! Inbound (Slack/Teams → AgentBBS) and the bridge-signing identity model are
//! Phase 1+ (see ADR-0025) and intentionally out of scope here.

use agentbbs_core::Message;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fmt;

pub mod inbound;
pub use inbound::{sign_inbound, BridgeIdentity, Inbound, SeenSet};

pub mod irc;

/// Which external system an outbound post targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Slack,
    Teams,
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Target::Slack => "slack",
            Target::Teams => "teams",
        })
    }
}

/// A single concrete outbound HTTP POST the bridge intends to make.
#[derive(Clone, Debug, PartialEq)]
pub struct OutboundPost {
    pub target: Target,
    pub url: String,
    pub payload: Value,
}

/// One board's outbound webhook targets. A `None` webhook means "don't mirror
/// to that platform". A board absent from the config is never mirrored
/// (opt-in allowlist).
#[derive(Clone, Debug, Deserialize)]
pub struct BoardMapping {
    pub board: String,
    #[serde(default)]
    pub slack_webhook: Option<String>,
    #[serde(default)]
    pub teams_webhook: Option<String>,
}

/// The bridge configuration: the set of board→webhook mappings.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct BridgeConfig {
    #[serde(default)]
    pub mappings: Vec<BoardMapping>,
}

impl BridgeConfig {
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
    fn mapping_for<'a>(&'a self, board: &str) -> Option<&'a BoardMapping> {
        self.mappings.iter().find(|m| m.board == board)
    }
}

/// A message is treated as bridge-originated (and never re-mirrored) when its
/// handle carries the reserved `bridge:` prefix. Phase 1 will replace this with
/// explicit origin metadata on the federation envelope.
pub fn is_bridged(msg: &Message) -> bool {
    msg.body.handle.starts_with("bridge:")
}

/// Slack Incoming Webhook payload (mrkdwn). Incoming Webhooks accept the same
/// message shape as `chat.postMessage`'s `text`/`blocks`.
pub fn format_slack(msg: &Message) -> Value {
    let who = display_handle(msg);
    let sig = if msg.verify().is_ok() {
        "✓ signed"
    } else {
        "✗ unsigned"
    };
    json!({
        "text": format!("*{}* in #{}  _{}_\n{}", who, msg.body.board, sig, msg.body.body),
    })
}

/// Microsoft Teams payload for the Workflows ("when a Teams webhook request is
/// received") trigger: a `message` carrying an Adaptive Card attachment.
pub fn format_teams(msg: &Message) -> Value {
    let who = display_handle(msg);
    let sig = if msg.verify().is_ok() {
        "✓ signed"
    } else {
        "✗ unsigned"
    };
    json!({
        "type": "message",
        "attachments": [{
            "contentType": "application/vnd.microsoft.card.adaptive",
            "content": {
                "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
                "type": "AdaptiveCard",
                "version": "1.4",
                "body": [
                    { "type": "TextBlock", "weight": "Bolder", "text": format!("{} in #{}", who, msg.body.board) },
                    { "type": "TextBlock", "isSubtle": true, "spacing": "None", "text": sig },
                    { "type": "TextBlock", "wrap": true, "text": msg.body.body },
                ]
            }
        }]
    })
}

fn display_handle(msg: &Message) -> String {
    if msg.body.handle.is_empty() {
        format!(
            "@{}",
            &msg.body.author.to_hex()[..8.min(msg.body.author.to_hex().len())]
        )
    } else {
        msg.body.handle.clone()
    }
}

/// The outbound bridge.
#[derive(Clone, Debug, Default)]
pub struct Bridge {
    pub config: BridgeConfig,
}

impl Bridge {
    pub fn new(config: BridgeConfig) -> Self {
        Self { config }
    }

    /// Pure planning: given a message, decide exactly which outbound POSTs to
    /// make. Honors the opt-in allowlist (unmapped board → none) and the loop
    /// guard (bridge-originated → none). No I/O.
    pub fn plan(&self, msg: &Message) -> Vec<OutboundPost> {
        if is_bridged(msg) {
            return Vec::new();
        }
        let Some(mapping) = self.config.mapping_for(&msg.body.board) else {
            return Vec::new();
        };
        let mut posts = Vec::new();
        if let Some(url) = &mapping.slack_webhook {
            posts.push(OutboundPost {
                target: Target::Slack,
                url: url.clone(),
                payload: format_slack(msg),
            });
        }
        if let Some(url) = &mapping.teams_webhook {
            posts.push(OutboundPost {
                target: Target::Teams,
                url: url.clone(),
                payload: format_teams(msg),
            });
        }
        posts
    }
}

/// Bridge delivery error.
#[derive(Debug)]
pub enum BridgeError {
    Http(reqwest::Error),
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BridgeError::Http(e) => write!(f, "bridge http error: {e}"),
        }
    }
}

impl std::error::Error for BridgeError {}

impl From<reqwest::Error> for BridgeError {
    fn from(e: reqwest::Error) -> Self {
        BridgeError::Http(e)
    }
}

/// Deliver a planned set of posts over HTTP. Thin wrapper over `reqwest` — the
/// interesting decisions already happened in [`Bridge::plan`]. Returns the
/// targets that were delivered successfully; the first transport error aborts.
pub async fn deliver(
    client: &reqwest::Client,
    posts: &[OutboundPost],
) -> Result<Vec<Target>, BridgeError> {
    let mut delivered = Vec::with_capacity(posts.len());
    for p in posts {
        let resp = client.post(&p.url).json(&p.payload).send().await?;
        resp.error_for_status()?;
        delivered.push(p.target);
    }
    Ok(delivered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentbbs_core::{Identity, Message, MessageBody};
    use chrono::Utc;

    fn msg(board: &str, handle: &str, body: &str) -> Message {
        let id = Identity::generate();
        let body = MessageBody {
            board: board.to_string(),
            parent: None,
            subject: String::new(),
            body: body.to_string(),
            author: id.id(),
            handle: handle.to_string(),
            created_at: Utc::now(),
        };
        Message::sign(&id, body).unwrap()
    }

    fn cfg() -> BridgeConfig {
        BridgeConfig {
            mappings: vec![BoardMapping {
                board: "general".into(),
                slack_webhook: Some("https://hooks.slack.com/services/T/B/X".into()),
                teams_webhook: Some("https://prod.westus.logic.azure.com/workflows/abc".into()),
            }],
        }
    }

    #[test]
    fn slack_payload_has_handle_board_and_body() {
        let m = msg("general", "alice", "hello world");
        let p = format_slack(&m);
        let text = p["text"].as_str().unwrap();
        assert!(text.contains("alice"));
        assert!(text.contains("#general"));
        assert!(text.contains("hello world"));
        assert!(text.contains("✓ signed")); // it's a real signed message
    }

    #[test]
    fn teams_payload_is_an_adaptive_card_attachment() {
        let m = msg("general", "alice", "hello teams");
        let p = format_teams(&m);
        assert_eq!(p["type"], "message");
        assert_eq!(
            p["attachments"][0]["contentType"],
            "application/vnd.microsoft.card.adaptive"
        );
        let card = &p["attachments"][0]["content"];
        assert_eq!(card["type"], "AdaptiveCard");
        let blocks = card["body"].as_array().unwrap();
        let joined: String = blocks
            .iter()
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join(" ");
        assert!(joined.contains("hello teams"));
        assert!(joined.contains("#general"));
    }

    #[test]
    fn plan_posts_to_both_configured_targets() {
        let b = Bridge::new(cfg());
        let posts = b.plan(&msg("general", "alice", "hi"));
        assert_eq!(posts.len(), 2);
        assert_eq!(posts[0].target, Target::Slack);
        assert!(posts[0].url.contains("hooks.slack.com"));
        assert_eq!(posts[1].target, Target::Teams);
        assert!(posts[1].url.contains("logic.azure.com"));
    }

    #[test]
    fn unmapped_board_is_not_mirrored() {
        let b = Bridge::new(cfg());
        assert!(b.plan(&msg("secret-board", "alice", "hi")).is_empty());
    }

    #[test]
    fn only_configured_platform_receives_a_post() {
        let b = Bridge::new(BridgeConfig {
            mappings: vec![BoardMapping {
                board: "general".into(),
                slack_webhook: Some("https://hooks.slack.com/services/x".into()),
                teams_webhook: None,
            }],
        });
        let posts = b.plan(&msg("general", "alice", "hi"));
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].target, Target::Slack);
    }

    #[test]
    fn bridge_originated_messages_are_not_re_mirrored() {
        let b = Bridge::new(cfg());
        assert!(b.plan(&msg("general", "bridge:slack", "echo")).is_empty());
    }

    #[test]
    fn config_parses_from_json() {
        let c = BridgeConfig::from_json(
            r#"{"mappings":[{"board":"general","slack_webhook":"https://x"}]}"#,
        )
        .unwrap();
        assert_eq!(c.mappings.len(), 1);
        assert_eq!(c.mappings[0].board, "general");
        assert!(c.mappings[0].teams_webhook.is_none());
    }
}
