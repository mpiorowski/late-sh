# The Crown

Own domain (`app/crown/`); chat owns only the `/crown` command and the glyph (`../chat/CONTEXT.md`).

One slot, one holder, one 💎 after their name; last month's winner, the
laureate, wears a 👑 before theirs all month. The crown is not a rental and
not a Shop item: it is a single row you take off whoever has it by paying
at least the next rung, and every chip is destroyed.
`late-core/src/models/crown.rs` owns the table (migration 156) and the price
ladder; `late-ssh/src/app/crown/svc.rs` owns the transaction, the refusals,
the telemetry, and the #lounge line. It lives outside `chat/` because it is
its own domain; only the command and the glyph are chat's.

- **Price.** `next_price(paid)` = `max(500, ceil(paid * 1.5))`, and a
  vacant crown is always 500, a Bronze gild's price, so the month's race
  starts on day one. The ladder from empty is 500 / 750 / 1,125 / 1,688 /
  2,532 / 3,798 / 5,697 / 8,546. Nobody tunes it: it ratchets with whoever
  last paid, and the ratchet, not the floor, is what makes it dear.
- **Bids.** `/crown take` pays the price (`CrownBid::AtPrice`); `/crown
  take N` pays exactly N (`CrownBid::Offer`) when N is at least the price.
  The reign records what was paid, so the next rung is 1.5x the bid: an
  overbid is how a holder puts the crown out of a rival's reach. The bid is
  checked under the take's lock (`CrownBid::charge`), so an offer a racing
  take overtook is refused (`BidTooLow`), never charged at a rung the bidder
  never saw. The composer only admits a positive whole number.
- **Burn.** `ChipMove::CrownTaken` is a floor-guarded debit with
  `source_ref` = the reign id, and there is no matching credit reason
  anywhere. The whole price leaves the money supply, so the burn is the
  absence of a credit rather than a transfer to a house wallet. Like
  `ShopPurchase` it is `counts_as_earnings = false`: Top Chips ranks
  earnings and a debit never counts (pinned by
  `chips_test::earning_exclusions_and_reason_uniqueness`).
- **Guards** (`CrownService::take`, one closed `CrownRefusal` enum with the
  wording): you already wear it, a bid under the price, and the chip floor.
  That is all: there is
  **no hold or cooldown**. A reign is takeable the moment it exists, at the
  next rung, so the month end is a real auction (the last take before
  midnight wins the badge) rather than a clock game around a hold window.
  The 1.5x ladder is the only throttle on a war, and a war is the story.
  Known cost: a bare take that races another one pays the rung the other
  just set, not the price `/crown` quoted a moment earlier; the receipt
  banner says what was paid (a bid caps that). Every refusal is uncharged, and a refusal drops the
  transaction, so nothing is left behind.
- **Transaction** (`CrownReign::lock_open` then close, open, debit, notify).
  Two locks, and neither replaces the other: `pg_advisory_xact_lock` is what
  serializes a take from a *vacant* crown (there is no row to take
  `FOR UPDATE`, so without it two first takes would both insert and one
  would die on the partial unique index), and the `FOR UPDATE` keeps the
  read-then-write on an existing reign exact. A second racing take therefore
  reads the reign the first one opened and pays the next rung, not a
  constraint violation.
  After both locks, a separate `clock_timestamp()` query captures the take's
  effective time. That one timestamp drives the current-month check, closes
  the outgoing reign, and stamps the incoming reign and its UTC month.
  Do not use `current_timestamp` here: transaction-start order can differ
  from lock-acquisition order, making an end timestamp predate its reign.
- **One open reign, ever.** `crown_reigns_single_open` is a unique index on
  the constant `(ended_at IS NULL)` filtered to open rows, so "at most one"
  is a table fact.
- **Month rollover has no sweeper.** A reign carries the first of the UTC
  month it was taken in (stamped in SQL from the same clock as `taken_at`,
  so a take blocked across midnight cannot be born stale), and
  `CrownReign::is_current` requires that month to still be the current one.
  At the boundary the crown simply reads as vacant at the minimum, the glyph
  stops resolving on the next 1Hz tick, and the next take closes the stale
  row on its way past. Same read-time expiry the rentals use. That makes
  `ended_at` the moment the row was replaced, not the moment the reign
  stopped counting; a history reader wants `LEAST(ended_at, month + 1
  month)`.
