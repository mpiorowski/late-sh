# Plan: the remaining new Arcade dailies

Status: the refactor that makes a new daily one roster variant ("the
tree") is in, and so are Netwalk and Chess Puzzle
(`late-ssh/src/app/arcade/{netwalk,chess_puzzle}/`); the two word games
are investigated, not implemented, one PR each in the order below. As each lands, its shipped behavior moves into
`late-ssh/src/app/arcade/CONTEXT.md` and `GAMES.md` and its section here is
deleted; delete this file with the last one.

## Why these games

`AUDIENCE.md` section 4: the Arcade is the second room, 32% of active time,
and its daily is the strongest ritual the site has. The shape that works is
one shared object a day, finished in minutes, single-player, with a share
card worth pasting. Every game here is a daily puzzle in the existing
category, never a high-score run (score games hold a tenth of the hours).

| Game | Shape | Tiers | Content | Why first/last |
|---|---|---|---|---|
| Spelling Bee | find words from seven letters | one board, three tiers banked by rank | offline pack built from a dictionary | needs a dictionary decision (below) |
| Connections | sort sixteen words into four groups | one board, one tier | hand-authored pack | content is the cost; the pack must be skimmed |

## The shape every game copies

Read `late-ssh/src/app/arcade/CONTEXT.md` and `GAMES.md` ("Adding a new
Arcade game") first. The reference implementations:

- **Tiered, daily + personal:** `arcade/sliding_puzzle/` (migration 167),
  `arcade/netwalk/` (265) and `arcade/chess_puzzle/` (266):
  `<game>_games (user_id, mode, difficulty_key, puzzle_date,
  puzzle_seed|puzzle_id, <board>, UNIQUE(user_id, difficulty_key, mode))`
  and `<game>_daily_wins (user_id, difficulty_key, puzzle_date, <score>,
  UNIQUE(user_id, difficulty_key, puzzle_date))`.
- **One global board a day, claimed in the DB:** `arcade/le_word/svc.rs`
  (`DailyWord::lock_daily_creation`, the unused-first pick, the `oneshot`
  reload drained by `poll_word_reload`). Spelling Bee and Connections copy
  this: a `<game>_daily_puzzles (puzzle_date UNIQUE, puzzle_id)` row claimed
  under the advisory lock, picked among pack ids not yet used, else the id
  used longest ago (never "no puzzle today").
- **No-tier win publish:** `arcade/rubiks_cube/svc.rs::record_win_and_publish`
  (the `detail` is `"daily"`). Tiered games pass the difficulty key as
  `detail`, as Sliding Puzzle does.
- **Offline pack built by a generator, embedded with `include_bytes!`:**
  `late-core/src/chess_puzzle.rs` and `late-core/src/bin/gen_chess_puzzles.rs`
  (a dump path in, one JSON per tier out, one puzzle per line), with
  `chess_puzzle/state_test.rs::every_shipped_puzzle_replays_through_the_board`
  pinning the shipped pack; the nonograms are the older copy. Spelling Bee
  copies the generator half; its daily pick is the claimed row below.

Every game ships with the same file roles: `mod.rs` (declarations only),
`state.rs` pure rules and per-session state, `input.rs`, `ui.rs`, `share.rs`
pure card, `svc.rs` tasks calling `late-core` models, and the `_test.rs`
twins beside each. SQL lives in `late-core/src/models/<game>.rs`.

## Wiring a game

Adding a variant to `roster!(DailyPuzzle: ...)` fails the build at every
site that needs the new game; the order to fix the red, the three tests
that hold what the compiler cannot see, and the prose to update are in
`GAMES.md`, "Adding a new Arcade game". A board's chips and Arcade Wins
points both come from its tier in `DailyPuzzle::tiers()`.

### Tests, per game

- `state_test.rs`: whole-state tests. Drive the pure state N steps from a
  fixed seed or pack entry and assert the full result; the generator is
  deterministic per (date, difficulty); the solved check is right on the
  solved and one-off boards; the rollover swaps the board only off-screen.
- A pack validator test pinning the shipped assets (bee, connections),
  like the chess and nonogram ones.
- `svc_test.rs` beside `svc.rs`: a fresh win inserts, publishes `GameWon`
  with the right `detail`, and bumps `daily_win_totals`; a replay inserts
  nothing and publishes nothing; a personal finish publishes nothing.
- `share_test.rs`: the card for a fixed finish, whole card asserted.
- Run `make test-llm ARGS="-p late-ssh --lib -E 'test(arcade)'"` and
  `make test-llm ARGS="-p late-core --lib -E 'test(<game>)'"`; `make check`
  is the owner's.

## 1. Spelling Bee

**Rules.** Seven distinct letters, one of them the centre. A word counts
when it has four or more letters, uses only those letters (repeats
allowed), and contains the centre. A pangram uses all seven. Four-letter
words score 1, longer words their length, a pangram its length plus 7. No
`s` among the letters (the classic rule, and it keeps `s` free as the share
key). Ranks are a share of the puzzle's maximum score.

**Tiers, one board.** The three win rows are ranks on the same board,
banked as each threshold is crossed: `easy` at 15% (Solid), `medium` at 40%
(Great), `hard` at 70% (Genius). `tiers()` is `Keyed(easy/medium/hard)`,
so points, status and the card marks follow. The lobby card shows the three
✓/✗ marks like Sudoku's. A `MIN_RANK` table in `state.rs` is one closed
`enum Rank` with its share and label; the three paid ranks map to
`Difficulty` exhaustively.

**Pack** (`late-core/src/bin/gen_spelling_bee.rs`, dictionary path as the
argument, writes `late-ssh/assets/spelling_bee/puzzles.json`). The pack
holds, per puzzle, the letters, the centre, and the full answer list, so the
runtime embeds no dictionary at all and "not in the word list" is a set
lookup. Build: from the dictionary keep lowercase `a`-`z` words of four or
more letters with at most seven distinct letters and no `s`; candidate
letter sets are the distinct letters of each pangram-able word; keep a
(letters, centre) when it yields 25 to 70 answers and at least one
pangram; 2,000 puzzles by seeded shuffle. Dictionary: a decision for the
owner, see below. The pack validator test pins every shipped puzzle's
answers against its letters and centre, and the pangram count.

**Daily claim.** Le Word's pattern: `spelling_bee_daily_puzzles
(puzzle_date UNIQUE, puzzle_id)` claimed under `lock_daily_creation`,
unused ids first, else the one used longest ago. The state loads the day's
puzzle through a `oneshot` drained on tick, like Le Word's word.

**State.** `found: Vec<String>` (persisted `TEXT[]`, one row per
`(user_id, puzzle_date)`), the typed buffer, the outer-letter order (session
only, shuffled by `Tab`), `message`. Score and rank are derived, never
stored. Each submission saves; crossing a paid rank calls
`record_win_task(user_id, puzzle_date, difficulty_key)` once per tier (the
table's UNIQUE is the gate, the state keeps a `banked` set so it never
re-sends).

**Controls.** Type `a`-`z`, `Enter` submits, `Backspace` deletes, `Tab`
shuffles, `!` rules, `Esc` leaves (`q` is a letter, as in Le Word), `s`
share card once Solid is reached.

**Rendering.** The seven letters as a hexagon with the centre in
`AMBER`, the typed word under it with the centre letter coloured, the rank
meter (`▰▱` like the quest strip, nine slots), score, and the found words
in columns, pangrams bold. Messages: "Too short", "Missing centre letter",
"Not in word list", "Already found", "Pangram!".

**Share card.** Header `Spelling Bee #N`, then the rank reached and the
counts ("Great · 31 words · 1 pangram"), a nine-glyph rank row (Yellow for
each rank reached, White beyond), a row of Green per pangram found. No
words on the card.

**Tables:** `spelling_bee_daily_puzzles`, `spelling_bee_games`,
`spelling_bee_daily_wins (user_id, puzzle_date, difficulty_key, score,
UNIQUE(user_id, puzzle_date, difficulty_key))`. Key `spelling_bee`, title
`Spelling Bee`, icon `🐝`.

## 2. Connections

**Rules.** Sixteen words, four hidden groups of four, each group a colour
by difficulty: yellow (plainest), green, blue, purple (the trick). Select
four, submit; a right guess collapses into its bar with the group's title;
a wrong guess costs one of four mistakes, "One away" when three of four
match. Out of mistakes is a lost finish, the groups revealed.

**Pack.** `late-ssh/assets/connections/puzzles.json`, hand-authored: a
list of `{ "groups": [ { "tier": "yellow"|"green"|"blue"|"purple", "title",
"words": [4] } ] }`. No generator; the implementer writes the first 60 and
a validator test pins the pack: sixteen distinct words, four groups of
four, each tier exactly once, lowercase ASCII, title at most 24 chars. The
owner skims the pack before merge (a model-written set reads fine for
yellow and green and weak for purple; expect to rewrite those). Topping
up the pack is a plain JSON edit.

**Daily claim.** Le Word's pattern, as Spelling Bee: `connections_daily_puzzles
(puzzle_date UNIQUE, puzzle_id)`, unused first, else the id used longest
ago. A repeat is noticed in this game, so this is the one place the date
hash is wrong.

**State.** The sixteen words in their shuffled order (persisted `INT[]` of
pack indices so a reload keeps the board), `solved: Vec<Tier>` in solve
order, `mistakes`, `guesses: Vec<[u8; 4]>` (persisted, the share card is
the guess history), cursor and `selected` (session only). One row per
`(user_id, puzzle_date)`. Solved when four groups are in; lost at four
mistakes; one win row per `(user_id, puzzle_date)` with `mistakes` as the
score. One tier: `tiers()` is `Single(Medium)` like Rubik's, `detail`
`"daily"`.

**Controls.** Arrows or `h/j/k/l` move, `Space` toggles the word, `Enter`
submits four, `x` clears the selection, `Tab` shuffles, left click toggles,
`!` rules, `s` share card on a finished board.

**Rendering.** A four-by-four grid of word tiles (tile width from the
longest word, 10 to 16 cells), selected tiles inverted, solved groups
collapsed to full-width bars at the top in their tier colour with the
title and the four words, the mistakes as four dots that go out. The tier
colours come from the theme (`AMBER`, `SUCCESS`, `INFO`, a purple token if
the palette has one, else `ACCENT`); the share card uses the glyphs.

**Share card.** The classic: one row of four glyphs per guess in the
colour of each word's group (Yellow, Green, Blue, Purple), so the picture
is the guess history and spoils no words. Add `Glyph::Purple` (`🟪`, ASCII
`%`) to the closed set in `arcade/share.rs`; the build walks every match.

**Tables:** `connections_daily_puzzles`, `connections_games`,
`connections_daily_wins (user_id, puzzle_date, mistakes, UNIQUE(user_id,
puzzle_date))`. Key `connections`, title `Connections`, icon `🟪`.

## Decisions for the owner

- **Dictionary for Spelling Bee.** The Le Word lists are five-letter only.
  Candidates: SCOWL `en_US` up to size 60 (permissive, needs its
  attribution line in `LICENSING.md`; common words, few oddities), or
  ENABLE (public domain, 172k words, many obscure entries that make "not in
  word list" feel arbitrary). Recommendation: SCOWL size 60. The dictionary
  is a generator input, never vendored; only the pack ships.
- **Chips.** Every daily is a faucet and `AUDIENCE.md` calls the economy
  deflated. The plan keeps the standard tiers so the new games read like
  the rest; lowering payouts is a `reward_templates` edit, not a design
  change.
- **Personal mode** is out for the two word games (one board a day is the
  game).

## Out of scope

A typing test (named in the same conversation, deliberately left out), a
mini crossword (clues need editing), solve timers and per-game streaks (a
separate plan: they touch every existing daily). The "solved today" count
on the lobby card and the `o` solvers roster are shipped
(`late-ssh/src/app/arcade/solvers.rs`), not part of this plan.
