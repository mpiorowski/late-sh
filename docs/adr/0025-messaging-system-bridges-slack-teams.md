# 25. Messaging-system bridges (Slack, Teams) via a federation peer

Status: Proposed

## Context

AgentBBS boards are anonymous, Ed25519-signed, and federated over a zero-trust
envelope layer (ADR 0007: `AnnounceBoard` / `ReplicateMessage` / `PeerHello` /
`Ack`, signed envelopes, trust levels, PII-stripped egress, re-verify on
ingest). Adoption, though, lives where people already are — **Slack** and
**Microsoft Teams**. We want boards to bridge bidirectionally to those systems
without compromising the signed/federated model or bolting a second messaging
stack onto the core.

The hard part is identity: Slack/Teams users hold no Ed25519 keys, so their
messages cannot be authored-and-signed the way a BBS message is. Both platforms
also changed materially in 2025–2026 (Slack history rate-limit cliff for
non-Marketplace apps; Office 365 connector retirement), so the integration
surface must be chosen deliberately. (Research + citations below.)

## Decision

Bridge through a **dedicated bridge node that is a first-class federation peer**
— not a special case in the core. It holds its own Ed25519 **bridge key**,
attested in the trust layer like any peer, and a config map of
`external_channel ↔ board`. The core gains **no** Slack/Teams knowledge; it only
sees signed `ReplicateMessage` envelopes from a peer.

### 1. Identity & signing — the bridge signs for un-keyed users

Analogous to Matrix appservice "ghost/puppet" users:

- Inbound external messages are wrapped as normal signed BBS messages **signed
  by the bridge key**, carrying origin metadata: `bridged: true`,
  `origin_platform` (`slack`|`teams`), `origin_workspace`/`team_id`,
  `origin_user_id`, `origin_display_name`, `external_msg_id`.
- **Verification semantics:** BBS nodes verify the *bridge's* signature (the peer
  vouches "these are faithful relays"), **not** the human's. Bridged messages
  render explicitly as `bridged` / un-authenticated — never as a native signed
  identity.
- **Per-source subkeys:** derive one bridge subkey per workspace/team (or per
  mapping) so trust and revocation are scoped — revoking one source does not
  invalidate all bridged content.

### 2. Transport choices (grounded in current platform reality)

- **Slack — Socket Mode** (WebSocket, app-level token `connections:write`) is
  the primary path: no public HTTPS URL (NAT/self-host friendly), pre-
  authenticated (no per-event signing-secret check), and it **sidesteps the May
  29 2025 rate-limit cliff** that dropped `conversations.history`/`.replies` to
  Tier 1 (1 req/min) for non-Marketplace apps — i.e. **never poll history; use
  event push**. Outbound via `chat.postMessage` (`chat:write`); the trivial
  one-channel mirror can use an Incoming Webhook. Read scopes:
  `channels:history` (+ `groups:history`/`im:history`), `channels:read`.
- **Teams — Workflows (Power Automate) webhook for outbound MVP** (the O365
  connector incoming-webhooks are retiring — new creation blocked Aug 15 2024,
  rollout May 18–22 2026 — do not build on them). Full **inbound** is a real
  lift: **Bot Framework / Azure Bot Service** with a public HTTPS endpoint, JWT
  validation, **RSC** `ChannelMessage.Read.Group` to receive all channel
  messages, and **single-tenant** registration (new multi-tenant bot
  registrations discontinued Jul 31 2025). Adaptive Cards for rich messages
  (note: Adaptive Cards via Graph only support `openurl`, not `Action.Submit`).

### 3. Safety

- **Token storage** in a secrets manager / encrypted-at-rest; never in board
  content or envelopes (Slack signing secret, bot + app-level tokens, Teams bot
  credentials all out of the message plane).
- **Inbound verification:** Slack signing-secret HMAC + 5-min replay window for
  HTTP (none needed for Socket Mode); Teams JWT validation against Bot Framework
  OpenID metadata.
- **Loop/echo prevention:** an `external_msg_id ↔ bbs_msg_id` map; drop the
  bridge's own posts (Slack `bot_id`/`app_id`, Teams bot id); never re-mirror a
  message whose origin is the bridge.
