# SHOP.md, the chip sink roadmap

Status: **seed doc, decided design, phased for implementation.** Sibling of
GAME.md. Each phase below is one implementation session with a clean
context, followed by a separate verification session. Nothing in a later
phase is in scope for an earlier one.

## Why (the diagnosis)

Revenue data from the Shop says one thing clearly: the only items that sell
are **username effects** (glow / gradient / shimmer, 24h and 30-day) and
**bartender drinks**. Badges sold once per user and stopped. Room effects
(10-second flourishes) never sold. Ultimates at 10M chips never sold.

What the two winners share, and nothing else in the Shop has:

1. **Seen in chat, next to the name, in every message.** The gradient, the
   `(tipsy)` word. A thing visible only on the profile page or in a modal
   follows the aquarium's curve (GAME.md "Why").
2. **Renewable.** The effect expires (24h / 30d) or is consumed (a drink
   wears off in 12h). A permanent cosmetic is a sink that dies after one
   purchase per user.

And a third axis the current Shop misses entirely:

3. **Price set by competition or by the buyer's own ego, never by us.** The
   economy is deflated: whales sit on six-figure balances and a fixed price
   is either too low for them or insulting for a 3k player. Tiers, ratchets,
   and pots let the price find the wealth.

Every phase below satisfies 1 and 2; phases 2, 3, and 5 also satisfy 3.

North-star check, borrowed from GAME.md: **does it ship a story into
#lounge?**

## Hard rules (apply to every phase)

- **Chips never buy power.** Only visibility (cosmetics, markers) and
  stakes (tickets, the crown). No item in this doc changes any game outcome.
- **Every chip movement is a named `ChipMove` variant** in
  `late-core/src/models/chips.rs` (the `chip_moves!` roster). Adding one
  forces the reason string, the label, the direction/floor rule, and the
  `counts_as_earnings` decision. Never reuse `ShopPurchase` or `Credit` for
  a new mechanic.
- **Burns are the gap between a debit reason and a credit reason.** No house
  wallet, no chips held in limbo. Where a sink burns 100%, there is a debit
  and nothing else.
- **Everything is DB-backed and replica-safe.** No in-process state that a
  second SSH replica would disagree with. Cross-process refresh rides
  Postgres `LISTEN/NOTIFY` the way `ShopService` and the chip triggers do
  (`late-core::models::marketplace::listen_for_shop_changes`,
  `late-core::models::chips::listen_for_chip_changes`). Anything drawn or
  settled by a sweeper uses a status-transition `UPDATE ... WHERE status =
  'open' RETURNING *` so exactly one replica wins.
- **One identity system.** New name-adjacent visuals ride the existing
  flair pipeline (`late-ssh/src/app/common/username_effect.rs`,
  `NameFlairDirectory`, resolved once a second in `tick.rs` into
  `App.name_styles`) or the chat label query in
  `late-core/src/models/user.rs` (around line 578, where `chat_flag` /
  `chat_badge` come from). Never a second directory, never a second wallet.
- **Feed lines go through `ActivityKind`** (`late-ssh/src/app/activity/`),
  and `filter::lounge_includes` is the one exhaustive match that decides
  what ships. A new kind must add an explicit arm there. Bodies never
  contain `@`.
- **Composer commands are registered in `late-ssh/src/app/chat/commands.rs`**
  (the `global(...)` list) and documented in
  `late-ssh/src/app/help_modal/data.rs`. Both, every time.
- **Catalog edits are migrations** (`late-core/migrations/NNN_*.sql`,
  `ON CONFLICT (sku) DO UPDATE` shape as in `112_seed_username_effects.sql`).
  Retire SKUs by `active = false`, never delete: `user_purchases` and
  `shop_consumable_effects.source_sku` keep history.
- **GUIDE.md governs the code.** In particular: tests beside the file
  (`foo_test.rs` + `#[cfg(test)] mod foo_test;`), no defaults that choose
  behavior, `match` with one arm per outcome, closed enums, telemetry at the
  orchestration layer, lowercase log strings, no em dashes anywhere, no
  `git commit`.
- **Each phase ends with its CONTEXT.md updates**: the domain file it
  touched (`late-ssh/src/app/hub/CONTEXT.md` for Shop items,
  `late-ssh/src/app/chat/CONTEXT.md` for chat surfaces, and so on) and the
  routing/summary lines in the root `CONTEXT.md` if a new surface or table
  appeared. Stale context is a bug.

## Fixed numbers (decided; change here, not in code comments)

