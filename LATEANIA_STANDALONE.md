# Running Lateania Standalone

How to run Lateania as its own independent server, on your own host and
database, with no dependency on late.sh's own deployment. Players connect
straight into the door: no lobby, no other games, no clubhouse.

This works because Lateania is a normal part of the `late-ssh` binary, not a
separate program. Setting `LATE_SSH_LATEANIA_ONLY=1` makes every incoming
session skip the lobby screen and enter Lateania immediately, the same
transition the hub's own launcher takes, just run automatically on connect
instead of waiting for a keypress. Everything else (accounts, persistence,
the SSH transport itself) is the real, unmodified late-ssh code.

## Quick start (local test)

```bash
git clone https://github.com/mpiorowski/late-sh
cd late-sh
make start-lateania
```

Then connect:

```bash
ssh localhost -p 2222
```

`make start-lateania` is `make start` with an override group
(`LATEANIA_OVERRIDES` in the `Makefile`) that sets
`LATE_SSH_LATEANIA_ONLY=1` and turns off the other door games, voice, IRC,
and AI, since none of them are needed for a Lateania-only box. Postgres,
migrations, and the SSH host key (`make keys`) are all handled the same way
`make start` already handles them for the full clubhouse.

## Deploying it for real

For a box other people connect to, don't run it with the local dev defaults.
At minimum, override these on top of `LATEANIA_OVERRIDES`:

```bash
make start-lateania \
  LATE_FORCE_ADMIN=0 \
  LATE_DB_PASSWORD=<a real password, not "postgres"> \
  LATE_SSH_PORT=22 \
  LATE_WEB_URL=http://<your-host>:3000
```

- `LATE_FORCE_ADMIN` defaults to `1` in the Makefile: fine for local testing,
  wrong for anything public, since it makes every connecting session an
  admin. Set it to `0`.
- `LATE_DB_PASSWORD` defaults to `postgres`. Postgres isn't exposed outside
  the compose network by default, but set a real password anyway if the host
  port is ever opened.
- `LATE_SSH_OPEN=1` (the default) is intentional and matches the real
  `late.sh`: connecting needs no key or password, since login state lives
  inside the app itself. Leave it as-is.
- Point `LATE_SSH_PORT` at whichever port you want players to connect to
  (22, 2222, whatever's open on the box) and update `LATE_WEB_URL` to match
  your actual host/domain if you're pointing DNS at it.
- `make keys` (already run by `make start`) generates `server_key`, an
  Ed25519 host key, once and reuses it after; don't regenerate it on every
  restart or returning players will get a host-key-changed warning.

Run it detached and keep it up across reboots however you'd normally manage a
long-running Docker Compose stack on your box (a systemd unit wrapping
`docker compose`, or `docker compose up -d` plus a restart policy).

## What this is not

This isn't a from-scratch SSH server for Lateania. It's the real, proven
late-ssh binary with one flag flipped, so it inherits every existing
guarantee (persistence, reconnect handling, the actual game code) instead of
reimplementing any of it.
