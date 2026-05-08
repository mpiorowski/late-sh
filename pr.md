# Native API hardening follow-up

## What changed

This PR hardens the native token + websocket path that shipped in the initial native API work.

- Store only SHA-256 token hashes in `native_tokens` (raw bearer tokens are never persisted).
- Add token metadata (`last_used_at`, `user_agent`, `created_ip`) and update `last_used_at` atomically on successful auth.
- Add migration `046_native_token_metadata.sql` to append metadata columns and invalidate pre-hash tokens (`TRUNCATE native_tokens`).
- Add periodic cleanup task in `late-ssh` startup to purge expired native tokens every hour.
- Add per-IP rate limiting for native challenge, token issuance, and websocket connect paths.
- Add one-time short-lived websocket tickets (`GET /api/native/ws-ticket`) and support ticket-first auth in `/api/ws/native`.
- Add native logout endpoint (`DELETE /api/native/logout`) to revoke current bearer token.
- Enforce room membership checks for native history reads and websocket room subscription changes.

## Hardening details

### Token storage and lifecycle

Native tokens are now write-only secrets from client perspective:

1. Server generates raw token and returns it once.
2. Server hashes token with SHA-256 and stores only hex digest.
3. Auth lookups hash incoming token before DB query.
4. Successful auth updates `last_used_at`.
5. Expired tokens are removed by scheduled purge job.

This reduces impact of DB leaks and improves auditability for active token usage.

### Abuse resistance

Native endpoints now use dedicated IP limiters:

- challenge issuance (`/api/native/challenge`)
- token minting (`/api/native/token`)
- websocket connect (`/api/ws/native`)

Client IP derivation reuses existing trusted-proxy logic so limits apply to real client IP when requests pass through approved proxies.

### WebSocket auth hardening

`/api/ws/native` now prefers ephemeral one-time tickets over long-lived bearer token query params:

1. Authenticated client requests ticket from `GET /api/native/ws-ticket`.
2. Server mints ticket valid for 30 seconds, single-use.
3. WS connect consumes ticket; replay fails.
4. Bearer token auth remains as fallback via `Authorization` header or query param for compatibility.

This limits token exposure in logs/URLs for clients that adopt ticket flow.

### Authorization fixes

- `GET /api/native/rooms/{room}/history` now returns `403` unless caller is room member.
- Native websocket `subscribe` now switches rooms only if caller is member of requested room.

These checks close cross-room data access gaps.

## Operational impact

- Existing rows in `native_tokens` are intentionally invalidated by migration (`TRUNCATE native_tokens`) because old records contain unhashed raw token values that cannot match new lookup semantics.
- Clients must re-authenticate and receive fresh tokens after deploy.

## Why

Initial native API implementation proved feature path. This follow-up focuses on production hardening: secret-at-rest protection, abuse throttling, revocation support, reduced token leakage during websocket auth, stronger room-level authorization, and better token observability.

## Validation

- Ran `cargo check -p late-ssh`.