| Dial | Value |
|---|---|
| Month tier price | 30x the day tier (existing convention from migration 146) |
| Badge rental, basic | 100 / 24h, 3,000 / 30d |
| Badge rental, premium | 250 / 24h, 7,500 / 30d |
| Flag rental | same as basic badge |
| Title rental, curated | 200 / 24h, 6,000 / 30d |
| Title rental, custom | 2,000 / 24h, 60,000 / 30d |
| Title max length | 20 characters |
| Gild tiers | 500 / 5,000 / 50,000 |
| Gild split | 2/3 to the author (`GildReceived`), 1/3 never re-minted |
| Gild feed threshold | a message's 3rd gild, once |
| Crown minimum price | 5,000 |
| Crown ratchet | next price = max(5,000, ceil(paid x 1.5)), 100% burned |
| Crown hold | 30 minutes before a fresh reign can be taken |
| Crown reset | UTC month boundary; month-end holder gets a `profile_awards` row |
| Burn milestones | 100,000 / 500,000 / 1,000,000 |
| Pot ticket | 100 chips |
| Pot per-user cap | 50 tickets per pot |
| Pot payout | 80% of ticket sum to one ticket-weighted winner; 20% never re-minted |
| Pot draw hour | 21:00 UTC, one constant |
| Pot threshold lines | 50,000 and 100,000, once each per pot |

## Process

- One phase per implementation session (Claude Opus 5), started with a
  clean context and this file plus GUIDE.md and the relevant CONTEXT.md
  files. The session implements exactly the phase's scope, runs the
  targeted tests with `make test-llm ARGS="<filter>"` (never raw `cargo
  test`: the capped runner exists because full builds can freeze the
  machine), updates the CONTEXT.md files, and ends with a written report:
  files touched, tests run with their output, and every deviation from this
  doc with the reason.
- A separate verification session (Claude Fable 5) reads the diff against
  the phase's acceptance list below. Verification is a review: it reads
  and reasons, it does not run builds or tests (GUIDE.md "Reviews").
  Findings go back to a fresh implementation session for the fix.
- The owner runs `make check` and commits. Sessions never commit.
- Phases are ordered; do not start N+1 while N has open findings.

## Phase 0: badge and flag rentals

Goal: badges and flags become the username-effect shape (rent for 24h or
30d, rebuy replaces and resets the clock), so the largest catalog category
turns into recurring spend.

Before designing, read how a badge is bought and shown today:
`late-core/src/models/marketplace.rs` (`item_kind = 'badge'`, `slot =
'chat_badge' | 'chat_flag'`, `user_purchases`), the chat label query in
`late-core/src/models/user.rs` (~578), and the Shop's Badges/Flags tabs in
`late-ssh/src/app/hub/shop/`.

Behavior:
- New `item_kind = 'badge_rental'` items with payload
  `{ "emoji", "slot": "chat_badge" | "chat_flag", "tier", "duration_secs" }`,
  one day SKU and one month SKU per legacy badge/flag (`badge_cat_day`,
  `badge_cat_month`, ...), month listed directly under day by `sort_order`
  as in migration 146.
- Buying writes a user-scoped `shop_consumable_effects` row with
  `effect_kind = 'chat_badge'` or `'chat_flag'`, `room_id IS NULL`,
  `ends_at = now + duration_secs`, deactivating any live row of the same
  `effect_kind` for that user (one active badge, one active flag; a rebuy
  across tiers replaces). Mirror `activate_username_effect_in_tx`.
- The chat label query prefers a live rental row over the legacy permanent
  purchase, so existing owners keep what they bought until they rent
  something over it. Legacy permanent badge/flag SKUs go `active = false`.
- Shop copy quotes the duration through one reader the way
  `username_effect_duration_secs` does; the detail pane shows the active
  row and remaining time; `ShopState::tick` prunes at `ends_at`.
- Distribution: badges are read by the label query, not the flair
  directory, so a purchase must invalidate whatever caches that query's
  result (check how `chat_badge` reaches the renderer and where the
  per-user `shop_user_changed` notify already refreshes).
- Purchases announce through the existing `UsernameEffectApplied` shape or
  a new `BadgeRented` kind; either way the lounge filter gets an explicit
  arm. Recommendation: a new kind, the old one's copy is specific to name
  effects.

Acceptance:
- [x] Migration seeds day/month rentals for every legacy badge and flag and
      retires the permanent SKUs (`active = false`, nothing deleted).
- [x] One active badge and one active flag per user, rebuy replaces, month
      over day replaces, clock resets.
- [x] Legacy owners still render their permanent badge when no rental is
      live.
- [x] Rental expiry removes the badge from chat labels with no background
      task (decay-at-read or the existing `ends_at` prune path).
