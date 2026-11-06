# late-cli distribution

How the `late` binary reaches users: the public installers and their overrides, the supply chain (checksums, attestations, the release workflow), and the pre-connect update check. Part of the CLI; the rest is `CONTEXT.md`.

## Installers

Public installers:
- macOS/Linux/Termux: `curl -fsSL https://cli.late.sh/install.sh | bash`
- Windows PowerShell: `irm https://cli.late.sh/install.ps1 | iex`
- Nix/NixOS: `nix run github:mpiorowski/late-sh#late`

Installer defaults:
- `scripts/install.sh` and `scripts/install.ps1` default to `https://cli.late.sh`
- `LATE_INSTALL_BASE_URL` overrides distribution host
- `LATE_INSTALL_CHECKSUM_BASE_URL` overrides where `sha256sums.txt` is read from; default is the GitHub Release assets, `https://github.com/mpiorowski/late-sh/releases/download`
- `LATE_INSTALL_VERSION` selects a specific version instead of `latest`
- `LATE_INSTALL_DIR` overrides install directory
- Shell installer detects WSL, Termux, and Git Bash/MSYS/Cygwin; Termux receives the Android build and Windows shell environments receive the Windows `late.exe` build
- Shell installer targets `/usr/local/bin`, `$HOME/.local/bin`, the Termux prefix, or `%LOCALAPPDATA%\Programs\late` under Windows shell environments, depending on platform and permissions
- PowerShell installer places `late.exe` under `%LOCALAPPDATA%\Programs\late` unless overridden and prints a PATH hint when needed
- PowerShell installer uses environment-based architecture detection instead of `RuntimeInformation.OSArchitecture` so older Windows PowerShell/.NET hosts can run it
- PowerShell installer passes `-UseBasicParsing` on download requests for Windows PowerShell 5.1 compatibility.
- Checksum verification is mandatory and fails closed (see Supply chain below)

#### Supply chain

- `latest` is only a pointer. Both installers resolve it by reading `{base}/latest/VERSION`, validate the tag against `^[A-Za-z0-9][A-Za-z0-9._-]*$` (it is interpolated into URL paths on two hosts, so a tampered pointer must not be able to walk to another GitHub path), then fetch every file from `releases/<tag>/`. Nothing is installed from `latest/<target>/`.
- `sha256sums.txt` is read from the GitHub Release assets (`{LATE_INSTALL_CHECKSUM_BASE_URL}/<tag>/sha256sums.txt`), not from the distribution bucket. A binary from R2 must match a checksum from GitHub, so a compromised bucket or bucket credential alone cannot swap the binary under a trusted copy of the installer. Two residual attacks from a bucket alone: a downgrade, pointing `latest/VERSION` at an older genuine tag; and replacing the installer itself, since `install.sh`, `install.ps1`, and `index.html` are served from the same bucket and `curl ... | sh` trusts it for the script. The second is closed only by running the installer from a checkout or verifying the provenance bundle by hand.
- Verification is fail-closed in both installers: unreachable checksum file, missing entry, no SHA-256 tool, or mismatch abort the install. The `late-webview` helper, when its download succeeds, is verified the same way. `install.ps1` enables TLS 1.2 on `ServicePointManager` first, because github.com refuses older TLS and Windows PowerShell 5.1 on older .NET hosts does not offer 1.2 by default.
- Releases published before the GitHub checksum upload existed have no `sha256sums.txt` on their Release and cannot be installed through `LATE_INSTALL_VERSION` unless the file is backfilled by hand with `gh release upload`. If `latest/VERSION` (or a tag users are likely to pin) points at a release without checksums, backfill it, or a CDN-cached pointer makes the installer fail closed.
- Every binary carries a keyless Sigstore build-provenance attestation from `actions/attest`, published next to it as `<binary>.sigstore.json` (R2 only; bundles are self-verifying, so their origin does not matter). One attestation per file keeps bundles single-subject for `cosign verify-blob-attestation --bundle`. The certificate identity is always `deploy_cli.yml@refs/tags/<tag>`: Sigstore takes the identity from `job_workflow_ref`, which for a reusable workflow is the called file, and `release.yml` only appears in the build-config extension. Verification commands live in `README.md` "Verifying downloads".
- The build must run on the release tag. The OIDC token carries the ref the workflow started on, not the ref checked out, so a dispatch from `main` would sign `main`'s commit as the release. `build_cli` fails first thing unless `GITHUB_REF` is `refs/tags/<release_tag>`; a manual redeploy is `gh workflow run deploy_cli.yml --ref <tag> -f release_tag=<tag>`. The release event already runs on the tag.
- A published tag is immutable. `publish_cli` uploads `sha256sums.txt` to the Release only when the Release has none; if one exists and matches the build it is left alone (a re-run of failed jobs reuses the same artifacts), and if it differs the job fails so the rebuild ships under a new tag. There is no `rust-toolchain` pin, so a rebuild on a newer runner image will generally differ. To retry a tag on purpose, `gh release delete-asset <tag> sha256sums.txt` first. Manual dispatch also requires the GitHub Release to exist, since the checksum asset lives on it.
- Permissions: `build_cli` holds `id-token`, `attestations`, and `artifact-metadata` write for attestation; `publish_cli` holds `contents: write` only for `gh release upload` of the checksum file. `release.yml` grants the union to the `cli` job and `deploy_cli.yml` narrows per job. The checksum upload runs before any bucket upload so a tag never exists on R2 without its GitHub checksums.

