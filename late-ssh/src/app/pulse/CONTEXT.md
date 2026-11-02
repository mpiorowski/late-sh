# Pulse

Home's `pulse` tile: the people online right now over the last day of them
as a sparkline, the day's peak named, a time axis under it. One question,
"how is the house tonight", answered at a glance.

## Where it lives

| File | Role |
|---|---|
| `svc.rs` | `PulseService`: this replica's sampler. Once a bucket (five minutes) it writes the replica's human headcount (`crate::state::online_human_count`) to `presence_samples` and reads the last day back onto a `watch` (`History`, an `Arc` of `HeadcountPoint`s) every session copies on its tick. Logs and `metrics::record_pulse_sample` live here. |
| `state.rs` | Pure maths: `columns` cuts the day into the tile's width (the highest sample a column, `None` a gap), `peak`, `glyphs` (a column's block steps over the tile's rows, one person never less than one step), `height` for the colour. |
| `ui.rs` | `draw`: the header (`online 12` left, `peak 31 · 21:05` right, `sampling` until there is a day), the sparkline (amber climbing with height, the last column bright as now, a gap a faint dot), the axis (`24h`, `12h`, `now`). One row is the sparkline alone, two add the header, three the axis. |

The table is `late-core/src/models/presence_sample.rs` (`PresenceSample`,
migration 277): one row per replica per bucket, summed per bucket on read,
pruned past two days on write. Replicas sum because the roster
(`active_users`) is in-process, so each pod sees its own sessions.

## Invariants

- The session never queries: `App::pulse_history` is the watch's last
  value, copied on the ~1s tick like the runner looks, and the tile reads
  that and `online_count`.
- The last column is live: the current headcount, which may sit above the
  bucket's sample, so the sparkline never lags the header.
- A missing bucket is a gap (`·`), never a zero: a pod that was down did
  not have nobody online.
- The service starts before any session (`main.rs`), so the first sample
  is at most a minute after boot.

## Tests

`state_test.rs` covers the columns (gaps, the highest sample, the day's
edge), the peak tie, the eighths climb, and the height. The model's DB test
(`presence_sample_test.rs`) covers the bucket floor, replicas summing, a
rewrite replacing, and the day window.
