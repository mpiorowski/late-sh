# News, second edition: reader, saves, reactions, watch

Spec for the next pass on the News surface (`late-ssh/src/app/chat/news/`).
Read `GUIDE.md`, `CONTEXT.md` §4/§8, `late-ssh/src/app/chat/CONTEXT.md` §11
(News), `late-ssh/src/app/live/CONTEXT.md`, and `DESIGN_GUIDE.md` §6.1 before
touching code. PR 1 (the reader and article bodies) is built; this file
specs what PR 2 (saves), PR 3 (note, reply linkage, reactions) and PR 4
(watch a video in the terminal through the paired CLI) add on top. Each
lands alone. PR 2 and PR 3 come next, in that order. PR 4 is independent of
2 and 3 and can land before or after them; it also touches `late-cli`, so
read `late-cli/CONTEXT.md` §6, §8, §13 first.

## Goal

A link shared to News can be read in the terminal. Readers can keep a story
for later, react to it, and find the #lounge talk about it. Sharers can say
why they shared. Nothing here pays chips: the share reward stays the one
chip path in News.

## Non-goals (second pass, do not build)

- Video understanding: no transcripts, no chapters, no Gemini video input,
  no ASCII video. A YouTube share takes the fallback path in the reader.
  Watching one *is* in scope now, as PR 4, and only through the paired CLI.
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

---

# PR 4: Watch

A YouTube link shared to News plays inside the terminal, in a window over
the TUI or full screen, for a user who runs late.sh through the `late` CLI
in a Kitty-graphics terminal with `mpv` and `yt-dlp` installed. Every
YouTube card reads `watch` for everyone, because watching is the user's
intent, not a capability: a user whose setup can play gets the player, and
a user whose setup cannot gets a setup box that lists exactly what is
missing on their side (the CLI, the terminal, tmux, `mpv`, `yt-dlp`) and
how to get it. There is no server-side video: not a frame, not a byte of
media crosses SSH.

## How it works in one paragraph

The CLI owns the user's real tty in native mode and is already paired to
the session over `/api/ws/pair`. When the TUI opens the watch modal it
paints the modal's body black, reserves that rectangle, and sends the CLI a
`video_play` control with the URL and the rectangle in cells. The CLI spawns
the user's `mpv` with the kitty video output pointed at that rectangle and
shared-memory transfer, so frames go from mpv to the terminal locally.
Kitty graphics placements sit above text, so the TUI keeps drawing under
it. Keys stay with the TUI, which relays pause, seek and mute as
`video_control` messages; the CLI applies them through mpv's IPC socket and
reports position and state back as `video_state`. Closing the modal, a page
change, a resize, the pair socket dropping or the SSH session ending all end
in `video_stop` or in the CLI killing the child itself.

## Gate: play or explain

`watch` is always offered on a YouTube story. What it opens depends on two
gates, both required, evaluated when `w` is pressed:

1. The paired CLI advertises the `video` capability in `client_state`
   (`late-ssh/src/paired_clients.rs`, add `ClientAudioState::supports_video`
   and `PairedClientRegistry::has_video_cli`, same shape as `has_voice_cli`
   and `supports_open_url`).
