# A browser terminal with sound (proposed design, gate undecided)

Status: proposed, not built, and not decided. The open question is the
gate (below): whether a door with no `ssh` in front of it lets in a crowd
the house does not want. Nothing here should be built before that is
answered and the first spike has run.

## Why

- 84% of first-week accounts arrive over plain ssh, into a session with no
  radio to hear (root `AUDIENCE.md`, section 1). The tour's biggest single
  drop on the old route was the Stations stop, on sessions with nothing to
  listen to.
- Arriving on the `late` CLI goes with coming back: 14% against 8% for
  plain ssh, on small numbers.
- The CLI is an install. A page is a click, and a click is also the
  gesture a browser needs before it will play audio.

The claim to test: a newcomer who hears the house in the first minute
stays longer and returns more. `ssh late.sh` stays the headline; this is
the "or try it here" beside it.

## The design: real SSH in the page

`late-ssh` should not be able to tell a browser from a terminal.

1. **The key.** The page makes an Ed25519 key on first visit and keeps it
   in IndexedDB. Its fingerprint is the account, exactly as today
   (`auth_publickey` in `late-ssh/src/ssh.rs`). A "download my key" button
   writes it as an OpenSSH private key, so the same account works from
   `ssh -i` and the CLI.
2. **The transport.** An SSH client compiled to wasm speaks SSH over a
   WebSocket to a small gateway that pipes the bytes to port 2222 and
   writes the PROXY header with the browser's address. Public-key auth,
   admission before the handshake, the render loop and the drain all stay
   as they are.
3. **The screen.** xterm.js: truecolor, SGR mouse, resize.
4. **The sound.** The page asks for the session token the way the native
   CLI does (the `late-cli-token-v1` exec request, `late-cli/CONTEXT.md`
   §5), pairs on `/api/ws/pair` as a new `ClientKind`, and plays the radio
   and the YouTube queue the way `/listen` does, obeying the same mute,
   volume and source controls a CLI obeys.

Turned down: **a second, non-SSH transport inside `late-ssh`** (a
WebSocket straight into the app, with a signed challenge for auth). It
would duplicate the session lifecycle in `ssh.rs`, which is the
highest-risk code in the repo: admission, the render loop under
backpressure, the drain handoff.

## The gate (undecided)

Today the door is its own filter: to get in, a person needs a terminal, an
ssh client and a key. A page removes all three. The worry is a flood of
trolls and throwaway accounts the moment a link works.

What is true now: admission caps connections per IP before the handshake.
Unverified: whether anything caps accounts made per IP.

Options, not exclusive:

- **Browser-born accounts are guests.** They hear the radio, play the
  Arcade and read the room; writing to a shared surface (chat, the
  Artboard, the calendar) needs the account to have come in once over ssh
  with its downloaded key. The gate stays where the harm is. It costs
  little of what the page is for: a board won on day one predicts a return
  at twice the rate a chat message does.
- **Existing accounts only.** A browser is linked from inside an ssh
  session, never created from the page. No flood at all, and no newcomer
  benefit either: it becomes a second client for regulars.
- **An open door with a cap** on accounts made per IP per day, and the
  moderation tools as they are.

If none of these reads as safe, this stays unbuilt.

## What else could sink it

- **The wasm SSH client is unproven.** No client has been checked against
  `late-ssh` from a browser. This is the first spike, and it decides
  whether the design above is real.
- **The gateway cannot sit behind ingress-nginx.** Every reload kills the
  websockets it holds 240 s later (`SCALE.md`, Pain Point 9), which here
  would be whole sessions. It has to hang off the SSH edge
  (`infra/ssh-tcp.tf`) or stand on its own.
- **Site data is the account.** Clearing it loses the key unless it was
  downloaded. `user_ssh_keys` holds many keys per account; whether a
  member can add one from inside a session is unverified.
- **The browser keeps some keys.** Ctrl+W, Ctrl+T and Ctrl+N cannot be
  taken from a normal tab. Check the global keys in `late-ssh/CONTEXT.md`
  §11 against that list.
- **Phones.** xterm.js without a key bar is poor on a touch screen. Desktop
  first.
- **Inline images.** Surfaces that draw PNG through the terminal's
  graphics protocol fall back to their ASCII form.

## What would say it worked

Arrival client becomes three-way (browser, plain ssh, CLI), read the way
section 1 of `AUDIENCE.md` reads the two today: two-day return by the
client a newcomer arrived on, and the share of first sessions that had
sound.

## Work order

1. Spike: a wasm SSH client over a WebSocket against the dev stack. Stop
   here if it does not hold.
2. Decide the gate.
3. The gateway, with the PROXY header, off the SSH edge.
4. The page: xterm.js, the key store, download my key.
5. The token request, the pair socket as its own `ClientKind`, the audio.
6. The gate as decided, with its telemetry.
