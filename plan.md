# News, second edition: reader, saves, reactions

Spec for the next pass on the News surface (`late-ssh/src/app/chat/news/`).
Read `GUIDE.md`, `CONTEXT.md` §4/§8, `late-ssh/src/app/chat/CONTEXT.md` §11
(News), `late-ssh/src/app/live/CONTEXT.md`, and `DESIGN_GUIDE.md` §6.1 before
touching code. PR 1 (the reader and article bodies) is built; this file
specs what PR 2 (saves) and PR 3 (note, reply linkage, reactions) add on
top. Each lands alone.

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

## Starting point

PR 1 is built: the reader (`news/reader/`), article bodies and their fetch,
and the per-user reads. `late-ssh/src/app/chat/CONTEXT.md` §11 (News) is the
truth for all of it; do not re-derive it from this file.

## UX contract

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

Built in PR 1, except the note row, the social row, and the footer's `s`,
`e`, `t` hints, which PR 2 and PR 3 add.

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

Migrations are new files from `247_` on. Never edit an applied migration.

### Body retention (PR 2)

The hourly body sweep's `ArticleBody::retire_fetched_before` retires every
ready body older than 30 days. With saves it skips a story anyone saved:
`AND NOT EXISTS (SELECT 1 FROM article_saves s WHERE s.article_id =
article_bodies.article_id)`, with a case in `article_body_test.rs`.

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
separate load, requested when `/` enters the saved view and after a save
toggle while in it.

Replica-ready shape (`CONTEXT.md`, the multi-replica rule): an answer to one
session's own request (the saved view, a toggle's result) comes back on a
private `oneshot` that session holds, the way `ArticleService::load_body_task`
answers the reader, never on the shared `ArticleEvent` broadcast, where every
session of the same user would take it. What other sessions must see rides
`articles_changed` and the snapshot, as bodies do.

## Where code lives

Everything new is inside `late-ssh/src/app/chat/news/` except the models,
migrations, and the two chat-surface changes named above.

- `news/reader/`: the picker row (`e`) and the `s`, `t` keys on the
  built reader.
- `news/svc.rs` orchestrates: save toggle, reaction toggle, saved-list
  load, marks load, with every failure logged in one `match` and answered
  to the asking session, which turns it into a banner.
- `news/state.rs`: view enum `NewsView { All, Mine, Saved }` replaces the
  `mine_only` bool (`toggle_mine_only` → `cycle_view`), social fields.
- `news/ui.rs`: card rows 2, 4, 8 per Cards above.
- `late-core/src/models/`: `article_save.rs`, `article_reaction.rs`, each
  with the adjacent `_test.rs`. `article.rs` gains the joins, `note`,
  `list_saved_by_user`.
- `late-ssh/src/metrics.rs`: `record_news_save(SaveToggle)`,
  `record_news_reaction(ArticleReactionAction)`, each with its no-op twin
  under `#[cfg(not(feature = "otel"))]`. Labels from the closed enums.

## Tests (next to the code, `make test-llm ARGS="-p late-ssh -E 'test(news)'"`)

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

- `late-ssh/src/app/chat/CONTEXT.md` §11 News: saves, reactions, note,
  reply linkage, keys.
- `CONTEXT.md` §4 data model table: `article_saves`, `article_reactions`,
  the two new columns.
- `late-ssh/src/app/help_modal/data.rs` `HelpTopic::News` lines and its test.
- Nothing in `plan.md` itself; delete this file when PR 3 merges.

## Decisions already made

- Reactions mint nothing. One reaction per person per story, four icons.
- Retention: 30 days unless saved by anyone.
