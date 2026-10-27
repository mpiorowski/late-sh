# Artboard Gallery Context

## Metadata
- Scope: `late-ssh/src/app/artboard/gallery`, plus `late-core/src/models/artboard_piece.rs` and the `artboard` arm of `late-core/src/models/profile_award.rs`.
- Purpose: hung pieces of the live board: framing, hanging, applause, take-downs, the month's award, the login splash wall, Sliding Puzzle's tiles, moderation.
- Parent context: `../CONTEXT.md` (the board, its rail and input model).

## Files

Same file roles as the parent slice:

- `frame.rs`: pure. `frame_piece(canvas, provenance, bounds, username)` crops the frame into a `FramedPiece` (own canvas, cropped provenance, glyph count, own share, credits, content hash) or a `FrameError` with its notice.
- `svc.rs`: `GalleryService` (db, `None` in a `disabled()` service, + the process-wide splash canvas `watch`), spawned listing/hang/applause/take-down tasks reporting `GalleryResult`, `refresh_splash` / `start_splash_refresh_task` (assigns and caches the day's piece), `splash_piece_for_login` (the disclaimer gate plus a fresh day and removal check at authenticated login; see §Gallery). Owns the gallery's logs and metrics.
- `state.rs`: `GalleryState`: rail rows and focus, the four listings, the hang flow (`HangFlow`), notices, the draw-published rects for hit tests, `tick()` draining results.
- `input.rs`: keys and mouse while the gallery claims input, the archive list in the rail included; returns `GalleryAction` (`FocusBoard` / `BeginHang` / `OpenArchive(kind)` go back to `page.rs`).
- `ui.rs`: the rail, the listing pane (list + preview), the full-frame piece, the hang modal, the framing bar, `draw_splash_piece`.

## The gallery

What it is: a piece is an immutable crop of the live board, taken when the hanger frames it. The monthly wipe and the next vandal cannot touch it. Pieces gather applause; the month's best pieces win the `artboard` profile award and chips.

The rail (view mode): `Board` (the live board or the archive being viewed), a GALLERY group (`Latest` newest first across months, `Ranking` this month's pieces by applause, `Hall of fame`, `Mine`), `Hang a piece`, then an ARCHIVES group last (`Daily`, `Monthly`, `Curated`), in a `RAIL_WIDTH + 1` column whose right edge is the full-height rule every page rail has. The gallery rows and `Hang a piece` exist only when the service has a database (`GalleryService::is_enabled`). The rail shows while it, a listing, a piece, or an archive list has focus, and folds away when the board takes the keys (Enter on `Board`, `i`, a board click, framing); Esc on the board brings it back. The page lands on the rail with `Board` selected. The numbers next to the rows are there on entry: `ArtboardPiece::listing_counts` (one query, no canvases; refreshed after a hang) for the gallery rows until a listing loads and its own length takes over, and the archive key lists, requested for all three kinds in `State::new`.

Focus (`gallery::state::Focus`): `Rail` (the landing), `Canvas` (the board cursor), `List` (a listing's pieces), `Piece` (one piece full frame), `Archive` (an archive kind's key list drawn in the rail's place). Esc on the board focuses the rail; Enter on a gallery row moves into its list; Enter on an archive row turns the rail into that kind's list; Esc walks back out to the rail; Esc on the rail is not the page's (global Esc); Tab backs out of a pane to the rail (on the rail and the board Tab stays the page switch). There is no `g` key. Arrow keys follow focus. Mouse: a click on a rail row selects and activates it, a click on a list row selects it, the wheel scrolls the list.

Archives from the rail: the list shows keys only (one summary query per kind, fired on entry, newest first, the count on the rail row); the key under the cursor loads in the background and replaces the board as it lands (`Mode` reads `snapshot`, the Board row's tail reads `archive`, the list marks the loading key with `…` and the one on the board with `•`). Enter on a key moves focus to the board with the archive up; Esc goes back to the rail rows and leaves the archive up; the Board row returns to live.

Hanging (`HangFlow`): `Hang a piece` (the rail is the only way in) runs `App::begin_artboard_hang`: the artboard ban gate, a database behind the gallery, and "not an archive". Then `Framing`: Shift+arrows or a left drag select on the board, Enter runs `State::frame_selection_for_hang` (`frame_piece`), Esc cancels. A refused frame stays in framing with the reason on the framing bar. Then `Confirm`: the modal shows the crop, its numbers, and the credits; type a title (`PIECE_TITLE_MAX_CHARS` = 40), Enter hangs, Esc cancels. `Submitting` waits for the row. A landed hang selects `Mine` and says so in the pane's notice line.

Local rails (`frame.rs`, from `late_core::models::artboard_piece` constants): at least `PIECE_MIN_GLYPHS` = 40 non-blank glyphs, at most `PIECE_MAX_WIDTH` x `PIECE_MAX_HEIGHT` = 120 x 50, at least `PIECE_MIN_OWN_SHARE_PERCENT` = 75 of the glyphs painted by the hanger per cell provenance (wide glyphs count once, at their origin; one whose second half is outside the frame is not in it). SQL rails (`ArtboardPiece::hang`): `PIECE_DAILY_CAP` = 3 per UTC day, counted in the insert's own guard, and `UNIQUE (content_hash, period_month)`: the same glyphs at the same relative positions, colours ignored, cannot hang twice in a month. Copy theft beyond that is a mod's call (`/mod artboard remove`).

Applause: `v` on a piece in a list or full frame. One per person per piece (`artboard_piece_votes` PK), free, `v` again withdraws it, never on your own piece (CHECK on the denormalized `author_user_id`, and the state refuses before the round trip). One applause in flight per session. **A month closes at the rollover**: `toggle_applause` refuses applause and withdrawals alike on a piece whose `period_month` is past (`ApplauseOutcome::Closed`), so the counts the award was minted from never move again; the hall of fame reads live applause and stays in step with `ART1`-`ART3` because of it. `gallery::state::applause_refusal` says the same thing locally first.

Taking a piece down: `x` on your own piece in a list or full frame asks (`take_down_asked`), `x` again on the same piece sends `ArtboardPiece::take_down`, any other key withdraws the question (`forget_take_down_question` at the top of the gallery's key dispatch; moving the cursor or leaving the pane does too). Owner and month are in the row's own `UPDATE`: only the hanger, only while the piece's month runs (`TakeDownOutcome::{NotYours, Closed}` otherwise, `gallery::state::take_down_refusal` locally first). Soft delete (`removed_at`, migration 175): the row stays, so the daily cap still counts it and `UNIQUE (content_hash, period_month)` still refuses the same cells this month; `PIECE_VIEW_SQL` and the count queries see only rows with `removed_at IS NULL`, the award arm filters it too. A landed take-down drops the piece from every loaded listing, asks again for any listing still in flight (its answer may have been read before the piece came down), and refreshes the rail's counts. Every listing request carries the section's `generation` counter and `tick()` lands only the answer to the latest request, so a slow listing can never put a taken-down piece back. Only a mod takes down somebody else's piece, or a settled month's.

Month end (`late-core/src/models/profile_award.rs`): the `artboard` category ranks hangers by their best piece's applause over their pieces of the month (`period_month`, the UTC month hung), only pieces at or over `GALLERY_AWARD_MIN_APPLAUSE` (3). The rank is `ROW_NUMBER` over applause then earliest hang, never `RANK`: this is the one arm that mints chips, and a tie must not pay two first prizes; the tiebreak is the one the hall of fame uses. Ranks 1-3 print `ART1`-`ART3` and pay `gallery_prize_chips` (40,000 / 15,000 / 10,000) as `ChipMove::ArtboardPrize` (off Top Chips, like every monthly prize: `counts_as_earnings`) inside the snapshot transaction, keyed off the insert's `RETURNING` rows. **The month is settled once**: this is the arm that pays, so it must complete exactly once however many passes run (the 24h fallback, a restart, another replica). `toggle_applause` closes the month at the rollover, but a mod removal still moves the ranking afterwards, and `ON CONFLICT DO NOTHING` alone would let a hanger who climbed into the top 3 on a later pass get a fresh row and a fresh prize; the snapshot insert carries `NOT EXISTS (row for this category and period)` for every board, so once any `artboard` row exists a later pass inserts nothing and pays nothing. Applause is therefore counted once, by the first pass within the hour after the rollover.

Where a piece shows up beyond the page:
- The splash. The wall assigns every hung piece one UTC day, in hang order, the day after it was hung at the earliest (`ArtboardPiece::splash_for_day`, migration 195). The partial unique index on `splash_on` handles concurrent claims; a removal leaves its assigned day empty. The herald posts the day's piece on its art days through the same claim (`late-ssh/src/app/herald/CONTEXT.md`), so the wall is also what late.sh's accounts show. `GalleryService::refresh_splash` fills a process-wide canvas `watch` at startup and hourly and records queue depth. Both authenticated session builders (`session_bootstrap.rs` and `ssh.rs`) call `splash_piece_for_login` with this device's `artboard_disclaimer` (the key's, else the account's): while it is on (the default, so every newcomer and anyone who never pressed `A`) the door is the coffee cup; only with it off does the day's piece show. A lightweight fresh read (`ArtboardPiece::still_splash_on`) checks the cached piece is still up and still today's, so a take-down leaves new logins at once. Yesterday's cached art never shows today: when the cache still holds an earlier day's piece (the hourly refresh has not crossed UTC midnight yet), the login runs `refresh_splash` for today first, which assigns and publishes today's piece, so the first login of the day shows the new piece and not the cup. An unavailable cache, failed DB check, empty day, small terminal or held haunt door falls back to the cup. The splash remains post-authentication. Its caption counts days since hanging; the monthly awards and hall of fame are independent.
- Sliding Puzzle's tiles: the most applauded piece not yet featured, hung before the day, claimed once per UTC day by the first board opened (`ArtboardPiece::feature_for_day` stamps `featured_on`, migration 188, unique among pieces still up, so a removal frees the day and pieces hung the same day queue one per day). `/mod artboard feature <id-prefix>` pins a piece for today at once (`ArtboardPiece::feature_now`), skipping the queue and the hung-before-today rule, which is how a piece is checked on the board the day it was hung. The cut and the fallbacks are the Arcade's (`arcade/GAMES.md`, Daily puzzles).
- The profile's Artboard gallery line (`ArtboardPiece::counts_for_user`); chat labels and the badge legends through the award machinery.

Moderation: `/mod artboard remove <id-prefix> [reason]` (`RESTORE_ARTBOARD` cap; the first 13 characters of the id are printed on the key line of the full-frame view, `gallery::ui::piece_id_prefix`; at least `PIECE_ID_PREFIX_MIN_CHARS` = 8 characters; must match exactly one piece still up; the same soft delete as the hanger's, any owner, any month; applause rows stay, audit row keeps the title).

Telemetry: `record_gallery_hang(GalleryHangResult)` (hung / daily_cap / duplicate / failed), `record_gallery_applause(GalleryApplauseResult)` (applauded / withdrawn / own_piece / not_found / closed / failed), `record_gallery_take_down(GalleryTakeDownResult)` (taken_down / not_found / not_yours / closed / failed), and `record_gallery_splash_queue_depth` (gauge `late_ssh_artboard_gallery_splash_queue_depth`, pieces waiting for a day on the splash wall, from the hourly refresh), all from `svc.rs`; failures log through `late_core::error_span!`. Time and arrivals on the page split by `place`: `canvas` (the board) or `gallery` (a section's list or one piece, `GalleryState::shows_gallery_pane`), in `late_ssh_attention_seconds_total` and `late_ssh_place_visits_total` (`tick.rs::attention_place`).

Tests: `frame_test.rs` (crop, credits, hash, the three local rails), `state_test.rs` (rail rows and focus, the hang flow's title rule), `ui_test.rs` (the splash caption's day count), `late-core/src/models/artboard_piece_test.rs` (applause rules, daily cap and duplicate in SQL, mod lookup and soft removal, the hanger's take-down scoped by owner and month with the cap and the duplicate rail still counting the row, the closed month refusing applause, the splash wall's claim: hang order, one per day, two replicas, a removal leaving the gap), `state_test.rs::applause_and_take_down_refuse_before_the_round_trip`, `svc_test.rs` (the refresh publishes the day's piece, every login shows it, a service with no database publishes nothing, the disclaimer gate, a take-down leaving new logins at once, the first login after UTC midnight refreshing the wall), `late-core/src/models/profile_award_test.rs::the_gallery_award_ranks_best_pieces_and_pays_once`, `app/input_flow_test.rs::artboard_gallery_hangs_a_framed_piece_from_the_rail` (paint, rail, frame by drag, a title made of global hotkeys, hang, `v` applause reaching the gallery, `x` asking then taking the piece down, back out), `moderation/command_test.rs::parses_artboard_gallery_commands`.

Not done: the web `/gallery` page does not list pieces. Each piece still up has its own page and PNG on the web (`/gallery/pieces/{id}`, `late-web/CONTEXT.md`), and the herald posts each piece once, newest first within a week of its hanging (`late-ssh/src/app/herald/CONTEXT.md`); a take-down 404s the page at once, but a post already out is removed by hand from the herald's accounts.

Splash gate coverage: `session_bootstrap_test.rs` and `ssh_test.rs` log in through both session builders as a newcomer (no setting), with the disclaimer on, and after Always View; only the last sees the piece.

## Local gallery fixtures

`make seed-artboard` uses `scripts/seed_artboard_test_data.{sh,sql}` against the
local Compose Postgres, after the current app has applied its migrations. It
reuses the leaderboard seeder's namespaced-account convention without invoking
the game-stat seeder. Eleven accounts have stable `seed:artboard:v1:` identities
and retained SSH keys under the gitignored `tmp/artboard-seed-keys/`: artists
`art_artist1`–`art_artist3`, voters `art_voter1`–`art_voter4`, moderators
`art_mod1/2`, and admins `art_admin1/2`. Account preferences survive reruns.
The tutorial is marked completed on initial creation and every rerun so test
sessions skip the first-visit walkthrough.
The seeder also joins each fixture account to public auto-join rooms, including
`#lounge`, using signup's active-ban exclusion and preserving existing read
cursors. This makes the Home chat composer and `/mod` available to fixtures.

The twelve numbered 32×12 canvases have distinct ASCII patterns, full artist
provenance, actual content hashes, and enough glyphs to pass framing. Pieces
01–11 predate today UTC; 12 is freshly hung. The first three have 6, 5, and 4
applause. All drawings are harmless.

| Piece | Scenario |
| --- | --- |
| 01 Checkerboard | Default splash |
| 02–11 | Earlier pieces, queued for the splash wall; any can be selected |
| 12 Fresh Ladder | Hung today, ineligible for today's splash |

`ART_SPLASH_PIECE=1` (1–11) selects the day's fixture while preserving any
non-fixture holder, even if taken down. A rerun restores fixture pieces,
applause and roles, including TUI test changes; other users and pieces are
untouched. Reopen the gallery after a seed. Restart `service-ssh` and reconnect
to load the selected splash canvas; the piece shows only on a device whose
content disclaimer is off (`A`, Always View, on that key, or an account seed of
off the key never overrode), every other login gets the cup. The dev profile forces admin privileges, so testing role-based permission
checks requires setting `Config::dev`'s `force_admin` to `false` locally before
rebuilding.