Release workflow:
- `.github/workflows/deploy_cli.yml` builds `late-cli` release artifacts
- `deploy_cli.yml` triggers on published `*-cli` GitHub Releases and also supports manual `workflow_dispatch` with `release_tag` and `environment` inputs. Manual dispatch is the recovery path when GitHub misses a release event; it must be dispatched on the tag itself (`gh workflow run deploy_cli.yml --ref <tag> -f release_tag=<tag>`), see "Supply chain".
- Linux CI/release jobs install `libwebkit2gtk-4.1-dev` to build the `late-webview` helper crate. `late-cli` itself does not need WebKitGTK dev packages on Linux (`cargo build -p late-cli` works without them).
- Linux glibc release artifacts are two binaries per target: `late` plus the `late-webview` helper, uploaded and checksummed together; `install.sh` installs both into the same directory (helper download is warning-only so older releases still install). Android/Termux, macOS, and Windows remain single-binary.
- Desktop release artifacts include native LiveKit voice media on Linux, macOS, and Windows. Keep Windows MSVC release builds on the static CRT (`crt-static`/`/MT`) because LiveKit's bundled WebRTC objects are built that way. macOS release builds depend on the two `build.rs` darwin link args (see "macOS voice link requirements" below).
- Publishes versioned releases plus `latest`
- Publishes `install.sh` and `install.ps1` at the distribution root
- Uploads `sha256sums.txt` to the GitHub Release and `<binary>.sigstore.json` provenance bundles next to every binary on R2 (see "Supply chain")

Version stamping:
- The release tag is the single source of truth for the CLI version. `deploy_cli.yml`'s `build_cli` job exports `LATE_CLI_VERSION=<tag>`, and `late-cli/build.rs` embeds it via `cargo:rustc-env` so the binary version matches the published `VERSION` file (`publish/VERSION`, `publish/latest/VERSION`) byte-for-byte. Local/dev, Nix and CI test builds fall back to the `Cargo.toml` version, so nothing needs to be set for `cargo build`, and a Nix `late --version` always prints `late 0.1.0` whatever revision it was built from.
- `late --version` / `late -V` prints `late <version>` (`config::VERSION`). The published `VERSION` file carries a trailing newline; the update check `trim()`s the fetched body. No manual `Cargo.toml` version bumps are required per release.

