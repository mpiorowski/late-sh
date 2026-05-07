# Native API token auth and websocket routes

## What changed

This change adds a native-client API surface to `late-ssh` and a matching token model in `late-core`.

- Added a `native_tokens` table and `NativeToken` model for persistent bearer tokens with expiry.
- Added an in-memory `NativeChallengeStore` for short-lived nonces issued by `GET /api/native/challenge`.
- Added `late-ssh/src/native_api.rs` and mounted its routes under `/api/native/*` plus `/api/ws/native`.
- Wired the new module and shared state into the existing API server and application bootstrap.
- Added `deadpool-postgres` where needed for the new DB-backed native endpoints.

## Auth flow

The native auth flow is challenge-based:

1. A client requests a nonce from `GET /api/native/challenge`.
2. The client signs that nonce with its SSH key.
3. `POST /api/native/token` verifies the SSH signature and fingerprint.
4. If the fingerprint maps to a known user, the server issues a time-limited bearer token and stores it in `native_tokens`.

This keeps the native API aligned with the existing SSH identity model instead of introducing a separate password-based auth path.

## API surface

The new native API exposes endpoints for:

- current user identity
- room listing and room history
- online user presence
- now playing and voting status
- submitting votes
- bonsai state and watering
- websocket updates for chat, votes, and now-playing changes

## Why

The repo already had the core app state and websocket/event infrastructure, but it did not expose a dedicated native-client API with a reusable bearer-token auth flow. This change fills that gap so a native app can authenticate with an SSH-backed identity and consume the same real-time app data without going through the browser-oriented routes.

## Validation

- Ran `cargo check -p late-ssh`
