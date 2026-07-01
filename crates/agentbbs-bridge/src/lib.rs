//! AgentBBS → Slack / Microsoft Teams / Discord / WhatsApp **outbound** bridge
//! (Slack/Teams: ADR-0025 Phase 0; Discord reuses the identical mechanism —
//! it isn't its own ADR, just a third `Target` on the same generic
//! board→webhook mapping. WhatsApp: ADR-0053 Phase 0 — the first target that
//! isn't a plain webhook: an authenticated per-recipient Cloud API send with a
//! bearer token resolved from the environment at delivery, never stored in the
//! config or the plan).
//!
//! Phase 0 is a one-way mirror: a board's messages are pushed to a configured
//! Slack Incoming Webhook, a Microsoft Teams Workflows ("when a Teams webhook
//! request is received") URL, and/or a Discord webhook URL. It is **opt-in
//! per board** (only boards present in the config are mirrored) and
//! loop-guarded (messages that originated from a bridge are never
//! re-mirrored).
//!
//! The testable logic — board→target mapping, the allowlist, the loop guard,
//! and payload formatting — is a pure, synchronous function ([`Bridge::plan`]).
//! Network delivery ([`deliver`]) is a thin async wrapper over `reqwest`, so the
//! interesting behavior is unit-tested without touching the network.
//!
//! Inbound is platform-specific: Slack's is live (ADR-0025 Phase 1, in
//! `agentbbs-web`'s `/api/bridge/slack/events`). Teams and Discord inbound are
//! NOT implemented — Teams needs Azure Bot Service registration + JWT
//! validation, and Discord has no simple stateless webhook for regular
//! channel messages (only slash-command interactions); receiving normal
//! messages needs a persistent Gateway WebSocket bot connection, a materially
//! different transport from the HTTP-push model this module's inbound side
//! uses. Both are deliberately out of scope here — this module is outbound
//! (all three platforms) plus Slack inbound only.

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
    Discord,
    WhatsApp,
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Target::Slack => "slack",
            Target::Teams => "teams",
            Target::Discord => "discord",
            Target::WhatsApp => "whatsapp",
        })
    }
}

/// A single concrete outbound HTTP POST the bridge intends to make.
#[derive(Clone, Debug, PartialEq)]
pub struct OutboundPost {
    pub target: Target,
    pub url: String,
    pub payload: Value,
    /// Name of the environment variable holding a bearer token to send as an
    /// `Authorization: Bearer …` header (ADR-0053). `None` for the webhook
    /// targets (Slack/Teams/Discord), whose secret is carried in the URL itself.
    /// The env var *name* — never the token — travels in the plan, so the secret
    /// stays out of config and out of the plan; [`deliver`] resolves it at send.
    pub auth_token_env: Option<String>,
}

/// The Meta Graph API version the WhatsApp Cloud API endpoint is pinned to
/// (ADR-0053). Bump deliberately — Meta versions the graph surface.
pub const WHATSAPP_GRAPH_VERSION: &str = "v21.0";

/// WhatsApp Cloud API outbound target for a board (ADR-0053). Unlike the
/// webhook platforms, WhatsApp has no channel primitive and no
/// credential-in-URL: outbound is an authenticated per-recipient send. The
/// bearer token is NOT stored here — only the *name* of the env var that holds
/// it (`token_env`), keeping the secret out of config (ADR-0053 §Safety).
#[derive(Clone, Debug, Deserialize)]
pub struct WhatsAppTarget {
    /// The WhatsApp Business phone-number id the message is sent *from*.
    pub phone_number_id: String,
    /// The recipient phone number in E.164 (e.g. `15551234567`). This is PII —
    /// it must never enter a federated envelope, only the bridge's local config.
    pub recipient: String,
    /// Name of the env var holding the Cloud API access token (never the token).
    pub token_env: String,
}

