# Plan: Non-Intoxicating Drinks

## Objective
Implement a way for `@bartender` to pour non-intoxicating drinks (like water, coffee, or soda) without increasing the patron's intoxication level or inappropriately resetting their sober-up timer, while still charging them chips (a minimum of 50 chips).

## AI Schema & Prompt Updates
- **Schema (`late-ssh/src/app/ai/ghost.rs`)**: `bartender_order_schema()` requires the nullable boolean `intoxicating` field. The prompt and response example request an explicit boolean for pours and offers, with null allowed for chat. Parsing defaults missing/null classifications to intoxicating for legacy or repaired responses.
- **Prompt**: Updated the bartender's system prompt to explicitly instruct it to set `intoxicating: false` if a patron is cut off, if they explicitly ask for a non-alcoholic drink (e.g., "NA", "Shirley Temple"), or if they order an obviously non-intoxicating beverage (water, coffee, orange juice, milk, etc.). The prompt also dictates the pricing tiers: intoxicating drinks run 100-1000 chips, while non-intoxicating drinks are exactly 50 chips (`SOFT_DRINK_PRICE`), and the server refuses any other price on them.

## Backend Parsing Logic
- **Price Floors (`parse_bartender_order`)**: Modified the price-validation bounds. Previously, any drink under 100 chips was silently downgraded to an uncharged chat line. Now, if `intoxicating` is `false`, the only accepted price is `SOFT_DRINK_PRICE` (50 chips), so the 50-chip coffee order is processed as a valid `Pour`.

## Service Layer (ChipService)
- **`buy_drink` & `cash_round_drink` (`late-ssh/src/app/games/chips/svc.rs`)**: Both core methods now accept an `intoxicating: bool` argument.
- When an order passes through the `Pour` or `PourComped` decisions, the extracted `intoxicating` boolean is carried down the stack to the database layer.
- Hardcoded calls like the buyer's portion of a round (`buy_round`) pass `intoxicating: true` by default since rounds are exclusively alcoholic.

## Database Layer (UserDrinks)
- **`record_purchase` and `record_comped_pour` (`late-core/src/models/drinks.rs`)**: Updated to accept the `intoxicating: bool` argument.
- **`record_pour` SQL Branching**: Rather than hacking the core decay math, the UPSERT branches based on `intoxicating`.
  - **Intoxicating**: The existing query continues to add points, correctly update `last_drink_at`, and calculate linear decay.
  - **Non-Intoxicating**: A new parallel query runs that explicitly inserts `0` points and `1970-01-01 00:00:00Z` for `last_drink_at` if the user's first drink ever is water. `ON CONFLICT`, it updates ONLY `lifetime_spent`, `drink_count`, and `updated`.
  - This perfectly ensures that a non-intoxicating drink does not reduce intoxication faster, and does not interact with the user's time-since-last-drink timer at all.
