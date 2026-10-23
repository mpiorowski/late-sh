# The herald (`app/herald`) Context

## Metadata
- Domain: late.sh's social accounts. One picture a day, posted by the server through an ordinary account on each network it is on (Mastodon, Bluesky), so the things members make to be seen (posters, the Artboard) are seen outside the terminal.
- Status: Active, shipped dark. Neither account exists yet (prod pins no network), and the `herald_enabled` row starts off; the operator flips both (below).

## What it is

An arrivals lever built to the shape in `AUDIENCE.md` ("The herald"): post, do not federate. One account per network, a token each, a few HTTP calls a day. No session side: nothing in the TUI knows the herald exists, and nothing a member does triggers a post. The rotation reads what members already made public (a poster hung on a public work profile, the shared board) and never chat. One post is picked, worded and fetched once, then delivered to every configured network, so a change to what it says or shows changes it everywhere.

## Module map

| File | Role |
|---|---|
| `svc.rs` | `HeraldService`: the checker on every replica (`start_task`, `check`), the day's claim, the stored pick, the picture fetch, one delivery per network, the settle; every log line, metric and error type. |
| `state.rs` | Pure rules: `post_due_day` and `HERALD_POST_TIME` (16:00 UTC), `shown_day` and `board_key`, `ink_cells` against `BOARD_MIN_INK`, `Picture` (its route, page and subject; serialized onto the row), `pick_picture` (the rotation), `status_text` under `TEXT_LIMIT` and `alt_text`. Reads no clock, does no I/O. |
| `mastodon.rs` | The Mastodon leg: `deliver` is the media upload (200, or 202 polled until ready) then the status call with the day's key as `Idempotency-Key`. Returns the status URL; logs nothing. |
| `bluesky.rs` | The Bluesky leg: `deliver` is a session from the app password, the blob upload (`fit_under` shrinks the PNG under `BLOB_MAX_BYTES` with the `image` crate), then `putRecord` under `tid_for(key)` with `facets` for the link and the tags. Returns the bsky.app URL; logs nothing. |

## Data

- `herald_posts` (model `late-core/src/models/herald.rs`, migration 252): one row per UTC day, the claim. `id` is minted at the first claim and never changes: Mastodon sees it as the `Idempotency-Key`, Bluesky as the record key derived from it. `posting` is held while the calls run; `kind`, `subject` (the runner's user id, or the board's `daily:YYYY-MM-DD` key) and `picture` (the serialized `Picture`) are written at pick time, before the first call, so a retry delivers the same thing; `posted` means every configured network delivered; `skipped` is a day with nothing worth posting; `failed` keeps `attempts`. Claim semantics as `job_press_runs` and the paper's rows: a stale `posting` (older than `HERALD_STALE_CLAIM`, 20 minutes) is taken over, a `failed` row is claimed again under `HERALD_MAX_ATTEMPTS` (3), then the day rests.
- `herald_deliveries`: one row per network a day's picture landed on, written as each call succeeds; a retry sends only to the networks without one. A pick on a day that failed for good does not count as that poster posted: the candidate query looks at `posted` rows only.
- `app_flags.herald_enabled` (model `late-core/src/models/app_flag.rs`): the operator's switch, read at every check, seeded off by migration 252. The first row in that table with a reader; the other seeded keys (`haunt_*`, `paper_*`, `jobs_enabled`) have none.
- Reads of other domains, through model surfaces only: the poster candidate query in the herald model joins `deadchannel_runners` (`poster`, `left_at`), `work_profiles` (`slug`) and `users` (`username`, `last_seen`); the board comes from `Snapshot::find_by_board_key` and is decoded with `dartboard_core::Canvas` to count ink.

## The post