- [x] Two SSH replicas agree within one refresh (notify path documented).
- [x] Tests beside `marketplace.rs`, `shop_consumable_effect.rs`, and the
      label query cover: buy, replace, expiry, legacy fallback, and that a
      user never sees another user's rental.
- [x] `hub/CONTEXT.md` Shop section, help modal Economy copy, root
      `CONTEXT.md` summary line.

Out of scope: any new badge art, pricing changes beyond the table above,
titles.

## Phase 1: title rental

Goal: a short text after the username in chat, rented for 24h or 30d, with
its own slot so it stacks with a color effect. The most be-seen item the
Shop can sell.

Behavior:
- Phase 1 ships **curated titles only**. A migration seeds 30 to 40 titles
  (`title_the_insufferable_day` / `_month`, ...) as `item_kind =
  'title_rental'`, payload `{ "text", "duration_secs" }`, on the Chat tab
  under the username effects. Write the list in the Blade Runner / noir
  register of GAME.md's theme section; every title must pass the
  screenshot test (legible from the screen, no dev jargon).
- Activation writes a user-scoped `shop_consumable_effects` row,
  `effect_kind = 'title'`, one active per user, rebuy replaces, exactly the
  username-effect path.
- Rendering: `, <title>` after the name in chat author headers and the
  clubhouse name label, painted in the muted label color (the title is
  text, it never takes the name's gradient). Add a title field beside
  the style in `NameFlairDirectory` so viewers resolve it from the same
  once-a-second tick with no per-render query; seed at startup, write
  through on purchase, refresh on `shop_user_changed`.
- Announce through a new `ActivityKind::TitleApplied` with the lounge
  filter arm; body never contains `@`.

Phase 1b (separate session, only after Phase 1 verifies): **custom titles**.
`Enter` on the custom SKU opens a text prompt (the `PendingUsernameEffect`
picker shape), max 20 chars, screened by `GhostService` with a
schema-enforced allow/deny JSON call before the purchase transaction;
denied text is refused uncharged with the bartender-style "never charge
for a no-op" rule. No AI configured means the custom SKU renders as
unavailable, never as unscreened.

Acceptance:
- [x] Curated titles buy, replace, expire, and render after the name in
      chat and the clubhouse for every viewer, including IRC-only rooms if
      the label reaches them (say explicitly if it does not).
- [x] A title and a username color are independent: buying one never
      clears the other.
- [x] Only the buyer's `ShopSnapshot` carries the active title; viewers
      resolve through the directory.
- [x] Tests: activation/replace/expiry in `late-core`, directory
      resolution in `username_effect_test.rs`, rendering in the chat
      render tests (assert the rendered text, not internals).
- [x] Help copy, `hub/CONTEXT.md`, `chat/CONTEXT.md`.

Out of scope in Phase 1: custom text, title colors, titles on the profile
page.

## Phase 2: gild, with tiers

Goal: pay chips to mark someone's message. Permanent marker on the row, a
gild count on the author's profile, two thirds to the author, one third
gone. Tiers let a whale spend 100x visibly.

Behavior:
- Table `chat_message_gilds(id uuidv7, message_id, user_id, tier, chips,
  created)`. A gild is a purchase, not a toggle: it never comes back, and
  the same user may gild the same message once per tier at most.
- Two `ChipMove` variants: `GildSent` (debit, floor-guarded like
  `GiftSent`, `source_ref = message id`) and `GildReceived` (credit, 2/3 of
  the tier price, `counts_as_earnings = false` like gifts). The remaining
  third has no ledger row: that is the burn.
- One transaction: debit, credit, insert. Reuse
  `UserChips::transfer_gift`'s shape (sender floor, recipient credit, two
  ledger rows, chip notifications) with the split.
- Guards: no self-gild (a CHECK like `notifications_no_self_mention` plus
  the service refusal), the gift per-sender cooldown, public rooms only
  (never DMs or private rooms), system/bot authors refused.
- UI: a new message action on a selected message (beside reply / edit /
  delete / profile / copy / react in the chat message-action set), opening
  a three-row tier picker with prices and the balance, `Enter` confirms,
  `Esc` cancels. Keyboard only, like Shop purchases.
- Rendering: a marker on the message row that shows the highest tier the
  message holds and the count (for example `$ x2`, `$$`, `$$$ x3`), loaded
  the way reaction summaries are (`ChatMessageReaction::list_summaries_for_messages`
  shape: one query per page of messages, never per row). Colors: bronze /
  silver / gold from the theme palette.
- Profile: gild counts per tier on the profile modal, one query in the
  model.