2. The server's own detection says Kitty: `App::terminal_image_protocol()
   == Some(TerminalImageProtocol::Kitty)`. That already folds in the Tweaks
   "Terminal images" row (`Off` hides `watch` too), the tmux exclusion, and
   XTVERSION/DA1 detection the CLI does not have.

Both pass: the player opens. Either fails: the setup box opens (below),
built from everything the server knows about why.

The CLI advertises `video` only when all of these hold at startup. The
check runs once, so a fixed setup is picked up on the next `late` launch:

- `ssh_mode` is `native`: the CLI owns the tty in raw mode. OpenSSH and old
  modes never advertise it.
- Platform is Linux or macOS. Windows is out for now: Windows Terminal has
  no Kitty graphics, and the console path has not been tested.
- Not inside a multiplexer: `TMUX` unset, `STY` unset, `TERM` does not start
  with `tmux` or `screen`.
- The terminal is Kitty-graphics family by the same env hints the CLI
  already forwards for images: `TERM=xterm-kitty` or `KITTY_WINDOW_ID`,
  `TERM_PROGRAM` in {`ghostty`, `WezTerm`, `rio`}, `GHOSTTY_RESOURCES_DIR`,
  `WEZTERM_EXECUTABLE`, `KONSOLE_VERSION`. The server's detection is the
  second opinion.
- The tty reports pixel size: `TIOCGWINSZ` on the CLI's own stdout gives
  nonzero `ws_xpixel`/`ws_ypixel`. mpv needs pixels to size the placement;
  a terminal that reports zero cannot host the video.
- `mpv` is on `PATH` (or `LATE_MPV_BIN` names it) and `mpv --version`
  parses to 0.36 or newer (the first release with `--vo=kitty`).
- `yt-dlp` is on `PATH` (or `LATE_YTDLP_BIN`). mpv's ytdl hook calls it.

The check is a pure function `video::blockers(env, winsize, mpv_version,
ytdlp_found) -> Vec<VideoBlocker>` returning every failing precondition,
not the first, from a closed enum `{ NotNative, Platform, Multiplexer,
NotKitty, NoPixelSize, NoMpv, MpvTooOld, NoYtDlp }`. Empty means `video` is
advertised. Non-empty is sent as `client_state.video_blockers` (wire
strings in snake case, below) so the server's setup box can say which rows
fail, and logged once at startup at `info`.

Nothing nags on its own: no banner, no card hint beyond the word `watch`.
The setup box appears only after the user asks to watch. The help modal's
News topic gains one line that names the requirement.

## UX contract

### Where `watch` appears (gated)

| Surface | Today | After PR 4 |
|---|---|---|
| News card meta, YouTube share | `link` | `watch` in `AMBER`, for everyone |
| News room key `w` on a YouTube card | nothing | player when both gates pass, otherwise the setup box |
| Reader on a YouTube story (fallback body) | `Read on the web: <url>` | `Watch: <url>`, footer gains `w watch`; `w` closes the reader and opens the player or the setup box |
| Live strip `o` / Live panel click on a YouTube article | opens the reader | player directly when both gates pass; otherwise the reader as today (an `o` is "open", not "watch", so it never lands on the setup box) |
| Any non-YouTube story | unchanged | unchanged, `w` does nothing |

"YouTube share" is `is_youtube_url(&article.url)` (`news/svc.rs`; make it
`pub(crate)` so `news/ui.rs` and `news/state.rs` can ask without a second
copy). Non-YouTube stories behave byte-for-byte as today.

### The watch modal

Two sizes, toggled by `f`:

- **Window** (the default): a centered box like the reader's (90 percent of
  width and height, width capped at 160, `BORDER_ACTIVE`, title ` Watching `
  in `AMBER_GLOW`, `[x]` top right). Header row inside the frame:
  `<title>` in `TEXT_BRIGHT` bold, then `youtube.com · shared by @mat 2h ago`
  in `TEXT_DIM`, then a `BORDER_DIM` rule. Body: every cell painted as a
  space on `Color::Black`; this is the rectangle sent to the CLI. mpv keeps
  the video's aspect inside it and adds its own bars, so the TUI does no
  letterbox math. Footer row at the bottom of the frame.
- **Full**: the whole terminal. No border, no header. Body is every row but
  the last; the last row is the footer. `f` again returns to Window.

Footer, left to right, in the reader's `key hint` style:
`Space pause  ←/→ seek 10s  m mute  f full  Esc`, with `f window` when
full, and the status at the right edge in `TEXT_DIM`:
`starting…` / `▶ 1:23 / 12:04` / `⏸ 1:23 / 12:04` / `ended` /
`couldn't play`. Trailing hints drop first when the width runs short, the
status drops last.

Keys (watch modal only; nothing else on the page receives keys while it is
open, like the reader):

