# DCSS Door Context

## Metadata
- Scope: the Dungeon Crawl Stone Soup door as a whole — the **client** in `late-ssh/src/app/door/dcss` (proxy/identity/state/render/mod) plus its screen lifecycle wiring in `late-ssh/src/app` (state/input/render/tick) **and the standalone host crate `late-dcss/`**. There is no separate `late-dcss/CONTEXT.md`; this file is the single source for both halves.
- Domain: Dungeon Crawl Stone Soup (crawl), the real upstream console roguelike (GPL-2.0-or-later), run on a PTY inside a **dedicated `late-dcss` SSH host** and reached by late-ssh as a network-proxied door (the same model as the NetHack door).
- Primary audience: LLM agents changing the DCSS launcher UI, the SSH client transport, the host crate (PTY bridge / auth / TERM handling), input forwarding, or its config/deploy wiring.
- Status: Active
- Parent context: `../CONTEXT.md` (the door family)
- Stability note: `[STABLE]` sections change rarely; `[VOLATILE]` sections change with the launcher UI, keybindings, or build/deploy wiring.

---

## 0. Context Maintenance Protocol [STABLE]

Read this after `../CONTEXT.md` whenever a task touches the DCSS launcher, launch/leave behavior, the SSH client transport, the `late-dcss` host (PTY bridge, auth, TERM resolution), input forwarding/filtering, the F1→`?` help remap, or DCSS config/deploy wiring.

- Keep this file aligned with the SSH transport contract, the client/host split, the spawn args, config knobs, and known gotchas.
- Update `../CONTEXT.md` when the hub roster or the door family changes, and `late-ssh/CONTEXT.md` for global keys.
- Treat tests and code as authoritative when comments drift; patch stale comments or this file before handoff.
- Do not add `pub use` re-export layers; `mod.rs` stays declaration-only.

---

## 1. Summary [STABLE]

DCSS runs the **real upstream crawl console binary on a PTY**, but **not** inside late-ssh. It lives in its own crate/pod, `late-dcss`, a minimal russh **server** that spawns one `crawl` child per SSH session. late-ssh reaches it exactly like the NetHack door reaches `late-nethack`: the door is a russh **client** that streams the remote terminal through a `vt100::Parser` and blits it into a ratatui widget below the top bar. SSH *is* the transport — there is no custom IPC.

It is a deliberate near-clone of the NetHack door, because crawl is the same shape of program — a curses roguelike with per-player saves and a SIGHUP hangup-save (the behavior every public dgamelaunch server relies on).