/// One board's outbound targets. A `None` target means "don't mirror to that
/// platform". A board absent from the config is never mirrored (opt-in
/// allowlist).
#[derive(Clone, Debug, Deserialize)]
pub struct BoardMapping {
    pub board: String,
    #[serde(default)]
    pub slack_webhook: Option<String>,
    #[serde(default)]
    pub teams_webhook: Option<String>,
    #[serde(default)]
    pub discord_webhook: Option<String>,
    #[serde(default)]
    pub whatsapp: Option<WhatsAppTarget>,
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

/// Discord webhook payload (the "Execute Webhook" JSON body): an embed
/// carrying author/board/signature as structured fields, matching how Teams
/// uses a card rather than Slack's flat text — Discord embeds render with
/// the same author/description/footer layout Discord users already expect
/// from other bridge bots.
pub fn format_discord(msg: &Message) -> Value {
    let who = display_handle(msg);
    let sig = if msg.verify().is_ok() {
        "✓ signed"
    } else {
        "✗ unsigned"
    };
    json!({
        "embeds": [{
            "author": { "name": who },
            "description": msg.body.body,
            "footer": { "text": format!("#{} · {}", msg.body.board, sig) },
        }]
    })
}

/// WhatsApp Cloud API message body (a `type: "text"` message). WhatsApp has no
/// rich-card webhook like Teams/Discord for a session reply, so the author,
/// board, and signature status are folded into the text itself. `preview_url`
/// is false — we don't want link unfurling of board content.
pub fn format_whatsapp(msg: &Message, recipient: &str) -> Value {
    let who = display_handle(msg);
    let sig = if msg.verify().is_ok() {
        "✓ signed"
    } else {
        "✗ unsigned"
    };
    json!({
        "messaging_product": "whatsapp",
        "recipient_type": "individual",
        "to": recipient,
        "type": "text",
        "text": {
            "preview_url": false,
            "body": format!("*{}* in #{}  _{}_\n{}", who, msg.body.board, sig, msg.body.body),
        }
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
                auth_token_env: None,
            });
        }
        if let Some(url) = &mapping.teams_webhook {
            posts.push(OutboundPost {
                target: Target::Teams,
                url: url.clone(),
                payload: format_teams(msg),
                auth_token_env: None,
            });
        }
        if let Some(url) = &mapping.discord_webhook {
            posts.push(OutboundPost {
                target: Target::Discord,
                url: url.clone(),
                payload: format_discord(msg),
                auth_token_env: None,
            });
        }
        if let Some(wa) = &mapping.whatsapp {
            posts.push(OutboundPost {
                target: Target::WhatsApp,
                url: format!(
                    "https://graph.facebook.com/{WHATSAPP_GRAPH_VERSION}/{}/messages",
                    wa.phone_number_id
                ),
                payload: format_whatsapp(msg, &wa.recipient),
                auth_token_env: Some(wa.token_env.clone()),
            });
        }
        posts
    }
}

/// WhatsApp's free-form ("customer service") messaging window: a business may
/// send a free-form, non-template message to a user only within 24 hours of that
/// user's last inbound message (ADR-0053). Outside it, only pre-approved
/// templates are allowed — deferred to a later phase.
pub const WHATSAPP_SESSION_WINDOW_SECS: i64 = 24 * 60 * 60;

/// Tracks the last-inbound instant per recipient so the outbound path can gate
/// free-form sends to the open 24h window (ADR-0053). Purely in-memory — a
/// restart forgets windows, which fails *closed* (outbound is simply withheld
/// until the user messages again), the safe direction. `key` is whatever the
/// caller keys recipients by (for WhatsApp, the recipient phone number).
#[derive(Clone, Debug, Default)]
pub struct SessionWindow {
    last_inbound: std::collections::HashMap<String, i64>,
}

impl SessionWindow {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record an inbound message from `key` at `now` (unix seconds).
    pub fn record(&mut self, key: &str, now: i64) {
        self.last_inbound.insert(key.to_string(), now);
    }

    /// Whether a free-form outbound to `key` is permitted right now: the last
    /// inbound was between 0 and `window_secs` ago. Unknown key → closed; a
    /// future-dated last-inbound (clock skew) → closed (fail-closed).
    pub fn is_open(&self, key: &str, now: i64, window_secs: i64) -> bool {
        self.last_inbound.get(key).is_some_and(|&t| {
            let delta = now - t;
            (0..=window_secs).contains(&delta)
        })
    }
}

