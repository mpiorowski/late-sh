# Chips, Leaderboards, Marketplace Rewrite

## Metadata
- Scope: full rewrite of the chip economy, leaderboard system, and a new marketplace.
- Status: design / pre-implementation. Discussed and agreed at a high level; pending second-LLM review and final pricing tweaks.
- Last updated: 2026-05-09.

## Vision

Right now chips exist but nobody cares about them. The 100-chip floor means everyone is always topped up; arcade games give nothing; the streak leaderboard rewards low-effort daily-easy clicks.

The rewrite has three big pieces:

1. **Every game pays chips.** Arcade wins pay tiered amounts (easy / mid / hard). Score-based games (Tetris, 2048) pay at game-over scaled to score. Multiplayer wins already pay through pots. TTT pays a small flat amount.
2. **Leaderboards are split, weighted, and monthly-reset.** Streaks are gone. Three categories: top monthly chip earners, weighted arcade champion, and Tetris/2048 monthly high score boards.
3. **Marketplace gives chips a sink.** 80+ cosmetic and consumable items across chat, profile, bonsai garden, aquarium, artboard, games, music, themes, and seasonal drops. Years-long collection game, not a one-month checklist.

End-of-month resets snapshot top-3 in each leaderboard category to a permanent profile-awards table. Lifetime balances and lifetime stats persist; only "this month earned" and leaderboard positions reset.

## Earn rates

All amounts are chips. "First daily" is a 2x multiplier on the first solve of that game per UTC day; "repeat" is the base rate.

| Source                         | First daily   | Repeat        | Daily cap      |
|--------------------------------|---------------|---------------|----------------|
| Sudoku / nonogram / etc easy   | 100           | 50            | natural (1/day for daily puzzles) |
| Sudoku / nonogram / etc mid    | 300           | 150           | natural        |
| Sudoku / nonogram / etc hard   | 1000          | 500           | natural        |
| Tetris                         | score / 500   | score / 500   | 500/day        |
| 2048                           | score / 500   | score / 500   | 500/day        |
| Tic-Tac-Toe win                | 10            | 10            | natural        |
| Blackjack                      | pot only      | pot only      | none           |
| Poker                          | pot only      | pot only      | none           |

Notes:
- Daily-puzzle games (Sudoku, Nonogram, Solitaire, Minesweeper) are naturally rate-limited because they only generate one puzzle per day. The "first daily" bonus is the only one realistically reachable.
- Tetris/2048 are uncapped per attempt but capped per day so AFK or scripted runs cannot farm. `score / 500` is a starting point, tune from telemetry.
- Blackjack and Poker do not get a separate chip credit on top of the pot. The pot is the reward.
- TTT pays only the winner. Draws pay nothing.

A top arcade player clearing all daily-hards earns roughly 4000 chips on first run, 2000 thereafter. ~20-30k chips per month for a hard grinder; ~10k for mid; ~3k for casual easy-only play.

## Leaderboards

Three boards. All windowed to the current calendar month (UTC). All reset at month rollover.

1. **Top chips** — sum of chips earned this month per user. Top 10 visible; full list paginated; "your rank" shown.
   - Tracks **earned**, not net balance and not lifetime balance. Hoarders do not dominate. Spending does not reduce leaderboard position.
2. **Arcade champion** — weighted points sum across all daily-puzzle games this month. Easy = 1, mid = 3, hard = 5. Top 10 visible; full list paginated.
3. **High scores** — Tetris and 2048, best score this month per user. Two boards, top 10 each.

Streak leaderboard is removed entirely.

Each leaderboard supports an "everyone + your rank" paginated view. UI is deferred (out of scope for this plan; just confirm the data exposes ranks).

## Marketplace

### Pricing tiers

- **Common (1k - 5k)**: small visual touches, repeated buys
- **Mid (8k - 25k)**: signature personal cosmetics
- **Big (30k - 75k)**: prestige items, signal high effort
- **Prestige (100k +)**: rare, year-long chase
- **Earned-only (not buyable)**: monthly leaderboard top-3 badges, anniversary items, founder tier

