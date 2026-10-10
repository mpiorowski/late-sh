# Zork Trilogy door context

Scope: `app/door/zork`, the standalone `late-zork` host, `vendor/frotz`, and
`assets/zork`. Parent: root `CONTEXT.md`. Design/acceptance history:
`devdocs/PLAN-ZORK-DOORS.md`. Development entry is enabled; production entry is
disabled pending the rollout below.

## Player contract

One **Zork Trilogy** Games card opens edition selection, then Continue,
Load manual save, Start new game, and Back. Each account has two slots per
edition: automatic progress and a deliberate manual fallback. These are six
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

Backtick steps out without killing the child; the workspace has one live stop
for the trilogy. Games-card entry opens the menus while retaining that child.
Changing editions or choosing manual/new waits for the old child's successful
checkpoint exit before starting the next. Twenty minutes without forwarded
gameplay input closes the proxy, including while detached. Continue then loads
the last completed automatic checkpoint. Menu navigation, mouse noise, and
metadata requests do not reset the gameplay timer.

## Module map

| File | Responsibility |
|---|---|
| `state.rs` | Native menu/confirmation state, async metadata, one proxy, switch ordering, timers, incremental input filtering. |
| `render.rs` | Games landing, native edition/save menus, slot metadata, existing vt100 cell blit with theme text for default game foregrounds. |
| `proxy.rs` | Private SSH client, bounded metadata JSON request, PTY exec/input/resize, exit status/errors, task teardown. |
| `protocol.rs` | Fixed wire editions/actions and metadata types; deliberately independent of the host crate. |
| `identity.rs` | Zork-specific shared-secret key derivation; no account names or handles in credentials. |
| `late-zork/src/server.rs` | SSH authentication, allowlisted exec requests/TERM, account-wide leases and metadata routing. |
| `late-zork/src/host.rs` | Private edition HOME, environment allowlist, child PTY, input/output bounds, resize/signals/reaping. |
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
changes do not change ownership. No password/shell/arbitrary exec is accepted.
Requests are `list` or `play zork1|zork2|zork3 auto|manual|new`. `list` uses its
own connection and never a terminal or child lease.

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
| Client profile | Dev enabled, `service-zork:2331`; production disabled, `late-zork-sv:2331`. |
| `LATE_ZORK_SECRET` | Required by the host and enabled clients; never passed to Frotz. |
| `LATE_ZORK_BIN` | `/usr/games/frotz`. |
| `LATE_ZORK_STORY_DIR` | `/usr/share/late-zork`. |
| `LATE_ZORK_DATA_DIR` | `/var/lib/late-zork`. |
| `LATE_ZORK_LISTEN_ADDR` / `LATE_ZORK_PORT` | `0.0.0.0` / `2331`, private service only. |
| `LATE_ZORK_IDLE_TIMEOUT` | Host fallback SSH timeout 3600 seconds; client gameplay timeout remains 20 minutes. |

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

Rollout order: publish the pinned `door-zork:2.56pre-r1` asset using the door
workflow's Zork dispatch on the introducing branch; build/release/deploy the
`-zork` runtime and bootstrap `module.door["zork"]`; verify all three editions
and volume persistence; then enable `zork_enabled` in the production client
profile and deploy SSH. Its optional secret reference permits deploying the
disabled client before the Zork secret exists. Rollback disables entry and
keeps the PVC. Do not downgrade the checkpoint format silently.

## Validation

```sh
make -C vendor/frotz curses SOUND_TYPE=none ZORK_DOOR=1 -j4
uv run --no-project vendor/frotz/src/curses/ux_door_test.py
cargo nextest run -p late-zork -p late-ssh -E 'package(late-zork) or test(door::zork)'
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
fragmentation, and client errors. The opt-in runtime script creates and removes
its own container/volume, exercises I → II → III → I with all six slots, then
SIGTERMs/restarts the host with a live game and verifies persistent continuation.
The local SSH TTY review exercised real gameplay, SAVE/RESTORE/QUIT, arrow editing,
Ctrl+S routing, and backtick detach/resume at 140×40 and 80×24. Native menus and
gameplay were raster-reviewed with GitHub Light and GitHub Dark; captures use the
matching canvas because tmux's cell capture omits OSC 11 background changes.
Default game foregrounds inherit `theme::TEXT()`, while explicit colors and
reverse/bold/underline remain intact. Repeat the TTY review when changing this
rendering/input boundary.
