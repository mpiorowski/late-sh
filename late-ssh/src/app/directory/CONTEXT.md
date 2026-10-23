# People (page 5) and the profile editor

`late-ssh/src/app/directory` renders the People page (the code still names the
screen `Screen::Profiles`, and attention metrics label it `profiles`) and owns
the profile editor modal. The page has three sections; two belong to other
domains:

- **Map** (`app/usermap/CONTEXT.md`): the Earth shaded by people per country.
- **Profiles** (this domain): everyone with a work card or a shared project.
- **Jobs** (`app/jobs/CONTEXT.md`): the remote job feed.

## 1. Files

- `state.rs`: the section enum and its stepping (`[` left, `]` right, both
  wrap), the Profiles list and selection, focus inside the detail.
- `input.rs`, `mouse.rs`: page keys and clicks; a click on nothing this page
  draws falls through to the frame and sidebar handlers.
- `ui.rs`: the section row, the list, the detail pane, the footer.
- `editor/`: the profile editor (below). `app/tag_picker/` is the multi-select
  it opens for skills and langs (also used by Settings).

Showcase and Work data come from chat-adjacent services and unread cursors
(`app/chat/CONTEXT.md`, Showcase and Work): their badges are read at login and
cleared on visiting this page; nothing moves them live.

## 2. The page

- One row under the frame names the three sections with their counts, the
  current section's filters on the right (Map: online, month, all; Profiles:
  everyone, mine, search; Jobs: all, for me, search). The title bar reads
  `People · [ ] section`. The footer is the section's keys on the left and
  `w your profile · i add project` on the right. The page opens on Map.
- **Profiles list:** connected people first under an `online` rule, everyone
  else under `offline` (no rules while nobody is connected), each group by
  last login, newest first (`users.last_seen`, stamped at connect and not
  moved during a session); the cursor follows its person when a login or
  logout regroups the rows. Each row is three fixed lines: name, status dot
  and time since last login; headline or newest project; five tags and the
  project count.
- **Detail:** beside the list from 100 columns, stacked under it below that
  (`l`/`Enter` opens it over the list, `h`/`Esc` returns). Sections: header
  (`@user · status · type · location`), CARD, ABOUT (bio), PROJECTS, then
  late.fetch as a dim block.
- The first-visit tour walks the three sections as three short stops
  (`app/clubhouse/CONTEXT.md`, tutorial).

| Key | Action |
|-----|--------|
| `[` / `]` | Previous / next section (wraps) |
| `w` / `i` / `e` | Open the editor on your card, on a blank project, or on the focused item |
| `h` / `l` | Move the focus over the card and projects |
| `Enter` | Copy the focused link |
| `d` | Delete the focused item (your own, or any for a moderator) |
| `o` | Open the read-only profile modal |
| `s`, `/` | Search; filter to your own row |
| mouse | Click a person, job, section, filter, footer action, editor tab or field; the wheel scrolls the hovered pane without moving selection; links copy their full URL |

## 3. The profile editor (`editor/`)

One modal for everything a person shows here, three pages switched with
`Tab`: `card` (headline, status, type, location, contact, links, skills,
summary), `about` (bio, ide, terminal, os, langs: the same rows Settings has,
saved through the same profile call), `projects` (the list; `a` adds, `Enter`
edits, `d` deletes; each project is a small form).

- Rows are `label  value`. `j`/`k` move, `Enter` types into a row (Enter or
  Tab commit and step on), `←`/`→` cycle status and type, `Ctrl+S` saves every
  dirty page and closes (on a project form it saves that project and returns
  to the list), `Esc` stops typing, then asks before discarding.
- Status and type are closed enums (`late_core::models::work_profile`).
  Skills and langs are picked, never typed: the tag picker filters the
  vocabulary (`late-core/src/vocab.rs`; langs offer its language group alone),
  Space/Enter pick, Backspace drops the last pick, Esc hands the list back.
  `skills` and `skills_tags` hold the same canonical tags, twelve at most.
- A moderator editing someone else's card or project gets that page alone.
