# News, second edition: reader, saves, reactions

Spec for the next pass on the News surface (`late-ssh/src/app/chat/news/`).
Read `GUIDE.md`, `CONTEXT.md` §4/§8, `late-ssh/src/app/chat/CONTEXT.md` §11
(News), `late-ssh/src/app/live/CONTEXT.md`, and `DESIGN_GUIDE.md` §6.1 before
touching code. Ship as three PRs in the order below; each lands alone.

## Goal

A link shared to News can be read in the terminal. Readers can keep a story
for later, react to it, and find the #lounge talk about it. Sharers can say
why they shared. Nothing here pays chips: the share reward stays the one
chip path in News.

## Non-goals (second pass, do not build)

- Anything video: no transcripts, no chapters, no Gemini video input, no
  ASCII video, no "watch in CLI". A YouTube share takes the fallback path.
- X posts: already fully carried by oEmbed; fallback path too.
- Chips for reactions, saves, or reads.
- Syntax highlighting in code blocks, inline images in the body.
- Web (`late-web`) rendering of articles.

## What exists today (do not re-derive)

- `articles` row: `url`, `title`, `summary` (three AI bullets), `ascii_art`.
  Shared global snapshot of the newest `NEWS_FEED_LIMIT` (20) articles,
  re-read by every replica on `articles_changed` (statement trigger,
  migration 198) through `ArticleService::start_notify_worker`.
- Extraction never fetches the page. `extract_via_ai` asks Gemini with
  Search grounding; only the hero image is fetched (`article_ascii_art`).
  YouTube (`extract_youtube`) and X (`extract_tweet`) have their own paths.
- Per-user state is reads only: `article_reads`, loaded as
  `ArticleEvent::ReadsLoaded`, bounded to the snapshot's articles.
- The article modal (`NewsModalState` in `chat/state.rs`, drawn by
  `news/ui.rs::draw_article_modal`, keys in `app/input.rs::handle_news_modal_input`)
  shows thumbnail, title, URL, meta, three bullets. It opens only from the
  live strip (`o`) and the Live panel; the News room has no key that opens it.
- News room keys: `i` share, `Enter` copy link, `j`/`k`, `d` delete,
  `r` reply in #lounge, `/` toggle mine-only.
- A reply quotes `> @sharer: 📰 Title` with no id back to the article
  (`ReplyTo::Article`, `chat/state.rs`), so the article cannot count or
  find its replies.
- The paper modal (`paper/state.rs::PaperModal`, `paper/ui.rs::draw`) is the
  house's scrolling reader: j/k, scrollbar track and thumb, drag, footer.
  Copy its shape; do not share its code unless a shared primitive falls
  out naturally.

## UX contract

### Opening a story opens the reader

One surface replaces the small modal: **the reader**. Every path that opened
the modal opens the reader instead: live strip `o`, Live panel row click,
and, new, `Enter` on a card in the News room. There is no intermediate
"summary modal with a Read full button"; the reader *is* the modal.

The reader has two bodies, decided by the stored body status:

- **Ready**: title, meta line, the article text (see Reader layout).
- **Unavailable or Pending** (every YouTube and X share, a paywall, a
  JavaScript-only page, a page still being fetched): the hero ASCII art,
  title, meta line, the three bullets, and one line in `TEXT_DIM`:
  `Read on the web: <url>` for unavailable, `Fetching the article…` for
  pending. Same frame, same keys. This is the old modal's content inside
  the new frame.

### Keys

News room (card list), full set after all three PRs:

| Key | Action |
|---|---|
| `Enter` | Open the reader on the selected card |
| `c` | Copy the link (moves off `Enter`) |
| `s` | Save / unsave the selected story |
| `e` | Open the reaction picker (footer row) |
| `t` | Jump to the latest #lounge reply to the story (absent when zero) |
| `r` | Reply in #lounge (unchanged) |
| `d` | Delete (own, or admin; unchanged) |
| `/` | Cycle the view: all → mine → saved → all |
| `i` | Share: `<url>` or `<url> <why this>` |
| `j`/`k`, arrows | Move (unchanged) |

Reader, same letters mean the same thing:

| Key | Action |
|---|---|
| `j`/`k`, arrows, wheel | Scroll one line |
| `Space`/`b` | Page down / up |
| `g`/`G` | Top / bottom |
| `1`..`9` | Copy link `[n]` from the Links list |
| `c` | Copy the story's link |
| `s`, `e`, `t`, `r`, `d` | As in the room; `r`, `t`, `d` close the reader first |
| `N` | Go to the story in the News room (unchanged) |
| `Esc`, `[x]` click | Close |

Reaction picker (after `e`), one footer row, closes on any key:
`f 🔥   w 👀   h 🤔   l 😂   Esc`. Pressing the key of your current
reaction removes it; a different key replaces it.

### Cards

The card stays 10 rows. Rows, top to bottom:

1. `● ` unread dot + title (unchanged)
2. Sharer's note, when present: `“<note>”` in `AMBER_DIM` italic. Absent
   row when no note.
3. URL (unchanged)
4. Meta: `@sharer - 2h ago - Mon 2026-10-19 14:02 UTC - 7 min read`.
   The last segment is `7 min read` (body ready), `link` (unavailable), or
   nothing (pending). A video share therefore reads `link`.
5.–7. Summary bullets: three lines, or two when a note is shown.
8. Social row, only when something is nonzero: `🔥 3  👀 1   2 in #lounge   saved`.
   Reaction glyphs with counts, in the closed enum's order; the replies
   segment; `saved` in `AMBER` when the viewer saved it. Empty row when all
   are zero (never `🔥 0`).

### Views

`/` cycles `all`, `mine`, `saved`. The status row above the list
(`draw_mine_only_status`) names the view: `news · mine`, `news · saved`.
`saved` lists the viewer's saved stories newest-saved first, including
stories that have left the 20-deep snapshot. Reading a saved story does
not unsave it.

### Banners

Reuse `Banner::success` lines: `Saved for later.` / `Removed from saved.`,
`Link copied!` (unchanged), `Reacted 🔥` / `Reaction removed.`.
Reaction and save failures surface as `Banner::error` from the service event,
never silently.

## Reader layout

Frame: centered box like the paper's (`PAPER_*_PERMILLE` style sizing,
`BORDER_ACTIVE`, title ` Reading ` in `AMBER_GLOW`, `[x]` top right,
scrollbar track in the right margin when content overflows). Body wrapped
at `min(inner_width - 4, 80)` columns; narrower terminals wrap narrower.

Header block (both body kinds):

```
Why LISTEN/NOTIFY stops scaling                      (TEXT_BRIGHT bold)
example.com · 7 min read · shared by @mat 2h ago     (TEXT_DIM)
“Finally someone measured it.”                       (AMBER_DIM italic, note)
🔥 3  👀 1   2 in #lounge                            (social row, when nonzero)
────────────────────────────────                     (BORDER_DIM rule)
```

Body, from the stored line model (closed enum, see Data):

| Line | Rendering |
|---|---|
| `Heading { level, text }` | Blank line before; `AMBER` bold; level 1 and 2 also uppercase |
| `Paragraph(text)` | `TEXT`, wrapped, blank line after |
| `Quote(text)` | `│ ` in `BORDER_DIM` then `TEXT_DIM` italic, wrapped |
| `Code(lines)` | Each line verbatim in `TEXT_DIM` with a `│ ` gutter, no wrap, clipped at width |
| `ListItem(text)` | `• ` then `TEXT`, wrapped with hanging indent |
| `Rule` | A dim rule |

Links inside a paragraph render as `text [n]`, numbered in first-appearance
order. After the body, a `Links` heading and the numbered list
`[1] https://…`, one per line, clipped. `1`..`9` copy that URL. Only the
first nine are numbered; later links render as plain text.

Footer: `j/k scroll  Space page  1-9 copy link  c copy  s save  e react  t thread  r reply  Esc`,
dropping `t` when there are no replies and trailing hints when the width
runs short, the way the paper's footer does.

## Data

Migrations are new files from `246_` on. Never edit an applied migration.

### `article_bodies` (PR 1)

