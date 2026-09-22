# Work on late.sh: profiles, jobs, and the splash wall

Design doc for the work side of late.sh: the Profiles page (Directory, page
`5`), a remote job feed pulled from sources built for syndication, the paper
section that matches those jobs to people, and moving the Artboard wall from
the paper to the splash. Three steps, delivered in order. Each step is a PR
or a small series; nothing in a later step is needed by an earlier one.

Status: step 1 built; steps 2 and 3 design. Decisions marked OPEN need Mat.

---

## Why

- The Profiles page is a wall of text. Every person's row is four lines of
  the same weight, projects and work cards are mixed in one column, and the
  detail pane prints the card as a paragraph. The editor is eight stacked
  text areas cycled with `Tab`; status and type are typed strings with alias
  normalization. Adding and editing both feel like filling a form in a chat
  box.
- People ask for a careers page. The community is too small for a job board
  people post to: an empty board reads as abandoned. What we can do is pull
  remote postings from feeds that exist to be consumed, keep the page moving
  every day, and tell each person which of them fit their own card. The AI
  we already run reads postings; it never finds them.
- The paper's ON THE WALL column is the wrong place for gallery pieces: the
  paper is read once, scrolled past, and the pieces compete with the columns.
  The splash is a full screen that every login sees first.

---

## Step 1. The Profiles page

### What exists

`late-ssh/src/app/directory/` owns page `5`: a `40/60` split, the people list
on the left (`PersonEntry`: one row per person aggregated from the Work feed
and the Showcase feed, sorted by latest activity), the detail pane on the
right (late.fetch grid, then the work card, then the projects). Composers are
borrowed from `chat/work` (`ComposerField`: Headline, Status, Type, Location,
Contact, Links, Skills, Summary; eight `TextArea`s) and `chat/showcase`
(Title, Url, Tags, Description) and drawn into the detail pane. Keys: `j/k`
people, `h/l` focus, `Enter` copy link, `o` profile, `i` new project, `w`
work card, `e` edit, `d` delete, `/` mine, `s` search.

Status is a `String` that must be `open`, `casual`, or `not-looking`; type
is free text ("contract, full-time, freelance"); skills are free text split
on commas. The web `/profiles` page renders the same rows.

### Target

The page becomes the work page: two shelves in the same frame, People and
Jobs, one detail pane, one editor style.

**Shelves.** A strip under the page title: `people` and `jobs`, the active
one highlighted with its count. `Space` switches shelves (free on this
page; nothing global claims it). `/` keeps its meaning per shelf: on People
it filters to mine, on Jobs it filters to postings that match my card.

**People rows.** Three lines, fixed roles, so the eye can scan a column:

```
@meythewitch  ● open        4d
Full Stack Developer
rust · kotlin · typescript · react           2 projects
```

Line 1: name, status as a coloured dot plus word (green open, yellow casual,
dim not looking), age right-aligned. Line 2: headline, or the newest project
title for a person with no card. Line 3: up to five tags, then a project
count right-aligned. Nothing else on the row. A person with only projects
shows the project count on line 1 and the newest title on line 2.

**Detail pane.** Sections with headings, the way the profile modal does it:

1. A header row: `@user · status · type · location`.
2. **card**: headline bold, summary wrapped, then `skills` as a tag line,
   `links` one per line (display form, full URL copied on `Enter`),
   `contact` last.
3. **projects**: title, tags, one-line description each. Focus moves through
   them with `h/l` as today.
4. **for you** (own card only, when the card is open or casual): the newest
   job matches, up to five, each as one line: `company · role · remote scope
   · tags`. `Enter` on a match copies its link. The section reads from the
   jobs table (step 2); until step 2 lands it is absent, not empty.
5. late.fetch (country, langs, ide, os, terminal) moves to the bottom of the
   pane as a dim block. It is context, not the point of the page.