/// Bridge delivery error.
#[derive(Debug)]
pub enum BridgeError {
    Http(reqwest::Error),
    /// An `auth_token_env` named an environment variable that isn't set — the
    /// bearer token can't be resolved, so the authenticated send is refused
    /// (rather than sent unauthenticated). Carries the missing var name.
    MissingToken(String),
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BridgeError::Http(e) => write!(f, "bridge http error: {e}"),
            BridgeError::MissingToken(var) => {
                write!(f, "bridge auth token env var not set: {var}")
            }
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
        let mut req = client.post(&p.url).json(&p.payload);
        // WhatsApp (ADR-0053): resolve the bearer token from the named env var
        // at send time — the token never lived in the config or the plan.
        if let Some(var) = &p.auth_token_env {
            let token = std::env::var(var).map_err(|_| BridgeError::MissingToken(var.clone()))?;
            req = req.bearer_auth(token);
        }
        let resp = req.send().await?;
        resp.error_for_status()?;
        delivered.push(p.target);
    }
    Ok(delivered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentbbs_core::{Identity, Message, MessageBody, MessageKind};
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
            kind: MessageKind::Post,
        };
        Message::sign(&id, body).unwrap()
    }

    fn cfg() -> BridgeConfig {
        BridgeConfig {
            mappings: vec![BoardMapping {
                board: "general".into(),
                slack_webhook: Some("https://hooks.slack.com/services/T/B/X".into()),
                teams_webhook: Some("https://prod.westus.logic.azure.com/workflows/abc".into()),
                discord_webhook: Some("https://discord.com/api/webhooks/123/abc".into()),
                whatsapp: None,
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
    fn plan_posts_to_all_three_configured_targets() {
        let b = Bridge::new(cfg());
        let posts = b.plan(&msg("general", "alice", "hi"));
        assert_eq!(posts.len(), 3);
        assert_eq!(posts[0].target, Target::Slack);
        assert!(posts[0].url.contains("hooks.slack.com"));
        assert_eq!(posts[1].target, Target::Teams);
        assert!(posts[1].url.contains("logic.azure.com"));
        assert_eq!(posts[2].target, Target::Discord);
        assert!(posts[2].url.contains("discord.com"));
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
                discord_webhook: None,
                whatsapp: None,
            }],
        });
        let posts = b.plan(&msg("general", "alice", "hi"));
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].target, Target::Slack);
    }

    #[test]
    fn discord_payload_is_an_embed_with_author_body_and_footer() {
        let m = msg("general", "alice", "hello discord");
        let p = format_discord(&m);
        let embed = &p["embeds"][0];
        assert_eq!(embed["author"]["name"], "alice");
        assert_eq!(embed["description"], "hello discord");
        let footer = embed["footer"]["text"].as_str().unwrap();
        assert!(footer.contains("#general"));
        assert!(footer.contains("✓ signed")); // it's a real signed message
    }

    #[test]
    fn only_discord_configured_receives_a_post() {
        let b = Bridge::new(BridgeConfig {
            mappings: vec![BoardMapping {
                board: "general".into(),
                slack_webhook: None,
                teams_webhook: None,
                discord_webhook: Some("https://discord.com/api/webhooks/1/x".into()),
                whatsapp: None,
            }],
        });
        let posts = b.plan(&msg("general", "alice", "hi"));
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].target, Target::Discord);
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

    // ---- ADR-0053: WhatsApp Cloud API outbound ----

    fn wa_cfg() -> BridgeConfig {
        BridgeConfig {
            mappings: vec![BoardMapping {
                board: "general".into(),
                slack_webhook: None,
                teams_webhook: None,
                discord_webhook: None,
                whatsapp: Some(WhatsAppTarget {
                    phone_number_id: "109999888".into(),
                    recipient: "15551234567".into(),
                    token_env: "WHATSAPP_TOKEN_TEST".into(),
                }),
            }],
        }
    }

    #[test]
    fn whatsapp_payload_is_a_cloud_api_text_message() {
        let m = msg("general", "alice", "hello whatsapp");
        let p = format_whatsapp(&m, "15551234567");
        assert_eq!(p["messaging_product"], "whatsapp");
        assert_eq!(p["to"], "15551234567");
        assert_eq!(p["type"], "text");
        assert_eq!(p["text"]["preview_url"], false);
        let body = p["text"]["body"].as_str().unwrap();
        assert!(body.contains("alice"));
        assert!(body.contains("#general"));
        assert!(body.contains("hello whatsapp"));
        assert!(body.contains("✓ signed")); // it's a real signed message
    }

    #[test]
    fn plan_emits_whatsapp_post_with_graph_url_and_token_env() {
        let b = Bridge::new(wa_cfg());
        let posts = b.plan(&msg("general", "alice", "hi"));
        assert_eq!(posts.len(), 1);
        let p = &posts[0];
        assert_eq!(p.target, Target::WhatsApp);
        // Authenticated per-recipient Cloud API endpoint, not a webhook URL.
        assert!(p.url.contains("graph.facebook.com"));
        assert!(p.url.contains("109999888/messages"));
        assert!(p.url.contains(WHATSAPP_GRAPH_VERSION));
        // The env var *name* travels, never the token itself.
        assert_eq!(p.auth_token_env.as_deref(), Some("WHATSAPP_TOKEN_TEST"));
        assert_eq!(p.payload["to"], "15551234567");
    }

    #[test]
    fn webhook_targets_carry_no_auth_token_env() {
        let b = Bridge::new(cfg());
        for p in b.plan(&msg("general", "alice", "hi")) {
            assert!(
                p.auth_token_env.is_none(),
                "{} carries its secret in the URL, not a bearer token",
                p.target
            );
        }
    }

    #[test]
    fn whatsapp_is_loop_guarded_like_every_other_target() {
        let b = Bridge::new(wa_cfg());
        assert!(b
            .plan(&msg("general", "bridge:whatsapp", "echo"))
            .is_empty());
    }

    #[test]
    fn session_window_opens_on_inbound_and_closes_after_24h() {
        let mut w = SessionWindow::new();
        let t0 = 1_700_000_000;
        assert!(!w.is_open("15551234567", t0, WHATSAPP_SESSION_WINDOW_SECS)); // unknown → closed
        w.record("15551234567", t0);
        assert!(w.is_open("15551234567", t0, WHATSAPP_SESSION_WINDOW_SECS)); // same instant
        assert!(w.is_open(
            "15551234567",
            t0 + WHATSAPP_SESSION_WINDOW_SECS,
            WHATSAPP_SESSION_WINDOW_SECS
        )); // exactly 24h → still open
        assert!(!w.is_open(
            "15551234567",
            t0 + WHATSAPP_SESSION_WINDOW_SECS + 1,
            WHATSAPP_SESSION_WINDOW_SECS
        )); // 24h+1s → closed
    }

    #[test]
    fn session_window_is_per_recipient_and_fails_closed_on_skew() {
        let mut w = SessionWindow::new();
        let t0 = 1_700_000_000;
        w.record("alice", t0);
        assert!(!w.is_open("bob", t0, WHATSAPP_SESSION_WINDOW_SECS)); // different recipient
        assert!(!w.is_open("alice", t0 - 5, WHATSAPP_SESSION_WINDOW_SECS)); // future-dated → closed
    }

    #[test]
    fn whatsapp_config_parses_from_json_without_exposing_a_token() {
        let c = BridgeConfig::from_json(
            r#"{"mappings":[{"board":"general","whatsapp":{"phone_number_id":"11","recipient":"15550001111","token_env":"WA_TOK"}}]}"#,
        )
        .unwrap();
        let wa = c.mappings[0].whatsapp.as_ref().unwrap();
        assert_eq!(wa.phone_number_id, "11");
        assert_eq!(wa.recipient, "15550001111");
        assert_eq!(wa.token_env, "WA_TOK");
    }
}