```sql
CREATE TABLE article_bodies (
    article_id UUID PRIMARY KEY REFERENCES articles(id) ON DELETE CASCADE,
    status TEXT NOT NULL,            -- 'ready' | 'unavailable'
    domain TEXT NOT NULL,            -- host of the final URL after redirects
    word_count INTEGER NOT NULL,     -- 0 when unavailable
    lines JSONB NOT NULL,            -- Vec<ReaderLine>, empty when unavailable
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT current_timestamp
);
```

No row means **pending**. The three states are a closed Rust enum
`BodyStatus { Pending, Ready, Unavailable }`; `status` text parses
exhaustively and an unknown value is an error, not a default. A statement
trigger on `article_bodies` fires `articles_changed` (same function as
migration 198) so every replica's cards flip from pending to `7 min read`.

Bodies never ride the snapshot: `ArticleFeedItem` carries `body: BodyStatus`
and `reading_minutes: Option<u16>` from a `LEFT JOIN`, and the reader loads
the one row by id on open (`ArticleBody::find`). The snapshot is re-read on
every notify on every replica; a 50 KB body per article would multiply that.

`ReaderLine` (in `late-core/src/models/article_body.rs`, serde-tagged):

```rust
pub enum ReaderLine {
    Heading { level: u8, text: String },
    Paragraph { spans: Vec<ReaderSpan> },
    Quote { spans: Vec<ReaderSpan> },
    Code { lines: Vec<String> },
    ListItem { spans: Vec<ReaderSpan> },
    Rule,
}
pub enum ReaderSpan {
    Text(String),
    Link { text: String, href: String },
}
```

Retention: once a day, delete bodies whose `fetched_at` is older than 30
days and whose article has no row in `article_saves`. Hook it where the
house already runs daily sweeps (the paper's print sweeper is the model;
verify the location in `paper/svc.rs` and `main.rs`). A deleted body means
the card reads `link` and the reader shows the fallback; nothing else
changes.

### `article_saves` (PR 2)

```sql
CREATE TABLE article_saves (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    article_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (user_id, article_id)
);
CREATE INDEX idx_article_saves_article ON article_saves (article_id);
```

Per-user, no notify. Every query carries `WHERE user_id = $1` in SQL.
Model `ArticleSave` in `late-core/src/models/article_save.rs`: `toggle`
(returns the closed `SaveToggle { Saved, Unsaved }`), `ids_for_user`
bounded to the snapshot (rides the per-user load, below), and
`Article::list_saved_by_user(user_id, limit)` returning `ArticleFeedItem`s
newest-saved first for the saved view (limit 100).

### Sharer's note (PR 3)

```sql
ALTER TABLE articles ADD COLUMN note TEXT;
```

`NULL` when absent. Max 140 chars, whitespace-normalized, set only at share
time by the News composer: the body is split at the first whitespace into
`url` and the optional note. RSS `s` shares carry no note. `Article.note:
Option<String>`; `ArticleParams` gains it; `create_shared` writes it.

### Reply linkage (PR 3)

```sql
ALTER TABLE chat_messages
    ADD COLUMN reply_to_article_id UUID REFERENCES articles(id) ON DELETE SET NULL;
CREATE INDEX idx_chat_messages_reply_to_article
    ON chat_messages (reply_to_article_id) WHERE reply_to_article_id IS NOT NULL;
```

A row trigger `WHEN (NEW.reply_to_article_id IS NOT NULL)` fires
`articles_changed`, so the reply count on cards is live. `ReplyTo::Article`
becomes `ReplyTo::Article(Uuid)`; the composer submit path passes it to
`ChatMessage::create_with_reply_targets`, which gains a
`reply_to_article_id: Option<Uuid>` parameter (chat public surface change;
every caller passes `None` explicitly, no default). The quote text is
unchanged. `ArticleFeedItem.reply_count: i64` comes from a correlated count
in the feed query. `ChatMessage::latest_reply_to_article(article_id) ->
Option<(room_id, message_id)>` backs the `t` jump.

### `article_reactions` (PR 3)

```sql
CREATE TABLE article_reactions (
    article_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    icon TEXT NOT NULL,              -- 'fire' | 'eyes' | 'thinking' | 'laugh'
    created TIMESTAMPTZ NOT NULL DEFAULT current_timestamp,
    PRIMARY KEY (article_id, user_id)
);
```

