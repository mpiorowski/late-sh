# The job feed (`app/jobs`) Context

## Metadata
- Domain: the Jobs shelf of the Profiles page (`5`, `Space` from People, `/jobs` from anywhere), the post form on it (`n`, `/jobs post`), the FOR YOU lines under a person's own card, and the NEW WORK section of The Late Edition. Remote postings pulled once a night from feeds published to be read, read into a card by the model, plus postings people write on the shelf themselves, matched to work cards by tag.
- Status: Active. The design and the measured source numbers are in `JOBS.md` (Step 2).

## What it is

A shelf the feeds keep full, that people can also write on. Three sources (Ask HN Who is hiring, We Work Remotely's RSS, Jobicy's tag API) are fetched once a night, each posting is read once by the model into `company · role · scope · tags · pay · excerpt · link`, and the card links out. Nothing beyond the excerpt is stored. HN's monthly burst is released over fourteen days so the shelf has something new every morning; the feeds are already spread, so their rows go active as read. A posting written on the shelf (`n`, the post form: company, role, link, where, regions, stack from the tag picker, pay, excerpt) is the fourth source, `late`: active the moment it is saved, `posted_by` its writer, `JOBS_POSTS_PER_USER` = 3 live per person, `d` takes it down (its writer, or a moderator), and it expires like every other row.

## Module map

| File | Role |
|---|---|
| `svc.rs` | `JobsService`: the nightly press (`check_press`, `press`: fetch, read, release, expire) under the day's `job_press_runs` claim, the replica's shelf snapshot (`JobsSnapshot` behind a `watch`), the admin's `/jobs pull|release` on demand; `tick(app)`: the session side (snapshot copy, `/jobs`, banners, flag writes). Every log line and metric of the press lives here. |
| `sources.rs` | The three parsers, pure: HN Firebase items, the WWR RSS, a Jobicy page, into `FetchedPosting`s. HTML to text, entity decoding, the word-boundary recheck. |
| `state.rs` | `JobsState` (per session: the copied rows, selection, the for-me filter, the stacked detail), `JobsCommand` + parser, `viewer_tags`, `matches`, `match_line`. No I/O. |
| `ui.rs` | The shelf: list beside detail, stacked under 100 columns; `for_you_lines` for the People shelf's detail pane; `draw_post_form`, the modal over the page. |
| `input.rs` | Keys on the shelf: `j/k`, `h/l` (stacked), `Enter`/`c` copy the link, `/` for me, `n` the post form, `d` take down; the post form's keys (`handle_post_input`). |
| `post.rs` | `PostForm`: the post form's rows, typing, `validate_post` into a `NewPosting`. Pure. |
| `fixtures/` | Trimmed real pulls for `sources_test.rs`. |
| `late-core/src/models/job_posting.rs` | Every read and write of `job_postings` and `job_press_runs` (migrations 193 and 194): the press's upsert/settle/release/expire, and `post`, `retract`, `count_active_by` for the shelf's own writes. |
| `late-core/src/vocab.rs` | The tag vocabulary: canonical tags in groups (languages, frameworks, data, infra, practices, tools) with aliases, and `TAG_LIMIT` = 12. The read's prompt, the matcher, the tag picker, a card's `skills`/`skills_tags`, and a profile's `langs` hold nothing outside it. |

The directory page (`app/directory`) owns the shelf strip and the `Space`/`w`/`i` keys and delegates the Jobs shelf's draw and keys here; `app/paper` reads `JobPosting::list_released_on` for NEW WORK and scores with `matches`.

## The press (multi-replica rule as applied)

