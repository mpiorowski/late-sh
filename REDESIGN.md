# late.sh — Home/Chat Merge Redesign

Status: proposed, not yet implemented
Last updated: 2026-05-12
Audience: human + LLM agents picking this up cold

This is a forward-looking design spec. Treat decisions here as load-bearing; treat open questions as things to resolve before coding, not after.

For current-state architecture and ownership, read these first and assume their contents:
- `/CONTEXT.md` (root)
- `/late-ssh/src/app/chat/CONTEXT.md`
- `/late-ssh/src/app/rooms/CONTEXT.md` (game rooms)
- `/late-ssh/src/app/arcade/CONTEXT.md` (solo games)

---

## 1. Decision

**Chat is the spine of late.sh.** The first thing a user sees should expose the room ecosystem, not hide it behind a screen switch.

**Merge Dashboard (screen `1`) and Chat (screen `2`) into a single shell.** Both keys still exist, both still mean something, but they become content modes inside one persistent layout rather than two separate screens.

**Do not turn everything into a room.** Tempting but wrong. Rooms are presence-shared spaces. Solo arcade, settings, music player, bonsai, vote widget, etc. are not rooms. Game rooms, DMs, chat channels, feeds, and the artboard already are or can be modeled as rooms — that's enough.

**No DB schema changes for v1.** `chat_rooms.kind` stays as it is. Home is a *view* in the shell, not a row. Migrating Home into the rooms table is a v2 question.

---

## 2. The Spine

Every surface in the merged shell follows the same skeleton:

```
[ rooms list ][ primary content ][ ambient sidebar ]
```

- **Rooms list (left):** persistent navigation. Same component, same data, no matter which content mode is active.
- **Primary content (center):** varies by what is selected.
  - Home view → curated lounge widgets + dashboard chat composer.
  - Selected chat room → message stream + composer.
  - Selected feed (News / Mentions / Showcase / Work / RSS) → feed content + chat below where applicable.
  - Game/Artboard rooms continue to live on their own screens for now (see §8).
- **Ambient sidebar (right):** now playing, activity, bonsai. Same as today. Yieldable, not load-bearing.

Chat (when present) is always at the bottom of the center pane. Whatever sits above varies. This is the pattern the rest of the app should reinforce over time.

---

## 3. Layout & Breakpoints