A casual easy-only player (~3k/month) clears one common item per month, ~one mid item per quarter. A top player (~30k/month) clears one prestige item every 3-4 months. Total non-prestige catalogue ≈ 200k+, ~7 months for a top grinder. Prestige + seasonal items keep even fully-cleared accounts chasing.

### Item catalogue

#### Chat presence

| Item                          | Tier     | Approx price |
|-------------------------------|----------|--------------|
| Title slot (curated word pool, prefix or suffix) | Common  | 3k       |
| Username flat color           | Mid      | 8k           |
| Username gradient (2-3 color blend) | Big | 35k          |
| Animated username (subtle hue cycle) | Prestige | 100k    |
| Custom join/leave line (curated pool) | Common | 4k         |
| Mention sound variant         | Common   | 2k           |
| Reply flourish (border / glyph) | Mid    | 12k          |
| Auto-signature line (per-message toggle) | Mid | 10k        |
| Bot-style prefix icon         | Mid      | 10k          |
| Sticky status under username  | Mid      | 8k           |
| Emoji slot remap (per slot, 8 slots total) | Mid | 5k each |

Emoji remap is **per-user**: when you press slot N, your reaction posts as your chosen emoji from a curated pool. Render-time lookup keyed by reaction author, so others see your custom emoji on your reactions. Stored as `(user_id, slot, emoji)` overrides; reaction column stays integer-keyed.

#### Profile

| Item                          | Tier     | Approx price |
|-------------------------------|----------|--------------|
| Profile frame / border        | Big      | 50k          |
| ASCII portrait slot (curated) | Mid      | 15k          |
| Banner art (curated)          | Big      | 40k          |
| Bio styling (markdown / color spans) | Mid | 10k         |
| Bio length extension          | Common   | 5k           |
| Achievement showcase (pick 3 monthly badges to inline) | Mid | 8k |
| Anniversary item (yearly window only) | Earned-only | n/a |
| Founder tier (first N buyers, then gone) | Prestige | 75k |

#### Bonsai garden

| Item                          | Tier     | Approx price |
|-------------------------------|----------|--------------|
| Tree species (cherry, maple, juniper, pine, willow) | Mid | 12k - 20k each |
| Pot variants (glazed, wood, stone, marble, kintsugi) | Common - Mid | 5k - 15k |
| Background scenes (mountain, sunset, snow, fog, rain) | Mid | 10k each |
| Weather effects (petals, snow, fireflies) | Mid | 12k each |
| Stones / lantern / moss accents | Common | 3k - 8k |
| Multi-bonsai display slot     | Prestige | 100k         |
| Day/night cycle               | Mid      | 15k          |

#### Aquarium

| Item                          | Tier     | Approx price |
|-------------------------------|----------|--------------|
| Tank size tier (small / medium / large) | Big | 25k / 50k / 80k |
| Decor (castle, ship, kelp, coral, chest) | Mid | 8k - 18k each |
| Fish species (common to legendary) | Common - Big | 5k - 25k each |
| Bubble pattern / lighting color | Common | 3k       |
| Fish food (consumable, fish do tricks 1h) | Common | 2k |
| Sea floor variant (sand, gravel, slate) | Common | 4k |

Aquarium replaces bonsai when bought, or coexists as a second slot (decide at implementation).

#### Artboard

| Item                          | Tier     | Approx price |
|-------------------------------|----------|--------------|
| Canvas size upgrade           | Big      | 50k          |
| Extra color palette (synthwave, vintage, mono, pastel) | Mid | 10k each |
| Saved palette slots           | Common   | 5k           |
| Layer slots                   | Big      | 30k          |
| Animated stroke effect        | Big      | 40k          |

#### Game cosmetics

| Item                          | Tier     | Approx price |
|-------------------------------|----------|--------------|
| Card back skin (Blackjack, Poker, Solitaire) | Mid | 12k each |
| Felt color (Blackjack, Poker) | Mid      | 10k each     |
| Tetris piece theme (neon, glass, pixel, gems) | Mid | 15k each |
| 2048 tile theme (planets, fruits, currencies, kanji) | Mid | 15k each |
| Sudoku notation style         | Common   | 5k           |
| Minesweeper mine icon (heart, skull, custom) | Common | 4k each |
| Nonogram color scheme         | Common   | 5k each      |
| TTT mark style (sun/moon, cat/dog, custom) | Common | 4k each |
| Win celebration animation     | Mid      | 12k          |
| Personal dealer name (Blackjack, only visible to you) | Common | 5k |

