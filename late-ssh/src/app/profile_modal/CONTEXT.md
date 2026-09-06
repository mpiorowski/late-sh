# Profile modal Context

## Metadata
- Domain: the read-only profile modal (`/profile [@user]`, `/chips [@user]`, `p` on a chat author, avatar clicks)
- Last updated: 2026-09-06 (redesign: one scrolling layout for every terminal, no tabs, no dashboard; the bonsai is the neofetch logo beside the late.fetch grid; every badge is listed; the public chips ledger closes the column)
- Status: Active

## 1. Shape

One layout, whatever the terminal. The body is a single column that scrolls:

1. **late.fetch** (a heading like every other section). Two equal halves: the fact grid on the left, the bonsai as the neofetch logo on the right. The tree is fitted to the hero's height, which is the grid's height or `HERO_MIN_HEIGHT` (14), whichever is taller: a Dynamic Bonsai goes through `render_preview_lines`, which scales down to the box and never up; the classic sprite is cropped from the crown so the pot stays on the last row. No caption, and no name (it is the modal's title). The grid: `country`, `local`, `chips` (balance and this month's Top Chips figure, the board's own sum via `UserChips::earned_this_month`), `gilds`, `gallery`, `badges` when non-zero, `created`, `member` (days since), `ide`, `os`, `terminal`, `theme`, `langs`. Unset values are dim, never absent. Below 90 body columns the hero stacks: tree above grid, full width. That is the only reflow.
2. **bio** (markdown), **showcases** (when any), **badges** (every award, wrapped to the width by `badges::badge_lines`; never a "+N more").
3. **aquarium** (when the viewed user has fish): an 11-row reef band.
4. **chips**: `balance · this month`, the off-board note, then the newest `PROFILE_LEDGER_ROWS` ledger rows, one line each (`ledger.rs`): date, signed delta, a label per `ChipMove` (exhaustive), a readable detail when the ref means something to a person (gift counterparties and gild parties, the gilded message's author and buyers, resolved to usernames by `ProfileService`; drinks, SKUs, links), and an `off` marker on rows `ChipMove::counts_as_earnings` ignores. Anyone can open anyone's profile, so this is where a place on Top Chips is audited by the room.

The modal is as wide as the terminal allows up to 110 columns and as tall as the content needs up to the terminal height, so a short profile is a short card.

## 2. How it draws

`ui::draw` builds `Segment`s (text lines, the two-column hero, the aquarium), sums their heights, composes them into an off-screen `Buffer` exactly that tall, and blits the rows `[offset, offset + viewport)` into the frame. The aquarium paints into its own fixed-size buffer keyed on the band's width (`aquarium::ui::draw_into`), so scrolling never rebuilds the reef. A scrollbar hugs the right border when the body is taller than the viewport.

The measured heights go back to the state as a `ScrollExtent` (interior-mutable, like `popup_area`), which is what clamps `scroll_by` and honours a pending `/chips` jump: `jump_to_chips` is a flag the next measured draw consumes by setting the offset to the chips section's row.

## 3. Data

`ProfileSnapshot` carries everything the modal shows; `ProfileService::do_find_profile` loads it in one pass, including `chip_ledger`, `chips_earned_month`, and `ledger_usernames` for the gift rows. The modal never queries.

## 4. Keys

`j`/`k`, arrows, wheel: scroll. `PageUp`/`PageDown`: page. `g`/`G`: top/bottom. `Esc`/`q`, or a click outside: close.

## 5. Tests

- `ui_test.rs`: a real profile rendered into a `TestBackend` at a wide and a short size; section order, the grid keys, the ledger rows (gift named and marked off, quest unmarked, stipend last), the summary agreeing with the board rule, the `/chips` jump landing on the heading, and scroll clamping.
- `ledger_test.rs`: every reason has a label, row copy per kind, clipping, thousands grouping.
- `badges_test.rs`: every badge is listed; wrapping never drops one.