**The editor.** One modal for everything a person shows on the page,
replacing the in-pane composers: three pages, `card`, `about` (bio and the
late.fetch rows, the same ones Settings edits, saved through the same
profile call so Settings keeps them too), and `projects` (the list, each
project a small form). `Tab` switches pages. Rows are `label  value`, the
active row highlighted, `j/k` move between rows, `Enter` on a row starts
editing it (Enter or Tab commits and steps to the next), `Esc` stops
editing the row, `Ctrl+S` saves every dirty page and closes (on a project
form, saves that project and returns to the list), `Esc` on the form asks
to discard when dirty. The hint line names those keys.

Closed fields stop being typed:

- `status`: `WorkStatus { Open, Casual, NotLooking }`, `←/→` cycles.
- `type`: `WorkType { FullTime, Contract, Freelance, PartTime, Any }`,
  `←/→` cycles. "Open to any" is the fifth variant so the column is never
  null; existing free text folded onto the closest value (migration 192).
- `skills`: typed as text, normalized on save into the tag vocabulary
  (`jobs/vocab.rs`, step 2). The row shows the normalized tags live under
  the input, and what did not match the vocabulary stays as a free tag,
  shown dim. Both are stored: `skills` as written, `skills_tags` as the
  normalized array the matcher joins on.
- `summary`: the one multi-line field, a tall row.
- `headline` is required; the form refuses to save without it and says so
  on the row.

`late-core/src/models/work_profile.rs` grows `status` and `work_type` as
parsed enums with a text column behind each (`FromStr` at the row boundary;
an unknown stored value is a crash, not a fallback), and a `skills_tags
text[]` column (migration). The alias table in `chat/work/state.rs` goes
away with the typed status. The web `/profiles` index sorts by the enum and
shows the same three-line row.

**Mobile of the terminal.** Under 100 columns the split stacks: the list
full width, the detail pane opens over it on `l` or `Enter` and `h` returns.
Under 24 rows the row drops line 3.

### Tests

`directory/state_test.rs` (shelf switching, focus, the row model for a
person with card only, projects only, both), `directory/ui_test.rs` (new:
one person drawn at a wide and a narrow size, whole-frame assertion),
`chat/work/state_test.rs` (the form: closed field cycling, required
headline, tag normalization on save, dirty discard), `late-core/src/models/work_profile_test.rs`
(enum round trip, `skills_tags` written, the index order).

---

## Step 2. The job pull

### Sources

Measured on 2026-09-22 with plain `curl`; the raw pulls are in
`tmp/jobs-probe/` locally (gitignored) with a README table.

| Source | How | What it gives | Volume |
|---|---|---|---|
| HN "Ask HN: Who is hiring?" | Firebase API, keyless: `user/whoishiring.json` lists the thread, `item/{id}.json` the comments | Free text; 238 of 259 posts open with `Company \| Role \| Location \| REMOTE (scope)`; salary in 62, a URL in 224 | 259 live posts (September), 233 (August). 133 say REMOTE on the first line. Remote posts naming rust / go / elixir: 21 a month; JS/TS: 50 |
| We Work Remotely | `remote-jobs.rss`, keyless | `<region>` on every item, 81 of 82 "Anywhere in the World"; description is HTML | 82 items in 8 days, steady |
| Jobicy | `api/v2/remote-jobs?tag=rust|golang|elixir`, keyless JSON | `jobGeo` region, level, excerpt. The tag is a substring match (`rust` returns "trust") | 12 real language roles a month after a word-boundary recheck |

Not doing, with the reason measured:

- Company ATS feeds (Greenhouse, Lever, Ashby): curation is the product and
  nobody wants to curate.
- rustjobs.dev and golang.cafe: `429` Vercel security checkpoint for any
  user agent.
- justjoin.it, No Fluff Jobs, Bulldogjob, theprotocol: no feed, `/api/`
  disallowed in robots, the site API answers `503` to non-browser clients.
  The only sanctioned door is a sitemap of 10,735 pages with JobPosting
  JSON-LD in each, which is a crawler, not a feed.
