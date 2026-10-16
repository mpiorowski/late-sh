# Exec Tunneling Through Bastion

Status: design sketch.

## Problem

`late-cli` native and OpenSSH modes fetch their audio pairing token with an SSH
`exec` request before opening the interactive TUI shell:

```text
late-cli-token-v1
```

Direct `late-ssh` supports this. `late-bastion` currently rejects all `exec`
requests and only tunnels `pty-req` + `shell` sessions over `/tunnel`.

We want native `late-cli` to work through bastion without teaching the bastion
about individual command names or token minting rules.

## Design Goal

Bastion should understand SSH `exec` transport shape, not application command
semantics.

Command ownership stays in `late-ssh`:

- bastion accepts an SSH `exec` request
- bastion forwards the command to `late-ssh`
- `late-ssh` runs a shared exec handler
- bastion maps the result back to SSH stdout, stderr, exit status, EOF, and close

This keeps the bastion thin while leaving room for future exec commands.

## Channel Model

SSH already multiplexes streams over one transport connection. The TUI and token
fetch are separate SSH session channels:

- token fetch: `session` channel + `exec`
- TUI: `session` channel + `pty-req` + `shell`

The exec channel can have stdin, stdout, and stderr streams. For the MVP, we do
not need full streaming over the bastion/backend hop. We only need bounded,
whole-message execs.

## Backend Transport

Prefer carrying exec over the same bastion-to-backend `/tunnel` WebSocket as the
TUI, using separable JSON text-control frames.

This makes the WebSocket intentionally stateful. That would be a smell for a
general web API, but here the WebSocket is a tunnel for SSH-like session
protocols where connection boundaries and event order are meaningful. Keeping
exec and shell setup on one ordered channel lets the backend enforce sequencing
rules such as "exec is only valid before shell start" or "PTY metadata must
arrive before shell bytes."

The important change from today's shape: `/tunnel` must be opened before the
first exec or shell channel that needs backend service. Today bastion opens
`/tunnel` from `shell_request`; exec tunneling requires bastion to open the
backend tunnel earlier, probably on first accepted session channel or first
backend-bound request.

An independent `/exec` endpoint remains a fallback option, but it is not the
preferred design because it splits ordering and lifecycle across multiple
backend connections.

The request uses the same trust envelope as `/tunnel`:

- `X-Late-Secret`
- `X-Late-Fingerprint`
- `X-Late-Username`
- `X-Late-Peer-IP`
- `X-Late-Session-Id`
- `X-Late-Via: bastion`

`X-Late-Session-Id` should be minted before the first exec or shell channel and
then reused for the lifetime of the `/tunnel`. Today bastion mints it in
`shell_request`; exec tunneling should move that to per-connection or first-use
state.

## Tunnel State Machine

The `/tunnel` text-control vocabulary should remain explicit. Existing binary
frames continue to mean raw PTY bytes, and JSON text frames carry control events.

Initial state:

```text
Connected
```

Allowed setup/control sequence:

```text
Connected
  -> ExecDone / Connected          via one exec_request before shell start
  -> PtyReady                      via pty
  -> ShellRunning                  via shell_start
  -> Closed
```

Resize remains valid after PTY setup. Raw binary PTY bytes are only valid once
the shell is running.

Suggested text frames:

```json
{"t":"exec_request","id":"018f...","command":"late-cli-token-v1"}
{"t":"exec_response","id":"018f...","stdout":"...","stderr":"","exit_status":0}
{"t":"pty","term":"xterm-256color","cols":120,"rows":40}
{"t":"shell_start"}
{"t":"resize","cols":120,"rows":40}
```

For MVP, a tunnel may carry only one pre-shell exec and one shell session. Future
support for multiple execs is allowed by the protocol shape, but the first
implementation should reject additional exec requests with a normal exec failure.
Future support for multiple concurrent SSH channels would need channel ids in
every frame.

## Message Shape

Default mode is UTF-8, no stdin, UTF-8 output. It avoids base64 for readability
and lower overhead.

Request:

```json
{
  "t": "exec_request",
  "id": "018f...",
  "command": "late-cli-token-v1"
}
```

Response:

```json
{
  "t": "exec_response",
  "id": "018f...",
  "stdout": "{\"session_token\":\"abc\"}",
  "stderr": "",
  "exit_status": 0
}
```

Bastion then writes:

- `stdout` as SSH channel data
- `stderr` as SSH extended data
- `exit_status`
- EOF
- close

## Command Prefix Convention

Reserve prefixes for future expansion:

```text
<command>
stdin://<command>
bin://<command>
stdin+bin://<command>
```

Semantics:

| Prefix | Stdin | Stdout/stderr | JSON fields |
| ------ | ----- | ------------- | ----------- |
| none | not allowed | UTF-8 | `stdout`, `stderr` |
| `stdin://` | bounded UTF-8, collected until SSH EOF | UTF-8 | `stdin`, `stdout`, `stderr` |
| `bin://` | not allowed | binary | `stdout_b64`, `stderr_b64` |
| `stdin+bin://` | bounded binary, collected until SSH EOF | binary | `stdin_b64`, `stdout_b64`, `stderr_b64` |

MVP only needs the no-prefix form. Prefixes are reserved so future behavior has a
clear path without changing the basic envelope.

## Stdin Handling

For no-prefix commands, stdin is contractually unsupported. For the MVP, if
client stdin arrives on a no-prefix exec channel, bastion should discard it and
log a warning with the command name and discarded byte count:

```text
discarding N stdin bytes on exec for command <command>
```

It should not fail or tear down the exec solely because stdin arrived.

For future `stdin://` and `stdin+bin://` commands, bastion can buffer stdin until
SSH channel EOF, a size cap, or a timeout. Reasonable initial cap: 64 KiB. This
turns SSH's stream into one delimited backend request without implementing full
streaming.

## UTF-8 Rules

Non-binary mode requires valid UTF-8 for stdin, stdout, and stderr.

If bastion receives invalid UTF-8 where text is required, it should fail the exec
with a clear stderr message. If `late-ssh` needs to return arbitrary bytes later,
that command should use `bin://`.

## Shared Backend Handler

`late-ssh` should route both direct russh exec and bastion exec through a shared
handler:

```text
handle_exec_command(ctx, command, stdin) -> ExecResponse
```

The direct russh path for `late-cli-token-v1` and the backend exec endpoint
should produce the same payload and exit status.

For `late-cli-token-v1`, the handler returns:

```json
{"session_token":"..."}
```

as UTF-8 stdout with exit status `0`.

## Non-Goals

- No full stdout/stderr streaming over WebSocket for now.
- No bastion-owned command registry.
- No bastion-owned audio token minting.
- No binary payload support in MVP.
- No shell command execution on the backend; commands are protocol verbs handled
  by `late-ssh`.

## Open Questions

- Should the backend endpoint be `/exec`, `/tunnel/exec`, or a generalized
  control WebSocket?
- Should unsupported stdin be rejected immediately on first data frame, or after
  EOF with a normal exec response?
- What should the exact stdin cap and timeout be when `stdin://` lands?
- Should future binary mode use base64 in text frames or binary WebSocket frames
  with a small JSON header?
