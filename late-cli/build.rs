use std::env;

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
}
