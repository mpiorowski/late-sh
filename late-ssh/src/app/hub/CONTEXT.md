# Hub Context

## Metadata
- Scope: `late-ssh/src/app/hub`
- Last updated: 2026-05-21
- Purpose: local working context for the Hub domain: global modal, leaderboard, dailies, shop, guide, and future event surfaces.
- Parent context: `../../../../CONTEXT.md`

## Scope

`late-ssh/src/app/hub` owns the global Hub modal opened with `Ctrl+G` and the cross-product domains surfaced inside it: Leaderboard, Shop, Dailies, Events, and Guide.

Hub is a cross-product domain surface. It may render Arcade, Rooms, economy, marketplace, and event information, but it must not own those runtimes. Arcade game state stays under `late-ssh/src/app/arcade`; Rooms/table runtime stays under `late-ssh/src/app/rooms`; generic chip earn/spend primitives stay in `late-core/src/models/chips.rs`. Hub-owned marketplace state and entitlement projections live under `hub/shop`.

Keep `mod.rs` declaration-only. Do not add `pub use` re-export layers.

## Source Map

- `state.rs`: selected Hub tab and tab cycling.
- `input.rs`: Hub-only key routing (`Tab`/arrows cycle, `1-5` jump, `Esc/q` close).
- `ui.rs`: modal frame, tabs, footer, and tab dispatch.
- `leaderboard.rs`: compact leaderboard panels.
- `dailies.rs`, `events.rs`: placeholder product surfaces.
- `shop/`: Hub-owned marketplace domain.
  - `catalog.rs`: Shop categories and SKU helpers.
  - `entitlements.rs`: lightweight owned-feature projection for render/input gates.
  - `svc.rs`: `ShopService`, per-user watch snapshots, purchase tasks, and Postgres LISTEN/NOTIFY refresh listener.
  - `state.rs`: selected category/item, snapshot/event drains, and purchase activation.
  - `input.rs`: Shop-only item/category/buy input.
  - `ui.rs`: Shop tab rendering.
- `guide.rs`: user-facing guide for chip earning and leaderboard rules.
- `svc.rs`: `LeaderboardService`, a shared watch-backed leaderboard refresh task.

## Tabs

- `Leaderboard`: functional compact leaderboard view.
- `Dailies`: placeholder for daily puzzle status/streaks.
- `Shop`: functional first unlockable marketplace surface. Cat Companion is the first durable unlock.
- `Events`: placeholder for seasonal/monthly event surfaces.
- `Guide`: functional FAQ-style explanation of how chips and boards work.

If another tab is added, update `HubTab::ALL`, `HubTab::label`, `input.rs`, `ui.rs` dispatch, footer jump copy, and this file.

## Leaderboard Data

`hub::svc::LeaderboardService` refreshes `LeaderboardData` from DB every 30 seconds and publishes it through a `watch::Receiver<Arc<LeaderboardData>>`.

Current compact boards:
- `Top Chips`: monthly positive chip earnings from `chip_ledger`, excluding `floor_restore`. Spending does not reduce this rank.
- `Arcade Wins`: monthly weighted daily-puzzle completions across Sudoku, Nonogram, Solitaire, and Minesweeper.
- `Tetris`, `2048`, `Snake`: each score-game panel shows monthly score events and all-time high scores.

Monthly windows use UTC calendar months. Score all-time boards persist.

## Economy Rules

Current user-facing chip amounts:
- New chip rows start at 1,000 chips.
- Table losses can restore users to the 100-chip floor.
- Daily puzzle completions pay once per solved daily board:
  - easy / solitaire draw-1: 50 chips
  - medium: 150 chips
  - hard / solitaire draw-3: 500 chips
- Bonsai watering pays 200 chips once per day when the daily care row changes from unwatered to watered.
- Blackjack and Poker chips move through bets and pots.
- Tic-Tac-Toe currently publishes activity wins but does not pay chips.

`late_core::models::chips::difficulty_bonus` is the source of truth for daily puzzle chip payouts. Keep `guide.rs`, `dailies.rs`, root context, and Arcade context aligned when those constants change.

## Arcade Wins Scoring

The monthly Arcade Wins board is not a chip board. It awards points for daily puzzle completions:
- easy / draw-1: 1 point
- medium: 3 points
- hard / draw-3: 5 points

This scoring lives in `late-core/src/models/leaderboard.rs` SQL. Completing more hard dailies across more daily games is the intended path to win the board.

## Shop / Marketplace

Durable marketplace ownership lives here with the Hub domain context.

Implemented:
- `late-core` owns durable data models in `late_core::models::marketplace`.
- `marketplace_items` defines curated purchasable items; `user_purchases` records durable per-user ownership.
- Purchases debit `user_chips`, write `chip_ledger` with reason `shop_purchase`, then insert `user_purchases` in one transaction.
- `ShopService` publishes per-user `ShopSnapshot` values through watch channels. UI/input reads the current snapshot and does not query the DB per keypress/render.
- `ShopService::start_listener_task` opens a dedicated long-lived Postgres connection (outside the pool) and `LISTEN`s on `shop_user_changed` and `shop_catalog_changed` via `late_core::models::marketplace::listen_for_shop_changes`; all SQL stays in `late-core`. `shop_user_changed` carries a `user_id` payload and refreshes that user's snapshot when active; `shop_catalog_changed` refreshes every active user.
- The only `pg_notify` sender today is `purchase_durable_item_by_sku`, which notifies `shop_user_changed` inside the purchase transaction so it fires on COMMIT. The buyer's own snapshot is already updated by a direct `refresh_user` call, so LISTEN/NOTIFY is the cross-process / external-mutation path and is redundant in a single process. `shop_catalog_changed` has a listener and handler but no sender yet; it is reserved for a future admin/catalog-edit flow.
- Cat Companion is seeded as SKU `cat_companion` and costs 3000 chips. It gates the sidebar cat and the `c` cat-care launcher through `ShopEntitlements::has_cat_companion()`.

Future Shop work:
- Add a small curated set after the cat MVP: username flat color, title slot, starter badge, force-music vote consumable, mention sound variant, emoji slot remap.
- Keep user-provided free text and uploads out of MVP; use curated pools to avoid moderation load.
- Cosmetic render hooks should read purchase/equip state, not duplicate marketplace state in chat/profile/game modules.

Future Events work:
- Add `profile_awards(user_id, category, place, month, awarded_at)`.
- At UTC month rollover, snapshot top 3 per monthly category.
- Do not delete source ledger/event rows; monthly boards naturally re-window.
- Monthly placement should award permanent profile/status badges, not chip bonuses.

## Testing Guidance

- Pure state/input/layout helpers can have inline unit tests.
- DB/service behavior belongs in `late-ssh/tests/` and must use the shared testcontainers helpers.
- Root test policy applies: agents do not run `cargo test`, `cargo nextest`, or `cargo clippy`.

## Known Gaps

- `Dailies` and `Events` are still placeholders.
- Shop has only the Cat Companion unlockable; categories beyond Companions are not implemented.
- Leaderboard refresh is polling-based, so Activity events can appear before leaderboard panels catch up. Shop snapshots refresh on session init, purchase completion, and Postgres notifications.
- There is no paginated detail view yet; compact panels only show top rows plus an around-you tail where implemented.
- Profile-award snapshots are not implemented.