Statement trigger fires `articles_changed`. One reaction per person per
story. `ArticleReaction { Fire, Eyes, Thinking, Laugh }` is closed, with
`storage_key()`, `glyph()`, `picker_key()` mapped exhaustively; parsing an
unknown key is an error. Model mirrors `ChatMessageReaction::toggle`:
`ArticleReactionToggle { action: React | Unreact | Replace, icon,
previous }`. Counts ride the snapshot as
`ArticleFeedItem.reactions: Vec<(ArticleReaction, i64)>` in enum order,
zero entries omitted. The viewer's own reactions ride the per-user load.

### Per-user load

`ArticleEvent::ReadsLoaded` becomes `ArticleEvent::MarksLoaded` carrying
`read_article_ids`, `saved_article_ids`, and `reactions: Vec<(Uuid,
ArticleReaction)>`, all bounded to the snapshot's articles, loaded at session
start and after every per-user write (read, save toggle, reaction toggle),
merged into `news::State` the way reads are today (reads only grow; saves
and reactions replace, since they can be removed). The saved *view* is a
separate load, `ArticleEvent::SavedLoaded { user_id, items }`, requested
when `/` enters the saved view and after a save toggle while in it.

## Fetch pipeline (PR 1)

Inside `do_process_url`, after `create_shared` returns and
`ArticleEvent::Created` is published, for every article that is **not**
YouTube and **not** an X status (`is_youtube_url`, `tweet_status_id`):

1. `fetch_body(article_id, url)` runs as its own step inside the same task
   and the same 5-minute budget, after the share is already visible. Its
   failure never fails the share.
2. GET with `User-Agent: late.sh (https://late.sh)`, follow up to 5
   redirects, 10 s timeout, read at most 2 MB, accept only
   `text/html` (or `application/xhtml+xml`). `domain` is the final URL's host.
3. Readability: add `dom_smoothie` (Rust port of Mozilla Readability) to
   `late-ssh`. It yields the article node tree. Walk it into `Vec<ReaderLine>`:
   `h1`–`h6` → `Heading`, `p` → `Paragraph`, `blockquote` → `Quote`,
   `pre` → `Code`, `li` → `ListItem`, `hr` → `Rule`, `a` → `ReaderSpan::Link`
   with absolute `href` resolved against the final URL; `img`, `figure`,
   `table`, scripts, nav are dropped. Collapse whitespace; drop empty
   paragraphs. Cap at 2,000 lines and 200 KB of text; past the cap, truncate
   and append `Paragraph("… continues on the web")`.
4. `word_count` is the body's words; `reading_minutes = ceil(words / 220)`.
5. Insert `article_bodies` `ready`, or `unavailable` on any failure. The
   failure reason is a closed `BodyFetchFailure { Fetch, NotHtml, TooLarge,
   Timeout, Extract, Empty }` used for the metric and the log, never stored.
6. YouTube and X get an `unavailable` row immediately, so their cards read
   `link` and never sit in pending.

Blocklist: `is_ai_blocklisted_url` already stops cyberspace.online before
extraction; the body fetch sits after it and needs no second check.

## Where code lives

Everything new is inside `late-ssh/src/app/chat/news/` except the models,
migrations, and the two chat-surface changes named above.

- `news/reader/` subdomain, same roles as the paper modal: `state.rs`
  (`ReaderModal`: article id, header, `Vec<ReaderLine>` or the fallback
  payload, scroll, viewport, link list, picker open), `ui.rs` (`draw`),
  `input.rs` (keys, wheel, `[x]`, scrollbar drag). `news::State` owns
  `Option<ReaderModal>`; `chat/state.rs` keeps thin `open_reader_for_article`
  / `close_reader` / `reader()` delegations because `render.rs`, `app/input.rs`,
  and `live/input.rs` reach it through `ChatState`. `NewsModalState`,
  `draw_article_modal`, `handle_news_modal_input` and `ArticleModalView` are
  deleted, not kept beside the reader.