The leftbar is 26 cols (same as today's chat room list). The rightbar is 24 cols (same as today). With the app frame border consuming 2 cols, available widths:

| Terminal width | Layout |
|---|---|
| ≥ 104 cols | leftbar + center + rightbar |
| 80–103 cols | leftbar + center (rightbar hidden) |
| < 80 cols | center only (leftbar and rightbar are toggled in as temporary panes) |

Rationale: at 80 cols, three columns leaves ~28 cols for the center pane, which is unusable for chat. The rightbar is ambient and the least costly to hide; the leftbar carries discoverability and stays as long as possible.

Toggle keys (proposed; finalize during implementation):
- Leftbar visibility: persistent setting, toggle via a quiet key (likely `Ctrl+B`-style) and the existing settings modal.
- Rightbar visibility: persistent setting (already exists in settings modal as "Right sidebar"). Honor that; auto-hide at < 104 cols.
- Narrow-mode pane cycling: `Tab` or a dedicated chord. Don't reuse `Tab` if it conflicts with screen cycling — pick one.

---

## 4. Sidebar Grouping

The leftbar is the discoverability story. Section budgets matter: terminals are 30–50 rows tall, and a sidebar that scrolls at rest is a sidebar that hides things.

Default resting state (collapsed where noted):

```
home
  > home                       (1 row, always visible, no header)

channels
  # general              •8
  # rust
  # books              (44)
  # coffee
  # programming
  # security
  …N more                      (expand to show all)

feeds
  news                   •2
  mentions               •1
  showcase
  work
  my rss feeds                 (hidden if no subscriptions)

games
  blackjack-1           2/4
  poker-night           4/6
  ttt-friendly          1/2
  …N more                      (expand to show all)

art
  …N total                     (collapsed by default)

dms
  @ kirii.md             •1
  @ mevanlc
  …N more

  + browse public
```

Rules:
- Sections are quiet lowercase labels with a blank line above. No `▼`/`▶` chevrons; clicking/`Space` on a header toggles collapse.
- Channels: show 8 by default, sorted by current chat order (permanent first, then alphabetical). Rest behind "…N more".
- Games: show top 3 by seat occupancy. Rest behind "…N more". This is the change vs. today, where game rooms are excluded from the main chat list entirely (`is_chat_list_room` in `chat/state.rs`). Surfacing them is intentional — discoverability is the whole point of the merge.
- Feeds: fixed small count, always visible.
- DMs: 5 most recent visible, rest behind "…N more".
- Art: collapsed by default (low frequency).
- Unread shown as a dim trailing number/dot, right-aligned. No bright colours.
- Channel total count `(44)` only shown when high; suppress on quiet rooms.

Target resting sidebar height: ≤ 28 rows so it fits comfortably alongside the center pane in any terminal ≥ 30 rows tall.

---

## 5. Home View

Replaces today's Dashboard screen. Renders in the center pane when Home is selected.

Content (in order, top to bottom):
1. **Vote panel** — current music vote, same data as today's `L`/`C`/`A`/`Z` flow.
2. **Featured room card** — the most-occupied multiplayer room, same as today's dashboard box 1.
3. **Daily game card** — current unfinished daily game, same as today's dashboard box 2.
4. **On-the-wire strip** — rotating top 3 news headlines, same as today's dashboard box 3.
5. **Dashboard chat** — `#general` activity below, composer at the bottom. Same as today's dashboard chat behavior (favorites resolution via `App::dashboard_active_room_id`).

Layout: at wide widths, cards 1–4 arrange as a 2x2 grid; at normal widths, drop the wire card and stack the rest. Whitespace, not borders, separates them.

Composer on Home posts to the resolved dashboard room (today: `#general` or favorite). This is deliberately the same as today's dashboard chat composer.

---

## 6. Vibe Direction (cozy / chill)

Discord, Slack, and Linear all got cozy by hiding affordances until asked. Same rule here.

Strip from default rendering:
- Inline keybinding hints inside widgets (`b1 to join`, `L / C / A / Z vote`, `w care`, `- / = vol  m mute`). Move to onboarding + `?` modal.
- Loud section chevrons (`▼`/`▶`). Use lowercase dim labels and whitespace.
- Heavy box borders around Home widgets. Use a faint title line and blank-line separators.
- Coloured unread dots. Use dim trailing numbers.

Add for discoverability:
- **First-run tour** (3 short slides) that runs the first 1–3 sessions, then disables itself. Captures: how to switch rooms, how to chat, where music/bonsai are, what `?` does.
- **Dim status line** at the bottom of the shell that cycles 1–2 context-sensitive keys when the user has been idle. Disappears on first keypress.
- The existing `?` help modal stays the canonical reference. Make sure it covers everything the inline hints used to cover.

Settings should let users re-enable hint mode if they want (accessibility / new-user toggle).

---

## 7. Keybindings

| Key | Action |
|---|---|
| `1` | Home view (lounge widgets + dashboard chat) |
| `2` | Last selected chat room (focused chat view, no widgets) |
| `3` | Arcade (unchanged) |
| `4` | Rooms / game-room directory (unchanged) |
| `5` | Artboard (unchanged) |
| `Tab` | Cycle screens (currently global; revisit if it conflicts with narrow-mode pane cycling) |
| Sidebar toggle | Likely a dim key (e.g. `Ctrl+B`); also reachable from settings |
| Rightbar toggle | Existing setting (auto-hides below 104 cols regardless) |
| `?` | Help (unchanged) |
| `Ctrl+O` | Settings (unchanged) |
| `w` | Bonsai modal (unchanged; this is the bonsai escape hatch when rightbar is hidden) |
| Music keys (`+`/`-`/`m`) | Global, unchanged |

Selecting a room from the sidebar while on Home switches the center to focused-chat for that room. Pressing `1` returns to Home. Pressing `2` returns to the last focused room. The two keys are *content modes*, not screens.

Long-term: if users only press `1` once per session and live on `2`, Home dies in v2 and `1` becomes an alias for "scroll the sidebar to the top." Decide after a month of real use.

When implementing, update everything on the keybinding-change checklist in `/CONTEXT.md` §11.

---

## 8. What Is NOT Changing in v1

- **Arcade screen (`3`)** — solo games stay as their own screen. Not rooms.
- **Rooms screen (`4`)** — multiplayer game tables keep their dedicated screen for now. Game rooms also appear in the Home/Chat shell's leftbar under `games` (this is the change), but the Rooms screen itself stays. Long-term, this screen may be redundant; revisit in v2.
- **Artboard screen (`5`)** — stays as is. May fold into rooms in v2; not now.
- **`chat_rooms` schema, kinds, visibility, membership, DM canonicalization** — untouched.
- **Notification model, mention resolution, ignore semantics** — untouched.
- **ChatService channels, refresh cadence, history limits** — untouched.

The merge is a layout and navigation change, not a data model change.

---

## 9. Ship Plan (v1)

Order is important. Each step should be independently shippable.

1. **Layout shell.** Introduce a unified `HomeChat` shell that can render either the Home view (today's dashboard widgets + dashboard chat) or a focused chat room in its center pane. Behind a feature flag (`LATE_UI_NEW_SHELL=1` or similar) so both shells coexist while in development.
2. **Sidebar reuse.** Render the existing chat room list inside the new shell. Add the new section grouping (home / channels / feeds / games / art / dms) and section-budget logic with "…N more" expansion.
3. **Game rooms in sidebar.** Flip `is_chat_list_room` (or its equivalent) so game rooms surface under the `games` section in the new shell only. Keep the old chat screen's list unchanged so existing users aren't disturbed until they opt in.
4. **Responsive breakpoints.** Implement the 104 / 80 / <80 col rules. Auto-hide rightbar < 104. Add narrow-mode pane cycle.
5. **Vibe pass.** Strip inline hints, soften section headers, remove heavy widget borders, drop coloured unread dots. Status line at the bottom for cycling tips.
6. **Onboarding tour.** 3 slides, gated on first N sessions, dismissible. Lives in its own module.
7. **`1`/`2` rebinding.** Wire `1` to Home, `2` to last focused room within the new shell. Both still cycle with `Tab`.
8. **Flag flip.** When stable, make the new shell the default. Keep the old shell behind a setting for a release or two, then delete.

Touched modules (preliminary, verify when implementing):
- `late-ssh/src/app/render.rs` (layout entry points, breakpoint logic)
- `late-ssh/src/app/dashboard/` (today's dashboard rendering — likely largely reused inside Home view)
- `late-ssh/src/app/chat/ui.rs` (room list renderer reused; sectioning logic)
- `late-ssh/src/app/chat/state.rs` (section budgets, expand/collapse state)
- `late-ssh/src/app/chat/input.rs` (selection routing in new shell)
- `late-ssh/src/app/input.rs` (global key dispatch for `1`/`2`)
- `late-ssh/src/app/help_modal/data.rs` (every keybinding update)
- A new `late-ssh/src/app/onboarding_tour/` module for the first-run tour.

Tests:
- New unit tests for section budgeting, expand/collapse state, breakpoint resolution.
- Update existing `chat/ui.rs` unit tests for the sectioned room list.
- Integration test for game-room visibility in the new shell vs. old shell.

---

## 10. Open Questions

Resolve before locking the v1 spec:

1. **Section budget exact numbers.** Proposed 8 channels / 3 games / 5 DMs. Validate against real user data (largest joined channel count, busiest game-room hour, most-DM'd users). Adjust before shipping.
2. **Sidebar visibility on Arcade / Rooms / Artboard screens.** Recommendation: yes, the leftbar lives on every screen, because chat is the spine. Confirm with mat.
3. **Narrow-mode pane cycle key.** `Tab` clashes with global screen cycling. Candidates: `Ctrl+Tab`, dedicated `Ctrl+B` chord, or a "press `Esc` to cycle when no modal owns input." Pick during implementation.
4. **Home view at narrow widths.** Wire card dropped — confirmed. What about vote panel? It's load-bearing for music engagement; might need to stay even at narrow widths.
5. **What `4` (Rooms) does long-term.** If game rooms surface in the leftbar, screen `4` becomes a directory view of the same data. Consider whether `4` should also fold into the shell as a Discover-style filtered room view.
6. **Onboarding tour content.** Three slides — but which three? Probably: (a) sidebar navigation, (b) `?` for help and `Ctrl+O` for settings, (c) music vote + bonsai keys. Write the copy before building.

---

## 11. v2 Decision Points

After v1 has been in use for ~1 month:

- Does Home survive as a distinct mode, or does pressing `1` just mean "scroll sidebar to top + select home channel"? If usage shows `1` is pressed once per session and never again, kill the mode.
- Does the Rooms screen (`4`) survive, or does it become "Discover for game rooms" inside the shell?
- Does the Artboard screen (`5`) fold into rooms? Multi-user artboards are presence-shared spaces; they fit the room model. Single global artboard does not.
- Should Home become a real `chat_rooms.kind = 'lobby'` row, with a real composer that posts to a real lobby room? Or stay a view forever?

None of these are urgent. v1 should be designed so any of them can ship later without re-architecting.

---

## 12. Non-Goals

Explicitly not part of this redesign:

- Solving #general's noise problem. That's a moderation / topic-split problem, not a layout problem. Mentioned in earlier discussions; punted.
- Rewriting the music streaming surface.
- Rewriting Arcade.
- Rewriting Artboard.
- Adding new chat features (threads, search, etc.).
- Touching the synthetic-entry data model (News, Showcase, Work, Feeds, Mentions stay as they are).
- Replacing the right sidebar's content. Same widgets, just yieldable.
