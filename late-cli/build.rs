use std::{env, path::PathBuf};

fn main() {
    // The published release tag is the single source of truth for the CLI
    // version. CI stamps it via LATE_CLI_VERSION so the binary always matches
    // the VERSION file published to cli.late.sh, with no hand-bumping of
    // Cargo.toml. Local builds and CI test runs fall back to the Cargo.toml
    // version so `cargo build` keeps working with nothing set.
    let version = env::var("LATE_CLI_VERSION")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| env::var("CARGO_PKG_VERSION").unwrap_or_default());
    println!("cargo:rustc-env=LATE_CLI_VERSION={version}");
    println!("cargo:rerun-if-env-changed=LATE_CLI_VERSION");

    println!("cargo:rerun-if-changed=macos/Info.plist");

    let target = env::var("TARGET").unwrap_or_default();
    if !target.ends_with("apple-darwin") {
        return;
    }

    // LiveKit's static libwebrtc.a carries ObjC categories (for example
    // `+[NSString stringForAbslStringView:]`) that the linker drops unless
    // `-ObjC` forces every category-only object file in. Without it the CLI
    // links fine and then aborts with an uncaught NSException the first time
    // LiveKit builds its video encoder factory, which happens during
    // peer-connection setup even though our rooms are audio-only.
    //
    // Both `webrtc-sys` and `livekit` already emit this flag from their own
    // build scripts, but `cargo:rustc-link-arg` does not propagate to a
    // downstream crate's link (rust-lang/cargo#9554), so the `late` binary
    // never saw it. Emitting it here is what actually reaches the linker.
    // Tests get it too, so a future mac test that touches LiveKit fails for a
    // real reason instead of an unrecognized selector.
    println!("cargo:rustc-link-arg-bin=late=-ObjC");
    println!("cargo:rustc-link-arg-tests=-ObjC");

    // macOS refuses microphone access to a binary with no
    // `NSMicrophoneUsageDescription`, and it refuses by aborting the process.
    // Abort bypasses `RawModeGuard::drop`, so the terminal is left in raw mode
    // and the privacy exception prints as one long line.
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let plist = manifest_dir.join("macos").join("Info.plist");

    println!(
        "cargo:rustc-link-arg-bin=late=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        plist.display()
    );
}