#### Music

| Item                          | Tier     | Approx price |
|-------------------------------|----------|--------------|
| Force-vote (consumable)       | Big      | 15k          |
| Skip-vote (consumable, weaker) | Mid     | 7k           |
| Queue-jump (consumable)       | Mid      | 10k          |
| Theme playlist unlock         | Mid      | 12k each     |
| Now-playing footer customization | Common | 5k         |

#### Themes / dashboard

| Item                          | Tier     | Approx price |
|-------------------------------|----------|--------------|
| Premium themes beyond base set | Mid     | 10k each     |
| Theme tweaks (warmer Nordic, harsher Cyberpunk, etc) | Common | 5k each |
| Custom dashboard MOTD shown to profile visitors | Mid | 8k |

#### Consumables / boosters

| Item                          | Tier     | Approx price |
|-------------------------------|----------|--------------|
| 2x chip booster (1 hour)      | Common   | 3k           |
| 2x chip booster (1 day, rare) | Mid      | 20k          |
| First-daily reset             | Common   | 4k           |
| Mystery box (random low-tier cosmetic) | Common | 3k     |
| Gift chips to another user (10% sink fee) | n/a | variable |
| Public shoutout (1 line to global chat) | Big | 30k     |

#### Seasonal / time-limited

- **Holiday badges**: Halloween hat, Christmas tree, New Year fireworks, etc. Available only that month, then gone permanently.
- **Monthly themed cosmetic drop**: one item only purchasable that month. Becomes a status marker for "I was here in May 2026".

#### Social

| Item                          | Tier     | Approx price |
|-------------------------------|----------|--------------|
| Sticker pack (one-shot decorative messages, curated) | Mid | 10k |
| Mention highlight color (gift to receiver) | Common | 5k |
| Anonymous mode toggle (1 hour, logged server-side) | Mid | 10k |

### Items deliberately not shipped

- ❌ **Hint chips** for sudoku / nonogram / minesweeper. Pay-to-win on daily puzzles, corrupts arcade leaderboard.
- ❌ **Reroll daily puzzle**. Same reason.
- ❌ **Lucky shoe / dealer reveal** in Blackjack. Breaks game fairness.
- ❌ **Hide your activity / hide profile**. Anti-social, hurts community.
- ⚠️ **User-uploaded portrait / banner / emoji**. Moderation rabbit hole. Curated pools only.
- ⚠️ **Free-text join / leave / signature lines**. Moderation. Curated pools only.
- ⚠️ **Anonymous mode**: cute but abusable for harassment. If shipped, log every anon-mode message server-side with the real user_id.

## Monthly reset and permanent profile badges

At UTC month rollover:

1. Snapshot top-3 in each leaderboard category to a `profile_awards` table (one row per (user_id, category, place, month)).
2. Award the corresponding permanent badge. Date-stamped, finite supply.
3. Leaderboards naturally re-window because all queries filter on `>= date_trunc('month', now())`. No data deletion needed.

UI invariants:
- Top-1 of any monthly category awards a profile crown next to the chat username.
- Top-1 / 2 / 3 all stored in profile section, but only the most recent 3 inline-render in chat.
- Long-term: monthly badges roll up. After a year, "5x Arcade Champion (2026)" replaces five individual month badges in the inline display. Underlying data preserved.

**No chip rewards for placing on the leaderboard.** The badge is the reward. Adding chip bonuses to winners just inflates the economy and widens the gap between top and casual players. Chips are for cosmetics; monthly badges are for status. Two separate currencies, intentionally.

## Foundation: chip ledger

Before any of the visible pieces, build the unglamorous core:

- New table `chip_ledger`: `(id, user_id, delta, reason, source_kind, source_ref, created_at)`.
- Every chip credit and debit goes through it. Replaces ad-hoc balance updates.
- `reason` is structured (enum): `arcade_win`, `tetris_score`, `2048_score`, `ttt_win`, `blackjack_pot`, `poker_pot`, `marketplace_purchase`, `gift_sent`, `gift_received`, `monthly_reset` (none, kept for parity), `admin_grant`, etc.
- Monthly leaderboard "top chips" is `SUM(delta) WHERE delta > 0 AND created_at >= start_of_month` per user. Marketplace spends do not reduce leaderboard position because the filter excludes negative deltas.
- Per-source daily caps enforced at write time: query sum of positive deltas for that source today, reject if over cap.
- Daily-first-win bonus enforced at write time: query existence of any prior credit for that source today; if none, write at 2x rate.
- All anti-cheat / refund / "spent this month" / monthly reset logic queryable from one place.

This is the keystone. Without it, monthly leaderboards are guesswork and marketplace history is lost.

## Chip floor decision

Keep the 100 floor for now. Revisit in phase 6 once we know whether new users can realistically earn chips before they want to sit at a multi table.

If we decide to remove it later, the right replacement is a **one-time signup grant** of 200-500 chips, not auto-restore. After that, you earn or you're broke. But not yet.

## Implementation phases

### Phase 1: Chip ledger + earn rates

- Migration: `chip_ledger` table.
- `ChipService::credit(user_id, delta, reason, source_kind, source_ref) -> Result<i64>`. Returns new balance. Enforces daily caps and first-daily 2x.
- `ChipService::debit(...)` symmetric. Used for marketplace.
- Backfill `user_chips.balance` is unchanged, but every mutation also writes a ledger row going forward.
- Wire chip credits into every game-over / win path:
  - `late-ssh/src/app/arcade/sudoku/`, `nonogram/`, `solitaire/`, `minesweeper/`: on daily-win publish.
  - `late-ssh/src/app/arcade/tetris/`, `twenty_forty_eight/`: on game-over with score.
  - `late-ssh/src/app/rooms/tictactoe/svc.rs`: on win settlement.
  - Blackjack and Poker pot settlements already debit/credit; route them through the ledger but keep the same payout math.
- Reuse the existing `ActivityPublisher::game_won` events for source_ref where applicable.

Files touched: `late-core/src/models/chips.rs`, new `late-core/src/models/chip_ledger.rs`, every game's svc.rs, new migration in `late-core/migrations/`.

### Phase 2: Leaderboard rewrite

- Rewrite `late-core/src/models/leaderboard.rs::fetch_leaderboard_data`:
  - Drop `streak_leaders`, `user_streaks`, `today_champions`.
  - Add `monthly_chip_earners: Vec<LeaderboardEntry>` (sum positive ledger deltas this month).
  - Add `arcade_champions: Vec<LeaderboardEntry>` (weighted arcade points this month).
  - Keep `high_scores: Vec<HighScoreEntry>` but window to current month.
  - Return `your_rank: Option<UserRanks>` for the requesting user across each board.
- `late-ssh/src/app/arcade/leaderboard/svc.rs` now needs to know which user is asking (refresh per session, or refresh global + compute per-session ranks at read time).
- Update all UI render paths that consumed `today_champions` / `streak_leaders` / `user_streaks` / `BadgeTier` from streak.
- BadgeTier logic moves: streak-based tiers are removed; new BadgeTier (or a new type) sources from `profile_awards` instead.

Files touched: `late-core/src/models/leaderboard.rs`, `late-ssh/src/app/arcade/leaderboard/`, every consumer of `LeaderboardData::badges()` or `badge_for()` (chat ui_text.rs, profile ui, etc).

### Phase 3: Marketplace MVP

- Migrations:
  - `marketplace_items(id, slug, name, description, tier, price, category, rendering_hint, available_from, available_until)`. Static-ish; seed from a Rust const list at migration time.
  - `user_purchases(user_id, item_slug, acquired_at, equipped, metadata jsonb)`. `metadata` holds e.g. selected color, selected emoji slot, etc.
