# Gilds

Part of the chat domain (`../CONTEXT.md`). The modal lives here in `chat/gild/`.

A gild is chips paid to mark someone else's message, permanently. It is a
purchase, not a reaction: there is no un-gild, and the row outlives every
session. `late-core/src/models/chat_message_gild.rs` owns the table
(migration 154) and `GildTier` (Bronze 500 / Silver 2,000 / Gold 5,000);
`chips.rs::UserChips::transfer_gild` owns the money.

- **Split.** The author receives `floor(price * 2 / 3)` as
  `ChipMove::GildReceived`; the buyer pays the full price as
  `ChipMove::GildSent` (floor-guarded like a gift). The last third has no
  ledger row at all: the burn *is* the gap between the two reasons.
  `GildReceived` counts on Top Chips: a gild is paid for a message other
  people rated, the way the gallery prize is paid for applause, and the
  burned third means it cannot funnel chips for free the way a gift can.
  `GildSent` does not: Top Chips ranks earnings and a debit never counts.
- **Guards** (`ChatService::gild_message`, one closed `GildRefusal` enum with
  the wording): message gone, not a member, not a public room (so never a DM
  and never a private room), a game room (`kind = 'game'`: arcade tables,
  daily matches and `#user-live` stream chats are public by visibility but
  not on the Home rail, and the #lounge line must point somewhere people can
  go), self-gild, bot author, the 30s per-buyer cooldown, the same tier
  already held on this message (`AlreadyGilded`), a higher tier already held
  (`HeldHigher`: a gild never goes down), and the chip floor.
  Every refusal is uncharged, and a refusal after the cooldown was stamped
  releases it again.
- **Transaction** (`ChatService::settle_gild`): `SELECT ... FOR UPDATE` on the
  `chat_messages` row first, so every gild on one message serializes. That is
  what makes both the duplicate-tier check and the "third gild" count exact.
  Then insert (`ON CONFLICT DO NOTHING` = already gilded), move the chips,
  count, `pg_notify`, commit. Any early return drops the transaction.
- **Repaint is DB-backed, not broadcast-backed.** The gild transaction
  notifies `chat_message_gilded` with a `<message id>:<room id>` payload;
  `ChatService::start_notify_worker` (fed by the process listener,
  `pg_listener.rs`, subscribed in `main.rs`; it also carries the
  haunting's `deadchannel_name_hit` channel) turns each notification into
  a local `ChatEvent::MessageGildsUpdated`. This process is not special-cased: it
  learns about its own gilds the same way a second replica does, so there is
  exactly one code path that draws a marker. `GildSucceeded` / `GildFailed`
  ride the in-process broadcast, but they only carry the two banners (buyer
  and author).
- **Desktop notification.** The author's `GildSucceeded` arm also pushes
  `Notification::gilded` (`app/notify`), so the OSC 777 / OSC 9 alert fires
  for someone who is away from the terminal or in another room; the banner
  alone would be missed. It rides the `mentions` kind rather than a kind of
  its own: it is the same "a person in chat singled you out" opt-in, and it
  inherits the outbox cooldown like every other notification. The buyer gets
  no notification, they are the one who pressed the key.
- **Rendering.** `message_gilds: HashMap<Uuid, ChatMessageGildSummary>` on
  `ChatState`, loaded with the room tail and patched by the notify. The
  marker leads the message footer, ahead of the reaction chips
  (`ui_text.rs::render_message_footer_lines`): the tier's glyphs
  (`GildTier::marker`, `◆` / `◆◆` / `◆◆◆`) alone, `◆◆◆ ×3` once more than one
  gild is on it, bold in the tier's `BADGE_BRONZE/SILVER/GOLD`. The same
  color runs a heavy bar (`┃`) down the gutter of every line the message
  renders (`ui_text.rs::Gutter`, the cell the yellow mention bar uses), so a
  gilded message reads as a block. `Gutter` is a closed enum ranking the two
  claims on that cell: a mention of you keeps its bar, since the footer chip
  still spells the gild and the mention bar has no other signal. The selection marker `▸` paints over whichever bar holds the cell (`ui.rs::visible_chat_rows` asks `Gutter::is_glyph`), so selection is always visible. Both ride
  the footer and gutter rather than the author header because a message
  inside a run has no header, and a paid marker that vanishes on the second
  line of a run is the one thing nobody would accept. Gild changes bump
  `room_version`, so the row cache repaints.
- **Feed.** `ActivityKind::MessageGilded` fires once, on the message's third
  buyer (`GILD_FEED_THRESHOLD`, `GildOutcome::fires_feed_line`), and names
  only the author: "mira got a message gilded 3 times in #lounge". Never per
  gild, and never on a raise, which adds no buyer. Keyed on the message id
  in the lounge repeat throttle.
- **Who gilded.** `ff` on the message (the reaction-owners overlay) lists the
  gilds above the reactions, one block per tier held with the buyers under
  it. Same event, same membership check (`../CONTEXT.md` §10).
- **Profile.** `ChatMessageGild::counts_for_author` (owner-scoped in the
  query, off the denormalized `author_user_id`) fills `ProfileSnapshot.gild_counts`
  and renders as a "Gilds received" section, hidden entirely when empty.
- **IRC sees nothing.** The marker is a TUI footer chip; IRC clients get the
  message body and nothing else. There is no `/gild` command either: gilding
  is a message action on a selection, and IRC has no selection.
- **Message deletion takes the gilds with it** (`ON DELETE CASCADE`), and
  `ChatState::remove_message` drops the marker to match. The chip ledger rows
  survive; they are keyed on the message id as `source_ref`.

The tier picker is `chat/gild/` (`state.rs` selection only, `input.rs`
j/k/1-3/Enter/Esc, `ui.rs` the popup). It reads `App.chip_balance` at draw
time rather than caching a balance of its own, and dims tiers the balance
cannot cover; the floor guard in the chip move is what actually decides.

---