- **When.** Every replica checks every `HERALD_CHECK_INTERVAL` (5 minutes). The day's post is due at `HERALD_POST_TIME`, 16:00 UTC (late afternoon in Europe, noon on the US east coast); `post_due_day` names the day owed, so a replica that was down at the hour catches up and stamps the right day. A settled day is memoized per replica, so the rest of the day costs one flag read per check and nothing else.
- **What.** `pick_picture` over three inputs: what the last posted day carried, the next poster candidate (a standing runner with a poster and a public work profile, never posted before, the member seen most recently first), and yesterday's daily Artboard snapshot once it has `BOARD_MIN_INK` (200) drawn cells (the board is wiped on the first of the month). A poster goes out unless the last post was one, then the board; the board when no poster waits; the poster again when the board is a smudge; nothing when both are missing (the day is `skipped`, not retried).
- **Where the picture comes from.** late-web's PNG routes (`late-web/CONTEXT.md`): `/profiles/{slug}/poster.png` and `/gallery/{key}/image.png`, fetched once per attempt over `HeraldConfig::web_internal_url` (the cluster service in prod, the compose service in dev). One render a day on service-web's CPU.
- **The words.** One text for every network, under `TEXT_LIMIT` (300, Bluesky's limit; Mastodon allows 500): a line of plain speech, the page on late.sh, four tags (`#tildeverse #ssh` plus `#ASCIIart #MastoArt` for the board, `#ASCIIart #terminal` for a poster). The runner is credited as `~name`, never `@name`: a bare `@` is a mention of whoever has that name on the instance. The same alt text on every picture.
- **Mastodon.** `POST /api/v2/media` as multipart (`file`, `description`), 200 with the attachment or 202 polled on `GET /api/v1/media/{id}` until 200; then `POST /api/v1/statuses` with `status`, `media_ids`, `visibility` (`HeraldConfig::visibility`: public in prod, unlisted in dev), `language: en` and the `Idempotency-Key` header. The status's `url` is the delivery's.
- **Bluesky.** `com.atproto.server.createSession` with the handle and the app password, `com.atproto.repo.uploadBlob` with the PNG (shrunk by 20% a side per pass until under `BLOB_MAX_BYTES`, 950,000, the PDS's cap being a megabyte), then `com.atproto.repo.putRecord` of an `app.bsky.feed.post` with the text, `langs`, the image embed (alt, blob, aspect ratio) and facets (`#link` for the page, `#tag` per hashtag, by UTF-8 byte range). The record key is a TID built from the day's UUID v7 (its millisecond timestamp and ten bits of its randomness), so the same day puts the same record. The delivery's URL is `https://bsky.app/profile/{handle}/post/{rkey}`. Bluesky has no unlisted post: a dev post there is public.
- **Never.** Chat, a frame of #lounge, anything a member did not make to be seen. Two posts a day on one network.

## Config and switches

- `Config::herald` (`HeraldConfig` in `late-ssh/src/config.rs`): `mastodon: Option<MastodonConfig>` (instance, token), `bluesky: Option<BlueskyConfig>` (pds, identifier, app password), `web_internal_url`, `visibility`. A network configured is the herald on (`enabled()`); none is off. Prod pins neither network yet: going live on one is a profile change (pin the instance URL or the handle as a literal, read the secret with `required`, add its Terraform variable, Kubernetes secret, env on `service-ssh` and the two workflow lines like `LATE_YOUTUBE_API_KEY`) shipped as an `-infra` release before the ssh image that needs it (`infra/RUNBOOK.md`, deploy skew rule). Dev: each network on only with its pair in `.env.local` (`LATE_HERALD_MASTODON_INSTANCE` + `..._TOKEN`, `LATE_HERALD_BLUESKY_IDENTIFIER` + `..._PASSWORD`; half a pair refuses to start), `web_internal_url` the compose web service, Mastodon unlisted. `Config::validate` refuses a configured network with an empty credential or a non-HTTP URL.
- The row: `UPDATE app_flags SET enabled = true WHERE key = 'herald_enabled'` through `scripts/connect_db.sh`; `false` stops the next check, no deploy. The config is the deployment's switch; the row is the operator's.
- Account setup is by hand: Mastodon's bot flag and profile links in its web settings, the application with `write:media` and `write:statuses` under Development; Bluesky's app password under Settings, Privacy and security, and its `bot` self-label on the profile record (no setting in the app; any client that edits profile labels, or a one-off `putRecord` of `app.bsky.actor.profile` with `labels.values: [{val: "bot"}]`).

## Telemetry

- `late_ssh_herald_posts_total{result}`: `posted`, `skipped`, `lost` (another replica held the day; once per replica per day, then memoized), `failed`. `late_ssh_herald_deliveries_total{network,result}`: `delivered`, `failed` per leg. Grafana "Herald" row, `monitoring/dashboards/observability.json`.
- Span `herald.check` around every check. Error types `herald_flag_failed`, `herald_claim_failed`, `herald_delivery_failed` (per network, once, with the network), `herald_post_failed` (the day's summary); info lines `the herald delivered` (day, network, url), `the herald posted` (day, kind, subject, urls) and `the herald had nothing worth posting`; a warn when a failed day's row cannot be marked.

## Gotchas

- **Two idempotency mechanisms, one key.** Mastodon keeps an `Idempotency-Key` for about an hour; a status whose answer was lost is retried under the same key within 20 minutes (the stale bound) up to three times, inside the window. Bluesky's `putRecord` under the same TID is an upsert at any time. Either way a lost answer never double-posts; and a delivery that landed but could not be recorded is sent again under the same key, which both networks dedupe.
- **The picture is fetched per attempt, not per network**, and the pick is stored before the first call: a retry after a partial failure shows the same picture on the network that missed it.
- **Posters are posted sight unseen.** A fresh poster can go out before a moderator has looked at it. Posters are already public on the member's web profile, and `deadchannel remove-poster` takes one down there; the herald's copy on a network is removed by hand from the account. The table does not record when a poster was painted, so the herald cannot wait for one to age; each poster goes out at most once (`subject` is the runner's user id).
- **No session side.** No `/herald` command: the row and the logs are the operator's surface. Preview is reading `pick_picture` in a test, not a dry run.
- **The human half is operator work.** Answer replies, boost members, follow people (`AUDIENCE.md`). Fosstodon's rule for automated accounts is that an active member runs them; Bluesky's is that a bot never interacts unprompted, so boosting members there is done from a person's own account.

## Testing

- `late-core/src/models/herald_test.rs`: the claim (once per day, stale takeover, failed retry to the cap, the key surviving a retry), the stored pick and deliveries surviving a failure, the settle, the rotation reads, the poster candidate query (standing, public, unposted, a failed day's pick not spent, most recently seen first). `app_flag_test.rs`: the seeded switch flips.
- `state_test.rs`: the due day, the rotation's arms, the words (under `TEXT_LIMIT`, no bare `@`, the page linked, four tags, the picture round-tripping through JSON), the ink count. `bluesky_test.rs`: the TID's shape, determinism and order, a big picture shrunk under the cap, the facets' byte ranges.
- `svc_test.rs`: fake late-web, Mastodon and Bluesky endpoints on one local port. Three days end to end on both networks (poster, board, skipped), the row id as idempotency key and record key, the same text and alt on both, the picture fetched once, the switch off posting nothing, one network down leaving the day failed with the other's delivery kept and retried alone.
- Run: `make test-llm ARGS="-p late-core -p late-ssh --lib -E 'test(herald) | test(app_flag)'"`.