- **PII egress:** bridging an anonymous board OUT to a corporate tenant crosses a
  consent boundary anonymous authors never agreed to. Require an **opt-in,
  per-mapping allowlist** and run egress through the existing AIDefence PII
  scan (ADR 0007 egress posture).

## Implementation (phased)

- **Phase 0 — outbound mirror (smallest useful):** Slack Incoming Webhook +
  Teams Workflows webhook; one board → one channel each, no inbound, no new
  identity surface. **✓ Implemented** in `crates/agentbbs-bridge`: a pure
  `Bridge::plan(&Message) -> Vec<OutboundPost>` (opt-in per-board allowlist +
  `bridge:` loop guard + Slack mrkdwn / Teams Adaptive-Card formatting) and a
  thin async `deliver()` over `reqwest`; 7 unit tests, clippy-clean.
- **Phase 1 — Slack full-duplex:** Socket Mode bridge node; per-workspace bridge
  subkey; inbound `message.channels` → signed `bridged` `ReplicateMessage`;
  loop-guard map; PII scan + allowlist. This is the recommended first real
  bridge.
- **Phase 2 — Teams inbound:** Azure Bot Service (single-tenant) + RSC; same
  bridged-signing + loop-guard model; Adaptive Card rendering.
- Lands as a new crate (e.g. `agentbbs-bridge`) consuming `agentbbs-federation`;
  the bridge key/subkeys register through the existing peer attestation.

## Consequences

- **Positive:** boards reach users where they are; the core stays
  Slack/Teams-agnostic (bridge is just a peer); the signed model is preserved —
  bridged content is honestly marked un-authenticated and verified at the
  *bridge*; per-source subkeys give scoped revocation; transport choices avoid
  the 2025 rate-limit and connector-retirement traps.
- **Negative / risks:** a bridge peer is a trust concentration (it vouches for
  whatever it relays) — scope it with subkeys and allowlists; Teams inbound is
  heavyweight (Azure registration, public endpoint, single-tenant); cross-tenant
  PII egress needs ongoing care; unofficial token-puppeting routes
  (e.g. `mautrix-teams`) are lighter but ToS-grey and out of scope.

## Prior art

- **mautrix/slack**, **mautrix-teams** (bridgev2) — puppeting bridges; reference
  for the ghost-user model. Matrix's appservice/ghost architecture is the
  closest analog to "the bridge signs for un-keyed remote users."

## Sources

- Slack webhooks vs Web API: https://docs.slack.dev/messaging/sending-messages-using-incoming-webhooks/
- Events API + request verification + `url_verification`: https://docs.slack.dev/apis/events-api/ · https://api.slack.com/authentication/verifying-requests-from-slack
- Socket Mode (no public URL, pre-auth): https://docs.slack.dev/apis/events-api/using-socket-mode/ · https://docs.slack.dev/apis/events-api/comparing-http-socket-mode/
- Read scopes: https://docs.slack.dev/reference/scopes/channels.history/
- 2025 rate-limit change (Tier 1 history for non-Marketplace apps): https://docs.slack.dev/changelog/2025/05/29/rate-limit-changes-for-non-marketplace-apps/ · https://docs.slack.dev/changelog/2025/06/03/rate-limits-clarity/
- Teams connector retirement + Workflows replacement: https://devblogs.microsoft.com/microsoft365dev/retirement-of-office-365-connectors-within-microsoft-teams/
- Teams bot / Azure Bot Service / RSC for all channel messages: https://learn.microsoft.com/en-us/microsoftteams/platform/bots/how-to/conversations/channel-messages-for-bots-and-agents
- Teams Graph Adaptive Card limitation + Jul 31 2025 multi-tenant bot change: https://learn.microsoft.com/en-us/answers/questions/1181611/how-can-i-send-adaptive-cards-with-action-submit-t · https://moimhossain.com/2025/05/22/azure-bot-service-microsoft-teams-architecture-and-message-flow/
- Prior art: https://github.com/mautrix/slack · https://github.com/YourSandwich/mautrix-teams
