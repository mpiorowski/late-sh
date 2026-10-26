# Zork Trilogy door context

Scope: `app/door/zork`, the standalone `late-zork` host, `vendor/frotz`, and
`assets/zork`. Parent: `late-ssh/src/app/door/CONTEXT.md`. Entry is enabled in
both client profiles; the first production deploy follows the rollout order
below.

## Player contract

One **Zork Trilogy** Games card, filed under "remakes" after A Dark Room, opens
edition selection, then Continue, Load manual save, Start new game, and Back.
Entry is gated on the client's `zork_enabled` by the hub (`../hub/input.rs`,
before the opening is counted), and play on a claimed arcade handle (see Watchers and suggestions). Each account has two slots per edition: automatic progress and a deliberate manual fallback. These are six
slots, not six independent campaigns. Missing, invalid, and unavailable slots
are distinct; Continue and manual load are disabled unless their slot is ready
(Continue also resumes the session's live edition). Manual load and new game
ask first, default to No, and preserve the manual fallback.

`SAVE` opens a description editor for the fixed manual slot. Descriptions are
optional, limited to 128 Unicode characters, stripped of controls, and never
used as paths. Escape cancels. Manual restore follows the story's SAVE return
branch and refreshes automatic progress at the next input boundary.

At a story READ, the first whole token `RESTORE` is intercepted case
insensitively; trailing text is ignored. The `[y/N]` overlay restores the
original screen and the same pending READ on No, Enter, or Escape, without
advancing a move. Yes commits the coherent checkpoint and exits to that
edition's actions. A failed write refuses voluntary exit and leaves the game
open. `RESTOREfoo` goes to the story parser (V3 dictionaries can independently
truncate it to RESTORE); the story's restore opcode also uses the menu, never
a filename chooser.

Inside a game every key is the story's, Esc included. The frame title leads
with the ways out (`` ` `` step out, `QUIT`), then `RESTORE` and `SAVE`, so an
80-column title still shows them. On the menus a lone Esc backs out one level,
from the edition list to the Games hub.

Backtick steps out without killing the child; the workspace has one live stop
for the trilogy. Games-card entry opens the menus while retaining that child.
Changing editions or choosing manual/new waits for the old child's successful
checkpoint exit before starting the next. Thirty minutes (the shared door
window, `door::game::IDLE_WINDOW`) without forwarded gameplay input closes the proxy, including while detached. Continue then loads
the last completed automatic checkpoint. Menu navigation, mouse noise, and
metadata requests do not reset the gameplay timer. Client state without a live
game does not outlive the visit: leaving the menus drops it (`App::leave_zork`),
and `App::tick` reaps a detached game that ended off-screen, so each visit
starts from a fresh menu.

## Watchers and suggestions

Every running game is watchable, as on the roguelikes
(`../spectate/CONTEXT.md`): the Games hub's live rows, the live strip and the
Live panel, the watch chat, and an open watch as a backtick stop. Every game
is played under the account's arcade handle, through the roguelikes' shared
`HandleFlow`: entering the menus looks it up, an account without one gets the
one-time claim modal over the menus (`input::handle_modal_input` routes its
keys), and choosing Continue, Load or New without one reopens it. The action
that opened it is kept with its edition (`after_claim`) and launched on that
edition when the claim lands, so one Enter still starts the game, and a New
confirmed for one edition never starts on another the player browsed to
meanwhile. `play` carries the handle and the host lists
the game under it. The player's watcher pane sits beside the game only while
the game is on screen, never beside the menus, under the same
`show_watch_chat` setting the `t` key on the Zork card flips.

Watchers suggest a command by starting a watch-chat line with `>`
(`>open mailbox`). The watch chat's header says how from each side
(`spectate/ui.rs::suggestion_hint`): `type >look to suggest` for watchers,
`F3 takes a >suggestion` for the player, also on the player's one-row
fallback. The newest fresh one
(ten minutes) from anyone but the player waits on its own row under the
pane's header, or takes the one-row form (`suggest.rs`). F3 types it onto
the story's prompt without Enter, so the player still reads, edits or sends
it; nothing a watcher writes reaches the game without that key. F3 types
only at an empty prompt (`suggest::at_empty_prompt`: the cursor right after
a lone `>`), so the letters never land in a half-typed command, a pagination
wait, the SAVE editor or the RESTORE confirmation. A command is printable
ASCII only, whitespace collapsed, at most 60 characters; once typed it is
not offered again, and an older one does not take its place. Frotz ignores
function keys at a V3 prompt, so F3 (and the watch chat's F2) cost the story
nothing.

## Module map

| File | Responsibility |
|---|---|
| `state.rs` | Native menu/confirmation state, async metadata, the arcade-handle flow (lookup, claim modal, launch after claim), one proxy, switch ordering, timers, incremental input filtering, typing a suggestion, and all of the client's telemetry (see Telemetry). |
| `input.rs` | Games-card launch (the hub has already gated on `zork_enabled`), parsed menu keys, raw passthrough to a visible game with backtick detach and F3. `App` only calls in. |
| `render.rs` | Games landing, native edition/save menus, slot metadata, existing vt100 cell blit with theme text for default game foregrounds (`inherit_theme_text`, also used by the watch view). |
| `suggest.rs` | Watchers' `>` commands: parsing a chat line into a command, the waiting suggestion for this session's own game, and the empty-prompt test. |
| `proxy.rs` | Private SSH client, bounded metadata JSON request, PTY exec/input/resize, the tagged `Ending` (exit status, closed, failed), task teardown. |
| `protocol.rs` | Fixed wire editions/actions, the `play` request, exit statuses, and metadata types; deliberately independent of the host crate. |
| `identity.rs` | Zork-specific shared-secret key derivation; no account names or handles in credentials. |
| `late-zork/src/server.rs` | SSH authentication (accounts and the reserved `late_watch`), allowlisted exec requests/TERM, account-wide leases and metadata routing, watch sessions. |
| `late-zork/src/host.rs` | Private edition HOME, environment allowlist, child PTY, input/output bounds, resize/signals/reaping, the watch mirror of every game. |
| `late-zork/src/watch.rs` | The fourth copy of the roguelike hosts' watch module (`../spectate/CONTEXT.md`): live registry, roster and screen streams, the status bar reader. |
| `late-zork/src/slots.rs` | Read-only inspection through the fixed interpreter executable, with timeout and bounded output. |
| `vendor/frotz/src/curses/ux_door.c` | GPL checkpoint container, metadata inspection, screen serialization, manual description editor, RESTORE overlay and signal flags. |

## Process, identity, and storage

The FSL Rust host executes the separate GPL Frotz executable; it neither links
Frotz nor contains VM serialization. The MIT story binaries have immutable
provenance and hashes in `assets/zork/`. Vendored source pin, modified-file
notices, format, and rebuild instructions live in `vendor/frotz/LATE-ZORK.md`.
Asset and runtime images include the complete modified interpreter source,
COPYING, and build recipe under `/usr/share/doc/frotz`.

`late-ssh` reaches the private russh listener at port 2331. Both sides derive
Ed25519 credentials with domain `late.sh/zork/v1\0zork\0`. SSH usernames are
`late_` plus the last 24 hex digits of the immutable account UUID; display-name
changes do not change ownership. No password or arbitrary exec is accepted, and a shell only for watching.
Requests are `list` or `play zork1|zork2|zork3 auto|manual|new <handle>`; the
arcade handle names the game in the watch roster, checked to the handle shape,
never a path or an identity. `list` uses its own connection and never a
terminal or child lease. The reserved `late_watch` username (same derived key)
gets watch sessions only: the `LATE_DOOR_WATCH` env request and a shell, never
`play` or `list` (`../spectate/CONTEXT.md`). Neither side takes the other's
older `play` shape (the three-word form without a handle), so `late-zork` and
the SSH image ship in the same release.

The host holds one process-wide lease per account across the trilogy, through
child teardown. Different accounts may play concurrently. Competing logins
receive status 75 and a visible busy explanation. Exit 20 means a successful
return to menus; 21 means a selected save is invalid/unavailable. The client
waits for stream close before switching and drains final PTY output.

Storage is `/var/lib/late-zork/<account>/<edition>/`, with account/edition
directories mode 0700 and fixed `autosave.lz` / `manual.lz` files mode 0600.
The child gets only TERM, HOME, UTF-8 locale, and `LATE_FROTZ_DOOR`; the host's
identity secret and ambient environment are cleared. Paths/commands never come
from a player. Interpreter file features and hotkeys are disabled in door mode.

The versioned `LZORK001` container wraps Quetzal VM state, decoded READ operands,
RNG, window records, visible Unicode terminal cells/styles/cursor, timestamp,
and description. Automatic checkpoints occur before publishing coherent READ
prompts, including story questions and death states. Resume calls the decoded
READ directly: no command replay, LOOK injection, or second operand decode.
Partial typing, scrollback outside the current screen, and pagination waits
are outside the persistence contract. A smaller terminal clips the saved
screen while keeping its pending prompt visible. The last committed save can
legitimately be a losing state.

Writes use an adjacent exclusive temporary file, flush/fsync, and atomic
rename; failure preserves the old slot. Version, story identity, lengths,
checksum, screen bounds, and payload structure are checked before restore.
SIGHUP/SIGTERM only set flags. Normal interpreter flow retries its cached
coherent checkpoint and exits; host teardown grants five seconds before kill,
host SIGTERM grants eight seconds, and the pod grants thirty seconds.
Voluntary switching uses SIGUSR1 and refuses exit on failed persistence.

## Configuration and delivery

| Setting | Default / policy |
|---|---|
| Client profile | Enabled in both; dev `service-zork:2331`, production `late-zork-sv:2331`. |
| `LATE_ZORK_SECRET` | Required by the host and by both client profiles at startup; never passed to Frotz. |
| `LATE_ZORK_BIN` | `/usr/games/frotz`. |
| `LATE_ZORK_STORY_DIR` | `/usr/share/late-zork`. |
| `LATE_ZORK_DATA_DIR` | `/var/lib/late-zork`. |
| `LATE_ZORK_LISTEN_ADDR` / `LATE_ZORK_PORT` | `0.0.0.0` / `2331`, private service only. |
| `LATE_ZORK_IDLE_TIMEOUT` | Host fallback SSH timeout 3600 seconds; client gameplay timeout remains 30 minutes. |

`docker/doors/zork.Dockerfile` builds the vendored curses frontend with
`SOUND_TYPE=none ZORK_DOOR=1`, checks story hashes, and preserves corresponding
source before compilation. Root stages are `builder-zork`, `dev-zork`, and
`runtime-zork`. Compose uses `zork-data`; the shared Terraform door module uses
one replica, kill-before-create rollout, a retained 1Gi PVC, non-root runtime,
seed ownership, and a private generated identity secret. Replicas must stay one
because the account lease is process local.

In `dev-zork`, the watcher/compiler keeps root-owned shared Cargo caches; the
launcher fixes volume ownership and uses `runuser` to execute the host as `late`.
Frotz refuses root, so both development gameplay and the production runtime must
keep the host/interpreter under that unprivileged user.

Rollout order, first production deploy (temporary: delete this list once
`late-zork` is live in production):

1. Publish the pinned `door-zork:2.56pre-r1` asset with the door workflow's
   Zork dispatch.
2. Release `-zork`: `deploy_service` finds no `late-zork` deployment and
   bootstraps `module.door["zork"]` (PVC, identity secret, deployment,
   service). Verify all three editions and volume persistence.
3. Release `-infra`: the full apply adds the `LATE_ZORK_SECRET` reference to
   `service-ssh`. It is a required reference, so it needs the secret from
   step 2.
4. Release the SSH image. The production profile requires `LATE_ZORK_SECRET`
   at startup, so this must come after step 3. Do not use `-full` for steps
   3 and 4: it ships the image before the apply, and the new pods would fail
   startup (the rolling update keeps the old pods serving, but the rollout
   fails and the apply leg never runs).

Rollback sets `zork_enabled` false in the production profile and keeps the
PVC. Do not downgrade the checkpoint format silently.

## Telemetry

Every client-side metric is recorded in `state.rs` (`tick` and `start`), the
one place that sees each session close, switch, idle reap, and save read.
Labels are closed enums from `metrics.rs`; `edition` and `action` reuse the
wire keys.

| Metric | Labels | Recorded when |
|---|---|---|
| `late_ssh_zork_launches_total` | `edition`, `action` (`auto`/`manual`/`new`) | A host child is requested. Continue on the live edition resumes and records nothing. |
| `late_ssh_zork_session_ends_total` | `edition`, `end` (`quit`, `returned`, `save_rejected`, `busy`, `crashed`, `connect_failed`, `idle`) | A proxy closes, mapped from the exit status, or the 30-minute reap drops it. |
| `late_ssh_zork_switches_total` | `result` (`switched`, `save_failed`, `timed_out`) | A switch away from a live game resolves. |
| `late_ssh_zork_catalogue_reads_total` | `result` (`read`, `failed`) | A menu save-slot read lands. |
| `late_ssh_zork_suggestions_total` | `result` (`typed`, `not_at_prompt`, `none_waiting`) | F3 on a visible game. |

Watching itself is counted by the shared `late_ssh_door_watch_*` metrics with
`game="zork"` (`../spectate/CONTEXT.md`).

Grafana: the "Zork Trilogy" row in `monitoring/dashboards/observability.json`
(launches, session ends, switches, save reads and suggestions, and a trouble
panel that should stay at zero). Connect and catalogue failures also log a warning with
the error.

## Validation

```sh
make -C vendor/frotz curses SOUND_TYPE=none ZORK_DOOR=1 -j4
uv run --no-project vendor/frotz/src/curses/ux_door_test.py
make test-llm ARGS="-p late-zork -p late-ssh -E 'package(late-zork) or test(door::zork)'"
docker build --platform linux/amd64 -f docker/doors/zork.Dockerfile \
  -t ghcr.io/mpiorowski/late-sh/door-zork:2.56pre-r1 .
bash docker/doors/smoke/zork.sh ghcr.io/mpiorowski/late-sh/door-zork:2.56pre-r1
docker build --platform linux/amd64 --target runtime-zork -t late-zork:local .
bash scripts/test_zork_host.sh
make check
```

The interpreter suite uses real curses PTYs under tmux, on macOS and non-root
Linux. Rust tests cover authentication, isolation/leases, failed spawns,
disconnect cleanup, catalogue traffic, menu defaults, switches, timers, input
fragmentation, client errors, and the client dropping an idle or ended state;
the host's watch path (every game in the roster under its handle and on
screen, a launch without a handle refused, `late_watch` unable to play, the
status bar reader), the claim prompt standing in for a launch without a
handle, a launch chosen during the handle lookup starting the edition it was
chosen on, the player's F2 composer taking typing, Enter and Esc
(`input_flow_test.rs`), and the suggestions (`suggest_test.rs`: which chat
lines are commands, a reply's quote never one, the newest fresh one from a
watcher, typed once, only at an empty prompt).
The opt-in runtime script creates and removes its own container/volume,
exercises I → II → III → I with all six slots, then SIGTERMs/restarts the host
with a live game and verifies persistent continuation.

The rendering/input boundary has no automated check: a change there needs a
manual SSH TTY pass covering gameplay, SAVE/RESTORE/QUIT, arrow editing, Ctrl+S
routing, and backtick detach/resume at 140×40 and 80×24, plus the native menus
and gameplay under both GitHub Light and GitHub Dark (capture with the matching
canvas: tmux's cell capture omits OSC 11 background changes). Default game
foregrounds inherit `theme::TEXT()`; explicit colors and reverse/bold/underline
pass through intact.
