# 0014. Linker: mold by Default, lld Override Documented

## Status

Accepted

## Context

The repository pins the **`mold`** linker for fast incremental link times. This
is configured two ways:

- `.mise.toml` installs `mold` (alongside `rust` and `cargo-nextest`) as a tool.
- `.cargo/config.toml` sets, for the GNU/Linux target,
  `rustflags = ["-C", "link-arg=-fuse-ld=mold"]`.

That committed config is great when `mise` has provisioned `mold`, but it breaks
builds in environments where `mold` is not installed (CI images, sandboxes,
contributors who skipped `mise install`): the link step fails because the
compiler is told to use a linker that is not on `PATH`.

We must not "fix" this by editing the committed `.cargo/config.toml` — that would
de-optimize the maintainers' setup and the canonical build.

## Decision

Keep `mold` as the committed default and **document a per-environment override**
rather than changing the committed file:

- Preferred: install `mold` (run `mise install`).
- Otherwise: prefix cargo commands with
  `RUSTFLAGS="-Clink-arg=-fuse-ld=lld"`, which causes the passed `RUSTFLAGS` to
  take precedence and link with `lld` instead. For example:

  ```bash
  RUSTFLAGS="-Clink-arg=-fuse-ld=lld" cargo build -p agentbbs
  RUSTFLAGS="-Clink-arg=-fuse-ld=lld" cargo run -p agentbbs -- tui
  ```

The README carries this note prominently. The rule is explicit: **never edit the
committed `.cargo/config.toml`** to work around a missing linker — use the
environment override locally.

## Consequences

**Positive**

- Maintainers and the canonical build keep `mold`'s fast links unchanged.
- Contributors and CI without `mold` have a one-line, non-invasive escape hatch.
- No churn or accidental commits to a shared build-config file.

**Negative / risks**

- Setting `RUSTFLAGS` on the command line **overrides** the project rustflags
  rather than merging, so anyone using the override loses any other flags the
  committed config might add later; if more flags are introduced, the documented
  override string must be updated to include them.
- It is easy to forget the prefix and hit a confusing link error; the README
  note is the mitigation.
- `lld` and `mold` can differ subtly; builds should be considered linker-portable
  but the canonical linker remains `mold`.

## Implementation

- Committed defaults (do not edit to work around missing linkers):
  `.cargo/config.toml` (`-fuse-ld=mold`), `.mise.toml` (`mold = "latest"`).
- Override documentation: `README.md` ("Linker note" and the build/test
  snippets using `RUSTFLAGS="-Clink-arg=-fuse-ld=lld"`).