- **Distribution.** `CrownService` holds a process-shared
  `watch<CrownWearers>`: the open reign and the last reign of each of the
  two newest months (`CrownReign::last_of_recent_months`), each a
  `CrownHolder` (user id plus month). `holder(now)` and `laureate(now)`
  resolve both at read time, so the rollover turns the last holder into the
  laureate with no refresh. The laureate is the same rule as the `CRWN`
  award's CTE, read from `crown_reigns` so the glyph does not wait on the
  award snapshot loop. The watch is seeded by the listener and refreshed on
  the `crown_changed` notify
  (`start_notify_worker` over the process listener, `pg_listener.rs`,
  resyncing after a reconnect). The notify payload is a
  `CrownChange` (taker name, price, deposed id): the holder is re-read from
  the table, and the deposed holder's banner is raised from the payload as a
  `CrownEvent::Deposed`, so it lands on whichever replica they are on. The
  selling replica is not special-cased: it learns about its own take, and
  tells its own deposed holder, the same way a second replica does
  (`svc_test::a_listening_replica_learns_the_holder_and_tells_the_deposed`).
- **Rendering.** `App::tick` reads the watch on the same once-a-second edge
  as the flair directory and passes `holder(now)` and `laureate(now)` into
  `common/username_effect.rs::resolve_all`, which sets `ResolvedName.crown`
  and `ResolvedName.laureate`. Every surface that already reads `name_flair`
  therefore gets both for free and no render ever queries for them; a holder
  or laureate who has bought nothing else gets an entry of their own. The
  holder's `CROWN_GLYPH` (💎, two cells) follows the name after one space
  (`👑 bob 💎, the night clerk 🐱` for someone who is both): it sits ahead of
  a rented title and ahead of the badge stack, carries no clickable segment
  of its own, and is painted in `AMBER_GLOW` rather than taking the name's
  effect (`ui.rs::build_author_prefix_and_segments_with_chat_badges` builds
  the range, `ui_text.rs::push_author_prefix_spans` paints it). The
  laureate's `CROWN_LAUREATE_GLYPH` (👑, two cells) leads the name, after
  the friend star, in the author style, with no segment of its own. Chat
  measures both with `unicode_width`. The Clubhouse floor label shows the
  holder's glyph only (`clubhouse/ui.rs::clubhouse_label` glues it on,
  `put_label_styled` paints the char at `name_len` amber and spends two
  cells on it, the emoji plus a `WIDE_TAIL` sentinel the row flush skips,
  so the walls stay aligned); names and titles there are folded to single
  width.
- **Commands.** `/crown`, `/crown take` and `/crown take N` are parsed in `submit_composer`
  and drained by `App::tick_crown`. Both answers arrive as banners off
  `CrownEvent`, because the crown service is not `ChatService` and has its
  own broadcast (the `StreamService` shape). The deposed holder is told who
  took it (`CrownEvent::Deposed`, off the notify): a glyph vanishing off
  your own name with no explanation reads as a bug.
- **Feed.** `ActivityKind::CrownTaken` fires on every takeover and names both
  players. It is the one event with two #lounge surfaces: the usual ticker
  line ("tom stole the crown from mira for 1,688", `· `-prefixed, diverted
  into the one-row ticker like every system line) and a **headline**, a
  real message from `system` with no prefix, so it renders as a chat row
  and stays in history: "💎 @tom stole the crown from @mira for 1,688
  chips. Next price: 2,532 chips." (`activity/filter.rs::lounge_headline`, the
  exhaustive twin of `lounge_includes`; posted by the lounge feed task right
  after the ticker line, behind the same repeat gate, keyed on the reign
  id). The event carries `next_price` so the headline quotes the ladder
  without re-deriving it. Both names are written as `@mentions` in the
  headline (never in the ticker line), so the deposed holder gets a
  notification even when they were offline for it. Unlike the gild line this one names the loser on
  purpose: the crown is a single slot, so the takeover *is* the story.
- **Award.** The month's last holder gets a `profile_awards` row, category
  `crown`, granted by the existing monthly snapshot
  (`snapshot_previous_month_profile_awards`, a `crown_holder` CTE reading
  last month's latest reign). The badge is `CRWN` with no rank digit
  (`profile_award::is_rankless_award`), and it is *not* a milestone. Chat
  never prints it: the label query skips the category and
  `profile_award::chat_award_categories` keeps it out of the Chat badges
  picker, since the laureate's 👑 already says it. The profile lists it.
- **IRC sees nothing, and cannot play.** The glyphs are TUI author-header
  spans, so IRC clients get the message body and nothing else, and `/crown`
  is composer-parsed (`submit_composer`), which the ircd send path never
  reaches.

---
