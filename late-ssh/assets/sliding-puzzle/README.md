# Sliding Puzzle artwork

The daily pool is managed through **#puzzle-art**. Migration 182 seeds these three original AI illustrations into the database pool; their PNGs are embedded in the binary. Reviewed community images are stored in late.sh's configured S3/R2 storage.

| File | Title | Credit |
| --- | --- | --- |
| `night-terminal.png` | Night Terminal | late.sh · AI illustration |
| `rooftop-garden.png` | Rooftop Garden | late.sh · AI illustration |
| `night-train.png` | Night Train | late.sh · AI illustration |

## Contributing

1. Open **#puzzle-art** in Core and read `/rules`.
2. Post one direct public image URL, optionally with a title of at most 120 characters. Existing image uploads work too: paste/upload an image, or use `/paste-image` with a paired client, then send the resulting URL here. `/upload <url>` is also available.
3. Use a square PNG, JPEG or WebP, 256–4096 pixels per side and at most 10 MiB. The server verifies and decodes the image, produces a static PNG, and rejects oversized output. The channel displays the managed copy that the reviewer and puzzle will use.
4. Submit artwork you made or have permission to share and allow late.sh to host and feature. Your username at submission time is stored as its credit.
5. A maintainer reviews it and adds **👍** (`f`, then `1`). Only database-admin or moderator accounts can approve. A receipt confirms when it becomes eligible. Other users' reactions, **👎**, removing 👍, or changing it never alter pool membership.

Replies are for discussion and never become submissions. Top-level text without an image is rejected, including staff posts. Pending and approved submissions cannot be edited: delete and resubmit to change an image or title. Active duplicate image content is rejected. Deleting a submission retires it from future daily selections; it does not alter an already assigned day's image. Reactions on other channels have no effect on this pool.

## Inspecting the local dev flow

Dev profiles provide upload storage automatically in `tmp/uploads`; no credentials or external services are required. Reconnect after rebuilding so the dev account receives database admin permissions.

1. Connect with the paired desktop client:
   `late --ssh-target localhost --ssh-port 2222 --api-base-url http://localhost:4001`
   Audio is optional: press `m` outside the composer to mute; clipboard uploads still work.
2. Copy a square image to the image clipboard, open **#puzzle-art**, and send `/paste-image`. Once uploaded, optionally add a title to the URL and press Enter to submit. Terminals that send raw image bytes can also paste those directly into the composer. Plain SSH cannot read your computer's image clipboard.
3. Select the submission and press `f`, then `1` for **👍**. The receipt confirms approval.
4. Open Sliding Puzzle and press **`a`** to cycle the approved pool until the submission appears. This dev-only preview includes tomorrow's approvals and automatically enables daily image view. Each press loads the next approved image; pending entries are excluded. It changes only this session's image, leaving daily assignments and board progress intact.

## Daily rotation and operations

- Approval makes an image eligible from the **next UTC day**, not necessarily featured that day.
- Postgres stores one immutable assignment per date. The first image-view request for a date claims it transactionally, selecting the least recently featured eligible artwork; never-used images go first. Already-assigned adjacent dates cannot repeat the same entry, including when an old open board requests its image after midnight.
- Image mode is the default for daily and personal boards until the user chooses otherwise. The `i` selection is saved per account and restored on reconnect; automatic fallback never changes that preference. Numbered tiles remain playable while loading or if artwork fails; `i` switches views, and pressing it twice retries a failed image.
- All players and difficulties share the assignment. A board left open across midnight retains its date and image. Personal puzzles continue using the three embedded illustrations selected by their saved seed.
- No redeploy is needed to approve community artwork. The initial deployment must apply migration 182 and configure the existing `Config.files` S3/R2 credentials. Without upload storage, submissions fail with a useful message; built-in daily artwork remains available.
- Submitted images have content-addressed object keys (`puzzle-art/<sha256>.png`). The message and pending pool entry are published in one transaction after upload. Approval is a trigger in the reaction transaction, so concurrent reviewers cannot create duplicate approvals. Message deletion retires its entry; hosted copies remain available to existing daily assignments.
- Selection metadata is loaded asynchronously once per session/board date with retry on failure; image downloads use the shared SSRF-guarded loader and per-session render caches and a source-byte cache capped at four artworks. While loading or on failure, numbered tiles remain playable. No database or network I/O runs synchronously in the frame/tick path.
- Models and assignments: `late-core/src/models/sliding_puzzle_artwork.rs`. Submission validation: `late-ssh/src/app/chat/puzzle_art.rs`. Embedded assets and renderer source identity: `late-ssh/src/app/arcade/sliding_puzzle/artwork.rs`.

Targeted verification: `make test-llm ARGS="-p late-core -p late-ssh -E 'test(sliding_puzzle) | test(puzzle_art)'"`. Run the human-owned `make check` gate before opening a PR, as required by `CONTRIBUTING.md`.

## Generation prompts

Built-in imagegen was called separately for each image, without reference images. Full prompt template (replace `{subject}` with the corresponding subject below):

> Use case: illustration-story. Asset type: square artwork for late.sh's sliding tile puzzle. Create one original full-bleed square illustration, 1024x1024. {subject} Style: crisp richly colored editorial gouache illustration with readable silhouettes and subtle grain. Fill the entire image with varied recognizable details so each cell in a 3x3, 4x4, or 5x5 crop is distinct; avoid large empty or repetitive areas. No text, no lettering, no logos, no watermark, no grid, no puzzle pieces, no border.

- `night-terminal.png`: A cozy late-night computer desk beside a rain-streaked window: amber desk lamp, teal CRT screen, coffee mug, trailing plant, books, and a sleeping cat. Strong diagonal desk perspective, city lights outside.
- `rooftop-garden.png`: A lush rooftop garden at sunset: terracotta plant pots, little bonsai, winding copper watering can, patterned paving, distant violet rooftops, an orange sun behind clouds. Asymmetrical layered composition.
- `night-train.png`: A small midnight train station: warm yellow train curving from lower left towards an illuminated station on the right, blue mountains, starry sky, signal lights, foreground flowers and cobblestone platform. Asymmetrical composition.
