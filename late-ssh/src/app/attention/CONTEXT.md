# late-ssh Attention Context

## Metadata
- Domain: the attention guest, the list of what waits on you that takes a Home tile while it has rows and hands it back when it does not
- Primary audience: LLM agents working in `late-ssh/src/app/attention` and on the pages that host it
- Status: Active
- Parent context: `../../../../CONTEXT.md`

---

## 1. What it is

A guest (`zen::state::Guest::Attention`, `../zen/CONTEXT.md`): invisible while nothing waits, the whole tile while something does, one row per thing. Every row is a fact the session already holds, and every row clears on the act that answers it. Nothing here fetches, polls or stores.

| Row | While | Clears on |
|---|---|---|
| your move: `game` vs `name` | a daily match's turn is yours (`DailyState::my_turn_matches`) | the move |
| result in: `game` vs `name` | a finished daily match you have not looked at (`DailyState::my_finished`) | opening the board |
| `@peer`: n unread | a DM room with unread messages (the Inbox tile's rows, `zen/rows.rs`) | reading it |
| `@actor` mentioned you in #room | an unread mention (the same rows, read ones left out) | reading it |
| the tank is unfed today | the reef is owned and `AquariumCare` has no meal today | `/aquarium feed`, `a` on the tank |
| the tree is unwatered today | `last_watered` is not today | watering |
| the pet is unpetted today | the pet is owned and not petted today | a pat |

Order: moves, results, DMs, mentions, tank, tree, pet. The care rows are keyed on the UTC day, so they come back each morning; ownership is gated by the glue, an unowned tank or pet is never due.

## 2. Files

| File | Role |
|---|---|
| `state.rs` | `Item` (the row, its `label`, `act` word and `glyph`), `Inputs`, `items(inputs)`: pure ordering, tested in `state_test.rs` |
| `ui.rs` | `draw`: the rows from the top of the tile, the selected one marked while the tile is focused, the act word at the row's end |

The glue is `App::attention_items` (`../state.rs`): it reads the domains' public surfaces (`DailyState`, `zen::rows::inbox_rows`, `App::care_today`) into `Inputs` once per frame (`render.rs`, into `ZenView::attention`) and once per key of the guest. The keys are `zen/input.rs::handle_attention`: `j` `k` move `ZenState::attention_selected`, Enter does the row's act through the page's global functions (`open_daily_modal_globally`, the Inbox row opener, `feed_aquarium_globally`, `BonsaiSession::request(Water)`, `pet_the_pet_globally`). They run where tile keys run: Zen, and Home in arrange mode; outside arrange mode Home's keys belong to its chat and the list reads only.

## 3. Invariants

- The guest is up exactly when `items` is non-empty; the page asks nothing else (`zen/ui.rs` `guest_up`).
- A stored `lobby` guest, the guest attention grew out of, reads as attention (`Guest::from_key`).
- The list never switches a layout, opens a modal or moves the focus on its own: only Enter on a row acts.