- New module `late-ssh/src/app/marketplace/`:
  - `svc.rs`: list, purchase (debit through ledger, write purchase row), equip / unequip, list-owned-by-user.
  - `state.rs`, `input.rs`, `ui.rs`: the screen, modeled on rooms screen.
  - Add a screen number to `Screen` enum. Probably key `5`.
- MVP item set (~5-6 items) to prove the flow:
  - Username flat color
  - Title slot
  - One starter badge
  - Force-music vote (consumable, exercises debit + one-shot apply)
  - Mention sound variant
  - Emoji slot remap (1 slot)
- Chat rendering reads `user_purchases` to apply username color, title, badge inline. Cache per session, refresh on purchase event.

Files touched: new `late-ssh/src/app/marketplace/`, `late-ssh/src/app/screen.rs` (or wherever Screen enum lives), new core models, chat ui_text.rs to wire color/title/badge in renders.

### Phase 4: Marketplace expansion

- Add the rest of the catalogue in batches: chat presence, profile, bonsai, aquarium, artboard, game cosmetics, music, themes.
- Bonsai already exists; add tree species / pot / scene / weather as variants of `bonsai_state`.
- Aquarium is new: `aquarium_state` table modeled on `bonsai_state`.
- Each cosmetic category needs its own renderer hook. Most are pure rendering changes once the purchase data is queryable.

### Phase 5: Monthly reset + permanent badges

- Migration: `profile_awards(user_id, category, place, month, awarded_at)`.
- Cron / scheduled task at UTC month rollover:
  - Snapshot top-3 per category from the just-ended month.
  - Insert into `profile_awards`.
  - No data deletion. Leaderboards re-window automatically because they filter by current month.
- Profile UI renders awards section.
- Chat ui_text.rs picks the most recent 3 awards for inline rendering next to the username.
- Top-1 of any current-month-finalized category renders a crown glyph.
- Roll-up logic: if a user has 3+ awards in the same category, the inline render collapses them to "Nx <Category Champion>".

### Phase 6: Chip floor revisit + signup grant

- Decide based on Phase 1-5 telemetry: are new users actually stuck at 0?
- If yes, replace `restore_floor` with a one-time signup grant. Remove the floor entirely on accounts that have ever earned chips.
- If no, leave the floor at 100 and move on.

### Phase 7: Daily quests + multipliers (deferred)

- Out of scope for this rewrite. Mentioned by user as "for later."
- Builds on the chip ledger, so it should be easy when we get there.

## Open decisions deferred to implementation time

1. Does buying the **aquarium** replace the bonsai, or do they coexist as two slots? Probably coexist.
2. Username **gradient** vs flat color: same render path or separate? Probably separate, gradient stores 2-3 hex codes in `metadata`.
3. **Per-IP rate limit** on chip earning to deter SSH multi-account abuse, or trust SSH-account-cost as friction? Probably no rate limit at first; revisit if abuse shows up.
4. **Show on hover** vs always-on for cosmetics like reply flourish: defer to UI taste.
5. **Refunds**: do we let users sell back items? Probably no. Adds economy complexity for marginal value.
6. **Gift chips fee**: 10% sink, 15% sink, or no gifting at all? Lean toward 15% to make farming unattractive.

## Test guidance

- Pure logic (chip cap math, weighted arcade points, monthly window math, marketplace purchase validation) gets inline unit tests.
- Anything touching `chip_ledger`, `marketplace_items`, `user_purchases`, `profile_awards`, or service tasks goes in `late-ssh/tests/` with testcontainers via the existing helpers.
- Do not run `cargo test`, `cargo nextest`, or `cargo clippy` as an agent in this repo. Leave those gates for the human owner.

## Numbers anchor for second-LLM review

- Top hard-arcade player: ~20-30k chips/month
- Mid arcade player: ~10k/month
- Casual easy-only player: ~3k/month
- Total non-prestige catalogue: ~200k chips
- A top player needs ~7 months to clear non-prestige; ~years for prestige
- Casual player gets one common item per month, one mid item per quarter
- Monthly reset means leaderboards stay fresh; permanent badges accumulate as the long-term prestige hook
