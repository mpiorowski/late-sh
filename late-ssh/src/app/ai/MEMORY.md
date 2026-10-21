# Bartender Memory: what @bartender remembers about a patron

@bartender remembers each patron and reads it back into his prompt as
continuity notes for how to treat that patron. System instructions stay
separate from the tagged memory, room-chat, and incoming-order data. The
rules and the SQL live in `late-core/src/models/bartender_memory.rs`
(migration 248); the recording task and the prompt section in
`bartender_memory.rs`; the wiring in `ghost.rs` (`bartender_reply`,
`bartender_remember`, `bartender_forget`). The rest of the AI domain is in
`CONTEXT.md`.

There are two kinds of memory (`MemoryKind`):

- **An exchange memory**, kept on his own: the patron's order, his reply,
  and up to five more lines the patron said in the same room after the
  order, until five seconds after the reply landed. It is stored only when
  he actually answered: a throttled or skipped mention keeps nothing, while
  a talk reply, a pour, an offer, a bounced tab, and a gift's or a round's
  receipt or refusal all count. Every bartender answer goes through
  `GhostService::bartender_reply`, which awaits `ChatService::send_bot_reply`
  for the reply row and, once the tail has closed (plus a second's grace for
  sends still committing), stores the exchange from a spawned task. A
  restart inside that window loses the one exchange; the unique
  `order_message_id` makes a repeat a no-op.
- **A requested memory**: a message that opens with the bartender's handle
  (a `,` or `:` after it allowed) and then "remember" as the very next word
  (any case, a `,` or `:` after it allowed, a `*hic*` between them or at the
  very end ignored) stores what follows, trimmed, and nothing else
  (`remember_request`). It is never an order: it answers after "forget me"
  and ahead of gifts, rounds, the ladder and the model, so "@bartender
  remember buy @alice a drink" buys nothing and pours nothing. Only the
  opening counts: "@bartender please remember ..." or "remember this
  @bartender" is talk for the model, whose prompt tells patrons the words.
  The row is written before the ladder is consulted, so a throttle never
  loses one; the scripted acknowledgement steps the ladder and is skipped
  when throttled. An empty request ("remember what?") or one outside a
  public room gets its scripted line, stores nothing, and steps the ladder.
  A requested memory has no reply and no tail (a table check holds the two
  shapes apart).

The rules both kinds share:

- **Prompt data has explicit speakers and boundaries.** `patron-memories`
  identifies the patron and lists memories oldest first, both kinds
  interleaved, with timestamps and ages. An `exchange-memory` contains
  `order-text`, `bartender-reply`, and optional
  `after-order-patron-messages`; a `requested-memory` is the patron's words
  alone. The latest 40 room messages remain background in
  `recent-room-chat`; `new-incoming-order` always contains the actual
  triggering request, even if later chat has arrived. Raw text and
  attributes are XML-escaped; memory lines are folded and capped at 240
  characters before escaping. These sections travel as request data,
  outside the system instructions.
- **Memories guide hospitality and drink selection.** They cannot create a
  new order or override house rules. Recent information wins when memories
  conflict; being a requested memory buys retention, not authority.
  Explicit standing preferences (a preference stated in a requested memory
  is one) differ from soft preferences suggested by previous choices:
  nonalcoholic orders may bias ambiguous "cider" toward apple cider, but
  carry less weight for a clear request for a particular drink.
- **An ongoing no-alcohol preference requires an explicit current opt-in.**
  The patron must positively request an "intoxicating" drink, an "alcoholic"
  drink, or one "with alcohol". Drink names, "boozy", "full-strength", and
  "add rum" alone do not qualify; neither do negated or quoted terms or someone
  else's request. Otherwise choose a virgin/non-intoxicating variant, or serve
  a normally non-intoxicating drink as-is. Opting in is for that order only and
  does not cancel the standing preference. The structured `intoxicating` field,
  drink description, and house price must describe the beverage actually chosen.
- **Public rooms only.** A memory reaches the prompt in whatever room he
  answers next, so neither kind is kept from a DM or a private room.
- **Copied, not referenced.** The words are copied into the row when it is
  stored, so a memory lives by these rules alone: a chat message pruned,
  aged out, or deleted afterwards (by its author or a moderator) leaves the
  memory that quoted it untouched. Only the cap, "forget me", and the account
  going away (FK cascade on `users`) end one.
- **Sixteen per patron.** The newest eight requested memories are kept
  first, and every other slot goes to the newest of the rest, older
  requested ones included (`evictions`, pure). A patron who asks him to
  remember everything therefore still gets room for new nights.
- **"forget me"**, said exactly (any case, one or more ASCII spaces between
  the words, the `@bartender` handle once at either end, a `*hic*` outside the
  phrase ignored; "please forget me" or "forget me?" is talk), wipes every
  memory said at or before it and stamps `bartender_memory_resets` with the
  message's time. Recording refuses any memory said at or before that stamp,
  so one still in flight (the model call, the tail) cannot be stored after the
  wipe, and a memory said after it survives even when it reached the lock
  first. This clears saved memories; it does not mask room history or cancel
  a reply already in flight. Insert, eviction, and wipe share a per-patron
  advisory lock. The wipe lands ahead of the ladder and never reaches the
  model, the way a purchase does; its scripted line steps the ladder like a
  remember's acknowledgement and is skipped when throttled, so alternating
  the two free commands cannot get a line out of him past the cooldown.
- **Drunk text spares both triggers.** The stored body is the slurred one,
  so `../chat/slur.rs` protects `memory_phrase_spans` (every "forget me", and
  a remember request from its "remember" to the end of the message) the same
  way it protects the spending phrases. A remember request is set aside
  whole before the line-by-line pass (`remember_request_span`), so every
  line of it is stored as typed. Without the bartender's handle, the slur
  pass treats any opening @mention followed by "remember" as a request. It
  also protects alcohol preference/opt-in vocabulary and gaps inside "with
  alcohol", so scrambling or hiccups cannot break the prompt's named opt-in
  wording.
- **Every write is counted.** `record_bartender_memory` reports each
  outcome (`exchange_stored`, `requested_stored`, `turned_away`,
  `already_stored`, `wiped`, and a `*_failed` per write), since none of the
  failures shows in the room: an exchange is stored off the reply path, and a
  failed remember or wipe gets no line.