- Feed: `ActivityKind::MessageGilded` fires once, on a message's third
  gild, naming the author only ("mira's message got gilded three times in
  #lounge"). Never per gild.
- IRC sees nothing new (no marker), documented as such.

Acceptance:
- [ ] Ledger math: sender pays the tier price, author receives exactly
      2/3, `SUM(chip_ledger)` for the pair shows the 1/3 gap.
- [ ] Self-gild, DM, private room, bot author, and cooldown are refused
      uncharged, each as its own tagged error and its own test.
- [ ] A message shows its marker to every viewer after one refresh on
      every replica.
- [ ] Profile counts per tier are correct and scoped to the profile's
      user in the query.
- [ ] Top Chips excludes `GildReceived` (assert through
      `excluded_earning_reasons`).
- [ ] Tests beside the model, the service, and the render; help copy;
      `chat/CONTEXT.md`.

Out of scope: gilding from IRC, un-gilding, a gild leaderboard.

## Phase 3: the crown

Goal: one slot, one holder, one glyph in chat. Taking it costs 1.5x what
the holder paid, burned. The price ratchets with the whales; we never tune
it. Every takeover is a #lounge story naming both players.

Behavior:
- Table `crown_reigns(id uuidv7, month date, holder_user_id, paid_chips,
  taken_at, ended_at)`. The current reign is `ended_at IS NULL`; a partial
  unique index enforces at most one.
- `/crown` prints holder, held-for, and the price to take it. `/crown take`
  runs one transaction: `SELECT ... FOR UPDATE` on the current reign,
  refuse if `taken_at` is inside the hold window or the caller already
  holds it, price = `max(5000, ceil(paid_chips * 1.5))` (5,000 when vacant),
  debit `ChipMove::CrownTaken` (floor-guarded, 100% burn, no credit row),
  close the old reign, insert the new one, `NOTIFY crown_changed`.
- Rendering: a crown glyph immediately after the holder's name in chat
  author headers and the clubhouse label. The glyph is additive: it never
  changes the name's color or a title. Distribution: a process-shared
  `watch<Option<Uuid>>` seeded at startup and refreshed on the notify,
  resolved in the same per-second tick as the flair directory. No
  per-render query.
- Month boundary: a reign ending at the UTC month rollover leaves the
  crown vacant at the minimum price. The previous month's final holder
  gets a `profile_awards` row, category `crown`, rank 1, granted by the
  existing monthly award snapshot loop
  (`LeaderboardService::start_profile_award_snapshot_loop`), so it renders
  in chat labels and the profile like every other monthly award. Add the
  category to the guide lines and the help modal; the badge coverage test
  in `app/profile_modal/badges_test.rs` will name it if forgotten.
- Feed: `ActivityKind::CrownTaken { from: Option<username>, price }`:
  "tom took the crown from mira for 57,000" or "tom claimed the vacant
  crown for 5,000". Every takeover ships; the hold window is the throttle.

Acceptance:
- [ ] Two concurrent takes settle to exactly one debit and one reign
      (test with two transactions against the same open reign).
- [ ] Price ladder asserted for the first six takes from vacant.
- [ ] Hold window and self-take refused uncharged.
- [ ] Glyph visible to every viewer on every replica after one notify.
- [ ] Month rollover: reign closed, award granted once, crown vacant at
      5,000.
- [ ] Tests beside the model and the service; help copy; `chat/CONTEXT.md`
      and `leaderboard/CONTEXT.md` (award category).

Out of scope: crown history page, multiple crowns, buying the crown from
the Shop modal.

## Phase 4: burn milestones

Goal: fill the empty price band between 5k and 10M with three permanent
badges that only those prices buy.

Behavior:
- Migration seeds three permanent items at 100,000 / 500,000 / 1,000,000
  in the Ultimates tab (rename the tab if "Ultimates" no longer fits, the
  category enum is closed), each with a unique emoji not used by any
  rental badge, shown in the permanent badge position the legacy path
  still supports after Phase 0.
- **Do not seed them as `item_kind = 'badge'`.** Migration 148 ends with
  `UPDATE marketplace_items SET active = false WHERE item_kind = 'badge'`,
  and its header invites re-running its INSERT shape for new badges; a
  permanent milestone seeded as `badge` would be retired by any such
  re-run. Use a distinct kind (`milestone_badge`) that the chat label
  query's legacy join reads through the same `equipped_slot` path.
- Purchase announces through `ActivityKind::BurnMilestone { amount }` with
  a lounge arm: "mira burned 500,000 chips for the <name>".

Acceptance:
- [ ] Migration only, plus the activity hook on the existing purchase
      path and its filter arm.
- [ ] A purchased milestone renders in chat labels and the profile.
- [ ] Help copy and `hub/CONTEXT.md`.

## Phase 5: the pot

Goal: a daily parimutuel raffle. The biggest sink that works at any
concurrency, one story a day, and the arena's betting engine (GAME.md
phase 4) built early. Do not generalize it into a "pool" abstraction; the
arena copies the shape when it exists.

