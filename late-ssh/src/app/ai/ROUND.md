# The Round: drinks and rounds from @bartender

A single drink, a whole-bar round, and the credits a round leaves behind: the
chips, the pour, the drunk points, the trigger phrase. @bartender's responder
lives in `ghost.rs`; the money and the rows are in `late-core`
(`models/drinks.rs`, `models/drink_round.rs`). The rest of the AI domain is in
`CONTEXT.md`.

**A single drink first.** `ChipService::buy_drink` is one transaction: a
floor-guarded `ChipMove::DrinkPurchase` debit (it pours only if the balance
keeps the 100-chip floor; `source_ref` the drink name) and the buyer's
`user_drinks` upsert (`late-core/src/models/drinks.rs`: drunk points decay at
read on wall clock alone, online or not, capped at `MAX_DRUNK_POINTS` 4000;
`DRUNK_SOBER_UP_HOURS` (12) is the one dial). @bartender's model
(`app/ai/ghost.rs`, ungrounded schema JSON) names the drink, the price and the
required `intoxicating` flag: intoxicating drinks cost 100-1000, a
non-intoxicating one exactly `SOFT_DRINK_PRICE` (50); any other price, or one
the patron cannot afford, is refused uncharged, so the debit always matches
the quoted line. A non-intoxicating pour, paid or comped, moves only the count
and the spend, never the buzz or its decay clock. Drunk levels print a
`(word)` label on chat author headers from level 1 and slur public-room text
(`../chat/CONTEXT.md` §14 Drunk Text).

One patron buys everyone at the bar a drink. The chips are burned like the
crown's; what the room gets is a #lounge line and a free drink each.
The same credit also serves a one-person gift: `@bartender buy @user a drink`
or another entry on the closed `GIFT_PHRASES` list (a verb, the name and what
is bought: "get @user a beer", "send @user a round", "pour @user one"; what is
bought for whom: "drink for @user", "one for @user"; or whose is on whom:
"@user's next one is on me"), read on the
round's rules (any case, opening a clause anywhere in the message, not a
`?` sentence, not in backticks; see "The trigger is a literal phrase" below) by
`drink_round::gift_drink_target`, which also refuses a message naming any
second handle besides the bartender's, so the recipient is never a guess. A
message places one order at most: `drink_round::bar_order` reads both lists
once for the bartender, and a gift and a round in the same message ring up
neither and go to the model as talk. The handle is resolved against the
account username, trailing dots dropped as sentence punctuation (a username
never ends in one, `sanitize_username_input`).
It costs 200 chips (`GIFT_DRINK_PRICE`, twice a round's head, since the one
person it is aimed at will drink it), pours the recipient the same 400 points
a round does, works for offline humans, shares the 24h expiry and
three-open-credit cap, and does not pour the buyer a drink. The gift and
its floor-guarded `drink_gift` debit (`ChipMove::DrinkGift`, its own reason
so the profile ledger reads "bought a drink for @user" off the round's
single credit and the round dashboards never count it) commit together
through `ChipService::buy_drink_for`; self-gifts, bot targets, unknown
names, full tabs and short balances are refused uncharged. A successful
gift gets a scripted receipt ahead of the bartender's mention ladder and
AI, while refusals step the ladder. `metrics::record_gift_drink_bought`
counts settled gifts and their burn, `record_gift_drink_refused` labels
`late_ssh_gift_drinks_refused_total` by the closed `GiftDrinkRefusal`, and
the pour itself lands under the shared `round_drinks_cashed_total`.
Redemption uses the same oldest-expiring credit path and names the buyer of
the credit actually spent.

**What each purchase costs and pours.** Points follow the bar that sold the
credit (`drink_rounds.bar`, `Bar::drink_points`), not where it is cashed: a
credit is good at either bar and is worth what its seller wrote.

| Purchase | Buyer pays | Each drinker gets | Buyer drinks too? | Ledger reason |
|---|---|---|---|---|
| Tavern round ("round for everyone") | `ROUND_PRICE_PER_PATRON` (100) a head | `ROUND_DRINK_POINTS` (400) | yes, 400 | `round_purchase`, bar `tavern` |
| Gift ("@bartender buy @user a drink") | `GIFT_DRINK_PRICE` (200) | 400 | no | `drink_gift`, bar `tavern` |
| Nightcap round (`r` on the stools) | `ROUND_PRICE_PER_PATRON` (100) a stool | 100 | yes, 100 | `round_purchase`, bar `nightcap` |

