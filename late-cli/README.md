# late

Companion CLI for [late.sh](https://late.sh) — a cozy terminal clubhouse for developers.

Connects to the SSH session and streams lofi audio locally with a live visualizer synced to your terminal.

## Install

macOS / Linux:

```bash
curl -fsSL https://cli.late.sh/install.sh | bash
```

Windows PowerShell (x64):

```powershell
irm https://cli.late.sh/install.ps1 | iex
```

The PowerShell installer places `late.exe` in `%LOCALAPPDATA%\Programs\late` and prints a PATH hint if that directory is not available in the current shell.

## Build from source

```bash
git clone https://github.com/mpiorowski/late-sh
cd late-sh
cargo build --release --bin late
# binary at target/release/late (late.exe on Windows)
```

## What it does

1. Opens an SSH session to `late.sh`
2. Streams audio (lofi/ambient/jazz/classical) to your local speakers
3. Runs a real-time FFT audio analyzer
4. Sends visualizer data back to the TUI over WebSocket
5. Syncs mute/volume controls between terminal and audio

## Usage

```
late
```

That's it. On first run it will generate a dedicated SSH key at `~/.ssh/id_late_sh_ed25519`.
If you want to use a different key, pass `--key /path/to/key`.

### Options

```
--ssh-target <host>        SSH target (default: late.sh)
--ssh-port <port>          SSH port override
--ssh-user <user>          SSH username override
--key <path>               SSH identity file override
--ssh-mode <mode>          SSH transport: native (default) or old
--ssh-bin <command>        SSH client command (subprocess mode only, default: ssh)
--audio-base-url <url>     Audio stream URL
--api-base-url <url>       API URL for WebSocket pairing
-v, --verbose              Debug logging to stderr
```

## Requirements

- Linux, macOS, or Windows x64 (WSL works too)
- Working audio output device
- Rust toolchain (if building from source)

`--ssh-mode old` keeps the old behavior and still depends on a system `ssh` binary.
`--ssh-mode native` uses an embedded `russh` client, records host keys in `~/.ssh/known_hosts`
with accept-new semantics, fetches the pairing token over a dedicated SSH exec handshake, and
does not require OpenSSH on `$PATH`. Native mode intentionally does not fall back to the legacy
`LATE_SESSION_TOKEN=` banner protocol, so it will fail fast against an older server.

If your audio device does not support the stream's native `44.1 kHz` output rate, the CLI now falls back to a supported device rate such as `48 kHz` and resamples locally. Native `44.1 kHz` playback is still preferred when available.

On WSL, audio startup failures now include a targeted hint covering `DISPLAY`, `WAYLAND_DISPLAY`, and `PULSE_SERVER` so users get an actionable fix path instead of only raw ALSA errors.

## Privacy

The CLI connects to `late.sh` using your SSH key. Only your key **fingerprint** is stored — not the full public key. No IP logging, no tracking.

If you'd rather not use your real key:

```bash
ssh-keygen -t ed25519 -f ~/.ssh/late_throwaway
late --key ~/.ssh/late_throwaway
```

## License

This repo is source-available under [`FSL-1.1-MIT`](../LICENSE). See
[`LICENSING.md`](../LICENSING.md) for the plain-English usage policy.
