# The herald (`app/herald`) Context

## Metadata
- Domain: late.sh's Fediverse account. One picture a day, posted by the server through an ordinary Mastodon account, so the things members make to be seen (posters, the Artboard) are seen outside the terminal.
- Status: Active, shipped dark. The `herald_enabled` row starts off; the operator flips it (below).

## What it is

An arrivals lever built to the shape in `AUDIENCE.md` ("The herald"): post, do not federate. One account on an existing instance, one bearer token, two HTTP calls a day. No session side: nothing in the TUI knows the herald exists, and nothing a member does triggers a post. The rotation reads what members already made public (a poster hung on a public work profile, the shared board) and never chat.

## Module map

| File | Role |
|---|---|
| `svc.rs` | `HeraldService`: the checker on every replica (`start_task`, `check`), the day's claim, the picture fetch, the media upload, the status call, the settle; every log line, metric and error type. |
| `state.rs` | Pure rules: `post_due_day` and `HERALD_POST_TIME` (16:00 UTC), `shown_day` and `board_key`, `ink_cells` against `BOARD_MIN_INK`, `Picture` (its route, page and subject), `pick_picture` (the rotation), `status_text` and `alt_text`. Reads no clock, does no I/O. |

## Data

- `herald_posts` (model `late-core/src/models/herald.rs`, migration 252): one row per UTC day, the claim. `id` is minted at the first claim and never changes: it is the `Idempotency-Key` on the status call. `posting` is held while the calls run; `posted` carries `kind`, `subject` (the runner's user id, or the board's `daily:YYYY-MM-DD` key) and `status_url`; `skipped` is a day with nothing worth posting; `failed` keeps `attempts`. Claim shape and semantics as `job_press_runs` and the paper's rows: a stale `posting` (older than `HERALD_STALE_CLAIM`, 20 minutes) is taken over, a `failed` row is claimed again under `HERALD_MAX_ATTEMPTS` (3), then the day rests.
- `app_flags.herald_enabled` (model `late-core/src/models/app_flag.rs`): the operator's switch, read at every check, seeded off by migration 252. The first row in that table with a reader; the other seeded keys (`haunt_*`, `paper_*`, `jobs_enabled`) have none.
- Reads of other domains, through model surfaces only: the poster candidate query in the herald model joins `deadchannel_runners` (`poster`, `left_at`), `work_profiles` (`slug`) and `users` (`username`, `last_seen`); the board comes from `Snapshot::find_by_board_key` and is decoded with `dartboard_core::Canvas` to count ink.

## The post

- **When.** Every replica checks every `HERALD_CHECK_INTERVAL` (5 minutes). The day's post is due at `HERALD_POST_TIME`, 16:00 UTC (late afternoon in Europe, noon on the US east coast); `post_due_day` names the day owed, so a replica that was down at the hour catches up and stamps the right day. A settled day is memoized per replica, so the rest of the day costs one flag read per check and nothing else.
- **What.** `pick_picture` over three inputs: what the last posted day carried, the next poster candidate (a standing runner with a poster and a public work profile, never posted before, the member seen most recently first), and yesterday's daily Artboard snapshot once it has `BOARD_MIN_INK` (200) drawn cells (the board is wiped on the first of the month). A poster goes out unless the last post was one, then the board; the board when no poster waits; the poster again when the board is a smudge; nothing when both are missing (the day is `skipped`, not retried).
- **Where the picture comes from.** late-web's PNG routes (`late-web/CONTEXT.md`): `/profiles/{slug}/poster.png` and `/gallery/{key}/image.png`, fetched over `HeraldConfig::web_internal_url` (the cluster service in prod, the compose service in dev). One render a day on service-web's CPU.
- **The calls.** `POST /api/v2/media` as multipart (`file`, `description` is the alt text), 200 with the attachment or 202 polled on `GET /api/v1/media/{id}` until 200; then `POST /api/v1/statuses` with `status`, `media_ids`, `visibility` (`HeraldConfig::visibility`: public in prod, unlisted in dev), `language: en` and the `Idempotency-Key` header. The status's `url` is what the row records.
- **The words.** One line of plain speech, the page on late.sh, four tags (`#tildeverse #ssh` plus `#ASCIIart #MastoArt` for the board, `#ASCIIart #terminal` for a poster). The runner is credited as `~name`, never `@name`: a bare `@` is a mention of whoever has that name on the instance. Alt text on every picture.
- **Never.** Chat, a frame of #lounge, anything a member did not make to be seen. Two posts a day.