Behavior:
- Tables:
  `pots(id uuidv7, opens_at, draws_at, status open|drawn|rolled,
  ticket_price, winner_user_id, ticket_count, payout_chips, drawn_at)`,
  `pot_tickets(id uuidv7, pot_id, user_id, count, created)`.
  The pot's size is `SUM(count) * ticket_price` over its tickets; no
  stored running total, no house wallet.
- `/pot` prints size, ticket count, the caller's tickets, and time to the
  draw. `/pot buy N` buys N tickets in one transaction: cap check as part
  of the insert (`SUM(count) + N <= 50` for that user and pot, in the
  query), debit `ChipMove::PotTicket` (floor-guarded, `source_ref = pot
  id`), insert, `NOTIFY pot_changed`.
- `PotService` (process-global, like `DailyService`): a `watch` snapshot
  (pot id, size, ticket count, draws_at, and per-session the caller's
  ticket count), a buy task, a 60-second sweeper. The sweeper's guard is
  `UPDATE pots SET status = 'drawn', ... WHERE id = $1 AND status = 'open'
  RETURNING *`; only the replica that gets the row pays and announces.
  Zero tickets: status `rolled`, no payout, next pot opens.
- Draw: winner picked in Rust from the ticket rows with `TinyRng`
  (seedable, so the draw is a pure function with a whole-state test),
  weighted by `count`. Payout `floor(size * 0.8)` credited as
  `ChipMove::PotWon` (`counts_as_earnings = false`; a lottery win never
  tops Top Chips). The next pot is inserted in the same transaction with
  `draws_at` = the next 21:00 UTC, so there is always exactly one open
  pot.
- Sidebar: a two-row "Pot" panel in the roster in
  `late-ssh/src/app/common/sidebar.rs`, on by default for new panel lists
  and appended for stored lists (read how legacy `"activity"` entries are
  dropped on read and do the inverse), shrink priority just above the
  music stage:
  `pot 84,200 · 312 tickets · draws in 3h12m` / `you: 5 tickets`.
- Feed: `ActivityKind::PotDrawn { winner, payout, winner_tickets,
  total_tickets }` ("mira won the 84,200 pot on 3 of 312 tickets") and
  `ActivityKind::PotThreshold { size }` at 50k and 100k, once each per
  pot (track in the pot row, not in memory). Purchases never post.
- Winner: the #lounge line is persisted, so an offline winner reads it on
  return; an online winner also gets a session banner off the
  `pot_changed` notify. No `notifications` row (that table is
  mention-bound).

Acceptance:
- [ ] Buy: cap enforced in the query, floor guard, ledger row per buy.
- [ ] Draw: two replicas sweeping the same pot produce one payout; zero
      tickets rolls; the next pot always exists after a draw.
- [ ] Whole-state test of the draw from a fixed seed and fixed tickets.
- [ ] Payout math: winner receives `floor(size * 0.8)`, the ledger shows
      the 20% gap.
- [ ] Panel renders on Home and Arcade, shrinks in the right order, and
      the stored panel list of an existing user gains it without a
      settings migration.
- [ ] Threshold lines fire once each per pot across restarts.
- [ ] Tests beside the model, the service, the sidebar, and the activity
      filter; help copy; new `late-ssh/src/app/pot/CONTEXT.md` plus the
      root routing table row.

Out of scope: a machine in the Late Lounge tavern (later, once the command
exists), a sealed-bid twin for the marquee, seeding the pot with minted
chips, more than one pot at a time.

## Parked

- **The round** ("mira bought the house a round"): price 100 x patrons
  present (a DB presence query, never lobby state), DB-backed
  `drink_credits` as consent (one open credit per user, 24h expiry, cashed
  by ordering from @bartender), `ChipMove::RoundPurchase`, a `round`
  action in the bartender schema. About the size of gild. After Phase 5.
- **Sealed daily bids for the marquee line**: the pot's tables with `max`
  instead of `random`. After the pot.

## Dropped

- Paid music slot (the booth is too quiet to sell against).
- Live marquee auction (replaced by the crown, which names the loser).
- Percentage-of-balance pricing (punitive, opaque, and it taxes the one
  category that sells).
- More one-time badge packs, more 10-second room effects, anything only
  visible on the profile page.
