# Sliding Puzzle local artwork

This directory is the trusted local-file root for Sliding Puzzle image sources. Image files are ignored by Git; only operator/developer-authored entries in `PLACEHOLDER_IMAGE_URLS` may reference them.

**This is a development affordance only.** No Dockerfile stage copies `artwork/` into a runtime image, so the directory exists solely because `docker-compose.yml` bind-mounts the repository at `/app`. A `file://` source added to a production build resolves nothing and falls back to numbered tiles.

For the Docker Compose preview:

1. Put a square image at `artwork/sliding-puzzle/preview-local.png` on the host.
2. Add `file:///app/artwork/sliding-puzzle/preview-local.png` to `PLACEHOLDER_IMAGE_URLS` in `late-ssh/src/app/arcade/sliding_puzzle/image.rs`.
3. Wait for `service-ssh` to rebuild, reconnect, and press `i`.

When running `late-ssh` directly from the repository root, use the corresponding absolute host URL, for example `file:///home/you/repos/late-sh/artwork/sliding-puzzle/preview-local.png`. Note that `LOCAL_ARTWORK_DIRECTORY` is relative to the process working directory, so launching from anywhere else will not find this directory.

Local paths are canonicalized and must remain inside this directory. Missing files, directories, symlink escapes, oversized files, and unsupported image data fail safely to numbered tiles. The existing limits still apply: 10 MiB encoded and 25 million decoded pixels.

**Use square artwork.** The native Kitty/iTerm2/Sixel renderer resizes exactly and accepts any aspect ratio, but the Chafa fallback fits by aspect ratio and will reject a non-square source with "image preview has unexpected dimensions" — so a non-square image works on capable terminals and shows numbered tiles everywhere else.

Replacing an image at the same path does not invalidate an already rendered session cache. Restart `service-ssh`, then reconnect; failed requests can also be retried by pressing `i` twice.

This path is for trusted curated/local artwork, not arbitrary paths submitted by SSH users. Production user submissions should use managed object storage plus moderation metadata.

## The shipped defaults are third-party

`PLACEHOLDER_IMAGE_URLS` currently hotlinks three signed `fastly.picsum.photos` CDN URLs. Lorem Picsum is a free placeholder service — Unsplash-sourced photos, no SLA, no published terms, no rate-limit policy — and the `hmac` query params come from their signing key. If they rotate it, all three 403 at once and every player drops to numbered tiles. Replacing these with licensed artwork embedded via `include_bytes!` (the pattern every other Arcade asset uses) would remove the runtime dependency, the per-session download, and this whole `file://` path.