macOS voice link requirements (`build.rs`, `apple-darwin` targets only):
- `-ObjC` on the `late` binary and on test binaries. LiveKit's static `libwebrtc.a` carries ObjC categories (`+[NSString stringForAbslStringView:]` and friends) that the linker drops without it, so the CLI links clean and then aborts on an uncaught `NSException` the first time LiveKit builds its video encoder factory, which happens during peer-connection setup even for our audio-only rooms. `webrtc-sys` and `livekit` both emit this flag from their own build scripts, but `cargo:rustc-link-arg` does not propagate to a downstream crate's link (rust-lang/cargo#9554), so `late-cli` has to emit it itself. Upstream tracking: livekit/rust-sdks#795.
- `-sectcreate __TEXT __info_plist macos/Info.plist`, which carries `NSMicrophoneUsageDescription`. Without that Mach-O section macOS aborts the process on first microphone access, and abort bypasses `RawModeGuard::drop`, leaving the terminal in raw mode with the privacy exception printed as one long line.

#### Update check (`src/update.rs`)

`update::check_for_update()` runs once early in `main()`, before identity/raw-mode/SSH, so the nag prints in normal cooked-terminal mode before the TUI takes over. It is best-effort and fail-open:
- No-op on unstamped local/dev builds (`config::VERSION == env!("CARGO_PKG_VERSION")`) so source/Nix/cargo builds never nag, and no-op when `LATE_NO_UPDATE_CHECK` is set.
- Fetches `{LATE_INSTALL_BASE_URL or https://cli.late.sh}/VERSION` with a 2s timeout. Any network/status/parse error is logged at debug and ignored (startup continues).
- `sanitize_version` guards against a misconfigured host returning HTML/junk. `is_outdated` compares the numeric dotted core after stripping a leading `v` and trailing `-cli`, falling back to plain string inequality when either side isn't cleanly numeric.
- When behind, prints an aligned per-platform install block (linux / macos / windows / nixos) built by `nag_lines`, then pauses 5s (`NAG_PAUSE`) before connecting anyway. Install commands are defined once in `install_methods` (curl installer for linux/macos, `irm` for windows, `nix run github:mpiorowski/late-sh#late` for nixos). The check matches the binary's stamped version against the same `VERSION` file `publish_cli` writes, so byte-for-byte equality means up-to-date.

Nix flake outputs:
- `packages.${system}.late` builds the `late` and `late-webview` binaries and sets `mainProgram = "late"`
- `apps.${system}.late` runs that CLI package for `nix run ...#late`
- `packages.${system}.late-sh` remains the default multi-binary package with `mainProgram = "late-ssh"`
- On Linux, the Nix package builds with WebKitGTK 4.1, GTK3, ALSA, glib-networking, and GStreamer base/good/bad/ugly/libav plugins. The GStreamer path uses `gstreamer.out`, and `gst-plugins-bad` is overridden with `-Dlv2=disabled` to avoid `libgstlv2.so` crashes during plugin scanning. The flake's `late` package builds both the `late` and `late-webview` binaries; the installed binaries are wrapped with a fixed `GST_PLUGIN_SYSTEM_PATH_1_0`, `GST_PLUGIN_SCANNER`, `GIO_EXTRA_MODULES`, and `LATE_WEBKIT_GSTREAMER_SANDBOX_PATHS`; on Linux the webview helper adds those GStreamer store paths to WebKitGTK's web-process sandbox before creating the webview.
- On Linux, `postFixup` adds `libpulseaudio` and `xorg.libX11` to the `late` binary's rpath with `patchelf --add-rpath`, before `wrapProgram`. WebRTC's voice ADM `dlopen()`s both from `late` itself: `libpulse.so.0` for the device, then `libX11.so.6` from the same `AudioDeviceLinuxPulse::Init()` for typing detection (`XQueryKeymap`). The linker records neither, fixup's shrink-rpath would drop them, and NixOS has no `/usr/lib` fallback. They fail differently. Without libpulse, voice join fails with `PlatformAudio: failed to acquire Platform ADM`, the CLI reports `voice_state { joined: false }`, and the server removes the user from the room, which reads as an instant kick. Without libX11, the lazy-load trampoline that `webrtc-sys` compiles into the binary prints `implib-gen: libX11.so.6: failed to load library` and calls `abort()`, which kills the whole CLI mid-session and skips `RawModeGuard::drop`, leaving the terminal in raw mode. Once the library loads, a NULL display is handled, so headless and Wayland-only hosts need nothing further. The other lazily loaded libraries (`drm`, `gbm`, `Xfixes`, `Xdamage`, `Xrandr`, `Xcomposite`, `Xext`, `va`) serve desktop capture and VAAPI, which the CLI never calls. Music is unaffected because cpal links `libasound` directly. Both go on the rpath rather than a wrapper `LD_LIBRARY_PATH` so child processes (`late-webview`, system `ssh`) do not inherit them.
- `default.nix` predeclares LiveKit's `webrtc-51ef663` WebRTC zip for all four voice-capable systems (`x86_64-linux`, `aarch64-linux`, `x86_64-darwin`, `aarch64-darwin`) and exports `LK_CUSTOM_WEBRTC` during the Cargo build. This keeps `webrtc-sys` from trying to download WebRTC from GitHub inside the Nix sandbox. All four archives unpack to the same `{triple}/` layout, so `preBuild` asserts the same three files everywhere. Darwin builds also get `xcbuild` in `nativeBuildInputs` because `webrtc-sys` shells out to `xcrun` for the macOS SDK path. Bumping `webrtc-sys` means re-fetching every archive's hash, not just the Linux pair.
