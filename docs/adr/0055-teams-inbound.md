# 0055. Teams inbound — a Bot Framework messaging endpoint, the first bridge needing asymmetric JWT/JWKS validation

Status: Proposed

## Context

ADR-0025 shipped the Teams *outbound* mirror (Phase 0) as a single
unauthenticated `POST` to a pre-provisioned Teams **Workflows (Power Automate)**
URL — the same URL-only webhook shape as Slack and Discord outbound
(`crates/agentbbs-bridge/src/lib.rs`: `Target::Teams`, `format_teams`,
`Bridge::plan`). But it deferred Teams *inbound* to a "Phase 2 — Teams inbound"
that it flagged as "a real lift": Azure Bot Service, JWT validation,
**RSC** `ChannelMessage.Read.Group`, single-tenant registration. This ADR
**refines that deferred Phase 2 into an implementable design.**

Two inbound bridges have since shipped, and they share one shape this ADR must
mirror so Teams slots into the same model:

- A **pure, unit-tested verify+parse module** — `crates/agentbbs-web/src/slack_bridge.rs`
  (`verify_signature` HMAC v0 + 5-min replay; `parse_event` →
  `SlackEvent::{UrlVerification, Message, Ignored}`; `parse_channel_map`;
  `parse_seed_hex`) and `crates/agentbbs-web/src/whatsapp_bridge.rs`
  (`verify_signature` `X-Hub-Signature-256`; `verify_handshake` GET
  `hub.challenge`; `parse_messages`; `parse_number_map`). Pure functions take
  `now`/secrets as parameters, so verification is deterministically testable.
