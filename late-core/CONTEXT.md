# late-core: models, migrations, shared infrastructure

`late-core` is the shared library under `late-ssh` and `late-web` (not
`late-cli`, which stands alone): the database layer, every SQL migration, the
models, and small shared pieces (telemetry init, rate limiting, the radio
catalogue, the tag vocabulary, the nonogram pack schema). A domain's own rules
for its tables live in that domain's `CONTEXT.md`; this file holds what every
model shares.

## 1. File map

```text
late-core/
  migrations/          NNN_name.sql, applied by Db::migrate at startup
  src/
    db.rs              pool (deadpool) + migrations; Db::get records checkout time
    model.rs           model! and user_scoped_model! macros
    models/            one module per table (or table family), with its _test.rs
    test_utils.rs      test_db(), create_test_user(), shared fixtures
    telemetry.rs       init_telemetry(), HTTP span middleware, TracedExt
    rate_limit.rs      sliding-window per-IP limiter
    radio.rs           station catalogue, RADIO_SLOTS (see app/audio/STATIONS.md)
    vocab.rs           the tag vocabulary (skills, langs)
    nonogram.rs        pack schema, clue derivation, daily selection
    icecast.rs audio.rs ascii.rs api_types.rs shutdown.rs
```

## 2. Rules every model follows

- **SQL lives here.** `late-ssh` and `late-web` call typed model methods; they
  do not embed SQL. All operations for a table live in its model module.
- **IDs are UUID v7** (`uuidv7()` default). `model!` hardcodes `id`, `created`
  and `updated`; never redeclare them under `@generated`, which is only for
  extra generated columns. Lists default to `ORDER BY created DESC`.
- **Owner scoping in the query.** User-owned reads and writes filter by
  `user_id` in SQL: `user_scoped_model!` and explicit `_by_user` variants. A
  missing scope is a security bug.
- **Cascades.** Foreign keys are `ON DELETE CASCADE`: deleting a user deletes
  their data.
- **Migrations are forward only.** Never edit an applied migration. A table's
  change notice belongs in an `AFTER INSERT OR UPDATE` trigger so no write path
  can forget it (chips do this); app-side `pg_notify` calls are debt, listed in
  `SCALE.md`, Small Debts.
- **Notify payloads carry ids, never state** (8000-byte cap, reorder and loss
  across a listener reconnect); the reader re-reads the committed row.

## 3. Shared tables

- **`users`.** `fingerprint` UNIQUE (the account's first key only); `is_admin`
  and `is_moderator`; `username` 1-32 chars, `^[A-Za-z0-9._-]+$`, no trailing
  dot, case-insensitive UNIQUE (`idx_users_username_lower`). `settings` JSONB
  holds per-account settings. `Profile` (`models/profile.rs`) is the typed
  projection and the list of keys: there is no `profiles` table.
  `Profile::update` merges with `settings || jsonb_build_object(...)` so
  concurrent writers keep each other's keys. Ignores and favorites are stored
  as ids (`ignored_user_ids`, `favorite_room_ids`), so renames keep them;
  `favorite_theme_ids` are opaque strings never validated against the theme
  list; `notify_kinds` is free-form text (new kinds need no migration).
  `langs` are folded to `vocab` tags on write and on read.
- **`user_ssh_keys`.** Many fingerprints to one user; every op in
  `models/user_ssh_key.rs`. `settings` JSONB holds this device's overrides
  (`late-ssh/CONTEXT.md`, Per-device settings). Account linking moves the losing
  account's keys to the kept account and deletes the loser; no data merges.
- **`account_link_codes`.** Short post-login codes with expiry and
  `consumed_at`, used only from Settings > Account.
- **`irc_tokens`.** One per user; `token_hash` is SHA-256 hex and unique. The
  plaintext exists only at mint time and is never stored or logged.
- **`user_chips` and `chip_ledger`.** `models/chips.rs` owns `ChipMove`, the
  closed roster of ledger reasons with each one's floor guard, source kind and
  `counts_as_earnings` flag. `UserChips::apply` is the single delta write path
  and takes a required `source_ref` (what the row paid for), and
  `UserChips::ensure` writes the 1000-chip starting balance as
  `ChipMove::InitialBalance` in the same statement that creates the row, so a
  ledger sums to its balance. The one exception is the admin `/grant` command
  (`UserChips::admin_grant`): no ledger row by decision. `chip_user_changed`
  comes from `user_chips` triggers. Top Chips ranks credits that count as
  earnings (no table bets, gifts, stipend, referral payouts or monthly prizes;
  no debit ever counts): `app/leaderboard/CONTEXT.md`.
- **`game_payout_claims`.** Reusable claim rows,
  `UNIQUE(user_id, game, payout_kind, period_kind, period_key)`.
  `GamePayout::grant_multi` writes several keys (an event row and a 30-day
  cooldown row) in one all-or-nothing transaction under an advisory lock; every
  row carries the full amount, so `chip_ledger` is the honest total. Old
  `period_kind = 'lifetime'` rows are invisible to the repeatable gates.
- **`chat_*`.** Room kinds, DM canonical ordering and visibility rules:
  `late-ssh/src/app/chat/CONTEXT.md`.

## 4. Tests

DB tests go through `test_utils::test_db()` and `create_test_user()`, never a
hardcoded connection string. A test that builds a service only to exercise
logic touching no DB path may hold an inert `Db::new(&DbConfig::default())`;
if the test would touch the DB at all, it uses `test_db()`. Model tests sit
beside the model (`models/user.rs` / `models/user_test.rs`).

## 5. Telemetry init

`telemetry::init_telemetry()` wires OTLP traces, logs and metrics and installs
the W3C trace-context propagator. `http_telemetry_middleware` emits server
spans (`http.route` when a route matched; an unmatched path still falls back
to the raw path, which mints junk span names under scanner traffic,
`SCALE.md`, Observability Gaps). `TracedExt::send_traced` names client spans
`{method} {scheme://host/path}`; query strings never reach a span
(`span_url`), so send API keys in headers.