| Key | Action |
|---|---|
| `Space` | `video_control toggle_pause` |
| `←` / `→`, `h` / `l` | `video_control seek_back` / `seek_forward` (10 s) |
| `m` | `video_control toggle_mute` (mpv's own mute, not the house mute) |
| `f` | Toggle Window / Full, which re-sends the rectangle |
| `c` | Copy the story's link (same as the reader) |
| `Esc`, `[x]` click | Close, `video_stop` |

Mouse: `[x]` closes, nothing else is clickable; wheel and clicks in the
body are swallowed so they never reach the room list under the modal.

### The setup box

A small centered modal in the reader's frame style, title ` Watch in the
terminal ` in `AMBER_GLOW`, `[x]` top right, width capped at 72. One intro
line in `TEXT`: `Videos play right here when your setup has:`. Then one row
per requirement, in this order, each marked `✓` in `AMBER` when the server
knows it holds, `✗` in `TEXT_DIM` when it knows it fails, and `·` when it
cannot tell:

```
✓ the late CLI           run late.sh through `late`, not plain ssh
✗ a Kitty-graphics terminal   kitty, Ghostty, WezTerm, Konsole, Rio
✓ no tmux or screen      the video is drawn straight to the terminal
✗ mpv 0.36 or newer      plays the video
✗ yt-dlp                 fetches it for mpv
```

Under the rows, one or two install lines in `TEXT_DIM`, chosen by what is
missing and by the paired client's `platform` (`client_state.platform`):

- No paired CLI (a plain `ssh` session, the most common case): the CLI
  row is the first ✗ and the line is the real installer,
  `curl -fsSL https://cli.late.sh/install.sh | bash`, with
  `irm https://cli.late.sh/install.ps1 | iex` when `TERM` or the forwarded
  env hints say Windows Terminal. This box is the one place a plain SSH
  user learns the CLI exists from inside News, so it is the install line,
  not a link to go find one.
- A CLI is paired and `mpv` or `yt-dlp` is missing: Linux shows `Install
  mpv and yt-dlp with your package manager (apt, dnf, pacman, nix)`, macOS
  shows `brew install mpv yt-dlp`, Windows shows `Windows is not supported
  yet`.
- Both cases can apply at once only in theory (no CLI means no blockers),
  so the match stays exhaustive and single-answer.

Then the web line `Watch on the web: <url>`. Footer: `c copy link  Esc`.

How each mark is decided, exhaustively, from what the server holds:

| Row | ✓ | ✗ | · |
|---|---|---|---|
| the late CLI | a paired client with `client_kind: cli` and `ssh_mode: native` | no paired client, or `ssh_mode` is `openssh`/`old` (blocker `not_native`) | never |
| Kitty terminal | server detection says Kitty and the CLI did not send `not_kitty`/`no_pixel_size` | server detection says another protocol or none, or the CLI sent `not_kitty`/`no_pixel_size`, or `platform` blocker | never (the server always has an opinion) |
| no multiplexer | `TERM` is not tmux/screen and no `multiplexer` blocker | either says tmux/screen | never |
| mpv | CLI paired and no `no_mpv`/`mpv_too_old` blocker | `no_mpv` or `mpv_too_old` (the row says `mpv is too old` for the latter) | no paired CLI |
| yt-dlp | CLI paired and no `no_ytdlp` blocker | `no_ytdlp` | no paired CLI |

The Tweaks "Terminal images" row set to `Off` is its own case: every mark
may be `✓` and the player still will not open. The box then shows one extra
row `✗ terminal images on   Ctrl+O, Tweaks, Terminal images` as the first
row. This is the only path where the box can show all green otherwise.

The box closes on Esc, `[x]`, or when the user acts on it elsewhere (page
change). Reopening after fixing the setup needs no restart on the server
side: the CLI re-sends `client_state` when its blockers change (a fresh
`late` launch in practice), and the next `w` re-evaluates.

### Failure

When the CLI reports `failed`, the body keeps its black fill, the status
reads `couldn't play`, and one centered line in `TEXT_DIM` appears in the
body: `Watch on the web: <url>`. A `Banner::error("Couldn't play the
video.")` fires once. Esc closes as usual. The CLI's reason goes to its log
and to the server's `video_state.message` for the log on that side; it is
never shown raw to the user.

When the pair socket drops while a video is up, the TUI's watch modal gets
no more `video_state`; after 10 s without one the status shows `lost the
CLI` and the same web line. The CLI kills mpv on its side when its socket
drops, independently.

### Audio while watching

The CLI silences its own output for the duration: the Icecast or radio
decoder output goes silent and its stream parks, through a dedicated
`silenced_for_video` flag beside `muted`. The webview helper has no command
channel from the CLI (its audio follows the server's fan-out on its own pair
socket), so it is stopped for the video and the heartbeat watchdog respawns
it after, resynced to the booth's current track. This is local and
temporary. No `set_muted` is sent to the server, so the user's persisted
mute and the sidebar state are untouched; when the play ends the output
and the helper come back. mpv starts at the CLI's current `volume_percent`
(`--volume=N`) and `m` toggles mpv's mute only.

## Pair protocol additions

Server to client (`PairControlMessage` in `late-ssh/src/paired_clients.rs`,
mirrored in `late-cli/src/ws.rs`; serde tag `event`, snake case):

```json
{ "event": "video_play",  "play_id": 7, "url": "https://www.youtube.com/watch?v=...", "left": 10, "top": 3, "cols": 100, "rows": 28 }
{ "event": "video_place", "play_id": 7, "left": 0, "top": 0, "cols": 180, "rows": 44 }
{ "event": "video_control", "play_id": 7, "control": "toggle_pause" }
{ "event": "video_stop",  "play_id": 7 }
```

`control` is a closed enum: `toggle_pause`, `seek_back`, `seek_forward`,
`toggle_mute`. `left`/`top` are 0-based terminal cells from the TUI's
`Rect`; `cols`/`rows` the rectangle's size. `play_id` is a per-session
counter the TUI increments on every `video_play`; the CLI ignores any
message whose `play_id` is not the one it is playing, and the TUI ignores
any `video_state` for a `play_id` that is not the open modal's. This is the
`request_id` pattern from `request_clipboard_image`, for the same reason.

Delivery: `PairedClientRegistry::send_control_to_video_cli(token, msg)`
filters on `supports_video`, like `send_control_to_open_url_cli`. Only the
CLI gets these; the webview helper never does.

Client to server:

```json
{ "event": "video_state", "play_id": 7, "state": "playing", "position_s": 83, "duration_s": 724 }
{ "event": "video_state", "play_id": 7, "state": "failed", "message": "mpv exited with status 2" }
{ "event": "video_state", "play_id": 7, "state": "stopped" }
```

And one new field on the existing `client_state`, present only when `video`
is not in `capabilities`:

```json
{ "event": "client_state", "...": "...", "capabilities": ["clipboard_image", "youtube", "voice", "open_url"],
  "video_blockers": ["not_kitty", "no_mpv", "no_ytdlp"] }
```

Blocker strings are the snake case of the closed `VideoBlocker` enum:
`not_native`, `platform`, `multiplexer`, `not_kitty`, `no_pixel_size`,
`no_mpv`, `mpv_too_old`, `no_ytdlp`. The server parses them into its own
closed enum and keeps them on `ClientAudioState`; a string it does not know
(a newer CLI than the server) is dropped with a `warn` log, not an error,
because the two binaries release on different schedules. The setup box
reads them through `PairedClientRegistry::video_blockers(token) ->
Option<Vec<VideoBlocker>>` (`None` when no CLI is paired).

`state` is a closed enum: `starting`, `playing`, `paused`, `ended`,
`failed`, `stopped`. `position_s`/`duration_s` are present for `playing` and
`paused` (`duration_s` is `null` for a live stream); `message` only for
`failed`. `stopped` answers a `video_stop` once mpv is gone. `ended`,
`failed` and `stopped` are final: whatever mpv left in the terminal is
settled, which is the server's cue to restore its terminal modes (below). The CLI sends one on every state change and
once a second while playing (ride the existing 1 s pair heartbeat tick in
`maintain_helper`'s loop; do not add a second timer). The server turns it
into `ArticleEvent::VideoState { user_id, play_id, state }` for the
session's `news::State`, which is the only consumer.

Document all of it in `late-cli/CONTEXT.md` §6 next to the clipboard and
voice messages, and add `video` to the capabilities list there.

## CLI: `late-cli/src/video.rs` (+ `video_test.rs`)

New module, same role split as the webview helper code in `ws.rs`: a
`VideoPlayer` struct the pair loop owns, with `play`, `place`, `control`,
`stop`, and a `poll` called from the 1 s tick.

Spawning:

```
<mpv> --no-config --really-quiet --terminal=no
      --vo=kitty --vo-kitty-use-shm=<yes|no> --vo-kitty-alt-screen=no --vo-kitty-config-clear=no
      --vo-kitty-left=<left+1> --vo-kitty-top=<top+1> --vo-kitty-cols=<cols> --vo-kitty-rows=<rows>
      --vo-kitty-width=<cols*cell_px_w> --vo-kitty-height=<rows*cell_px_h>
      --ytdl=yes --ytdl-format=bestvideo[height<=720]+bestaudio/best[height<=720]
      --volume=<volume_percent> --keep-open=no
      --input-ipc-server=<runtime dir>/late-video-<pid>.sock
      -- <url>
```

- mpv's `--vo-kitty-left/top` are 1-based (`man mpv`), so the CLI adds one
  to the TUI's 0-based cells. `--vo-kitty-cols/rows` are the size of the
  drawing area in cells, and `--vo-kitty-width/height` its size in pixels,
  computed from the tty's `TIOCGWINSZ` cell pixel size at spawn. These six
  numbers are the whole geometry contract; build them in one pure function
  `mpv_args(url, rect, cell_px, volume) -> Vec<OsString>` and test it as a
  whole vector.
- `--no-config` so a user's `mpv.conf` cannot change the video output or
  pull in scripts that write to the terminal. `--terminal=no` so mpv never
  reads stdin or prints. `--vo-kitty-alt-screen=no` is mandatory: the
  default would switch the user's terminal to the alternate screen and
  wipe the TUI on exit. `--vo-kitty-config-clear=no` keeps mpv from
  clearing on a reconfigure. `--vo-kitty-use-shm` is `yes` only on kitty
  and Ghostty (`KittyTerminal::supports_shm`): mpv does not fall back when
  a terminal cannot read shared-memory frames, so WezTerm, Konsole and Rio
  get escape-coded frames.
- stdin null, stderr to `$XDG_STATE_HOME/late/video.log` (`LATE_VIDEO_LOG`
  overrides, same resolution as the webview log), and stdout a pipe: a
  relay thread carries it to the tty under the stdout lock the SSH
  forwarder uses, one complete escape sequence at a time, and drops what
  mpv writes besides frames (captured from mpv 0.41 with stdout on a pipe:
  hide cursor and any-motion mouse on at start, delete-all-images at both
  ends, show cursor and mouse off at exit). Those would change state the
  TUI owns, so they never reach the terminal and nothing has to be
  restored. mpv draws fine without a tty once all six geometry options are
  given. Scheme gate as in `open_url_in_browser`: only `http://` and
  `https://` URLs are ever passed.
- Geometry change (`video_place`): the kitty output options are startup
  options. Read `time-pos` over IPC, kill the child, respawn with the new
  rectangle and `--start=<pos>`, report `starting` then `playing`. The brief
  black is acceptable. If a runtime `set_property` of the `vo-kitty-*`
  options turns out to work in the supported mpv range, prefer it and note
  it in `late-cli/CONTEXT.md`; verify, do not assume.
- Controls over the IPC socket as JSON lines:
  `{"command":["cycle","pause"]}`, `{"command":["seek",-10]}`,
  `{"command":["seek",10]}`, `{"command":["cycle","mute"]}`. Position and
  duration come from `{"command":["get_property","time-pos"]}` and
  `duration` on the 1 s poll; `pause` gives paused. `--keep-open=no` quits
  at end of file, so an exit status of 0 is `ended`.
  Wait for the socket to appear after spawn (poll up to 5 s); not appearing
  is a `failed`.
- Stop: send `{"command":["quit"]}`, wait up to 500 ms, then kill. Delete
  the socket file, join the relay. mpv's last frame stays on screen and
  text repaints do not remove graphics, so after every final state the CLI
  writes `settle_sequence`: a kitty `a=d,d=P` delete at the rectangle's
  top-left cell with `q=2`. mpv anchors every frame at that cell with no
  centering, so it is always under the image; `P` frees the image data
  too; the TUI's own images elsewhere are untouched.
- Lifecycle invariants: at most one player per CLI; `video_play` while one
  is up stops the old one first. The player stops on `video_stop`, on pair
  socket loss (the reconnect loop's disconnect path), on SSH session end,
  and in `Drop`, so no mpv outlives `late`.
- Errors are a closed `VideoFailure { UnsupportedUrl, NoPixelSize, Spawn,
  Inspect, IpcTimeout, Ipc, Exited(status) }`, mapped exhaustively to the
  `failed` message and logged at `warn` in one `match` in the pair loop
  (the orchestration layer), never inside `video.rs`. A `yt-dlp` failure
  is not its own variant: it reaches the CLI only as an mpv exit status,
  and the reason is in `video.log`.

Capability detection lives in `video.rs` too (`blockers`, above), with
`client_capabilities` a function that appends `video` when the
list is empty, and the `client_state` builder adding `video_blockers` when
it is not.

## Server: where code lives

- `news/watch/` subdomain, same roles as `news/reader/`: `state.rs`
  (`WatchModal { Player(Player), Setup(SetupBox) }`, a closed enum; the
  `Player` holds article id, url, title, meta, `WatchSize { Window, Full }`,
  `play_id`, `WatchStatus { Starting, Playing { position_s, duration_s },
  Paused { .. }, Ended, Failed, Lost }`, last state instant, the body
  `Rect` last sent; the `SetupBox` holds the url and the five or six
  `SetupRow { requirement, mark: Known(bool) | Unknown }` already decided),
  `ui.rs` (`draw` for both, the player's painting the body black and
  recording the body rect), `input.rs` (keys, `[x]`, swallowing the rest).
  `Player::layout(area) -> WatchLayout { popup, header, body, footer,
  close }` is a pure function of size and area; the body rect it returns
  is what goes over the wire. `SetupBox::decide(inputs) -> SetupBox` is a
  pure function of a `SetupInputs` struct (paired client kind and mode,
  platform, blockers, detected protocol, `TERM`, the Tweaks mode) so the
  table above is one exhaustive match, tested as a whole.
- `news::State` owns `Option<WatchModal>`; `chat/state.rs` keeps thin
  `open_watch_for_article` / `watch()` / `close_watch` delegations because
  `render.rs`, `app/input.rs` and `live/input.rs` reach it through
  `ChatState`, exactly as the reader does. Opening the watch modal closes
  the reader and vice versa; they never coexist.
- Sending: the modal is pure state. The glue that already closes the reader
  on page change (`stop_composing` / the page-change hook in `app/state.rs`)
  also closes the watch modal. The TUI emits outbound controls through
  `App` (`app/state.rs`, next to `open_stream_url`): `video_play` on open,
  `video_place` when the body rect changes between two renders (the render
  path compares the rect the last draw produced to the one last sent, so a
  terminal resize and `f` both go through the same comparison),
  `video_control` from keys, `video_stop` on close. Every send goes through
  `send_control_to_video_cli`; a send that reaches nobody flips the modal to
  `Lost` immediately.
- `ArticleEvent::VideoState` arrives on the session's event drain
  (`news::State::drain_events`) and updates the modal if the `play_id`
  matches. A `Starting`/`Playing`/`Paused` modal with no state for 10 s
  becomes `Lost` on tick.
- Nothing to restore on a final `video_state`: the CLI's relay keeps mpv's
  cursor, mouse and delete-all writes off the tty, so the TUI's modes and
  images are exactly as it left them.
- `api.rs`: parse `video_state` from the client; route it to the session by
  token the way `clipboard_image` is routed.
- `news/ui.rs::reading_label` returns `watch` for an unavailable YouTube
  story and `link` for any other unavailable story; no gate input, since
  the word is the same for everyone.
- `metrics.rs`: `record_news_watch(WatchOutcome)` with
  `WatchOutcome { Opened, Played, Ended, Failed, Lost }` from a closed enum,
  no-op twin under `#[cfg(not(feature = "otel"))]`. Server-side failures
  (a `failed` state, a `Lost`) also get an `error_span!` with the CLI's
  message as a field.

## Decisions and the one risk to name in the PR

- `watch` is shown to everyone; only playing is gated. A user who asks and
  cannot gets told exactly what is missing, once, on their own request.
- Client-side only, opt-in by presence: the user installs `mpv` and
  `yt-dlp` themselves; `late` bundles no extraction code and the server
  never touches media. This is the "if ever revisited" shape
  `late-cli/CONTEXT.md` §13 describes: client-side, the user's own
  `yt-dlp`, the IFrame player stays the default for the booth queue.
- The residual risk is the one §13 names: a YouTube video played through
  `yt-dlp` is outside YouTube's terms, and the lever Google has is the Data
  API key the booth depends on. The exposure here is smaller than the
  booth's (one person watching a link somebody posted, no shared queue, no
  background audio, no ad skipping beyond what the user's own tools do),
  but it is not zero. Name it in the PR description as a decision, point at
  §13, and keep the feature behind the capability gate so it can be switched
  off from the CLI side with one release if it has to be.
- Window first, Full on `f`. Never black by default: the TUI stays visible
  around the video and owns every key.
- Geometry changes restart mpv at the same position. Simple and correct;
  revisit only if the runtime option path is verified.
- YouTube only in this PR. Direct media links and other hosts are a
  follow-up with the same pipeline and a second `is_*` predicate.
- Windows, tmux, Sixel and iTerm2 terminals are out. Sixel is too slow for
  video over any path; iTerm2's protocol has no shared memory and no
  placement update.

## Tests

CLI (`make test-llm ARGS="-p late-cli -E 'test(video)'"`, next to the code):

- `blockers` table: each failing precondition yields its reason, several
  at once yield all of them, the passing environment yields an empty list.
  Drive it with a fake env map, a fake winsize, a version string and a
  `ytdlp_found` flag; no process spawns.
- `mpv_args` whole-vector assertion for a Window rect and a Full rect,
  including the 1-based offsets and the pixel math.
- IPC command JSON for each `control`, and the `video_state` payload for
  each state (serde round trip against the server's enum strings).
- A `video_state` or `video_control` with a stale `play_id` is dropped.
- `ws_test.rs`: `client_state` carries `video` when the list is empty and
  `video_blockers` with the snake case strings when it is not, never both.

Server:

- `paired_clients_test.rs`: `has_video_cli` and `send_control_to_video_cli`
  skip the webview helper and a CLI without the capability; the control
  reaches the capable one.
- `api_test.rs`: `video_state` parses, routes to the session, and rejects
  an unknown `state` string.
- `news/watch/state_test.rs`: `layout` for Window and Full at two terminal
  sizes (whole-state); `f` toggles and changes the body rect; `VideoState`
  with the right `play_id` updates the status, the wrong one is ignored;
  10 s without state becomes `Lost`; the footer status strings.
- `news/ui_test.rs`: `reading_label` reads `watch` for a YouTube story and
  `link` for an unavailable non-video story.
- `news/watch/state_test.rs` (setup): `SetupBox::decide` whole-state for
  four inputs: no paired client (CLI ✗, mpv and yt-dlp `·`); a native CLI
  in tmux with everything installed (only the multiplexer row ✗); a native
  CLI on Kitty missing `yt-dlp` (one ✗); all green with Tweaks images Off
  (the extra first row). The rendered box text for one of them.
- Flow test (`input_flow_test.rs`): register a fake paired client on the
  session token (`PairedClientRegistry::register`, the way the clipboard
  tests do) advertising `video`, make the detected protocol Kitty, share a
  YouTube link, press `w`: the modal opens, a `video_play` with the body
  rect lands on the fake client's channel; `f` lands a `video_place` with
  the full-screen rect; Esc lands a `video_stop`. The same walk with the
  fake client sending `video_blockers: ["no_mpv"]`: the card still reads
  `watch`, `w` opens the setup box with the mpv row marked ✗ and the yt-dlp
  row ✓, and nothing lands on the client's channel.
- `help_modal/data_test.rs`: the News topic names `w` and the requirement.

## Docs to update in the same PR

- `late-cli/CONTEXT.md`: §3 (the `video.rs` file), §4 (`LATE_MPV_BIN`,
  `LATE_YTDLP_BIN`, `LATE_VIDEO_LOG`), §6 (the four controls, `video_state`,
  the `video` capability, its preconditions, and `video_blockers`), §8 (the tty pixel size
  requirement and that the TUI, not the CLI, decides the rectangle), §12
  (Windows, tmux, non-Kitty terminals cannot watch), §13 (a line pointing at
  the shipped watch path as the "if revisited" case).
- `late-ssh/src/app/chat/CONTEXT.md` §11 News: the watch modal, the setup
  box and its decision table, the gate, the keys, `watch` on cards, the
  reader's `w`, the live strip's `o`.
- `late-ssh/src/app/live/CONTEXT.md`: `o` on a gated YouTube article opens
  the watch modal.
- `CONTEXT.md`: the "Terminal side-channel commands bypass the frame diff"
  invariant gains a sentence: video frames are written to the tty by the
  paired CLI's `mpv`, never by the server; the server only paints the
  reserved rectangle black.
- `DESIGN_GUIDE.md` §6.1 table: a `News watch` row (90 percent, width
  capped at 160, or full screen on `f`).
- `late-ssh/src/app/help_modal/data.rs` `HelpTopic::News`.
