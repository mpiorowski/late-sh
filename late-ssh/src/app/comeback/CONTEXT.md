# Comeback moments

What the first return days hand a newcomer. The tour is the first session
(`../clubhouse/CONTEXT.md` §5); a comeback moment is one box on the first
login of a later UTC day, in series, one moment per day, three in all, for
an account in its first two weeks. Each sells one thing the tour never reaches, and is
shown only when it has something to sell this session. None is a stop: the
session lands where it would have, the box sits over it, Enter does the
moment's own thing and Esc keeps it for later. Keyed to days, not to
`users.login_count`: a dropped connection is not a return. The numbers
behind it and what each moment is measured by are in `AUDIENCE.md`
(section 5, the ladder). Weighed and not picked as moments: "the star"
(the rung is already the tour's send-off and the keypress-return push,
a third telling is nagging), "leave it open" (the regulars' behavior,
and it needs the CLI first, so it is one line inside the first box),
"say something in #lounge" (where the tour already lands, and chat on
day one predicts return at half a board's rate).

## Files

| File | Role |
|---|---|
| `state.rs` | `Moment` (the series, in order), `Standing` (what the session already has of what the moments sell), `due_moment` (the gate: which moment a login is owed), `parse_command` (`/comeback N`), `State` (how the session arrived, and the box's open state). |
| `input.rs` | Enter takes, Esc keeps for later, every other key holds. |
| `ui.rs` | The box, in the tour's frame and voice, by moment and by whether the session can hear the radio. |
| `svc.rs` | `tick`: reads the gate once the splash is down and the tour settled and opens the owed moment; `show`: opens one and, on a login, stamps the account, disarms the paper's pop and counts. |

## The series

1. **Hear it** (`/comeback 1`). The house has a radio and a YouTube
   booth, every enabled station by name under the count (from
   `late_core::radio`, never a copy), and over plain ssh you hear none
   of it: the `late` CLI's install one-liner (the same constant the Pair
   guide prints, `help_modal/data.rs`), then the listen page. Enter opens
   the guide on its Pair topic, which has the QR. A session with a client
   paired on its token hears it already, so the moment has nothing to
   sell it and is passed over at login; `/comeback 1` on such a session
   shows the paired box, which names the music keys, and Enter opens the
   Stations modal, what `v` then `r` does.
2. **The big games** (`/comeback 2`). Page `3`, the Games hub: its roster
   by group from `door::hub::state::HubGame::roster(false)` (the runners'
   own card left off, so a door added or dropped shows up here without a
   copy), the Lobby on `Ctrl+G`, the hub's live rail, and two dim lines
   that something is moving in the static for those who stick around:
   deadchannel is never named, its door is the haunting
   (`../deadchannel/haunt/CONTEXT.md`), and the line is the only foreshadowing
   that ever reaches a session the static has not picked. Enter opens the
   Games page, what the `3` key does. To every newcomer once.
3. **The people** (`/comeback 3`). Page `5`, People: its three shelves
   by name from `directory::state::Shelf::ALL` (the map and its `m`, the
   work cards and `w`, the job shelf and `/`), the web profile page, and
   `p` on a name in chat. Enter lands on People on its Map shelf, what
   `/map` does. To every newcomer once.

## The gate

`due_moment(mark, account_created, today, standing)`, read once per
session in `svc::tick` and not at `App::new`, because a client pairs after
the session stands:

- Never on a session with the tour still to walk, and never on a drain
  handoff, which is not a login (`../handoff/CONTEXT.md`); `App::new`
  says which as `Arrival`.
- The first moment on the first login of a UTC day after the account's
  creation day (the tour's day stays quiet); the next in the series on
  the first login of a day after the last one was shown. Nothing twice in
  a day: the account is stamped the moment the box opens
  (`users.settings.comeback_step` and `comeback_day`,
  `late_core::models::user::ComebackMark`, written fire-and-forget
  through `ProfileService::set_comeback_mark`), so a dropped connection
  never replays it. Nothing for an account past `SERIES_DAYS` (14) from
  its creation day: a regular is never handed a newcomer's box, the
  moments are for people still finding the house.
- From where the series stands, the first moment with something to say
  for this session (`Standing`: whether a client is paired) is the one
  owed; the ones with nothing to say are passed over, not spent. Nothing
  past the series, nothing on a stamp with a step the series no longer
  has.
- A login's box disarms the paper's login pop (`../paper/CONTEXT.md`):
  one box after the splash, never two.

## Keys and reruns

The box holds input ahead of the reserved global chords, under the quit
confirm and the tour gate (`app/input.rs`), so a stray key never takes
the pitch off the screen. `/comeback N` opens moment N's box from any
composer without the stamp or the count, a rerun like `/onboard`
(`../chat/COMMANDS.md`); a bad or missing number banners the usage.

## Telemetry

`late_ssh_comeback_moments_total{moment,beat}`: `moment` is `hear_it`,
`big_games` or `people`; `beat` is `shown`, `taken` (Enter) or `skipped`
(Esc), on a login's moment only. `taken` against `shown` is whether the
pitch lands; `skipped` against `shown` on `hear_it` says how many
plain-ssh newcomers want nothing to do with the CLI.

## Tests

`state_test.rs` (the gate's days, the pass-over, the command's parse, the
steps), `input_test.rs` (the keys), `svc_test.rs` (through a full `App`
on a real DB: a return day's login opens the box after the splash and
stamps the account, a stray key holds it, Esc keeps it; Enter on plain
ssh opens the Pair guide; a paired session reads the keys and Enter opens
the Stations modal; the games box names every hub card and Enter opens
the hub; the people box names the three shelves and Enter lands on the
map; `/comeback 2` reruns and a bad number banners),
`late-core/src/models/user_test.rs` (the mark's stamp and read).