Every drink taken off a round, the buyer's own included, is logged in
`drink_pours` like any other pour (`UserDrinks::record_pour`), under the bar
that poured it, not the bar that sold it; buying a round or a gift counts
nothing for the buyer beyond their own drink.

`late-core/src/models/drink_round.rs` owns both tables (migrations 164 and
168), the price, the cap, and the phrase list; `GhostService::bartender_round`
(`app/ai/ghost.rs`) owns the transaction's caller, the refusals, the
telemetry, and what @bartender says; `ChipService::buy_round` owns the money.
It touches chat twice, which is why it is documented here: the phrase gate
reads a chat message, and `chat/slur.rs` has to leave that phrase alone.

- **The trigger is a literal phrase.** `ROUND_PHRASES` ("round for everyone",
  "drinks for the house", "everyone a drink", "drinks on me", "round's on
  me", ...) matched case-insensitively on word boundaries, so "turn around
  for all of us" is not an order. Every entry names the whole house or says
  "on me"; a bare "a round" is not on the list. No model
  decides this: it is the only bartender action that spends more than one
  drink's worth, the price is the size of the room, so the phrase is the
  confirmation. An order is a statement: `contains_round_request` rejects a
  phrase whose sentence runs on to a `?` and never looks inside backticks, so
  "how much is a round for everyone?" is a question the model answers, not a
  bill. The `?` scan reads the whole message and skips code spans, so
  "round for everyone in `#lounge`?" is a question too. An order also has to
  open its clause (`is_order`, shared with the gift): between the last
  `, . ! ? ; :` or line break and the phrase there may only be the
  bartender's `@name` and entries from the closed `LEAD_INS` list: small
  talk ("a", "another", "please", "the next"), saying you are about to order
  ("I'll", "I'd like to", "let's", "can I"), and the verb when the phrase
  starts at what is bought ("buy", "get", "do", "put"). Nothing negative,
  past or conditional is on it. "I'll buy a round for everyone", "let's do a
  round for the house" and "don't worry, round on me" order; "no round for
  everyone tonight", "he said round on me", "we had drinks for everyone" and
  "@alice drinks on me" spend nothing and the model gives the words to say.
  (`round_phrase_spans`, the slur guard's view, still protects the words
  wherever they appear.) It also means a round costs no model call.
- **A settled round answers ahead of the mention ladder**, in both the event
  loop's pre-filter and the reply, because throttling a paid action would
  swallow a purchase in silence. A refusal is free, so it steps the ladder
  like any other answer and is dropped when throttled: repeating the phrase
  into an empty house costs the room one @bartender line per ladder window.
- **`slur.rs` never scrambles it.** Drunk text is stored, not rendered
  (`../chat/CONTEXT.md` §14 Drunk Text), so a wasted patron's order would otherwise reach the
  matcher as "ronud for eevryone" and the feature would break for exactly the
  people most likely to use it. `slur_segment` passes any token overlapping a
  `spending_phrase_spans` range (the round phrases and the gift phrases,
  each widened back over the lead-in words it opens its clause with, so a
  scrambled "please" cannot stop an order being one) through untouched, and
  `with_hiccup` will not drop a `*hic*` inside one; a hiccup right before the
  lead-in is itself on `LEAD_INS`.
  The guard and the matchers read the same lists in `drink_round.rs`, so they
  cannot drift apart. The words around the order
  still take their beating.
- **Price** 100 (`ROUND_PRICE_PER_PATRON`) for every credit that actually
  landed, never for the heads counted: the grant's own `RETURNING` is what
  the charge is computed from, in the same transaction, so two rounds racing
  cannot both bill for the same patron. `ChipMove::RoundPurchase`,
  floor-guarded, burned whole, out of Top Chips like the crown.
- **Presence is `state::online_human_ids_excluding`**, the in-process
  `active_users` roster minus the bots and the buyer. Single-replica by
  choice: there is no presence table (`users.last_seen` is written at connect
  only), building one was more work than the feature, and the round joins the
  multi-replica debt in the `SCALE.md` Pain Point 2 ledger; the credits it grants are DB
  rows and cash from anywhere. Excluding the buyer is what makes "nobody to buy for" a real
  refusal rather than a round bought for one.
- **Only the buyer is poured into**, on the spot, `ROUND_DRINK_POINTS` in the
  purchase transaction: they typed the order. That number is the tavern's,
  a premium over the price a head because this round buys for everyone
  online and most of them never walk up; the Nightcap's round is tagged
  `bar='nightcap'` and pours 1:1 instead (`Bar::drink_points`,
  `../clubhouse/nightcap/CONTEXT.md` §5). Everyone else gets a
  `drink_credits` row, not a drink, because a pour makes someone type drunk in
  public and they did not ask. It is cashed only by ordering from @bartender,
  24h to claim.
- **Credits stack, up to `MAX_OPEN_CREDITS` (3).** The cap lives in the
  grant (migration 168; there is no one-open-credit index), which counts a
  patron's open, unexpired credits and skips anyone at it: a round into a
  room that will not drink still reaches nobody and is still free, so the
  throttle is a number that can be tuned rather than a schema fact. A
  schema-level one-credit limit would do two jobs at once, keeping a patron
  from holding two drinks, which is not the point, and throttling the
  mechanic, and a patron heads-down through three rounds would end the night
  owed one drink while two buyers bought nothing. Expired credits are
  neither counted nor re-used, just rows nobody can drink.
- **Every grant takes an advisory lock** (`ROUND_GRANT_LOCK`) and holds it to
  commit. The cap is a read-then-write over rows a concurrent round is
  inserting and cannot see, so without it two rounds landing together would
  both read a patron at two credits and both grant a third. Rounds are rare,
  so serializing them against each other is the cheap fix, and it also pins
  the order the patrons' rows are locked in, which two overlapping rounds
  working off differently ordered presence reads could otherwise deadlock on.
- **A pour spends the credit closest to expiring**, picked `FOR UPDATE SKIP
  LOCKED`, so a banked drink is never lost to the clock while a fresher one
  sits behind it and two orders landing together take two different credits
  rather than fighting over one (two orders are two drinks). The same
  statement counts what is left, excluding the row it just cashed by id,
  because a data-modifying CTE's write is not visible to the rest of its own
  statement.
- **A cashed drink pours a flat 400 points** whatever the bartender named it:
  four times what the buyer paid. At exactly the buzzed threshold (300) the
  first decay tick (334 an hour) would drop the drinker back to tipsy seconds
  after the pour; 400 buys about eighteen minutes of the level
  the round is meant to hand out. `lifetime_spent` does not move.
  The open credit is read before the prompt is built and becomes the
  `BartenderTab` handed to `parse_bartender_order`: on a comped tab a "pour"
  (or an "offer", the model deciding they could not afford it) becomes
  `PourComped` and skips both price gates, since nothing is debited; that is
  what lets a patron sitting on the chip floor, or a model that obeyed "do
  not quote a price", still drink what was bought for them. If the credit is
  gone by the time the pour lands, nothing is poured or charged and a
  scripted line says so.
- **Scripted lines, not generated ones.** The announcement and all three
  refusals are consts in `ghost.rs`. This is the one line that has to quote
  the number the patron was charged, and it has to land the moment the chips
  move rather than after a model round trip that could time out or invent a
  different figure. The prompt hands out the phrase when asked but never
  claims a round happened. The same rule covers who bought the drink and what
  a patron has left: the pour picks the credit closest to expiring at pour
  time, which with a stacked tab is not always the one that was open when the
  prompt was built, so `ROUND_CREDIT_ON_LINES` names the buyer of the credit
  actually spent and, when the pour leaves credits behind it,
  `ROUND_CREDIT_REMAINING_LINES` follows with the count, both appended to the
  model's line in code. The prompt is told to guess at neither, because a
  name or a number in that line is a promise the bar has to keep.
- **Feed.** `ActivityKind::RoundBought` posts a ticker line and no headline:
  @bartender already says it in the room, and everyone it reached is online
  by definition, so a #lounge row would be the third telling of one drink.
