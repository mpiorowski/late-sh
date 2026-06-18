# irc-proto (vendored)

Vendored copy of the `irc-proto` crate from the `irc` repository.

- Upstream: https://github.com/aatxe/irc (subdirectory `irc-proto/`)
- Vendored from commit: `269b5179dd479c6ca025a3d8ce1f6f6dd26e232f` (2025-12-31)
- Upstream version: 1.1.0
- License: MPL-2.0 (see `LICENSE.md`)

Vendored so the late.sh embedded ircd (see `devdocs/FRD-IRCD.md`) can patch the
protocol layer in-tree (server-side numerics, mode quirks, codec tweaks) when
that is the cleanest implementation path. Keep local changes minimal and note
them below.

Like `vendor/potatis`, this crate is excluded from `make check`'s first-party
fmt scope.

## Local modifications

- (none yet)