Core shape:
- `Screen::Dcss` has no top-level number key. It is reached by selecting the DCSS card in the Games hub (page `3`, after NetHack) and pressing `Enter`. `Enter` constructs the `State` (`enter_dcss`) and `connect`s — opening the SSH connection and switching to `Mode::Running` — in one step; the standalone launcher render is normally skipped.
- One per-session `DcssProcess` (a russh client; the twin of `door::nethack::proxy::NethackProcess`) owns a background Tokio task that connects to `late-dcss`, requests a PTY + shell, and bridges the remote bytes into a shared `vt100::Parser`. The foreground reads that screen and a `ProxyStatus` flag.
- **Identity vs authorization are split, exactly like nethack.** The connection authenticates with a single Ed25519 key both ends derive from `LATE_DCSS_SECRET` (authorization; blake3 domain `late.sh/dcss/v1`). The account's **arcade handle** travels as the **SSH username** (identity); the host re-sanitizes it (`playname::sanitize`) and passes it as crawl's `-name`, which keys the per-player save and skips crawl's name prompt.
- **The arcade handle is a user-chosen, immutable, per-account public name** (`late-core` `models::arcade_handle`; migration 120): claimed once from the DCSS launcher's first-run prompt, unique case-insensitively (unique index on `lower(handle)`), shape `[A-Za-z][A-Za-z0-9_]{2,19}` (`handle_shape_valid`, a strict subset of crawl's name rules and the host sanitizer), `late`/`late_*` reserved (`handle_reserved`; those shapes are NetHack's legacy derived playnames). Rows outlive the account (`user_id` set NULL on user deletion) so a dead account's handle can never be re-claimed to open its saves. The launcher state machine (`HandleStatus`: Loading → Missing/Claimed/Failed, Claiming) lives in `state.rs` behind an `Arc<Mutex<..>>` written by background tasks via `ArcadeHandleService` (`door/arcade.rs`, threaded through `SessionConfig` like the other door services); `launch_pending` carries hub-Enter intent through the async lookup so returning players still launch in one keypress, and a fresh claim auto-launches. `tick.rs` holds the Dcss screen (`awaiting_handle`) while the prompt/lookup is up. The whole launcher flow lives in `door/arcade.rs` (`HandleFlow`, `HandleStatus`, `HandleKeyResult`) plus `landing::draw_name_modal` (the one-time centered claim modal; Esc dismisses, Enter or a launch attempt reopens) and `landing::handle_launch_block` for the inline status lines behind it; the NetHack door uses the same flow (its legacy `late_<hex>` saves were deliberately orphaned).
- The child is spawned `crawl -name <playname>`, per-player `-macro` and `-morgue` directories, plus `-extra-opt-last` display defaults (`view_max_width=81`, `view_max_height=71` so the map viewport grows with the terminal instead of crawl's 33x21 default; `use_terminal_default_colours=true` so the late.sh theme background shows through instead of crawl painting every cell ANSI black), with `env_clear()` + allowlist (`TERM`, `HOME` = the shared playground, `LANG`/`LC_ALL=C.UTF-8` for crawl's Unicode glyphs). `LINES`/`COLUMNS` are deliberately NOT exported: ncurses treats them as an override of the pty size and then ignores SIGWINCH, freezing crawl at its spawn-time geometry; the pty winsize (openpty + TIOCSWINSZ on `window_change`) is the only size source. SAVEDIR is the build default `~/.crawl`, so all writable state (saves keyed by name, shared scores/logfile/milestones, morgue dumps under `morgue/<playname>/`) lands under `$HOME/.crawl` on the host's PVC.
- While Running, raw client bytes are forwarded straight to the host→child (minus mouse/paste noise, and with the cursor keys retyped to match the game's current cursor-key mode — `keys_for_game` → `app/door/keys.rs`), so crawl — not late.sh — interprets keys. late.sh keeps two keys: `F1`, remapped to crawl's own `?` help (same rationale and encodings as nethack's remap), and `` ` ``, which **detaches**: the game keeps running (state + SSH connection survive `set_screen`) while the backtick workspace cycle hops to the next live dungeon or back to chat; resume from the hub card or the cycle. This deliberately costs crawl its own ` repeat-previous-command binding (players can macro another key to it in-game). A running game with no forwarded keystroke for 30 minutes (the shared door window, `door::game::IDLE_WINDOW`) is closed by the client (`IDLE_SHUTDOWN` in `state.rs`), which lands as the host's SIGHUP-save; an off-screen non-running state is reaped in `app/tick.rs`.
- **Teardown SIGHUP-saves.** On client disconnect or host SIGTERM with a live child, the host bridge sends SIGHUP so crawl saves-and-exits (resumable next launch), with a 5s grace then SIGKILL backstop; a pod SIGTERM broadcasts a `watch` channel and `main.rs` holds the process for `SHUTDOWN_GRACE` (8s) so saves land. Unlike nethack there is **no getlock-slot wedge to defend against** — a stale crawl lock only blocks that one player's save (crawl offers recovery), not the whole door — but the SIGHUP-save still matters so players don't lose runs to rollouts.
- **Per-account init.txt.** Same contract as NetHack's rc push (see the nethack CONTEXT §1 for the full story): the account's rc lives in Postgres (`door_rcs`, `late-core/src/models/door_rc.rs`), authored via the Games hub paste box (`c` on the DCSS card or landing, paste replaces, `x` clears; `app/door/rc.rs` + `app/door/hub/ui.rs`). At launch the client sends it base64-encoded as the `LATE_DOOR_RC_B64` env request before `request_pty`; the host decodes (`late-dcss/src/rc.rs`), writes `<data_dir>/rc/<playname>.rc` (`host.rs::materialize_rc`), and passes `-rc <path>` so crawl reads it instead of the shared `$HOME/.crawl/init.txt`. The `-extra-opt-last` display defaults above still apply on top of the player's rc. Empty push deletes the file, no push (old client) leaves disk as-is, filesystem errors fail soft.
- **The log pipe (boards, badges, feed events — no screen scraping).** The cross-door half of this story (transport contract, cursors and idempotency, handle identity, grant gates, feed gating, deploy order, and the settled decisions behind the badge pairs) lives in `../ingest/CONTEXT.md`; what follows is the DCSS-shaped part. crawl appends machine-readable xlog files under `$HOME/.crawl` on the host's PVC: `logfile` (one line per finished game, written unconditionally by upstream) and `milestones` (live mid-run events; exists only because the build defines `DGL_MILESTONES`, see `DEPLOY.md`). The host serves them over a reserved SSH username, `late_stats` (`late-dcss/src/stats.rs`; inside the reserved `late_*` namespace so no player can claim it): the client pushes per-file byte offsets in env requests (`LATE_DOOR_STATS_CURSORS`, split across several requests when large), the host streams `<file-id>\t<offset>\t<line>` frames with tail -f semantics and stays stateless — no cursor storage, no parsing, no DB. late-ssh's ingestion task (`app/door/ingest/`: one connect-with-retry loop from main.rs, gated on the `dcss_enabled` profile flag; `dcss.rs` pure parsers verified against the pinned 0.34.1 source; cursors in `door_log_cursors`) lands one `door_runs` row per finished game and `door_milestones` rows for orb/rune/branch-entry milestones, idempotent via unique `(game, source_file, source_offset)`. The playname on every line is the arcade handle; reserved `late`/`late_*` shapes and dead handles are skipped. From this pipe: the DCSS board triple on the Leaderboards page (Wins all-time, Deepest dive, Top score), the badge pair `DCO` (Orb pickup, 20k chips, granted at pickup from the milestone stream, the only Orb source) and `DCW` (escape with the Orb, 40k, granting only itself: a win line never back-grants the pickup, so a pickup the milestone stream missed is an ingest bug that shows as a missing badge, never a second payout) — the badge is once per account, the chips repeat once per ingested run behind a 30-day per-milestone lockout (migrations 158 and 208), and #lounge feed events for deaths, the Orb pickup, and wins (freshness+recency gated so backfill never floods the feed). Sprint/seeded games write suffixed files (`logfile-sprint`) the pipe deliberately does not stream, and explore/wizard games are never logged upstream.

- **The crawl files are also published read-only over HTTP** at `late.sh/crawl/...` (`late-dcss/src/publish.rs`, port 2329) for the public DCSS tooling (dcss-stats.com, Sequell): the two xlog files with `Range` support and the morgue tree. The surface, its path-handling security, the fixed morgue layout and the pending ecosystem registration are in [`PUBLISH.md`](PUBLISH.md).

- **Live spectating and the watch chat** are shared with NetHack, Brogue and Zork and live in `../spectate/CONTEXT.md`. DCSS's part is its host's `watch.rs` (the mirror, the `late_watch` sessions, `hud_status` reading `XL3 Lair:2` off crawl's HUD) and the DCSS screen drawing the player's watch chat F4 opens over the game.

The door is gated by the `dcss_enabled` profile flag in `late-ssh/src/config.rs` (enabled in every current profile); when disabled, `connect` is a no-op and the launcher shows "Currently unavailable". The host pod is deployed unconditionally (the flag gates only the client), and the publish listener runs with it: whether the files are reachable from outside the cluster is the ingress's call, not the host's.

---

## 2. Module Map [STABLE]

### Client — `late-ssh/src/app/door/dcss/`

| File | Responsibility |
|---|---|
| `mod.rs` | Module declarations + framing comment. Declaration-only. |
| `proxy.rs` | `DcssProcess`: per-session russh **client** to the host. Owns the bridge task (`run_bridge`), the shared `vt100::Parser`, the `ProxyStatus` flag, and the input/resize command channel; `ProcessConfig.playname` carries the arcade handle. Near-clone of `door::nethack::proxy`. |
| `identity.rs` | `derive_client_key(secret)`: the shared-secret → Ed25519 key derivation (blake3, domain `late.sh/dcss/v1`). Must stay byte-identical to the host's copy (see the cross-crate note in the source). |
| `state.rs` | Per-session `State`: launcher/running `Mode`, connection config (host/port/secret/term/enabled), the optional `DcssProcess`, last viewport `Rect`, the post-exit input grace, `connect`, `set_viewport`, `intercept_input` (F1→`?`), `forward_input`/`keys_for_game`/`strip_input_noise`, and `tick` (flips back to Launcher on close). No award/milestone scraping. |
| `render.rs` | Ratatui rendering: `draw_landing`/`draw_launcher` (CRAWL logo, blurb, dungeon strip, hints) and `draw_running` which blits the live `vt100` screen via `rebels::render::blit_screen`. No late.sh help overlay — in-game help is crawl's own `?`. |

### Host — `late-dcss/` crate (standalone binary)

| File | Responsibility |
|---|---|
| `main.rs` | Tracing init, `Config::from_env`, generate the ephemeral SSH host key, run the russh server alongside the publish listener (either returning is fatal). Broadcasts a shutdown `watch` on SIGTERM/SIGINT and holds the process for `SHUTDOWN_GRACE` so live games hangup-save. |
| `config.rs` | `Config`: `bin` (default `/usr/games/crawl`), `data_dir` (the child `HOME`, default `/var/lib/late-dcss`), `secret`, listen addr/port, publish port, idle timeout. |
| `server.rs` | russh `Server`/`ClientHandler`: `auth_publickey` (compares key DATA — see the nethack CONTEXT §7 comment-field gotcha), `pty_request`, `env_request` (the pushed rc, the stats cursors, the watch request), `shell_request`, `data`, `window_change_request`, `channel_eof/close`. Holds `effective_term` (TERM fallback) and the `SessionHost` split: a game `PtyHost` for players, a `StatsHost` for the reserved `late_stats` username, a `WatchHost` for the reserved `late_watch` username. |
| `host.rs` | `PtyHost`: the per-session PTY bridge — `openpty` + `env_clear` + `setsid`/`TIOCSCTTY` + `IXON/IXOFF/IXANY` clear + `TIOCSWINSZ` + the detached reader, plus the `StopReason` teardown (SIGHUP-save on `Teardown`, plain close on `ChildExited`). Same shape as late-nethack's `host.rs`. |
| `identity.rs` | `derive_client_key(secret)` — identical to the client copy. |
| `playname.rs` | `sanitize(username)`: keep `[A-Za-z0-9_]`, cap at crawl's `MAX_NAME_LENGTH` (30), fall back to `late`. Defense-in-depth on the `-name` arg. |
| `morgue.rs` | The morgue tree: `player_dir` (the `-morgue` path, `$HOME/.crawl/morgue/<playname>`) and `migrate_flat`, the boot-time carry-over of dumps written before that flag into their player's directory. `owner_of` reads a dump's owner off its filename, which works only because a playname cannot contain the dash its timestamp always does. |
| `rc.rs` | `decode_rc(env value)`: base64 → size cap → UTF-8 → no-NUL validation of the pushed per-account rc. Constants mirror the client (`RC_ENV_VAR`, `MAX_RC_BYTES`). |
| `stats.rs` | The `late_stats` log-streaming session: cursor env parsing, complete-line framing (`frame_lines`), and the stateless tail loop over `logfile`/`milestones`. Constants mirrored in late-ssh's `app/door/ingest/stream.rs`. |
| `watch.rs` | Live spectating (`../spectate/CONTEXT.md`): the `LiveRegistry` of live games (host-side `vt100` mirror + pulse per game, watcher counts), the `late_watch` sessions, and `hud_status` (crawl's `XL:`/`Place:` HUD). One of three copies, the others in `late-nethack` and `late-brogue`. |
| `publish.rs` | The read-only HTTP publisher for the public DCSS tooling: the axum router under `/crawl/...`, `Range` parsing (`parse_range`), morgue path sanitizing (`sanitize_rel`), and the autoindex-shaped directory listing. Never writes; reaches nothing outside `$HOME/.crawl`'s published files. |

Cross-module wiring (client side, outside this folder) mirrors nethack exactly: `app/state.rs` (`dcss_state`/`enter_dcss`/`leave_dcss`, Running-mode passthrough with F1 intercept + exit grace), `app/input.rs` (hub launch, launcher Enter, arrow no-op), `app/render.rs` (state taken out for `set_viewport` before blitting; `by crawl.develz.org` chrome + in-game key hints), `app/tick.rs` (tick + return-to-hub), `config.rs`/`state.rs` (`SessionConfig`)/`ssh.rs`/`session_bootstrap.rs`/`src/test_helpers.rs` (the `dcss_enabled`/`dcss_host`/`dcss_port`/`dcss_secret` fields), hub `state.rs`/`ui.rs` (the `HubGame::Dcss` card).

---

## 3. Config And Deploy [VOLATILE]

Env knobs on both halves, the pinned DCSS 0.34.1 source build (SHA-256 verified, `NOWIZARD=y`, the three `EXTERNAL_DEFINES` the log pipe and the published files need, each asserted fail-closed), the image stages, the `infra/doors.tf` entry with its `late.sh/crawl` ingress, and the `-dcss` release flow are in [`DEPLOY.md`](DEPLOY.md). The client reads only `LATE_DCSS_SECRET`, which must equal the host's; host/port are profile literals in `late-ssh/src/config.rs`.

---

## 4. Critical Invariants [STABLE]

Most of the NetHack door's invariants (§7 of its CONTEXT) apply verbatim — auth compares key DATA not the struct, `derive_client_key` byte-identical across crates, `env_clear` + allowlist, XON/XOFF off, close-channel-then-detach-reader, force `ProxyStatus::Closed` + wake render on close, all exits treated identically, fail-soft when disabled, `mod.rs` declaration-only. One deliberate divergence: the playname is the user-chosen arcade handle, not a derived hash (immutability moves from the derivation to the claim-once rule). DCSS-specific:

- The `-name` playname keys the save; the arcade handle is therefore immutable once claimed (no rename path exists on purpose), and a claimed handle row must never be deleted or reassigned. It must stay ≤30 chars (crawl rejects longer names at the prompt — and `-name` bypasses the prompt, so an over-long name would misbehave, not error cleanly); `HANDLE_MAX_LEN` 20 keeps margin.
- Keep `LANG`/`LC_ALL=C.UTF-8` in the child env: crawl's map uses Unicode glyphs through ncursesw, and a POSIX locale degrades the whole dungeon to mojibake.
- The host must pass `-macro $HOME/.crawl/macros/<playname>/` (created before spawn). Without it crawl persists in-game keymaps/macros to one shared `$HOME/.crawl/macro.txt` (the per-name filename needs the `DGL_NAMED_MACRO_FILE` compile flag our plain build doesn't set), letting one player rebind keys for everyone. Saves/scores/logfile/morgues deliberately stay shared under HOME for the stats-publishing path.
- **A game's morgue must land in `morgue/<playname>/`, never flat**: the host passes `-morgue <dir>`, never as `-extra-opt-last`, and never drops it. Both public consumers build dump URLs from that layout; why, and the boot-time `migrate_flat` carry-over, are in `PUBLISH.md`.
- **`macro_dir`/`save_dir` are compiled OUT of this build; never try to set either as an rc option or `-extra-opt-last`.** A `-extra-opt-last macro_dir=` re-force in `host.rs::crawl_args` fails every launch, and a pushed rc's `macro_dir =` line is rejected harmlessly, so `-macro <dir>` is the only mechanism. The root cause (the `/opt` prefix rule in crawl's Makefile) is in `DEPLOY.md`; re-verify there before touching `crawl_args` on a version or recipe change.
- **The cursor keys must be retyped into the mode the game asked for, not forwarded verbatim** (`app/door/keys.rs`, shared with every vt100-backed door; NetHack only exercises it under its opt-in curses windowport, see its `DEPLOY.md`). crawl calls `keypad(stdscr, TRUE)`, whose `smkx` (`ESC [ ? 1 h`) request for application cursor keys dies in our `vt100::Parser` and never reaches the player's terminal; that terminal therefore keeps sending the CSI form (`ESC [ A`) while crawl's ncurses only recognizes the SS3 form terminfo names for it (`kcuu1=\EOA` on every modern TERM but the `linux` console). Forwarded as-is, an Up arrow arrives as three keystrokes (ESC, `[`, `A`), which the roguelike doors read as random commands. `keys_for_game` reads the live mode off the parser (`Screen::application_cursor`) and rewrites the four arrows plus Home/End on the way out, so the door tracks `smkx`/`rmkx` the way a real terminal does. Page Up/Down and modified arrows are identical in both modes and are deliberately left alone. Never "simplify" this back to a verbatim pass-through, and never translate on a mode the door remembers itself: the parser is the only honest source.
- On teardown with a live child, SIGHUP before any SIGKILL so the run is saved; `PtyHost::Drop` must not abort the bridge task. (Stakes are lower than nethack — no door-wide wedge — but a SIGKILLed run is a lost game and crawl's next launch shows a crash-recovery prompt.)
- **The host mirror must see every byte and window change the player gets** (`bridge_loop` feeds `LiveHandle`); spectating's invariants are `../spectate/CONTEXT.md` §3.
- crawl wants ≥80x24; below that it draws a "terminal too small" notice itself. Don't add a late.sh-side size gate — the game's own messaging is the contract.

---

## 5. Tests And Verification [STABLE]

Agents run the focused tests through the capped runner: `make test-llm ARGS="-p late-dcss -p late-ssh -E 'package(late-dcss) or test(dcss)'"`.

Inline pure tests cover: `late-core` `models::arcade_handle` (shape + reserved rules), client+host `identity.rs` (derivation determinism + distinctness + the cross-crate known-answer fingerprint pinning `late.sh/dcss/v1`, mirroring nethack's), client `state.rs` (disabled connect no-op, forward without proxy no-op, mouse/paste stripping, cursor keys following the game in and out of application cursor mode, F1 both encodings, exit grace, claim-prompt byte handling/caps/validation errors), shared `door/keys.rs` (the CSI→SS3 rewrite and everything it must leave alone: plain keys, Page Up/Down, modified arrows, already-SS3 input, a sequence cut by the chunk boundary), host `playname.rs` (sanitize), host `morgue.rs` (owner parsing for both on-disk shapes and the names it must refuse, the flat carry-over, its idempotent second run, and never clobbering a dump crawl wrote in place), host `server.rs` (`effective_term` fallback), host `stats.rs` (cursor parsing, line framing), hub `state.rs` (card order incl. DCSS).

Spectating's tests (host `watch_test.rs`, `app/door/spectate/*_test.rs`, the rail and flow tests) are listed in `../spectate/CONTEXT.md` §4.

Host `publish_test.rs` covers the published surface (`PUBLISH.md`, Tests).

The log pipe's tests live with the ingestion slice: `app/door/ingest/dcss_test.rs` (pure parsers against realistic 0.34.1 lines incl. hostile cases), `stream_test.rs` (listener smoke test of the stats client against a stub SSH host, patterned on the rebels proxy test), `svc_test.rs` (DB-backed: replay idempotency, reserved/dead-name skipping, the per-run payout and its 30-day lockout, cursor advancement), and the board queries in `late-core/src/models/leaderboard_test.rs`.

The PTY bridge and russh loops are process/network-bound and not unit-tested; verify launch/save/quit manually against a real host (compose `service-dcss`).

---

## 9. Deferred / Future Work [VOLATILE]

- **Registering late.sh with dcss-stats and Sequell**, and recording ttyrecs to publish beside the logs: the rows each wants and the migrate-first ordering are in `PUBLISH.md`.
- Shared ghosts/scores already work (one playground), but nothing surfaces the shared scoreboard in the landing. crawl's `-scores` flag could feed it.
- Per-user/global concurrency cap on the host pod if the envelope gets too loose (same posture as nethack: bounded 1:1 by late-ssh's conn caps today).
- Rune-count and branch boards: the pipe already persists `rune` and `br_enter` milestones, so those boards are a query away when wanted.
