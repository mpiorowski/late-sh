# Profile modal Context

## Metadata
- Domain: the read-only profile modal (`/profile [@user]`, `/chips [@user]`, `p` on a chat author, avatar clicks)
- Status: Active

## 1. Shape

One layout, whatever the terminal. The body is a single column that scrolls:

1. **late.fetch** (a heading like every other section): the fact grid. No name (it is the modal's title). `country`, `local`, `chips` (balance only; the month's figures head the chips section), `gilds`, `gallery`, `badges` when non-zero, `created`, `member` (days since), `ide`, `os`, `terminal`, `theme`, `langs`. Unset values are dim, never absent. Beside the grid, against the right edge, the **runner** column, when the viewed user is a standing runner (`deadchannel_runners.left_at` unset) and the viewer is a runner too (`App::is_runner`, handed to `ui::draw` as `viewer_is_runner`; a civilian viewer sees the plain grid, so the wire stays a secret from outside the door). No frame anywhere: `RUNNER_WIDTH` (39: the portrait, two columns, the 32-column poster) columns under their own heading on the late.fetch heading's row, `runner` and the wire's badge (`▚7`, `▚7╬2`) in the level's band color (`level_color`), the late.fetch rule stopping `COLUMN_GAP` short of it. Under the heading, twelve rows: the three-row portrait at the top left, losing cells to static in proportion to the missing signal (`fight::ui::corrupt`, seeded by the user so the wound is the same on every open; the sheet is settled for the view, `ProfileService::do_find_profile` applying the lazy day roll to the parsed copy and writing nothing), and beside it the poster painted at PRINTS (`poster::ui::canvas_lines`, 32 by 12 on its own paper, the street's drawing). A runner with no poster shows the test card (`poster::state::test_card`, seeded by the user), and its owner (`ProfileModalState::viewing_own`, the session's user given at construction) reads one dim line under it, `PRINTS, west end, past the tailor.` The level and the marks are the badge; the rest of the sheet (signal, exp, bits, the kit, the tally) stays on the street. Below `BESIDE_MIN_WIDTH` (90) body columns the runner is a section of its own under the grid, the same heading and rows at full width. That and the reef-and-pet row (6) are the only reflows: a pair of columns stacking into sections. Contract in `late-ssh/src/app/deadchannel/CONTEXT.md`.
2. **bio** (markdown).
3. **showcases** (when any), right under the bio so a profile's projects read with it.
4. **bonsai**: never the preview, always the whole 81 x 26 canvas at its true size (`bonsai::render::canvas_lines_in`), full column width. A column wider than the canvas centers it; a narrower one cuts the same columns off both sides, so the trunk stays centered. It sways on the wall tick, which `draw` takes. An account with no tree yet shows "no bonsai yet". No caption.
5. **pet** (when the viewed user owns the Pet Companion and has no fish, or the body is narrow): the pet's three art rows (`pet::ui::portrait_lines`, blinking on the wall tick) in the mood its owner's session last wrote (`pet_companions.mood`; asleep once they log off), captioned `name · mood` (the art says which animal it is). The mood is inferred, never set, so it is an honest readout of how the owner's night is going.
6. **aquarium** (when the viewed user has fish): an 11-row reef band. When the user also owns the pet and the body is at least `BESIDE_MIN_WIDTH` (90) columns, the two are one row, not two sections: the reef under its `aquarium` heading, and against the right edge a `PET_WIDTH` (24, the longest pet name) column under its own `pet` heading on the same row, `COLUMN_GAP` (3) apart. The pet stands halfway down the band, the same three art rows, with its name and its mood on a row each so the longest name fits the column.
7. **badges** (every award, wrapped to the width by `badges::badge_lines`; never a "+N more").
8. **chips**: `balance · this month · net` (earned by the board rule, then every row summed), the dim-row note, then the newest `PROFILE_LEDGER_ROWS` ledger rows, one line each (`ledger.rs`): date, signed delta, a label per `ChipMove` (exhaustive), a detail when the ref means something to a person, and rows `ChipMove::counts_as_earnings` ignores rendered dim, detail kept. Details are resolved by `profile::ledger` (the service follows each ref into its table and hands the modal a closed `LedgerDetail`; the modal only writes the copy, one arm per variant): gift and gild counterparties, the game and milestone behind an arcade payout, who lost the crown, how many pot tickets a buy was or a win drew from, the quest's title, the gallery place and month, how many patrons a round reached, who a gifted drink was for, the song's title, drinks, SKUs, links, streak days. Blackjack, poker, and Super Snake rows have no table behind their ids, so they carry the label and the delta only. Anyone can open anyone's profile, so this is where a place on Top Chips is audited by the room.

The modal is as wide as the terminal allows up to 110 columns and as tall as the content needs up to the terminal height, so a short profile is a short card.

## 2. How it draws

`ui::draw` builds `Segment`s (text lines, the grid and the runner column side by side, the aquarium, the aquarium with the pet column beside it), sums their heights, composes them into an off-screen `Buffer` exactly that tall, and blits the rows `[offset, offset + viewport)` into the frame. The aquarium paints into its own fixed-size buffer keyed on the band's width (`aquarium::ui::draw_into`), so scrolling never rebuilds the reef. A scrollbar hugs the right border when the body is taller than the viewport.

The measured heights go back to the state as a `ScrollExtent` (interior-mutable, like `popup_area`), which is what clamps `scroll_by` and honours a pending `/chips` jump: `jump_to_chips` is a flag the next measured draw consumes by setting the offset to the chips section's row.

## 3. Data

`ProfileSnapshot` carries everything the modal shows; `ProfileService::do_find_profile` loads it in one pass, including `runner` (`ProfileRunner`: the parsed look, sheet, and poster of a standing `deadchannel_runners` row; a look or sheet that fails to parse is logged and shown as no runner, the directory's rule, and a poster that fails to decode is logged and shown as none, the test card), `chip_ledger` as resolved `LedgerRow`s (one batched primary-key lookup per source table, at most `PROFILE_LEDGER_ROWS` ids each) and `chips_month` (earned and net in one scan). The modal never queries.

## 4. Keys

`j`/`k`, arrows, wheel: scroll. `PageUp`/`PageDown`: page. `g`/`G`: top/bottom. `Esc`/`q`, or a click outside: close.

## 5. Tests

- `ui_test.rs`: a real profile rendered into a `TestBackend` at a wide and a short size; section order, the grid keys, the ledger rows (gift named, stipend last), the summary agreeing with the board rule, the `/chips` jump landing on the heading, and scroll clamping; the runner column beside the grid for a runner, to a runner viewer: none for a civilian viewer, the face and the test card, the owner's hint for the owner only, a painted poster in the card's place, the sheet rows gone, a section under the grid on a narrow terminal, and no column for a civilian profile; the pet beside the reef for an owner of both (two sections on a narrow terminal).
- `ledger_test.rs`: every reason has a label, every detail has copy, row layout per kind, clipping, thousands grouping. The resolution itself is tested in `profile/ledger_test.rs` (every pointer kind through `refs` and `resolve`, whole result asserted) and `profile/svc_test.rs` (the house rows and a payout through a real database).
- `badges_test.rs`: every badge is listed; wrapping never drops one.