- Due at `JOBS_PRESS_TIME` (23:30 UTC), half an hour before the paper prints, so NEW WORK reads rows. `press_due_day(now)` is today after 23:30 and yesterday before it: a replica that was down at 23:30 catches up at its next check and stamps the right day.
- Every replica runs `start_press_task` (`JOBS_CHECK_INTERVAL`, 5 min). A check claims the due day's `job_press_runs` row (`JobPressRun::claim`: a new row, a `running` one older than `JOBS_STALE_RUN` = 90 min, or a `failed` one under `JOBS_MAX_RUN_ATTEMPTS` = 3) and only the winner runs; a loser that reads the day as finished memoizes it and spends no query until the next day. The same task refreshes the replica's snapshot after its own press and every `JOBS_SNAPSHOT_REFRESH` (15 min).
- Fetch: `JobPosting::upsert_fetched` is a stamp on a known pair. HN only on days 1 to 4 of the month (`HN_FETCH_THROUGH_DAY`), the thread found by title among `whoishiring`'s newest submissions. WWR and Jobicy every night; their scope is decided at fetch time from the feed's region field. Jobicy is five queries (`rust`, `golang`, `elixir`, `typescript`, `javascript`), the newest `JOBICY_COUNT` = 30 rows each; rows that fail the word-boundary recheck are inserted `dropped`. One failed item (an HN comment, a Jobicy tag) is skipped, logged (`jobs_fetch_item_failed`), and counted in the banner; only a whole source failing (no HN thread, the WWR feed down) marks the source failed. JS/TS also arrive through HN (every remote post is kept, whatever the stack) and WWR (unfiltered).
- Read: one `generate_json` (schema enforced, `AI_MODEL`) per `pending` row, at most `JOBS_READ_LIMIT` = 400 a run; the model decides remote only for HN. A failed read counts an attempt and waits for the next night; at `JOBS_MAX_READ_ATTEMPTS` = 3 the row is tombstoned. One `HEAD` at the link: 404, 410, or no host is `dead`; everything else keeps the link. Each read settles and shelves on its own (`settle_read`): an `active` row joins this replica's snapshot at once, so the shelf fills while the run is still reading and a run that fails later keeps what it read. Other replicas see it at their next refresh.
- Release: `drip_slice` = `ceil(queued / days left of JOBS_DRIP_DAYS = 14 from the earliest queued post)`, released with `FOR UPDATE SKIP LOCKED`, stamped `released_on = the day`. Only under the day's claim (`Release::Slice`); `/jobs pull` on a day that already ran passes `Release::Skip`.
- Expire: `active` older than `JOBS_EXPIRE_DAYS` = 30 since release, and WWR rows unseen for `JOBS_WWR_ABSENT_DAYS` = 7.
- Switch: `jobs_enabled` (`app_flags`, seeded on). Off: no press, an empty snapshot on the next refresh, the shelf says so, no NEW WORK.
- Clients are scoped to each query; nothing holds a pooled connection across a model call or a fetch.
- A shelf write (`request_post`, `request_retract`) is fire-and-forget through the service: the cap check and the insert, then this replica's snapshot refreshes at once and the others catch it within `JOBS_SNAPSHOT_REFRESH`; the answer comes back as a `JobsEvent` for the form and a banner. The take-down's owner scope is in the query (`posted_by = $2 OR moderator`); feed rows are out of its reach. A moderator's take-down of someone else's posting writes a `job_posting_take_down` entry to `moderation_audit_log` naming the writer.

## The shelf

- Sessions never query. `tick` copies the snapshot when it moved (`watch::has_changed`), so a render reads local memory.
- The viewer's tags are their card's `skills_tags` plus their profile `langs` (`viewer_tags`), both picked from the vocabulary in the tag picker (`app/tag_picker`), so the two sides of a match share one closed list; a posting's score is how many of those it carries (`score`); `/` keeps score > 0, best first. The strip counts the visible rows.
- FOR YOU under the viewer's own card (People shelf) prints up to `FOR_YOU_LIMIT` = 5 matches when the card is open or casual; NEW WORK in the paper prints up to `PAPER_MATCHES` = 3 from the covered day's releases, or the one-line nudge for a reader with no card.

## Telemetry

`record_jobs_fetch(JobSource, JobsFetchResult)` per source per run and per skipped item (`item_skipped`), `record_jobs_read(JobsReadResult)` per posting (queued / active / dropped / dead / failed), `record_jobs_press(JobsPressResult)` per check that reached a claim (ran / lost / failed), `record_jobs_released(count)`, `record_jobs_post(JobsPostResult)` per shelf write (posted / retracted / at_cap / failed). Failures log through `late_core::error_span!` (`jobs_fetch_failed`, `jobs_fetch_item_failed`, `jobs_read_failed`, `jobs_claim_failed`, `jobs_press_failed`, `jobs_press_on_demand_failed`, `jobs_post_failed`, `jobs_retract_failed`).

## Tests

`sources_test.rs` (the parsers over `fixtures/`), `state_test.rs` (tags, ranking, the match line, `/jobs`), `post_test.rs` (the form's rows and rules), `svc_test.rs` (the due day, the drip arithmetic, the excerpt cut, a release reaching the shelf through the snapshot and `/jobs`, a read shelved as it settles, the admin gate, a shelf write landing, the per-person cap, and the take-down against a real DB), `late-core/src/models/job_posting_test.rs` (upsert, settle, the attempt cap, the slice, expiry, the daily claim, post and retract), `directory/ui_test.rs` (the shelf drawn), `paper/state_test.rs` (NEW WORK).

## Gotchas

- `JOBICY_TAGS` pairs a query word with title words on purpose: the API's `tag=` is a substring match (`rust` returns "trust"), and "go" as a verb is in every description, so `golang` rows survive only on "golang" anywhere or "Go" in the title. The same posting comes back under `typescript` and `javascript`; the upsert makes the second a stamp.
- HN comments come as HTML with `&#x2F;` slashes; `html_to_text` decodes numeric entities, and the anchor text (the URL itself) survives tag stripping, which is what the read's `url` field quotes.
- The read's schema names no enum for `tags`: Gemini answers 400 (`INVALID_ARGUMENT`, no detail in the body) to an enum of the vocabulary's size on an array item. The list rides in the system prompt and `vocab::normalize` drops anything outside it.
- A tag added to the vocabulary reaches new reads only; rows read before it keep their tags.
- The run row is the only claim; postings have no per-row claim. Two replicas can never read the same posting because only one holds the day.
- The web `/jobs` page is not built (JOBS.md, Surfaces).