- `news/svc.rs` orchestrates: body fetch, save toggle, reaction toggle,
  saved-list load, marks load, with every failure logged in one `match`
  and surfaced as an `ArticleEvent::Failed`-style event the state turns
  into a banner. The fetch and the DOM walk live in `news/body.rs` as pure
  work (`fn lines_from_html(html, base_url) -> Vec<ReaderLine>`), testable
  with fixture HTML and no network.
- `news/state.rs`: view enum `NewsView { All, Mine, Saved }` replaces the
  `mine_only` bool (`toggle_mine_only` → `cycle_view`), social fields,
  `reader`.
- `news/ui.rs`: card rows 2, 4, 8 per Cards above.
- `late-core/src/models/`: `article_body.rs`, `article_save.rs`,
  `article_reaction.rs`, each with the adjacent `_test.rs`. `article.rs`
  gains the joins, `note`, `list_saved_by_user`.
- `late-ssh/src/metrics.rs`: `record_news_body_fetch(BodyFetchOutcome)`,
  `record_news_reader_opened(BodyStatus)`, `record_news_save(SaveToggle)`,
  `record_news_reaction(ArticleReactionAction)`, each with its no-op twin
  under `#[cfg(not(feature = "otel"))]`. Labels from the closed enums.

## Tests (next to the code, `make test-llm ARGS="-p late-ssh -E 'test(news)'"`)

PR 1:
- `body_test.rs`: fixture HTML with headings, paragraphs with links, a
  quote, a `pre`, a list, an `hr`, nav and script junk → exact
  `Vec<ReaderLine>` (whole-state assertion); relative hrefs resolve; the
  line cap truncates and appends the continuation line.
- `svc_test.rs`: a non-YouTube share ends with a `ready` or `unavailable`
  row and the share succeeds either way (stub the HTTP client or point at a
  local server the way other svc tests do; a failing fetch still publishes
  `Created`); a YouTube share gets `unavailable` without a fetch.
- `reader/state_test.rs`: open on a ready body builds the link list in
  first-appearance order; `1` copies `[1]`; open on pending shows the
  fetching line; scroll clamps.
- `ui_test.rs`: card meta reads `7 min read` / `link` / nothing by status.
- Flow test: strip `o` opens the reader (replace the modal assertion in
  `live` or `input_flow_test.rs`), News room `Enter` opens it, `c` copies.

PR 2:
- `article_save_test.rs`: toggle saves then unsaves; `ids_for_user` is
  user-scoped (two users, no leak); `list_saved_by_user` returns a story
  older than the snapshot.
- `state_test.rs`: `/` cycles all → mine → saved → all; saved view holds a
  story the snapshot does not.

PR 3:
- `article_reaction_test.rs`: react, replace, unreact; counts per icon; an
  unknown stored key fails to parse.
- `chat_message_test.rs`: a reply with `reply_to_article_id` is counted and
  `latest_reply_to_article` finds it; deleting the article nulls it.
- `ui_test.rs`: social row absent when everything is zero; note row shrinks
  the summary to two lines.
- Composer: `<url> <note>` splits; a 141-char note is rejected with a banner.

## Docs to update in the same PRs

- `late-ssh/src/app/chat/CONTEXT.md` §11 News: the reader, body states,
  saves, reactions, note, reply linkage, keys, the deleted modal.
- `late-ssh/src/app/live/CONTEXT.md`: `o` opens the reader.
- `CONTEXT.md` §4 data model table: `article_bodies`, `article_saves`,
  `article_reactions`, the two new columns; §8 if the snapshot contract
  gains the "bodies never ride the snapshot" invariant (it should).
- `late-ssh/src/app/help_modal/data.rs` `HelpTopic::News` lines and its test.
- Nothing in `plan.md` itself; delete this file when PR 3 merges.

## Decisions already made

- Straight to the reader: no summary-first modal, no "read full" button.
- Videos and X posts are links. Their reader is the fallback body.
- Bodies live in their own table and never in the snapshot.
- Reactions mint nothing. One reaction per person per story, four icons.
- `Enter` opens, `c` copies, in both the room and the reader.
- `dom_smoothie` is the one new dependency. Flag it in the PR description.
- Retention: 30 days unless saved by anyone.