## Config and switches

- `Config::herald` (`HeraldConfig` in `late-ssh/src/config.rs`). Prod: `enabled`, `instance` `https://fosstodon.org`, `token` from `LATE_HERALD_TOKEN` (required; `infra/service-ssh.tf` injects the `herald-credentials` secret, filled from the `HERALD_TOKEN` Terraform variable, from the GitHub secret of the same name), `web_internal_url` `http://service-web-sv`, `visibility` public. Dev: on only with both `LATE_HERALD_INSTANCE` and `LATE_HERALD_TOKEN` in `.env.local` (one without the other refuses to start), `web_internal_url` the compose web service, unlisted. `Config::validate` refuses an enabled herald without a token or an instance URL.
- The row: `UPDATE app_flags SET enabled = true WHERE key = 'herald_enabled'` through `scripts/connect_db.sh`; `false` stops the next check, no deploy. The config switch is the deployment's; the row is the operator's.
- Deploy skew (`infra/RUNBOOK.md`): the prod profile requires the token, so the `-infra` release that injects it ships before the ssh image that needs it, and the GitHub secret exists before any Terraform plan runs on the merged code.

## Telemetry

- `late_ssh_herald_posts_total{result}`: `posted`, `skipped`, `lost` (another replica held the day; once per replica per day, then memoized), `failed`. Grafana "Herald" row, `monitoring/dashboards/observability.json`.
- Span `herald.check` around every check. Error types `herald_flag_failed`, `herald_claim_failed`, `herald_post_failed`; info lines `the herald posted` (day, kind, subject, url) and `the herald had nothing worth posting`; a warn when a failed day's row cannot be marked.

## Gotchas

- **The idempotency window.** The instance keeps an `Idempotency-Key` for about an hour. A status that went out whose answer was lost is retried under the same key within 20 minutes (the stale bound) up to three times, inside the window, so a lost answer never double-posts. A failed day rests after the cap; the log line names it.
- **Posters are posted sight unseen.** A fresh poster can go out before a moderator has looked at it. Posters are already public on the member's web profile, and `deadchannel remove-poster` takes one down there; the herald's copy on the instance is removed by hand from the account. The table does not record when a poster was painted, so the herald cannot wait for one to age; each poster goes out at most once (`subject` is the runner's user id).
- **No session side.** No `/herald` command: the row and the logs are the operator's surface. Preview is reading `pick_picture` in a test, not a dry run.
- **The human half is operator work.** Answer replies, boost members, follow people (`AUDIENCE.md`). The instance's rule for automated accounts is that an active member runs them.

## Testing

- `late-core/src/models/herald_test.rs`: the claim (once per day, stale takeover, failed retry to the cap, the key surviving a retry), the settle, the rotation reads, the poster candidate query (standing, public, unposted, most recently seen first). `app_flag_test.rs`: the seeded switch flips.
- `state_test.rs`: the due day, the rotation's arms, the words (under 500 characters, no bare `@`, the page linked, four tags), the ink count.
- `svc_test.rs`: a fake instance and a fake web on a local port. Three days end to end (poster, board, skipped), the idempotency key equal to the row id, the bearer token and the multipart body, the switch off posting nothing, a failed upload retried under the same key.
- Run: `make test-llm ARGS="-p late-core -p late-ssh --lib -E 'test(herald) | test(app_flag)'"`.
