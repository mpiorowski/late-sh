# DCSS door: the published crawl files

The host serves crawl's `logfile`, `milestones` and morgue dumps read-only over
HTTP so the public DCSS tooling (dcss-stats.com, Sequell) can ingest late.sh
games. The door's shape and transport are in `CONTEXT.md`; the log pipe that
tails the same two files into the boards is in
`late-ssh/src/app/door/ingest/CONTEXT.md`.

## The surface

`late-dcss/src/publish.rs` is a read-only HTTP listener on `LATE_DCSS_PUBLISH_PORT` 2329, spawned from `main.rs` beside the SSH server. Public crawl servers expose their `logfile`, `milestones`, and `morgue/` dumps so the ecosystem tooling (dcss-stats.com, Sequell) can ingest them, and this is late.sh's copy of that surface: `GET /crawl/` documents the layout, `/crawl/logfile` and `/crawl/milestones` serve the same two files the stats session tails (with `Range` support, since those fetchers pull incrementally by byte offset), and `/crawl/morgue/...` walks the dump tree with nginx-autoindex-shaped listings (a directory URL missing its trailing slash 301s to the slashed form, since those listings link relatively; 301 rather than axum's 308 because the scrapers here are written against the public servers' nginx). Routes are served natively under `/crawl`, so the ingress rule (`infra/doors.tf`, a `Prefix` path on the apex host that out-ranks late-web's `/` by longest-match) needs no rewrite annotation. Read-only and public by design (playnames are arcade handles), so the whole security surface is path handling: morgue request paths are rebuilt from sanitized components (`..` unrepresentable, dotfiles refused) and re-checked against the morgue root after canonicalization, which is what stops a symlink from reaching the saves and rc files sitting beside the published ones. Their fetcher and our ingestion then read identical bytes, so each validates the other. Their resume contract is equally fixed and equally met: go-sequell sends `Range: bytes=<local size>-`, requires a `206`, and treats a `416` as caught-up, while dcss-stats shells out to `wget --continue` for the same exchange.

## The morgue layout

**A game's morgue must land in `morgue/<playname>/`, never flat.** The whole public surface hangs on it: dcss-stats and Sequell both build a dump's URL from the xlog line alone, as `<morgue>/<player>/morgue-<player>-<YYYYMMDD-HHMMSS>.txt` (dcss-stats `getMorgueUrl`, Sequell `MorgueFilename#simple_name`), with the stamp in UTC, which is exactly what the build's `TIME_FN=gmtime` produces. So a flat tree serves the logs correctly and 404s every single morgue link on both sites. The mechanism is `-morgue <dir>` (`CLO_MORGUE` → `SysEnv.morgue_dir`, re-applied by `fixup_options()` AFTER the rc is parsed, which is also what settles a pushed rc's `morgue_dir =` line: unlike `macro_dir`/`save_dir` (`DEPLOY.md`), that option is live on this build). Never pass it as `-extra-opt-last` and never drop it: crawl's default is the flat `$SAVEDIR/morgue`. `morgue.rs::migrate_flat` carries pre-`-morgue` dumps over at boot, before either listener binds, so no child is writing and no fetcher sees the tree half-moved; it can be deleted once prod has reported `moved=0` for a while.

## Tests

Host `publish_test.rs` covers the published surface end to end by driving the router directly (`ServiceExt::oneshot`) against a throwaway crawl tree: whole-file and `Range` fetches of the logs (including the caught-up 416), the morgue walk, and the refusals that matter: traversal, an escaping symlink, and neighbouring files like `<name>.rc` that are not routes at all. `sanitize_rel`/`parse_range`/`render_listing` are pure and tested directly.

## Registering with the ecosystem

The files are served and the shape is verified against both consumers' source, but nobody fetches them yet. Registering is that is a message to the dcss-stats maintainer (O4epegb, GitHub issues or their Discord) and a `sources.yml` PR to `crawl/sequell`. What each wants is a row, and both are free of collisions today (`LSH`, `lsh`):

- dcss-stats (`apps/api/prisma/seedData.ts` shape): `name: 'late.sh'`, `abbreviation: 'LSH'`, `url: 'https://late.sh'`, `baseUrl: 'https://late.sh/crawl'`, `morgueUrl: 'https://late.sh/crawl/morgue'`, `logfiles: [{ path: '/logfile', version: '0.34' }]`.
- Sequell: `name: lsh`, `base: https://late.sh/crawl`, `logs: ['{logfile,milestones}*']`, `morgues: [https://late.sh/crawl/morgue]`.

A path-prefixed base is normal (cue, cszo and cdi all carry one), the morgue layout is settled above, and neither cares that our `logfile` is unversioned: both read the version off each line's `v=` field (go-sequell's `XlogGameVersion` returns `"any"` for a name with no version in it, and dcss-stats' `Logfile.version` is only a label and a fetch-cadence hint). One wart to disclose rather than fix: dcss-stats lists `tmsg` among its **required** keys, so the games logged before the `DGL_EXTENDED_LOGFILES` image (the first 97 lines of the logfile) land in their `InvalidGame` table and never appear; every game since parses. Timing matters for the morgues, though: their morgue fetcher writes an empty local file on a 404 and the caller then treats "file exists" as already-fetched, so a flat tree at the moment they switch us on caches blanks that are never retried. Migrate first, register second.

**ttyrecs.** Neither registration needs them (`ttyrecUrl` is optional on both sides), but the host's PTY reader already sees every byte, so recording ttyrecs under `$HOME/.crawl/ttyrec/<playname>/` and publishing them beside the logs would fill that field. (In-app live spectating exists, `../spectate/CONTEXT.md`; the live-games feed on dcss-stats reads **webtiles**, which we do not run.)
