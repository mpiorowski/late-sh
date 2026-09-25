# usermap — `/map`

## Metadata
- Owner surface: a modal, opened only by typing `/map` (no chord).
- Depends on: `common::worldmap` (the atlas and the painter),
  `profile::svc::ProfileService` (who is online, and where they said they are).

## 1. What it is

`/active` answers "who is online" as a list of names. This answers "where are
they", shading the Earth by how many people are in each country — in three
slices, walked with Tab:

| mode | what it counts |
| --- | --- |
| `Online` | connected right now (the default) |
| `Recent` | seen within `RECENT_DAYS` (30) — the living population, without the accounts that signed up once and never came back |
| `Everyone` | every account that ever set a country |


`ProfileService::country_counts(mode)` is the single entry point. `Online` is
the odd one out and the only one that cannot be a query: who is connected
lives in process memory (`ActiveUsers`, humans only — no fingerprint means a
bot) and where they say they are lives in the database, so it intersects the
two. `Recent` and `Everyone` are both `User::country_counts_since` with and
without a cutoff, counted in SQL (`last_seen` is what makes "this month"
mean anything).

Each mode caches its last result, so Tab is instant after the first look, and
a load carries its mode there and back — switching tabs while a slow query is
in flight must not paint one slice's numbers under another slice's title.

**Anybody who has not set a country is absent.** Not "unknown", not guessed
from a timezone — absent. The map is of people who said where they are, and
the empty state says so rather than showing a blank world.

## 2. The map is not ours

The atlas, the viewport and the painter are `common::worldmap`, shared with
realm's board. This module contributes exactly one thing the painter does not
know: a colour per territory. Everything else — half-blocks, mip levels,
screen-space borders, pan/zoom/clamp — is the shared code.

Country codes need no translation: the profile picker
(`settings_modal::data::COUNTRIES`) and the Earth asset both use ISO-3166
alpha-2, which is pinned by `profile_country_codes_find_their_countries_on_the_map`.
A handful of picker codes have no shape on a 242-territory map (Natural Earth
folds small dependencies into their parent); those countries still appear in
the list, marked "not on the map", so the numbers beside the map add up to the
numbers on it.

## 3. The shading

`HEAT` is five colours; the **floors are computed from the data**
(`heat_floors`), not fixed. A 1/2/4/8/16 ladder is right for a server with
twenty people and useless at either end: with four users everyone sits on the
bottom rung, with four thousand everyone sits on the top one. The floors are
drawn from the busiest country on the 1-2-5 ladder axis labels use, keeping
the busy end (where the differences are worth seeing) and always keeping `1`
(one person is its own thing). Fewer than five rungs when the range is small —
a legend reading "1, 2, 3" beats one reading "1, 1, 1, 2, 3".

Labels are the floor with a `+` on the top rung, not ranges: the legend sits
in a 30-column list and "1 50 100 200 500+" fits where "1-49 50-99 …" is
clipped at the right edge — losing the top rung, which is the one somebody is
looking for. It wraps by measured width rather than by hope.

The ladder is warm (the sea is dark blue, empty land olive) and climbs in
lightness, so "more people" reads without the legend.
`the_shading_ladder_is_readable` measures it in CIELAB: every rung at least
ΔE 8 from its neighbour and ΔE 15 from both water and empty land.

## 4. Keys and mouse

`tab` walks the three slices. All four arrows pan; `j`/`k` walk the country list, and picking a country
frames it on the map (finding Luxembourg by hand on a world view is not a
thing to ask of anyone). `+`/`-` zoom, `f` refits, `r` re-counts, `esc`/`q`
closes.

Mouse: wheel zooms over the map and scrolls over the list, left-drag pans
(the map follows the hand), a press that does not move picks the country
under it, and a click in the list picks that row. The draw records where the
map and the list ended up (`record_geometry`) because nothing else knows.

Two things that were wrong on the first cut, both worth not repeating:

- **Esc has to be handled in `input::dispatch_escape`, not only in the
  modal's own key match.** A lone Esc never reaches the modal stack — it is
  held as `pending_escape` and flushed on a later tick — so a modal that only
  answers `Byte(0x1B)` where its other keys live cannot be closed with Esc.
- **Arrows pan; do not give two of them to the list.** Up/down were bound to
  the country list, which reads as half the arrow keys being broken. And at
  most zooms the whole map *height* already fits (the world is 2:1, a
  terminal usually is not), so vertical panning is legitimately a no-op —
  `Viewport::clamped` now centres an axis with no room instead of pinning it
  to the top, and the footer says "the whole world fits · + zoom in" rather
  than advertising keys that cannot do anything.

## 5. Tests

- `late-core`'s `country_counts_can_ignore_the_long_gone`: the cutoff drops
  the dormant, keeps the present, and a blank country string never becomes a
  country.
- `state_test`: the ladder's readability, how the floors stretch to the data
  (from one person to a hundred thousand), the labels, mode cycling, the
  code→territory lookup (including lowercase and unknown codes), and that the
  picker and the map have not drifted apart.
- `ui_test`: draws at several sizes — while still counting, with tallies (the
  rung colours have to actually reach the screen), and with nothing to show.