- Elixir Forum jobs, elixir-radar, rustjobs.com: HTML only.
- Remotive, Working Nomads, Arbeitnow, Himalayas, RemoteOK: thin, non-dev,
  region-blind, or junk tags. RemoteOK's terms also require a follow link.
- Grounded search: it finds snippets, not postings, and a dead or invented
  link on a jobs page costs trust faster than an empty page.

Everything ingested is an API or feed published for consumption. No HTML
page is fetched for content, and the URL check is a `HEAD`.

### Remote only

Non-remote postings are never stored as content. The row keeps the
`(source, external_id)` pair as a tombstone (`status = dropped`, no text) so
the fetcher never reprocesses it. Remote kind is a closed enum:

```
RemoteKind { Worldwide, Regions, Hybrid }
```

with `regions text[]` beside it (free strings as the source gave them, "EU",
"US", "Americas"; a later pass can map them to the profile's country and
timezone). `Hybrid` is kept because HN posts say "REMOTE or HYBRID" and the
reader decides.

### Domain

`late-ssh/src/app/jobs/` (local `CONTEXT.md` there):

| File | Role |
|---|---|
| `svc.rs` | `JobsService`: the nightly press (fetch, read, release, expire) under the day's run claim, the replica's shelf snapshot, `/jobs` on demand; `tick`: the session side (snapshot copy, `/jobs`, the admin's banners). Owns logs and metrics. |
| `sources.rs` | The three feed parsers, pure: bytes in, `FetchedPosting`s out. |
| `vocab.rs` | The tag vocabulary: a closed list of canonical tags with aliases (`typescript` ← `ts`, `go` ← `golang`, `postgres` ← `postgresql`). The read's schema enum and the work card editor share it. |
| `state.rs` | The Jobs shelf's session state, `/jobs` parsing, the viewer's tag set, the match score, the match line. Pure. |
| `ui.rs` | The shelf (list beside detail, stacked under 100 columns) and the FOR YOU lines under a person's own card. |
| `input.rs` | Keys on the shelf. |
| `late-core/src/models/job_posting.rs` | Every read and write of `job_postings` and `job_press_runs` (migration 193). |

`job_postings`:

| Column | Meaning |
|---|---|
| `id` | UUID v7 |
| `source` | `hn`, `wwr`, `jobicy` (closed enum, text column) |
| `external_id` | HN comment id, WWR guid, Jobicy id. Unique with `source` |
| `status` | `pending` (fetched, not read), `queued` (read, waiting for release), `active`, `expired`, `dead`, `dropped` |
| `read_attempts` | Model reads spent; at `JOBS_MAX_READ_ATTEMPTS` = 3 the row is tombstoned |
| `url`, `company`, `title` | From the read (WWR and Jobicy keep the feed's link) |
| `remote_kind`, `regions` | Set at fetch when the feed says so, by the read for HN; never null on the shelf |
| `tags` | Canonical tags from the vocabulary only |
| `pay` | As written, or empty |
| `excerpt` | At most 400 chars, plain text |
| `raw` | The source text the read works from, cleared on settle |
| `posted_at` | The source's date |
| `released_on` | The UTC day the row went active, null while queued |
| `first_seen`, `last_seen` | Fetcher stamps |

`job_press_runs`: one row per UTC day (`run_on` PK), `status` running /
done / failed, `attempts`, `claimed_at`, `finished_at`, and the run's
counts. The row is the claim.

Nothing beyond the excerpt is stored. A card links out and names its source
(`via news.ycombinator.com`, `via weworkremotely.com`, `via jobicy.com`).

### The press

Once a day, not on a sweep: the run is due at `JOBS_PRESS_TIME` = 23:30
UTC, half an hour before the paper prints, so the day's releases are rows
when NEW WORK reads them. Every replica checks every `JOBS_CHECK_INTERVAL`
(5 min) whether the due day's run is unclaimed (`press_due_day`: today
after 23:30, yesterday before it, so a replica that was down at 23:30
catches up and still stamps the day the slice belongs to), claims the
`job_press_runs` row, and only the winner works. A replica that saw the
day finished memoizes it and asks nothing until the next day. A failed run
marks its row and is claimed again until `JOBS_MAX_RUN_ATTEMPTS` = 3; a
`running` row older than `JOBS_STALE_RUN` (90 min) is a dead replica's and
is taken over.

**Fetch.** `ON CONFLICT (source, external_id) DO UPDATE SET last_seen`, so
a repeat is a stamp and nothing else.

- HN: on days 1 to 4 of the month the run opens `whoishiring`'s newest
  submissions, finds the one titled `Ask HN: Who is hiring?`, and inserts
  its top-level comments one by one, `pending`, with the HTML turned into
  text and cut at 4,000 chars. Deleted and dead comments are skipped.
  After day 4 the thread is left alone; late posts are not worth the calls.
- WWR: every run reads `remote-jobs.rss`; the region tag decides
  `remote_kind` and `regions` at fetch time ("Anywhere in the World" is
  worldwide, "Europe Only" is a region).
- Jobicy: every run, five requests (`rust`, `golang`, `elixir`,
  `typescript`, `javascript`), the newest 30 rows each. The API's tag
  match is a substring match, so a row is kept only when the query word
  appears whole somewhere, or a title word (`Go` for `golang`, `React`
  for `javascript`) whole in the title; the rest are inserted `dropped`.
  `jobGeo` decides the scope. JS/TS is where Jobicy has volume (about
  four a day per tag, all real); rust, go, and elixir are a handful a
  month. HN and WWR carry JS/TS without a filter.

**Read.** One `AiService::generate_json` call per `pending` row, schema
enforced, no grounding: `remote`, `remote_kind`, `regions`, `company`,
`title`, `tags` (enum over the vocabulary), `pay`, `url`, `excerpt`. The
model decides remote only for HN; the feeds already said. `remote = false`
settles the row `dropped`. A usable answer settles `queued` (HN) or
`active` with `released_on = the run's day` (WWR, Jobicy: these sources
are already spread over the week). A failed call counts an attempt and the
row waits for the next night; at the cap it is `dropped`. One `HEAD` at
the link before the row settles: a 404 or 410, or no host to talk to, is
`dead`; anything else (a bot wall's 403, a 405 on HEAD, a timeout) keeps
the link, since the reader's browser may well get through. At most
`JOBS_READ_LIMIT` = 400 reads a run.

Cost: about 260 HN posts, 350 WWR items, and 250 Jobicy rows a month, on
the same Flash model as the paper. About a dollar.

**Release, the drip.** HN rows go active over `JOBS_DRIP_DAYS` = 14 from
the earliest queued post. The run releases one slice:

```
n = ceil(queued_hn_rows / days_left_in_window)   (at least 1 day left)
UPDATE job_postings SET status = 'active', released_on = $day
WHERE id IN (SELECT id FROM job_postings
             WHERE source = 'hn' AND status = 'queued'
             ORDER BY posted_at, id LIMIT n FOR UPDATE SKIP LOCKED)
RETURNING id
```

Past the window everything still queued goes at once. With 130 remote
posts a month this is 9 or 10 a day, next to WWR's 11. The slice is
released only under the day's claim, so `/jobs pull` on a day that already
ran fetches and reads but releases nothing.

**Expire.** `active` rows become `expired` 30 days after `released_on`, or
when a WWR guid has been absent from the feed for 7 days. Expired, dead,
and dropped rows leave every listing and stay as tombstones.

**The shelf snapshot.** Readers never query. Each replica keeps the active
rows in a `watch` (`JobsSnapshot`, newest release first), re-read after
its own press and every `JOBS_SNAPSHOT_REFRESH` (15 min), and every
session copies it in `tick`. The kill switch empties the snapshot on the
next refresh.

**Switches and commands.** `jobs_enabled` (`app_flags`, kill switch, seeded
on; the press does nothing while it is off, the shelf says so, the paper
prints no NEW WORK). `/jobs` opens the shelf for anyone. Admin: `/jobs
pull` runs the press now (the whole run when the day's claim is free,
fetch and read only when the day already ran), `/jobs release` releases a
slice on top of whatever the day did, `/jobs on|off`. Each answers with a
tally banner; `note_read` is the one place a read's outcome becomes a
tally line, a metric, and a log line.

**Telemetry.** `record_jobs_fetch(JobSource, JobsFetchResult)`,
`record_jobs_read(JobsReadResult)` (queued / active / dropped / dead /
failed), `record_jobs_press(JobsPressResult)` (ran / lost / failed),
`record_jobs_released(count)`. Fetch, read, claim, and run failures log
through `late_core::error_span!` with source and external id.

### Matching

A match is tag overlap, computed in memory over the snapshot: the viewer's
tags are their card's `skills_tags` plus their profile `langs`, both folded
through the vocabulary, and a posting scores the number of those tags it
carries. Best score first, newest release inside a tie. The paper reads
the covered day's released rows and the reader's card and langs at open
time and scores the same way. Region against country and timezone is a
later refinement, once `regions` has been seen in the wild for a month.

### Surfaces

1. **The Jobs shelf** on page `5` (`Space` from People, or `/jobs` from
   anywhere): every `active` row, newest release first, the list beside a
   detail pane (stacked under 100 columns: `l` opens the card, `h`
   returns). Row: `Company · Role` with the age at the right / `scope ·
   pay` / `tags` with `via <site>` at the right. The card: company, role,
   where, stack, pay, link, via, the excerpt. `Enter` copies the link. `/`
   keeps postings that carry the viewer's tags and says what to fill in
   when they have none. The strip counts the visible rows.
2. **FOR YOU** under the viewer's own card on the People shelf, when the
   card is open or casual: up to five matches as one line each, then a
   pointer at the shelf. Not-looking gets nothing; other people's cards
   never show it.
3. **The Late Edition**, section NEW WORK, after ANNOUNCEMENTS and before
   YOUR ROOMS. Covers postings with `released_on = D - 1`, read at open
   time with no claim and no model, the way ON THE WALL is read.
   - Card `open` or `casual`: up to three matches, one line each,
     `Company · role · scope · tags · pay`, then a dim line naming the
     total released and pointing at page `5`; with no match on the tags, a
     line saying so.
   - No card: one line, "11 postings landed yesterday, remote only. Open a
     work card on page 5 and the paper will pick yours." That line is the
     whole incentive to fill the card.
   - Card `not-looking`: nothing. They said so.
   - Zero postings released, or the switch off: the section is absent.
   `PaperIssue` carries `work: PaperWork`; a preview has none.
4. **The web `/jobs` page** in `late-web`: not built. The same active rows,
   public, with a source line per card and the `/profiles` link; the
   traffic door, not the feature.
5. No lounge bot card. The paper is the channel.

### Tests

`job_posting_test.rs` (upsert as a stamp, tombstones, settle, the attempt
cap, the drip slice and its order, expiry by age and by absence, the daily
claim with reclaim and stale takeover), `jobs/sources_test.rs` (the three
parsers over trimmed real pulls in `jobs/fixtures/`, the word-boundary
recheck, HTML to text), `jobs/state_test.rs` (the viewer's tags, the match
ranking, the match line, `/jobs` parsing), `jobs/svc_test.rs` (the due
day, the drip arithmetic, the excerpt cut, a release landing on the shelf
through the snapshot and `/jobs`, the admin gate), `directory/ui_test.rs`
(the shelf drawn empty, with rows, and filtered), `paper/state_test.rs`
(NEW WORK for a reader with a card, without, not looking, and on a day
with nothing).

---

## Step 3. The splash wall

### What exists

The paper prints ON THE WALL: yesterday's most applauded pieces, up to
three, in colour, under the Outside page (`PaperWall`, read at open time,
`ArtboardPiece::most_applauded_hung_on`). The splash hangs last month's
podium (`ART1` to `ART3`) one place per login until the account has seen
them all, then the coffee cup (`GalleryService::claim_splash_piece`,
`User::claim_splash_podium_slot`, `draw_splash_piece`).

### Target

The paper loses ON THE WALL: the section, `PaperWall`,
`PaperOutcome::Ready.wall`, the `piece_paint_lines` call in the paper, and
the layout test rows. The paper is words again.

The splash shows the wall instead, one piece per UTC day:

- Every piece hung on day D enters a queue in hang order. The splash shows
  the queue's head for the whole of the next day it is free: two pieces
  hung on Monday show on Tuesday and Wednesday, one each. The queue
  drains at one a day, so a busy weekend spills into the week.
- A day with nothing queued shows the cup.
- Each account sees the day's piece once: the first login of the day
  claims it, later logins that day get the cup. A terminal too small for
  the piece spends the claim, as today.
- A piece a mod removes after it was assigned a day shows as the cup on
  that day; nothing is promoted into the gap.

**Assignment** is a row claim on `artboard_pieces.splash_on date`, made by
the gallery's hourly refresh (`start_splash_refresh_task`), first pass of
each UTC day:

```
UPDATE artboard_pieces SET splash_on = $today
WHERE id = (SELECT id FROM artboard_pieces
            WHERE splash_on IS NULL AND removed_at IS NULL
            ORDER BY created LIMIT 1 FOR UPDATE SKIP LOCKED)
  AND NOT EXISTS (SELECT 1 FROM artboard_pieces WHERE splash_on = $today)
RETURNING id
```

Pieces hung today are eligible tomorrow, so the pass runs over rows with
`created < $today`. The splash `watch` carries today's piece (one
`SplashPiece`, or none), refreshed by the same task. The login claim
becomes `User::claim_splash_shown(today)`: one `UPDATE ... RETURNING` on
`users.settings.splash_shown_on`, replacing the podium slot counter.

**The podium.** OPEN: where last month's `ART1` to `ART3` go once the
splash is a daily wall. Recommended: the three podium pieces take the
first three days of the month, in place order, and hung pieces queue
behind them; that keeps one rule (one piece a day) and the prize still
gets the room. The alternative, dropping the podium from the splash and
leaving it to the hall of fame, is simpler and loses the moment.

**Backlog risk.** The hang cap is three a day per person, so a burst can
queue more days than it is worth; a piece shown three weeks after it was
hung is stale. Measure the hang rate on prod (`artboard_pieces` per day
over the last two months) before choosing a cap; if it is under one a day
the queue needs none.

### Tests

`paper/state_test.rs` and `paper/svc_test.rs` lose the wall rows;
`artboard_piece_test.rs` (the assignment claim: FIFO, one per day, skips
removed, two replicas), `user_test.rs` (the daily claim hands out one
view then the cup), `gallery/svc_test.rs` (the refresh publishes today's
piece and none on an empty day).

---

## Order of work

1. Step 1, the page. Two PRs: the typed card and the editor first (model,
   migration, form modal, web index), then the rows, the detail pane, and
   the shelf strip with the Jobs shelf empty behind it.
2. Step 2, the pull: the model, the press, the shelf, the FOR YOU lines,
   and the paper section in one branch. The web `/jobs` page is a
   follow-up, whenever.
3. Step 3, the wall. One PR.

## Open decisions

1. The podium's place on the daily splash wall.
2. A cap on the splash queue, after measuring the hang rate.