- A **route handler** on the existing `agentbbs-web` Cloud Run HTTPS service
  (Slack: `POST /api/bridge/slack/events`; WhatsApp: `GET`+`POST
  /api/bridge/whatsapp/events`), registered in the router, that verifies the
  request **before** acting, applies an opt-in `channel→board` allowlist (env,
  e.g. `AGENTBBS_SLACK_CHANNEL_MAP`), dedupes on the external id, and drops
  bot-authored events. It **soft-fails** (200 for signature-valid soft skips so
  the platform doesn't retry-storm); only bad/missing signatures are 401/403.
- The **platform-agnostic identity model** in
  `crates/agentbbs-bridge/src/inbound.rs`: `BridgeIdentity` (deterministic
  per-source Ed25519 subkeys via `blake3(domain‖root‖source)`), `sign_inbound`
  (external message → a `bridge:`-marked, verifying AgentBBS message authored by
  the source subkey), `SeenSet` (external-id loop guard). Reused **verbatim**;
  `platform: "teams"` slots in exactly like `"slack"`/`"whatsapp"`. Secrets are
  env-only, never logged.

**Teams inbound is the first bridge whose per-request auth is asymmetric with
remote key discovery**, not a shared-secret HMAC. That single fact — RS256 JWT
validated against the Bot Framework's rotating JWKS — is what makes it heavier
than Slack/WhatsApp, and it drives the phasing below.

## Decision

Add a **Teams inbound bridge** that extends the *same* model: bridge peer,
per-source subkeys, opt-in allowlist, `bridge:` loop guard, reusing
`agentbbs-bridge::inbound` wholesale. Teams users hold no Ed25519 keys, so
inbound is signed by a per-source bridge subkey (`platform: "teams"`,
`user_id` = `from.id`, `workspace` = team/conversation id, `external_msg_id` =
activity id) and rendered `bridged` / un-authenticated — identical semantics to
ADR-0025 §1 and the two shipped inbound bridges. A per-team subkey gives scoped
revocation.

Add `crates/agentbbs-web/src/teams_bridge.rs` (pure verify+parse) + a route
handler + a route test, an opt-in `AGENTBBS_TEAMS_CHANNEL_MAP`
(channel/conversation id → board) and `AGENTBBS_TEAMS_BRIDGE_SEED_HEX`.

### 1. Transport — a Bot Framework messaging endpoint

Inbound Teams messages arrive as **Bot Framework Activity** JSON, POSTed by the
Azure Bot Service to the bot's registered messaging endpoint. Reuse the existing
`agentbbs-web` public HTTPS Cloud Run service with a new handler:

- **`POST /api/bridge/teams/messages`** — the same pragmatic choice Slack inbound
  made (an HTTP webhook on `agentbbs-web`, not a second always-on process).

Unlike WhatsApp there is **no GET handshake** — Teams has no `hub.challenge` /
`url_verification` echo, so `teams_bridge.rs` has no `verify_handshake` variant.

### 2. Auth — JWT Bearer against Bot Framework OpenID metadata (the hard part)

Each inbound request carries `Authorization: Bearer <JWT>` issued by the Bot
Framework / Azure AD. Validation MUST check, fail-closed, all of:

- **signature** — RS256, against the **rotating public keys** published in the
  Bot Framework OpenID configuration (JWKS, fetched from the well-known OpenID
  config URL and cached; **keys rotate**);
- **issuer** — the Bot Framework token issuer;
- **audience** — the bot's **Microsoft App Id**;
- **expiry** — `exp`/`nbf`.

Unlike Slack/WhatsApp's symmetric HMAC (a dependency-free shared secret), this is
asymmetric RS256 with remote key discovery. It needs a JWT library (recommend the
`jsonwebtoken` crate) plus a small cached JWKS fetcher. **Split it the way the
other bridges kept `verify_signature` pure:** the *pure, testable* part — given a
supplied/allowed decoding key, validate signature + issuer + audience + expiry
with `now` injected (testable against a locally-generated RS256 keypair) — is
separable from the *I/O* part — fetch + cache + rotate JWKS. Phase B implements
the pure half; Phase C wires the fetcher.

The stakes: this endpoint is Internet-facing, and a forged Activity that passed a
weak validator would mint a **genuine bridge-signed board post** — exactly the
risk the Slack ADR calls out. JWT validation is non-optional and fail-closed.

### 3. Activity parsing

The pure `parse_activity(json) -> TeamsEvent` extracts from a Bot Framework
Activity: `type` (must be `"message"`), `text`, `from.id` + `from.name` (sender),
`conversation.id` and/or `channelData.channel.id` + `channelData.team.id` (the
allowlist key + origin), `id` (activity id → `SeenSet` loop guard). It drops the
bot's own echoes (`from.id == recipient.id`) and non-`message` activity types —
the parse-layer loop guard the Slack bridge uses. It returns
`TeamsEvent::{Message(TeamsMessage), Ignored}` (no handshake variant — Teams has
none).

### 4. Handler flow

`POST /api/bridge/teams/messages` validates the JWT (§2) **before** acting →
`parse_activity` → apply the opt-in `AGENTBBS_TEAMS_CHANNEL_MAP` allowlist (a
conversation/channel id absent from config is never mirrored) → dedupe on the
activity id via `SeenSet` → drop bot-authored activities → `sign_inbound` posts a
`bridged` message to the mapped board, reusing
`BridgeIdentity`/`sign_inbound`/`SeenSet` unchanged. Signature-valid soft skips
return 200 (no retry-storm); missing/invalid JWTs are 401.

### 5. Azure registration weight (adoption cost, not code)

Using Bot Framework at all requires an **Azure Bot Service** registration with a
Microsoft App Id + password, **single-tenant** (multi-tenant bot registrations
were discontinued Jul 31 2025), and — to receive *all* channel messages, not just
@mentions — **RSC (Resource-Specific Consent) `ChannelMessage.Read.Group`** in
the Teams app manifest. This is real adoption weight, parallel to how ADR-0053
flagged WhatsApp's Meta Business / WABA onboarding. It is Phase C.

### 6. Safety

- **Secrets:** bot App Id/password and any signing/JWKS config live in env / a
  secrets manager, never in board content, envelopes, or logs — ADR-0025 §Safety,
  ADR-0007 posture.
- **Loop/echo guard:** `SeenSet` on the activity id + drop bot-authored
  activities; never re-mirror a message whose origin is the bridge.
- **PII egress:** bridging an anonymous board OUT to a corporate tenant crosses a
  consent boundary anonymous authors never agreed to (ADR-0007 egress posture).
  Require the opt-in per-mapping allowlist + the AIDefence PII scan on ingest —
  the same posture ADR-0025 set for Teams.

## Consequences

- **Positive:** reuses the ADR-0025 peer / subkey / allowlist / loop-guard model
  and the `agentbbs-web` webhook pattern wholesale; `platform: "teams"` slots
  into the same identity model as `"slack"`/`"whatsapp"`; splitting pure JWT
  validation from JWKS I/O keeps the security-critical logic unit-tested exactly
  like `verify_signature` in the other two bridges; the `bridged` /
  un-authenticated marking is preserved (verify the bridge, not the human);
  per-team subkeys give scoped revocation.
- **Negative / risks:** JWT-against-JWKS is the **first asymmetric, remote-key-
  discovery verification in the codebase** — a new `jsonwebtoken` dependency, a
  cached JWKS fetcher, and key-rotation handling — materially heavier than the
  HMAC bridges. **Azure Bot Service single-tenant registration + RSC** is
  heavyweight adoption, parallel to WhatsApp's WABA onboarding. A **forged
  Activity on a misconfigured validator would mint a bridge-signed post**, so
  fail-closed JWT validation is load-bearing. Corporate-tenant PII egress needs
  the same care ADR-0025/0007 set.

## Implementation (phased — honest about the JWT/JWKS/Azure weight)

- **Phase A — design (this ADR).** No code. Refines ADR-0025's deferred "Phase 2
  — Teams inbound" into the shape above.
- **Phase B — pure verify/parse module + route (next fire).**
  `crates/agentbbs-web/src/teams_bridge.rs`: `parse_activity` (fully
  unit-testable) + the *pure* JWT-claims validation (signature against a supplied
  key + issuer/audience/expiry, `now` injected — testable with a locally-
  generated RS256 keypair), plus the `POST /api/bridge/teams/messages` route
  wired to `sign_inbound`/`SeenSet`/the `AGENTBBS_TEAMS_CHANNEL_MAP` allowlist +
  `AGENTBBS_TEAMS_BRIDGE_SEED_HEX`. Mirrors `slack_bridge.rs` /
  `whatsapp_bridge.rs`, with a route-integration test.
- **Phase C — production JWKS + Azure.** Cached JWKS fetch from the Bot Framework
  OpenID config, key-rotation handling, and the Azure Bot Service (single-tenant)
  + RSC `ChannelMessage.Read.Group` manifest registration docs. Larger; separate.

## Prior art

- ADR-0025 (Slack/Teams/Discord bridges) — the peer/subkey/allowlist/loop-guard
  model and the deferred "Phase 2 — Teams inbound" this ADR refines. ADR-0053
  (WhatsApp bridge) — the closest recent sibling and phased-honesty template.
  ADR-0031 (IRC front end) — the same `bridge:` re-signing on another transport.
  ADR-0007 — the zero-trust envelope layer + PII-egress posture the bridge honors.

## Sources

- Bot Framework / Azure Bot Service authentication (JWT, OpenID metadata, issuer/audience): https://learn.microsoft.com/en-us/azure/bot-service/rest-api/bot-framework-rest-connector-authentication
- Bot Framework Activity schema: https://learn.microsoft.com/en-us/azure/bot-service/rest-api/bot-framework-rest-connector-activities
- Receiving all channel messages via RSC `ChannelMessage.Read.Group`: https://learn.microsoft.com/en-us/microsoftteams/platform/bots/how-to/conversations/channel-messages-for-bots-and-agents
- Single-tenant bot change (multi-tenant registrations discontinued Jul 31 2025) + Azure Bot Service architecture / message flow: https://moimhossain.com/2025/05/22/azure-bot-service-microsoft-teams-architecture-and-message-flow/
