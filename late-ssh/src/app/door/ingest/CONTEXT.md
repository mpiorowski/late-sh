# Door log pipe context (`late-ssh/src/app/door/ingest`)

The three external roguelike doors (DCSS, NetHack, Brogue) feed their boards,
badges, chips, and feed lines from **host-written log files, never from the
terminal**. This slice tails those files and lands `door_runs` /
`door_milestones` rows (models `late-core/src/models/{door_run,door_milestone,door_log_cursor}.rs`,
migration `136_create_door_ingestion.sql`). Migrations 136 and 140 name a
deleted plan (`devdocs/PLAN-ROGUELIKE-BOARDS.md`) in their header comments;
this file is where they lead.

Related: the boards these rows fill are in `../../leaderboard/CONTEXT.md`
(`DoorGame`), the badge rules shared with every other milestone in
`../../leaderboard/AWARDS.md`. Per-game log formats, build flags, and host
internals stay in each door's own `CONTEXT.md` (`../dcss`, `../nethack`,
`../brogue`).

## Files

- `svc.rs`: orchestration, one connect-with-retry task per enabled door, spawned from `main.rs`.
- `stream.rs`: the stats SSH client.
- `dcss.rs`, `nethack.rs`, `brogue.rs`: pure line parsers, each verified against its pinned upstream source.
- `award.rs`: the shared `DoorAwards`/`DoorBadge` chip and badge sink.

## The contract



- **Transport: a stats SSH session on the door host.** Each host reserves one
  SSH username, `late_stats` (inside the already-reserved `late_*` handle
  namespace, so no player can claim it). Instead of a game child it opens a log
  stream: the client pushes its per-file byte offsets in env requests
  (`LATE_DOOR_STATS_CURSORS`, `logfile:123,milestones:456`; a large cursor set,
  e.g. Brogue's one-file-per-player history, is split across several requests
  the host concatenates), the host streams
  one `<file-id>\t<next-offset>\t<line>` frame per complete line with tail -f
  semantics, and stays **stateless**: no cursor storage, no parsing, no DB. All
  parsing lives in late-ssh, so a parser fix never needs a door redeploy, and
  door pods never hold DB credentials. Chosen over a new HTTP ingest surface
  because it reuses the russh servers and shared secrets already there.
- **Client side.** `app/door/ingest/`: `svc.rs` orchestration (one
  connect-with-retry task per enabled door, spawned from `main.rs` behind that
  door's `LATE_*_ENABLED`), `stream.rs` the stats SSH client, `dcss.rs` /
  `nethack.rs` / `brogue.rs` pure parsers, `award.rs` the shared
  `DoorAwards`/`DoorBadge` sink. Cursors persist in `door_log_cursors`.
  Observability: `late_ssh_door_ingest_lines_total` and
  `late_ssh_door_ingest_session_failures_total` (both labeled by game), so a
  dead stats session or a poisoned frame is visible in monitoring, not just a
  30s-retry warn log.
- **Idempotency.** Unique `(game, source_file, source_offset)` on both fact
  tables, with the fact insert and the cursor advance committing in one
  transaction. Files are append-only and hosts single-replica, so offsets are
  stable; a fresh cursor starts at 0 and ingests whatever history is already on
  the PVC, so a door's boards start non-empty. A file that shrinks
  (playground rebuilt) restarts from 0 and the idempotent inserts absorb the
  replay.
- **Identity.** The playname on every line is the account's **arcade handle**
  (NetHack `-u`, DCSS `-name`, Brogue's player directory name), mapped through
  `arcade_handles` (unique on `lower(handle)`). Handle rows outlive accounts
  with `user_id` NULL and those are skipped, as are the reserved `late`/`late_*`
  shapes (NetHack's legacy `late_<hex>` lines predate handles).
- **Grants are idempotent; the badge is once, the chips repeat.** Badges and
  chips fire from `award.rs` on every win or pickup. The badge is guarded by a
  `NOT EXISTS` award insert and lands once per account for life. The chips go
  through `credit_run_cooldown_reward_template`, an all-or-nothing grant behind
  two gates at once: the ingested line's own
  `(source_file, source_offset)` key, so a re-ingest or a crash between fact
  insert and grant settles to exactly one payout, and a 30-day per-account
  lockout per milestone (migration 208), so a lucky month pays once. Both gates refusing writes
  nothing at all. That is what makes backfill safe.
- **Feed events are gated twice.** Deaths and wins post to #lounge only when the
  fact row is freshly inserted AND the event is inside a 10-minute recency
  window, so a backfill of years of history never floods the feed. "Started a
  game" is connect-based in the client.
- **Never build boards or badges on a screen scrape.** Anything that pays
  chips or ranks a player reads the spoof-proof host files. Non-scoring
  games are excluded at the source, per door: wizard/explore xlogfile lines are
  flagged and skipped (and explore mode is locked off at the sysconf, since
  livelog lines carry no flag), crawl never logs wizard games, Brogue writes no
  run-history line for Easy or Wizard.
- **Deploy order matters.** The host half ships in that door's image-only
  release (`-dcss` / `-nethack` / `-brogue`), the client half with service-ssh:
  deploy the host first or together, or ingestion just retries against a host
  with no stats session. Manifest changes (a new initContainer file, a port, an
  ingress) ride `deploy_infra.yml` instead.
- **The DCSS files are also published outward.** The same `logfile`/`milestones`
  the pipe tails are served read-only over HTTP at `late.sh/crawl/...` for the
  public DCSS tooling (dcss-stats, Sequell), so their fetcher and this pipe read
  identical bytes and validate each other. Details in `../dcss/PUBLISH.md`;
  nothing in the pipe depends on it.

## Settled decisions (do not re-litigate)

- **Badge pairs, 20k/40k chips, one payout per run and at most one per
  30 days per milestone** (migrations 158 and 208), mirroring the original
  NetHack pair. DCSS's Orb *pickup* was chosen over first rune
  deliberately: it is the exact twin of the Amulet badge. **Every line grants
  only its own milestone.** A DCSS or NetHack win never back-grants the pickup
  it implies: a back-grant would carry the win line's own key, so once the
  pickup's 30-day window had passed it would pay the pickup a second time;
  and a pickup the milestone stream missed is an ingest bug to surface,
  not to patch from the win line. Brogue's Escaped/Mastered are alternative
  endings and grant only themselves for the same reason. The chat-label
  collapse is a display convention and implies nothing about granting.
- **Boards per door are uniform**: Wins (all-time), Deepest dive and Top score
  (monthly + all-time), joining the Games rail group. Adding a fourth door costs
  zero extra queries.
- **Backfilled historical wins grant** badges and chips (by decision);
  the idempotence above is what makes that safe.
- **Brogue variants do not count.** Rapid and Bullet Brogue write their own
  files beside the standard one and the host never opens them.
- **Badge codes** `DCO`/`DCW` and `BRE`/`BRM`.

## Tests

`{dcss,nethack,brogue}_test.rs` (pure parsers against real captured lines,
including unknown fields, dead and reserved handles, and truncated last lines),
`stream_test.rs` (the stats client against a stub SSH host), `svc_test.rs`
(DB-backed: replay idempotency, skipped names, the per-run payout and its
30-day lockout, a run past the lockout paying again with no second badge,
cursor advancement). Run them scoped:
`make test-llm ARGS="-p late-ssh --lib -E 'test(ingest)'"`.
