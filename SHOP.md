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
| Month tier price | 40x the day tier (migration 153; a convenience premium, not a discount: the day tier is the daily-login habit, the month tier sells skipping it) |
| Badge rental, basic | 100 / 24h, 4,000 / 30d |
| Badge rental, premium | 250 / 24h, 10,000 / 30d |
| Flag rental | same as basic badge |
| Title rental (Your Own Title, the only title on sale) | 1,000 / 24h, 40,000 / 30d (migration 155; was 2,000 / 80,000: the title alone ate a full arcade day, now title plus top effect is one day) |
| Username effect | 200 / 500 / 1,000 per 24h, 8,000 / 20,000 / 40,000 per 30d |
| Title max length | 20 characters |
| Gild tiers | 500 / 2,000 / 10,000 (an hour, a day, a week of completionist arcade play; Gold is also the most one buyer can put on a message, so it must be reachable) |
| Gild split | 2/3 to the author (`GildReceived`), 1/3 never re-minted |
| Gild feed threshold | a message's 3rd gild, once |
| Crown minimum price | 500 (lowered from 5,000 on 2026-08-26 so the month's race starts on day one; a fresh account can claim it, and the ratchet, not the floor, is what makes it dear) |
| Crown ratchet | next price = max(500, ceil(paid x 1.5)), 100% burned: 500 / 750 / 1,125 / 1,688 / 2,532 / 3,798 / 5,697 / 8,546 from empty |
| Crown hold | none (removed 2026-08-26: a hold turned month end into a clock game around the hold window; without it the last take before midnight wins, and the 1.5x ladder is the only throttle) |
| Crown reset | UTC month boundary; month-end holder gets a `profile_awards` row |
| Burn milestones | 🕯️ Wick 50,000 / 🧨 Fuse 150,000 / 🌋 Furnace 500,000 (lowered from 100,000 / 500,000 / 1,000,000 on 2026-08-26, when the ultimates came down to 1M and took the ceiling) |
| Ultimate spell | 1,000,000 (lowered from 10,000,000 on 2026-08-26: at ten million neither spell ever sold, and the shop's ceiling belongs on a thing people can actually reach) |
| Pot ticket | 100 chips |
| Pot per-user cap | 10 tickets per UTC day, 70 a week (was 50 per pot while the pot was daily; decided 2026-08-27) |
| Pot payout | 80% of ticket sum to one ticket-weighted winner; 20% never re-minted |
| Pot draw | Monday 21:00 UTC, two constants (weekly since 2026-08-27; the hour is the EU evening / US afternoon overlap) |
| News share | 500 per link, once per URL per person, at most 3 paid per UTC day (PR #565; the only chip reward outside the Shop and the games, and `s` on an RSS entry is one keypress, so the day cap is what keeps an inbox from being a printer) |
| Pot threshold lines | none (removed 2026-08-27, migration 162: the size rides the status HUD on every screen all week, so a mid-week #lounge nudge repeated the border) |
| Round price | 100 a head, matching the cheapest pour: a round is a lot of small kindnesses, and the room's size is the only multiplier |
| Round drink | 300 drunk points, three times what the buyer paid and exactly the buzzed threshold, so a sober room visibly moves a level; the buzz is free to hand out, only the chips are a sink |
| Round credit | one open per patron at a time, 24h to claim, cashed by ordering from @bartender |

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
- Legacy permanent badge/flag SKUs go `active = false`. The chat label
  query preferred a live rental over the legacy permanent purchase so
  existing owners kept what they bought; migration 165 closed that out (see
  the follow-up below) and the query now reads the rental alone.
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
      live. Superseded by the follow-up below.
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

Follow-up done (migration 165): the permanent equips are gone. Keeping them
readable left two things broken. An owner could never be shown as active in
the Shop, because the retired SKU is not in the catalog and no listed row
could carry the marker. And a rented flag only masked the permanent one,
which came back at expiry, so a permanent owner could never end up wearing
nothing. 165 clears every `chat_badge`/`chat_flag` equip and grants each
owner 30 days of the same emoji through the month SKU, which is a real
rental: the Shop shows it active with its remaining time and it lapses on
its own. Someone already renting over their permanent badge keeps their own
clock and is granted nothing, since one live row per (user, effect kind) is
the invariant. `bonsai_variant` is now the only slot anything equips, so the
Shop's clear-badge key and the permanent-badge branches went with it.

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

Status: shipped (migration 150, `late-ssh/src/app/ai/screen.rs`,
`CustomTitle::parse` in `rental.rs`). Verified 2026-08-25. Follow-up done
the same day: `ShopService::purchase_custom_title` now refuses (free, no
call) when the buyer cannot afford the tier or their last screen was
inside a 10s per-user cooldown, because every screen is a paid API call
and a refused one costs the buyer nothing.

Decision 2026-08-25: **the curated list is gone.** Migration 151 retires
all 72 curated rows (`active = false`, never deleted) and renames the
custom pair "Your Own Title"; it is the only title the Shop sells. The
same session moved Badges and Flags directly after the Chat tab
(`ShopCategory::ALL`) and split the Chat list with section rows (Name
effects / Title / Consumables). In the Consumables section, migration 152
retires Hack Room (`chat_pinned_vibe`, live `pinned_vibe` rows
deactivated, the room-rail styling it drove removed from `chat/ui.rs`)
and moves Room Bump to the top (`sort_order` 4005), so the section reads
Room Bump, Room Spark, Room Glow, Room Pulse.

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
  message holds and the count (for example `◆ ×2`, `◆◆`, `◆◆◆ ×3`), plus a
  tier-colored bar down the message gutter, loaded
  the way reaction summaries are (`ChatMessageReaction::list_summaries_for_messages`
  shape: one query per page of messages, never per row). Colors: bronze /
  silver / gold from the theme palette.
- Profile: gild counts per tier on the profile modal, one query in the
  model.
- Feed: `ActivityKind::MessageGilded` fires once, on a message's third
  gild, naming the author only ("mira's message got gilded three times in
  #lounge"). Never per gild.
- IRC sees nothing new (no marker), documented as such.

Status: shipped 2026-08-25 (migration 154,
`late-core/src/models/chat_message_gild.rs`, `ChatService::gild_message`,
`late-ssh/src/app/chat/gild/`). Deviations from the design above, each
deliberate:
- The marker rides the **message footer**, ahead of the reaction chips,
  not the author header. A message inside a run of messages has no header
  at all, so a header marker would vanish on every continuation line.
- The #lounge line reads "mira got a message gilded 3 times in #lounge".
  The feed format is `<username> <action>`, always space-joined, so the
  possessive "mira's message got gilded" is not expressible; the intent
  (name the author, name nobody who paid) is unchanged.
- Gilding has its **own** 30s cooldown map rather than sharing the gift
  one. Two separate sinks, and a gift silently blocking a gild would read
  as a bug.
- `GildReceived` is `counts_as_earnings = false` as decided, which is not
  what gifts do (`GiftReceived` counts). The explicit decision won.
- The marker crosses processes over a `chat_message_gilded` Postgres
  notify, and the selling replica repaints through the same path rather
  than through its own broadcast, so there is one code path per marker.
- One slot per buyer per message, and it only goes up: a higher tier from
  the same buyer raises the row at the new tier's full price, the same or a
  lower tier is refused (`AlreadyGilded` / `HeldHigher`). The marker's
  count is therefore distinct buyers, and a raise never fires the #lounge
  line.

Acceptance:
- [x] Ledger math: sender pays the tier price, author receives exactly
      2/3, `SUM(chip_ledger)` for the pair shows the 1/3 gap.
- [x] Self-gild, DM, private room, bot author, and cooldown are refused
      uncharged, each as its own tagged error and its own test.
- [x] A message shows its marker to every viewer after one refresh on
      every replica.
- [x] Profile counts per tier are correct and scoped to the profile's
      user in the query.
- [x] Top Chips excludes `GildReceived` (assert through
      `excluded_earning_reasons`).
- [x] Tests beside the model, the service, and the render; help copy;
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
  refuse if the caller already holds it, price = `max(500, ceil(paid_chips * 1.5))` (500 when vacant),
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
  "tom stole the crown from mira for 57,000" or "tom claimed the vacant
  crown for 500". Every takeover ships; the 1.5x ladder is the throttle.

Status: shipped 2026-08-26 (migration 156,
`late-core/src/models/crown.rs`, `late-ssh/src/app/crown/`). Deviations from
the design above, each deliberate:
- The take transaction takes a **`pg_advisory_xact_lock` before** the
  `SELECT ... FOR UPDATE`. A take from a vacant crown has no row to lock, so
  `FOR UPDATE` alone cannot serialize the first two takers: they would both
  insert and one would die on the partial unique index with a raw constraint
  violation instead of paying the next rung. Both locks stay; neither replaces the
  other.
- The shared value is a `watch<Option<CrownHolder>>` carrying the holder
  **and the reign's month**, not a bare `watch<Option<Uuid>>`. The month
  rollover has no sweeper and no notify to wake anyone up, so the month has
  to travel with the holder for the tick to expire it at read, the way
  rentals expire.
- The glyph rides `ResolvedName` in the existing flair map rather than a
  second lookup threaded through every render context. `resolve_all` takes
  the holder and gives them an entry even when they have bought nothing
  else, so every surface that already reads `name_flair` gets the crown for
  free and the ~1s comparison that repaints titles repaints the crown too.
- The `crown` award is monthly but **rankless**, which split a property that
  used to be one: `profile_award::is_rankless_award` (no rank digit) now
  covers the milestones plus the crown, while `MILESTONE_AWARD_CATEGORIES`
  (shown in chat labels whatever month they were earned) stays milestones
  only. `CRWN1` would have been noise for a slot with one holder.
- `/crown` answers in a **banner**, not an overlay: the crown is three facts
  and the command reads like `/gift`. That keeps the phase's "no new UI
  surface" promise literally.
- The deposed holder gets a banner naming who took it. Not in the design,
  but a glyph vanishing off your own name with no explanation reads as a
  bug. It rides the `crown_changed` notify payload (`CrownChange`: taker
  name, price, deposed id), not the in-process broadcast, so it reaches
  them on whichever replica they are connected to, the same way the glyph
  does.
- `CrownTaken` is `counts_as_earnings = false`, like `ShopPurchase`. The
  design never said; the choice is that a burn which bought a glyph must not
  knock the buyer off Top Chips, or nobody with a standing would ever take
  it. Gilds sent still count, since a gild is a tip to another player.
- The reign's `month` is stamped in SQL from the same `current_timestamp` as
  `taken_at`, not from the app clock captured before the pool checkout and
  the advisory-lock wait. Otherwise a take blocked across midnight on the
  last of the month is born with a month that is already over: chips
  burned, crown vacant, and last month's award handed to the wrong person.
- Every takeover also posts a **headline**: a real `system` chat message in
  #lounge with no ticker prefix ("👑 tom stole the crown from mira for
  1,688 chips. Next price: 2,532 chips."), on top of the ticker line. The
  ticker is glanceable and gone; the crown changing hands deserves a row in
  history. `filter::lounge_headline` is the exhaustive twin of
  `lounge_includes`, and the crown is its only arm today.
- **No hold.** The design's 30 minute hold is gone. With the `CRWN` badge
  going to whoever holds the crown at 00:00 UTC, a hold made the month end
  a clock game (take inside the last hold window and you are untouchable)
  instead of an auction; the 1.5x ladder throttles a war on its own, and a
  war in #lounge is the point. Accepted cost: two takes racing for one
  crown both land, the second at the rung the first just set rather than
  the price `/crown` quoted.
- The month rollover closes nothing. A reign left open across it keeps
  `ended_at IS NULL` until the next take, so `ended_at` is "when the row was
  replaced", not "when the reign stopped counting"; a future history page
  reads `LEAST(ended_at, month + 1 month)`.

Acceptance:
- [x] Two concurrent takes settle to exactly one debit and one reign
      (test with two transactions against the same open reign).
- [x] Price ladder asserted for the first six takes from vacant.
- [x] Hold window and self-take refused uncharged.
- [x] Glyph visible to every viewer on every replica after one notify.
- [x] Month rollover: reign closed, award granted once, crown vacant at
      the minimum.
- [x] Tests beside the model and the service; help copy; `chat/CONTEXT.md`
      and `leaderboard/CONTEXT.md` (award category).

Out of scope: crown history page, multiple crowns, buying the crown from
the Shop modal.

## Phase 4: burn milestones

Goal: fill the empty price band above the rentals with three permanent
badges that only those prices buy, and bring the shop's ceiling down to
somewhere a whale can actually reach.

Behavior:
- Migration seeds three permanent items, one emoji each, none of them used
  by any rental badge, flag, or the crown:

  | SKU | Name | Emoji | Price |
  |---|---|---|---|
  | `milestone_wick` | Wick | 🕯️ | 50,000 |
  | `milestone_fuse` | Fuse | 🧨 | 150,000 |
  | `milestone_furnace` | Furnace | 🌋 | 500,000 |

  The ladder is heat: the badge is the fire you paid for. Names stay one
  word so they fit a chat label and a #lounge line without wrapping.
- The same migration drops `ultimate_wonderland` and `ultimate_thematrix`
  from 10,000,000 to 1,000,000 (`ON CONFLICT (sku) DO UPDATE`, the shape
  migration 059 already ships). Nothing else about the spells changes.
- **Do not seed them as `item_kind = 'badge'`.** Migration 148 ends with
  `UPDATE marketplace_items SET active = false WHERE item_kind = 'badge'`,
  and its header invites re-running its INSERT shape for new badges; a
  permanent milestone seeded as `badge` would be retired by any such
  re-run. Use a distinct kind (`milestone_badge`).
- **A milestone is a fourth glyph, not a badge slot.** It renders after the
  rental badge and flag, never in place of either: a player wearing a
  rented cat and a pride flag who burns 500,000 shows all three. This is
  the one thing the chat label query must not get wrong, since the whole
  purchase is the glyph. Add a column to the query in
  `late-core/src/models/user.rs` (~578) beside `chat_badge` and
  `chat_flag`; it reads `user_purchases` joined on the milestone kind, with
  no `equipped_slot` and no rental LATERAL, because a milestone never
  expires and is never rented over.
- **Highest owned wins, automatically.** Owning Fuse and Furnace shows the
  Furnace. No equip flow, no slot column, no new state: the ladder only
  goes up, so the highest is the one a buyer would pick anyway. The query
  orders by price and takes one.
- The Ultimates tab holds all five items, so
  `ShopCategory::Ultimates => item.is_ultimate_spell()`
  (`late-ssh/src/app/hub/shop/catalog.rs:60`) becomes an arm that admits
  the milestone kind too, and `hub/shop/ui.rs:275` stops assuming an
  ultimate-tab row is castable. The tab label is a free rename if
  "Ultimates" stops fitting; the variant stays.
- Purchase announces through `ActivityKind::BurnMilestone { amount, name }`
  with a lounge arm: "mira burned 150,000 chips for the Fuse".

Status: shipped 2026-08-26 (migration 157,
`late-core/src/models/milestone.rs`, the flair directory). Deviations from the
design above, each deliberate:
- The glyph rides **`NameFlair` / `ResolvedName`**, the crown's map, not a
  fourth column on the chat label query. The design said to add one beside
  `chat_badge` and `chat_flag`, but those reach the renderer as a single
  joined string that `chat_badge_display_parts` splits back apart by
  detecting flag prefixes; a third value in that string would have to be
  recovered by matching against the three known emoji. The flair map is
  already read at every author header, already resolves on the once-a-second
  edge, and already carries the crown for exactly this reason.
- `refresh_user_flair` therefore has to read the milestone even though
  nothing about a purchase expires: that path rebuilds the whole entry, so
  leaving it out would drop a 500,000-chip glyph the moment its owner rented
  a badge.
- The milestone takes its **own click target** (`HeaderTarget::StoreMilestone`
  -> the Ultimates tab). Sharing `StoreBadge` would have sent a click on the
  dearest item in the shop to the tab that sells hundred-chip cats.
- The Ultimates tab gained **section rows** ("Burn milestones" /
  "Ultimate spells", `ultimates_section_label`). Five items at two unrelated
  price bands, one of which repaints the server for ten seconds and one of
  which does nothing at all, should not read as one list.
- **Not on the profile modal.** The acceptance list said "chat labels and the
  profile", but the profile modal renders a bare username today: no badge, no
  flag, no crown, no title. There is no surface to hang a milestone on, and
  inventing one is a bigger change than the rest of this phase. Left out on
  purpose; if the profile ever grows a worn-items row, every one of these
  belongs on it, not just the milestone.

Acceptance:
- [x] Migration seeds the three milestones and reprices both ultimate
      spells; no other catalog row moves.
- [x] A purchased milestone renders in chat labels alongside a live rental
      badge and a live rental flag, all three at once.
- [x] Owning two milestones shows the dearer one, in one query, for every
      viewer.
- [x] The Ultimates tab lists all five and refuses to cast a milestone.
- [x] Help copy, `hub/CONTEXT.md`, `chat/CONTEXT.md`, root `CONTEXT.md`.
- [ ] Profile rendering: no surface exists (see the deviation above).

Out of scope: milestone-only chat colors, a fourth rung, retiring the
ultimate spells, any equip choice between milestones.

## Phase 5: the pot

Goal: a parimutuel raffle (daily as designed, weekly as shipped, see the
status below). The biggest sink that works at any
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
- Sidebar (removed 2026-08-28, see the status block): a two-row "Pot" panel in the roster in
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

Status: shipped 2026-08-27 (migration 160, `late-core/src/models/pot.rs`,
`late-ssh/src/app/pot/`). Deviations from the design above, each deliberate:
- **Weekly, not daily**, drawn Monday 21:00 UTC, and the cap is **10 a
  day** rather than N per pot (decided 2026-08-27, same day). At the clubhouse's size a daily pot is a few
  thousand chips, the 50k / 100k threshold lines would never fire, and empty
  days rolling would quietly say nobody plays. One real story a week, a
  countdown in days, and a daily cap so nobody can buy the week on Monday:
  the raffle is about showing up seven times, and 1,000 chips a day is an
  arcade afternoon, reachable by anyone who plays. The pot therefore stops
  being a whale sink (7,000 a week per player at most); the crown and the
  milestones are the whale sinks. No schema change: `next_draw_at` picks the
  next Monday, `short_duration` learned days, and the cap check in the insert
  counts today's rows (`created` on today's UTC date) instead of the pot's. The 21:00 hour stays as the EU evening / US
  afternoon overlap.
- **The pot also rides the status HUD**, `pot 84,200 · 4d12h` right before
  the chips so the prize reads against the viewer's balance, on every
  screen. It sheds first under a tight border (countdown, then itself).
- **No sidebar panel** (removed 2026-08-28). The HUD badge already carries
  the size and the countdown on every screen, so the two-row panel spent
  rail rows on a second copy of the border; `/pot` remains the full read.
  `RightSidebarComponent::Pot` and `pot/panel.rs` are gone, and a stored
  `"pot"` key is dropped on read by `from_key` exactly like the retired
  `"activity"` and `"visualizer"` keys, so no settings migration is needed.
- **`/pot` costs no query.** The design put "the caller's ticket count" in
  the `watch` snapshot, which a process-wide watch cannot carry. Instead
  `PotSnapshot` holds a private `HashMap<Uuid, i64>` of holdings and
  `tickets_for(user_id)` is the only way out of it, so the HUD badge and the
  `/pot` line both read owned memory and no session can widen the read. The
  snapshot is one aggregate query per refresh, which the draw needs anyway.
  `PotEvent` therefore has no `Status` variant: the command is answered
  synchronously in `tick_pot` rather than through a task.
- **The draw posts a headline as well as a ticker line.** The design's own
  requirement was that an offline winner reads the result on return, and a
  `· ` ticker line is diverted out of the TUI message list; only a headline
  (a real un-prefixed `system` message, `filter::lounge_headline`) is a row
  they can still read. The crown was that arm's only occupant until now.
- **`ChipMove::PotTicket` is `counts_as_earnings = false` too.** The design
  only decided the win. Excluding the win and counting the ticket would make
  buying into the pot a pure negative on a board the winner cannot climb
  back up, so both sides are out.
- **No threshold lines.** They shipped as a high-water column
  (`pots.announced_threshold`, claimed by a guarded `UPDATE`) and were
  removed the same day (migration 162) once the pot moved into the status
  HUD: the size is on every screen all week, so a mid-week #lounge nudge
  only repeated what the border said. The draw is the pot's one story.
- **`ticket_count`, `payout_chips`, `winner_user_id`, `drawn_at` are
  nullable**, stamped once at the draw, with one CHECK per status so a
  half-settled row cannot exist. A drawn pot is told from a rolled one by
  `ticket_count > 0` rather than by the winner, because the winner's account
  can be deleted out from under a settled row (`ON DELETE SET NULL`); history
  keeps what was paid and the ledger keeps who was paid.
- **The buy holds the pot row `FOR UPDATE`** on top of the in-query cap
  check. The check alone is not exact: two concurrent buys by one player
  would read the same sum and both pass. The row lock also means a buy that
  arrives exactly as the sweeper draws is refused uncharged
  (`PotRefusal::Closed`) instead of landing in a settled pot.
- **`ensure_open` is folded into `settle_due`.** The migration seeds no pot,
  so the sweeper's first pass opens one; that is the same advisory lock and
  the same transaction that would otherwise have needed a second path.
- **The panel is two rows of left/right pairs**, not the wide single line the
  design sketched: the rail is 24 columns and the panel draws into 21, so
  `pot 84,200 · 312 tickets · draws in 3h12m` was never going to fit. It
  reads `84,200 / in 3h12m` over `842 tickets / you 5`, and dashes before the
  first refresh.

Acceptance:
- [x] Buy: cap enforced in the query, floor guard, ledger row per buy.
- [x] Draw: two replicas sweeping the same pot produce one payout; zero
      tickets rolls; the next pot always exists after a draw.
- [x] Whole-state test of the draw from a fixed seed and fixed tickets.
- [x] Payout math: winner receives `floor(size * 0.8)`, the ledger shows
      the 20% gap.
- [x] ~~Panel renders on Home and Arcade, shrinks in the right order, and
      the stored panel list of an existing user gains it without a
      settings migration.~~ Removed, see the status block.
- [x] ~~Threshold lines fire once each per pot across restarts.~~ Removed, see the status block.
- [x] Tests beside the model, the service, the sidebar, and the activity
      filter; help copy; new `late-ssh/src/app/pot/CONTEXT.md` plus the
      root routing table row.

Out of scope: a machine in the Late Lounge tavern (later, once the command
exists), a sealed-bid twin for the marquee, seeding the pot with minted
chips, more than one pot at a time.

## Phase 6: door payouts, repeatable

Goal: every door milestone pays again. A NetHack ascension is the same 20+
hours the second time, and a Green Dragon kill or an A Dark Room escape is a
full run every time, so a repeat pays the full amount. The gate is whatever
naturally limits the game; only where nothing does is a lockout added.

What the code says today (investigated 2026-08-25):
- Thirteen `reward_templates` rows, all `claim_policy = per_event`, all
  credited through `ChipService::credit_lifetime_reward_template`, which
  writes one `game_payout_claims` row keyed `(period_kind = 'lifetime',
  period_key = 'once')`. That row is what makes every door pay once per
  account forever. Roguelikes: `dcss_orb` 10k, `dcss_win` 20k,
  `nethack_amulet` 10k, `nethack_ascension` 20k, `brogue_escape` 10k,
  `brogue_mastery` 20k. Green Dragon `greendragon_dragon_slain` 10k. A Dark
  Room `darkroom_escape` 10k, `darkroom_beacon_escape` 10k. Lateania
  `lateania_archdemon_defeat`, `_frontier_king_defeat`,
  `_sundering_deep_defeat`, `_kaethyr_ascendant_defeat`, 10k each.
- The claim model already has the two other shapes needed:
  `GamePayout::grant_period` (any `period_kind`/`period_key`, used with
  `event` for the per-match daily payouts) and `GamePayout::grant_cooldown`
  (advisory-locked, looks back only at rows with `period_kind =
  'cooldown'`, so an old `lifetime` row never blocks a cooldown claim).
  `RewardTemplate::cooldown()` reads `cooldown_seconds`.
- **Roguelike replay hazard.** `door/ingest/svc.rs` grants awards on every
  sighting of a win line, fresh or replayed, on purpose: a cursor reset or a
  backfill re-ingests a log from offset 0 and relies on the lifetime claim
  to make the re-grant a no-op. Under a plain 7-day cooldown, a replayed
  line older than 7 days would pay again. The repeat claim therefore has to
  be keyed on the run's identity as well: `door_runs` and
  `door_milestones` carry `UNIQUE (game, source_file, source_offset)`, and
  `insert_ignore` returns whether the row was fresh. (Before 2026-08-27
  NetHack's Amulet and DCSS's Orb were also granted a second time off the
  win line, as a backstop for a missed pickup line; see the deviation
  below for why that is gone.)
- **Green Dragon.** `Character::slay_dragon` bumps `dragon_kills` and resets
  the character to level 1 (gold and gems restart from the kill count), so
  the loop already exists in-game. `reward_dragon_kill(user_id, kills)`
  already carries the kill number. `greendragon_characters` is one row per
  user with its own uuidv7 `id`; `delete_by_user_id` drops the row, so a
  deleted-and-recreated character restarts at kill 1 with a new row id.
- **A Dark Room.** `darkroom_saves` is one row per user, deleted by
  `delete_by_user_id` when an ending is reached (`reward_escape`), so a
  replay is a full run from nothing. The save blob carries no run id.
- **Lateania.** `mud_characters` has a uuidv7 `id` and, since migration 139,
  `UNIQUE (user_id, slot)`; `delete_slot` drops the row (`d` in the Games
  hub), so a recreated character is a new id. The grant site,
  `publish_kill_outcome`, only knows `KillOutcome { user_id, mob_name,
  achievement }`; the character slot is not in it.
- **In-door copy that says "once per account"** and must change with the
  gate: `door/nethack/render.rs:102,107`, `door/dcss/render.rs:106,109`,
  `door/brogue/render.rs:115,116`, `door/darkroom/ui.rs:507`
  (`ENDING_REWARD`) and `darkroom/state.rs:142`,
  `door/lateania/svc.rs:6546` (the loot log line),
  `profile_modal/badges.rs:98`, and the thirteen template descriptions.
- Asterion is the odd one: `asterion_daily_escape`, `utc_day`, 4,000,
  through `credit_asterion_daily_escape`. The maze is 8 deep
  (`asterion_core::MAX_MAZE_ID = 7`), heroes die to minotaurs, no idle kick.

Decided numbers (one number per milestone, no first/repeat split):

| Door | Milestone | Pays | Gate |
|---|---|---|---|
| NetHack | Amulet / Ascension | 20,000 / 50,000 | run identity + 7-day lockout, each |
| DCSS | Orb / Escape | 20,000 / 50,000 | run identity + 7-day lockout, each |
| Brogue | Escape / Mastery | 20,000 / 50,000 | run identity + 7-day lockout, each |
| Green Dragon | dragon kill | 10,000 (migration 159; was 20,000 in 158) | kill number per character row; the daily turn cap makes a kill 7-10 days |
| A Dark Room | escape / beacon escape | 15,000 / 20,000 | run id; the run is the gate (~5 days) |
| Lateania | Archdemon, Frontier King | 10,000 each | once per `mud_characters.id` AND 7-day lockout per crown per account |
| Lateania | Yssgar, Kaethyr | 20,000 each | same |

Rate check: everything lands near 2,000 chips per day of effort, a
completionist arcade day, so a door grind is a real alternative to the
arcade. The top end (a weekly NetHack ascension, 50k/week) is the best rate
in the app on purpose; the 40x month tiers are sized to eat it. Orb + Escape
in one DCSS run (70k) stays: the Orb row exists for the player who dies on
the way up.

Why Lateania has two rules: the character persists, so a maxed character
kills the easy two in an evening (lockout needed), and `d` deletes the
character, so per-character alone would be a reroll farm of the easy two
(lockout needed, and it must key on the account, not the character).
Together the worst week is 20k from rerolling the easy pair, and the hard
pair needs a leveled character each time.

Behavior:
- One new primitive in `late-core/src/models/game_payout.rs`: an
  all-or-nothing grant that inserts several `game_payout_claims` rows in
  one transaction (each its own `(period_kind, period_key)`), optionally
  with a cooldown look-back on one of them, under the same advisory lock
  `grant_cooldown` takes. Any conflict or live cooldown means no rows, no
  chips, no ledger line. Phase 7's pair-day cap is the same primitive;
  whichever phase lands first builds it.
- Roguelikes: `claim_policy = cooldown`, `cooldown_seconds = 604800`, and
  the grant carries the run identity as an `event` key. `award.rs` needs
  the identity: pass the `door_runs`/`door_milestones` natural key (`game`,
  `source_file`, `source_offset`) or the row id from the ingest handlers
  into `DoorAwards::grant`. Backfilled historical wins still grant (owner
  decision, unchanged), but each run at most once, and the lockout spaces
  them.
- Green Dragon: `per_event`, event key `<greendragon_characters.id>:<kill
  number>`. The row id keeps a recreated character's kill 1 distinct from
  the old character's kill 1. Load the id with the character (`svc.rs`
  already loads the row).
- A Dark Room: `per_event`, event key = a uuidv7 `run_id` stamped into the
  save blob when a run starts (a save with no `run_id` gets one on first
  load; the wipe on escape means the next run gets a new one). Never key
  on an escape count alone.
- Lateania: `KillOutcome` gains the character slot (the world knows which
  slot each player loaded), the grant task resolves `mud_characters.id`
  for `(user_id, slot)`, and the multi-key grant writes the `event` row on
  that id plus the `cooldown` row for the crown. Loot log line and the
  `BossReward` doc comment stop saying "once per account".
- Profile badges (`NHA`/`NHY`, `DCO`/`DCW`, `BRE`/`BRM`, `GDS`, `ADE`/`ADB`,
  `LMG`/`LKN`/`LYS`/...) stay once per account: the `NOT EXISTS`
  `grant_unique_milestone_award` insert is untouched and keeps running on
  every sighting. Only chips repeat.
- #lounge feed: unchanged (every kill/escape already posts, gated on
  freshness and recency in the ingest). A gated claim pays nothing and says
  nothing extra.
- Copy: every "once per account" listed above becomes the real gate ("pays
  again after 7 days", "once per character, 7 days between crowns", "every
  run").
- Asterion is NOT in this phase. Owner to time a final-maze escape first:
  under an hour means it is the best rate in the app and should drop to
  ~1,000 or take the lockout shape; multi-hour means it stays.

Status: shipped 2026-08-26 (migration 158, `GamePayout::grant_multi` in
`late-core/src/models/game_payout.rs`, `ChipService::credit_run_cooldown_reward_template`).
Deviations from the design above, each deliberate:
- The multi-key grant is `GamePayout::grant_multi` over a closed
  `GamePayoutKey` enum (`Unique { period_kind, period_key }` /
  `Cooldown { period_kind, window }`), not a cooldown flag hung off one key.
  A gate is either an identity or a rate limit, and a match arm per gate reads
  the way the callers do.
- **Every claim row in a multi-key grant carries the full amount.** The
  design said several rows, one payout; the table's
  `CHECK (amount > 0)` forbids recording a zero on the companion rows, and
  relaxing an applied migration's CHECK for a bookkeeping nicety was the worse
  trade. `chip_ledger` takes exactly one row per credited grant and is the
  money witness; `game_payout_claims.amount` is per-claim record. The ingest
  test helper that summed claim rows now sums the ledger instead.
- `grant_cooldown` and `grant_multi` share one advisory lock helper
  (`lock_payout`), on the same `(user, game, payout_kind, 'cooldown')` key
  `grant_cooldown` already used, so the two paths can never race each other on
  one payout even though no template uses both today.
- Green Dragon's character row id is read in the grant task through a new
  `GreenDragonCharacter::id_for_user`. The design said "load the id with the
  character", but every existing load returns the opaque blob and nothing
  else; threading an id through them all to serve one fire-and-forget task
  would have touched a dozen call sites for no other reader.
- A Dark Room's `run_id` is a plain `Uuid` with
  `#[serde(default = "Uuid::now_v7")]`, not an `Option` upgraded on load. Same
  behaviour for an old blob (it deserializes with an id of its own rather than
  a nil one every old save would share), and the in-memory game can never be
  without one, so the ending has nothing to unwrap.
- Lateania's character slot is resolved in `publish_kill_outcome` from the
  service's `live_slot` binding (falling back to `active_slot`, the rule
  `publish` already uses) rather than added to `KillOutcome`. The world state
  carries no slot at all: the binding lives on the service, and the read
  happens one step after the tick that produced the kill.
- Two copy sites the investigation missed are fixed in the same pass:
  `door/greendragon/ui.rs` ("the chip payout is a lifetime claim") and
  `door/lateania/screen.rs`, which still said Yssgar and Kaethyr pay "no
  chips, only glory", stale since migration 144 flattened the crowns.
- **No back-grant, found in verification 2026-08-27.** `award.rs::grant` used
  to pay the Orb / Amulet a second time off the win line ("in case the
  milestone stream missed it"), and the NetHack xlogfile `achieve` bit paid
  the Amulet off a death line. Under the new gate the back-grant carried the
  win line's own key, so the only thing stopping it was the 7-day window:
  a run whose pickup and win were more than a week apart paid the pickup
  twice (90k for a DCSS run the table prices at 70k). Both backstops are
  deleted, not re-keyed: a pickup line the pipe missed is an ingest bug
  that has to surface as a missing badge, not get paid from somewhere else.
  One line, one milestone, every door. The `NethackRun.amulet` field went
  with it.
- **A refused line can pay on a later replay** (noted in verification
  2026-08-27, accepted). A win refused by the 7-day window writes no claim
  row at all (all gates or none), so it looks fresh to a later re-read. A
  cursor reset or backfill after the window therefore pays every win that
  was refused inside its week, not only wins the pipe never saw. Only an
  operator can trigger a re-read, and it sits inside the "backfilled
  historical wins still grant" decision; know it before resetting a cursor.
- The amounts are quoted in the in-door landings, the badge guide, and the
  help modal because that is what a player reads, and in the CONTEXT.md files
  that already quoted the old ones. Every one of those points back here.

Checklist:
- [x] Migration: `reward_chips`, `claim_policy`, `cooldown_seconds`, and
      the description on the thirteen rows. Existing `lifetime` claim rows
      stay as history and must not block the first gated repeat (they have
      a different `period_kind`; add a test that proves it).
- [x] Multi-key grant in `game_payout.rs` with tests: all rows or none,
      cooldown honoured, conflict on any key pays nothing, concurrent calls
      serialize under the lock.
- [x] Roguelikes: a replayed win line (same natural key) never pays twice;
      a second distinct win inside 7 days pays nothing and the badge insert
      still no-ops; a win after 7 days pays.
- [x] Green Dragon: kill N pays once; a recreated character's kill 1 pays.
- [x] A Dark Room: `run_id` survives save/load, differs across runs, and an
      old save without one is upgraded on load.
- [x] Lateania: same character twice pays once; a second character inside
      7 days pays nothing; a second character after 7 days pays.
- [x] Copy sites above; door CONTEXT.md files, `door/ingest` notes, and the
      chips context updated; this table copied nowhere else (link here).

Out of scope: new milestones, Lobby game stakes and the loser payout
(Phase 7), Asterion until timed.

## Phase 7: Lobby economics (daily matches)

Goal: make a correspondence match worth finishing, and let two players put
chips on one. Three parts, in this order: close the collusion hole in the
win payout, pay the loser who saw the game through, add an optional wager.
House tables are not touched.

What the code says today (investigated 2026-08-25, `late-ssh/src/app/lobby`):
- Win payouts: chess and chess960 500, battleship 300, connect four,
  reversi, checkers, backgammon, briscola 400. One `per_event` template per
  game credited on the match id (`DailyService::finish_events`), no
  cooldown, no per-day cap. The only limit is `DAILY_MAX_ACTIVE_ENTRIES`
  (10), and a finished match frees its slot. Two accounts sitting together
  can post, claim, resign, and repeat: 500 chips per resign, as fast as the
  keys go. This is the largest open faucet in the app and it predates this
  document; it gets closed before anything is added on top.
- Effort signal: every roster arm bumps `state.revision` by exactly one per
  move (`state.revision.saturating_add(1)` in each `play_*` path) and
  resign bumps it once more. There is no claim timestamp on
  `daily_matches`, and only chess stores per-move timestamps, so the
  effort gate is a move count, not elapsed time.
- Timeout: `DailyMatch::forfeit_expired` makes the player whose turn it was
  the loser. That is the abandoner by construction.
- Draws pay nobody. Losers get a `Banner::info` and nothing else.
- Wagers: parked in `daily/CONTEXT.md` section 8 as "a `wager` column plus
  hold/settle in `ChipService`; claim and finish are the only touch
  points". That is still the right shape.
- House tables are chip transfers or small faucets and stay as they are:
  poker (1,000 stack, 10/20 blinds) and blackjack (10-chip stake) settle
  through `credit_payout`, Tron pays 50/75/100 on a 5-minute cooldown,
  Super Snake banks a per-visit tally on leave. Asterion (8 mazes,
  `MAX_MAZE_ID = 7`, 4,000 per UTC day) is the open dial from Phase 6, not
  this phase.

Decided numbers:

| Dial | Value |
|---|---|
| Paid results per opponent per game per posting day | 1 (win payout; the claim is scoped to the roster game, decided 2026-08-27; the consolation was dropped) |
| Consolation | 100 chips, flat, every roster game |
| Consolation gate | `state.revision >= DailyGame::consolation_min_moves()`: chess and chess960 40, battleship 40, reversi 30, checkers 30, connect four 20, backgammon 20, briscola 20 (revision counts both players' moves) |
| Who gets consolation | the loser on every decisive result except `timeout`; both players on `draw`; nobody on `timeout` |
| Wager stakes | challenger picks one of 0 / 100 / 500 / 1,000 / 5,000 at post time |
| Wager settle | winner receives `floor(2 x stake x 0.9)`, 10% never re-minted; draw refunds both; cancel refunds the challenger; timeout pays the pot to the winner |

Why these:
- The pair-day cap is the whole anti-collusion story. One paid result per
  (user, opponent, day) turns the resign loop into 500 + 100 per pair per
  day, and it does not touch honest play: nobody finishes two
  correspondence games against the same person in one day by accident. A
  rematch the same day is for the board and the wager only.
- 100 flat rather than a share of the win: the consolation is for showing
  up twenty times over three weeks, which costs the same in chess and in
  connect four. Timeouts pay nothing because the timed-out player is the
  one who left.
- The wager is zero-sum minus burn, so it needs no cap: two accounts
  trading wagers lose 10% a game. Abandoning a wagered match forfeits the
  stake, which is the strongest "finish your games" lever in the phase.
  Win payout and consolation still apply on top of a wager; they are the
  faucet, the wager is the transfer.

Behavior:
- `ChipMove` gains `DailyMatchConsolation` (credit, earnings),
  `DailyWagerHold` (debit, floor 0 like `Bet`, earnings), `DailyWagerWon`
  (credit, earnings), `DailyWagerRefund` (credit, earnings; a refund
  reverses a hold that counted, so it counts too). Reason strings follow
  the roster (`daily_match_consolation`, `daily_wager_hold`, ...).
- Migration: `daily_matches.wager BIGINT NOT NULL DEFAULT 0 CHECK (wager
  >= 0)`; seed `daily_match_consolation` (`game: daily_match`,
  `payout_kind: consolation`, `per_event`, 100). No pot column: the pot of
  a claimed row is `wager * 2`, the ledger rows are the witness.
- Pair-day cap: the win claim and the consolation claim each insert two
  `game_payout_claims` rows in one transaction, the existing `per_event`
  row on the match id and a `pair_day` row keyed `<opponent id>:<UTC
  date>`; either conflict means no chips and no ledger row. This is the
  same all-or-nothing multi-key grant Phase 6 needs for Lateania (per
  character plus per-account lockout): whichever phase lands first builds
  it in `late-core/src/models/game_payout.rs`, the other reuses it.
- `finish_events` grows the loser/draw branch: same fire-and-forget shape
  as the winner credit, gated on the result string and the revision the
  finishing path already holds. Nothing new reads the DB for it.
- Wager hold at `post_challenge` (a short balance fails the post with the
  usual `DailyEvent::Error`), matched at `claim_challenge` (a short balance
  fails the claim, the challenge stays open), settled in the same code
  paths that finish today: `finish` (decisive, draw), `resign`,
  `sweep_expired` (timeout), `cancel_challenge` (refund). Settlement is one
  ledger write per player per match, idempotent on the match id through
  the claims table like the win payout, so a sweeper retry cannot pay
  twice.
- UI: the `ChallengeDraft` gets a stake row (arrows cycle the fixed list,
  default 0), open-challenge rows and the panel show the stake, the claim
  confirm says "match 500 chips?", the result banner shows the wager
  outcome and the consolation ("you lost the match (checkmate), +100 for
  seeing it through"), the #lounge result line appends "for 1,000" on a
  wagered match. `DailyGame::win_payout` stays display-only and equal to
  the template, same rule for `consolation_min_moves`.

Status: part 1 shipped 2026-08-27 (migration 161, `DailyService::pay_winner`,
`ChipService::credit_per_event_pair_day_reward_template`). Part 2, the wager,
is still open. Deviations from the design above, each deliberate:
- **The pair-day cap is per opponent per game**, not per opponent. The
  `pair_day` claim rides `grant_multi` under the template's `game`
  (`daily_chess`, `daily_battleship`, ...) like every claim row, so chess and
  battleship against the same person on the same posting day both pay.
  Decided 2026-08-27 in review: friends who play several games together are
  never touched, and a colluding pair is bounded at one paid win per roster
  game per direction per posting day (about 3,300 chips each way, and only
  after playing all eight games). If Top Chips ever looks wrong, the
  per-account daily quota is the next step, not a tighter pair key.
- **A win pays only with at least 5 half-moves played**
  (`DAILY_WIN_MIN_MOVES`; both players' moves count, a resign is not a move,
  a timeout counts what was played). Not in the design. It stops the
  zero-move claim-and-resign and nothing more: a move count sets a rate, not
  a bound, since there is no minimum time per move. The pair-day cap is the
  bound.
- **The consolation is dropped, not built.** 100 chips for showing up could
  not be paid without reopening the faucet the cap just closed, and the wager
  is the real finish-your-games lever. The rows for it in the table above
  stay as the record of what was considered.
- The credit is awaited inside `finish_events` rather than spawned, so the
  `MatchFinished` event and the banner say what the chips did
  (`DailyWinPayout`: paid / unplayed / pair_day_capped / failed), and the same
  outcome is stored in `daily_matches.win_payout` for the lingering result
  row, so an offline winner learns why the chips did or did not come. A
  failed credit never un-finishes a match.

Checklist:
- [x] Pair-day cap on the win payout, with a test: two decisive matches
      against the same opponent on one UTC day pay once; the next day pays
      again; a different opponent the same day pays.
- [x] Test pinning the per-game scope: chess and chess960 against the
      same opponent on one posting day both pay
      (`pair_day_cap_is_scoped_to_the_game`).
- [x] Consolation: dropped, see the status block above.
- [ ] Wager: hold on post, match on claim, short balance fails the right
      step, settle on every finish path, refund on cancel and draw, timeout
      pays the pot, retry of any path is a no-op. Whole-ledger assertions:
      the sum of the four wager moves for a match equals minus the burn.
- [ ] `daily/CONTEXT.md` sections 1, 3, 6, 8 and the chips context updated
      (done for part 1); help copy for the stake row waits on the wager. The
      `consolation_min_moves` roster arm is gone with the consolation.

Out of scope: spectator side bets (the arena, GAME.md phase 4), tournaments,
draw offers, house-table stakes, an entry fee on unwagered matches, elapsed
time as a gate (no claim timestamp exists; add one only if a game ever
needs it).

## Phase 8: the round

Goal: a sink whose whole product is one line in #lounge, "mira bought the
house a round". The chips are burned; what the room gets is a story and a
free drink each.

Behavior:
- Tables (migration 164): `drink_rounds(id uuidv7, buyer_user_id,
  price_per_patron, created)` and `drink_credits(id uuidv7, round_id,
  user_id, expires_at, cashed_at, created)`, with a partial unique index on
  `(user_id) WHERE cashed_at IS NULL`. No stored total on the round: the
  credits are the witness of how many it bought and the ledger row is the
  record of what it cost, exactly like the pot's tickets.
- **The trigger is a literal phrase, not a model decision.** `ROUND_PHRASES`
  in `late-core/src/models/drink_round.rs` is the closed list ("round for
  everyone", "round for all", "round for the house", ...), matched
  case-insensitively on word boundaries. This is the only bartender action
  that spends more than one drink's worth, and the price is the size of the
  room, so the phrase itself is the confirmation. An order is a statement:
  a phrase whose sentence ends in `?` ("how much is a round for everyone?")
  or sits inside backticks is a question about the words, not the words,
  and goes to the model to be answered. @bartender never decides to buy
  one; the prompt tells him to hand out the words instead.
- **`chat/slur.rs` never scrambles the phrase.** Drunk text is stored, not
  rendered, so a wasted patron's "round for everyone" would otherwise reach
  the matcher as "ronud for eevryone", breaking the feature for exactly the
  people most likely to use it. Both the guard and the matcher read the one
  list, so they cannot drift. The hiccup is kept out of it too.
- **Price** `ROUND_PRICE_PER_PATRON` (100) for every credit that actually
  landed, `ChipMove::RoundPurchase`, floor-guarded, burned whole with no
  credit row anywhere (the crown's shape), excluded from Top Chips like
  every other vanity burn. Presence is the in-process `active_users` roster
  minus the buyer and the bots: single-replica, and deliberately so (see the
  status block).
- **Only the buyer is poured into.** "Round on me" includes me: the buyer
  drinks theirs on the spot, `ROUND_DRINK_POINTS` in the same transaction as
  the charge, riding on the round's price rather than adding a head to it. A
  drink makes a patron type drunk in public, so everyone else gets a credit,
  not a drink; it is cashed only when they walk up and order from @bartender
  themselves. One open credit per patron
  (across every round, so a second round moments later reaches nobody and is
  refused uncharged), `ROUND_CREDIT_TTL_HOURS` (24) to claim it, and cashing
  is one guarded UPDATE so it can only ever be drunk once.
- **A cashed drink pours `ROUND_DRINK_POINTS` (300)** whatever the bartender
  invented to call it: three times what the buyer paid, and exactly the
  buzzed threshold, so a sober room that drinks the round visibly moves a
  level for about an hour. `lifetime_spent` does not move; the chips are on
  the buyer's ledger row.
- Refusals (`RoundRefusal`, all uncharged): nobody else at the bar, everyone
  already holding one, and the chip floor. Each gets a scripted line from
  @bartender, behind the mention ladder since they are free (a settled round
  never is), as does the purchase itself: this is the one line that has to
  quote the number the patron was actually charged, and it must land the
  moment the chips move rather than after a model round trip.

Status:
- **Presence is in-process, not a DB query**, unlike the original sketch.
  There is no presence table in this repo: `users.last_seen` is written only
  at connect and `user_online_time` is a cumulative total. Building one was
  more work than the feature, so the round reads `active_users` like the
  clubhouse headcount does. On a second replica a round would only reach the
  buyer's own pod; the credits it grants are DB rows and cash from anywhere.
  Revisit with the rest of the multi-replica work (root `CONTEXT.md`).
- **No `round` action in the bartender's JSON schema**, unlike the sketch.
  The phrase gate replaced it, which also means a round costs no model call
  and cannot be throttled by the mention ladder.

Acceptance:
- [x] Phrase gate: every listed phrase fires, "turn around for all of us"
      does not, and the spans land on the phrase itself.
- [x] The order survives every drunk level at 200 seeds, verbatim, while
      the words around it still take their beating.
- [x] One open credit per patron; an expired one is re-used rather than
      blocking the slot forever.
- [x] A credit is drunk exactly once, and an expired one cannot be drunk.
- [x] The buyer pays for the credits that landed, not the heads counted;
      an unaffordable round leaves no credits and no charge.
- [x] A cashed drink costs the drinker nothing and lands them at buzzed.
- [x] Ticker line, metrics (granted vs cashed), help copy, and the
      `chat/CONTEXT.md` section.

Out of scope: a DB presence table, rounds for one named person (that is
`/gift`), a round in the Clubhouse tavern without @bartender, extending or
gifting a credit, and any per-buyer cooldown (the one-credit rule is the
throttle).

## Parked

- **Sealed daily bids for the marquee line**: the pot's tables with `max`
  instead of `random`. After the pot.
- **Split the shop snapshot refresh** (only if chip notifies ever get
  dense): a `chip_user_changed` notify rebuilds the whole `ShopSnapshot`
  today, catalog included; it only needs to update the balance. Not a
  problem at current traffic, noted so the fix is known.

## To discuss (not designed, not scheduled)

Owner notes to pick up in a later spitball, kept here so they are not lost:

- ~~Rebalance the big games' pricing and payouts.~~ Decided and written
  up as Phase 6 above (door milestones pay again, gated; month tiers at 40x
  eat what they pay). Still open there: Asterion's daily 4,000.
- ~~Lobby game economics.~~ Phase 7 above (pair-day cap, consolation,
  wagers). Still open in the Lobby: spectator side bets, tournaments.
- ~~A payout for the loser, gated on effort.~~ Was Phase 7's consolation;
  dropped 2026-08-27 (see the Phase 7 status block).

## Dropped

- Paid music slot (the booth is too quiet to sell against).
- Live marquee auction (replaced by the crown, which names the loser).
- Curated title list (36 noir titles, migration 149): never worth the
  moderation-free shortcut once custom titles were screened. Retired by
  migration 151; "Your Own Title" is the only title.
- Hack Room (`chat_pinned_vibe`): never sold, and the only consumable that
  restyled real room-list rows. Retired by migration 152.
- Percentage-of-balance pricing (punitive, opaque, and it taxes the one
  category that sells).
- More one-time badge packs, more 10-second room effects, anything only
  visible on the profile page.
