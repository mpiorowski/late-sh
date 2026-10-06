# Invites (referrals)

"Bring a friend": every account has a stable invite code, a new account can
name the account that invited it, and once the invitee has become an active
regular the sweeper pays both of them. The payout is announced in #lounge.

## Files

- `state.rs`: the rules, pure. SSH login-name and typed-code parsing, the
  attach checks (`attach_check`, `AttachRefusal`), the bar (`qualifies`,
  `verdict`), the payout numbers, and `NewcomerClock`.
- `svc.rs`: `ReferralService`, the one orchestration layer. Attach, the
  Invites dialog load, newcomer-minute writes, the sweeper (judge, expire,
  pay), every log line and metric, the #lounge publish.
- `late-core/src/models/referral.rs`: the SQL for `invite_codes`,
  `referrals`, and `newcomer_activity_days` (migration 218).
- UI: Settings (`Ctrl+O`) > Account > Invites, in `app/settings_modal`
  (`InvitesDialogState`, `handle_invites_dialog_input`, `draw_invites_dialog`).

## How an invitee attaches

- **SSH link:** `ssh invite-<code>@late.sh` (or `late --ssh-user invite-<code>`).
  `auth_publickey` reads the login name only on the connect that created the
  account. Plain `ssh late.sh` sends the local `$USER`, which is why the code
  rides a fixed `invite-` prefix and a bare username is never read as one.
- **Settings:** the Invites dialog shows a code field while the account has
  no inviter and is at most `ATTACH_WINDOW_DAYS` (7) old. It accepts the bare
  code, `invite-<code>`, or the whole pasted command.
- Refusals (`AttachRefusal`): unknown code, own code, past the window, an
  inviter not older than the invitee (nobody invites someone who was here
  first, which also rules out two accounts inviting each other), and already
  invited. The first attach is the only one: `referrals` is keyed by
  `invitee_id`.

## The bar

Judged over `[invitee.created, invitee.created + JUDGING_WINDOW_DAYS)` (60
days). Activity is stored per UTC day, so the window's last, partial day
counts: `NewcomerClock` stops reporting at the window's end, which is what
keeps anything later off it. Both must hold:

- at least `MIN_ACTIVE_DAYS` (14) UTC days with at least
  `MIN_DAY_ACTIVE_MINUTES` (20) active minutes each,
- at least `MIN_TOTAL_ACTIVE_MINUTES` (1,200) active minutes in total.

There is deliberately no chat requirement: plenty of regulars come to play
and listen and never talk in a public room, and a chat bar would leave their
inviters unpaid with no way to know why. A pure listener still never
qualifies, because listening sends no input and reads as idle.

**The bar is never shown to players.** The dialog says "once they become an
active regular" and nothing more, and a `qualified` row waiting on the cap
reads "settling in" like a `pending` one. Keep numbers out of UI copy.

An **active minute** is a wall-clock UTC minute in which the session received
any input. Connected-but-idle time never counts, which is the difference from
`user_online_time` (connected wall time, idle included, so an alt left open
in tmux would pass any online-time bar). `NewcomerClock` notes input from
`App::handle_input` and hands the 1Hz tick at most one minute per wall minute;
`NewcomerActivity::record_minute` counts it under a
`last_minute < EXCLUDED.last_minute` guard, so two sessions of one account
typing in the same minute count it once. Every account records minutes for
its first 60 days, whether or not it has an inviter yet, so a code typed on
day six still sees days one through five. Accounts past the window carry an
inert clock and write nothing.

## Life of a referral

`pending` -> `qualified` -> `paid`, or `pending` -> `expired`. Every move is a
guarded `UPDATE ... WHERE status = ...`.

The sweeper (`REFERRAL_SWEEP_INTERVAL`, 10 min) runs on every replica:

1. **Judge** every `pending` row: `Qualified` marks it qualified, a closed
   window short of the bar expires it, anything else waits.
2. **Pay** every `qualified` row whose inviter is under the cap this month
   (`Referral::list_payable`), oldest qualification first. A row waiting on
   the cap is off the queue until the month turns. One transaction:
   lock the referral row (`FOR UPDATE`), take a per-inviter advisory lock,
   count the inviter's `paid` rows this UTC month, and if under
   `MONTHLY_PAID_CAP` (3) mark it paid and credit both sides. Past the cap the
   row stays `qualified` and pays in a later month; it is never lost. The
   inviter lock is what makes the cap exact when two replicas pay two of one
   inviter's invitees at once.

Payout: `INVITER_REWARD_CHIPS` (50,000) as `ChipMove::ReferralReward`
(`source_ref` the invitee id) and `INVITEE_BONUS_CHIPS` (10,000) as
`ChipMove::ReferralWelcome` (`source_ref` the inviter id). Both are minted and
both are `counts_as_earnings = false`, so one invite cannot decide the Top
Chips board. The profile ledger reads them as "invite reward for @x" and
"welcome bonus from @x".

After commit the winning replica publishes `ActivityKind::ReferralRewarded`:
a ticker line ("mat earned 50,000 chips for inviting alice, now a regular")
and a #lounge headline that @mentions both people.

Deleting either account cascades the referral row away. Account linking
deletes the abandoned account, so linking an invitee into its inviter (or the
reverse) removes the referral too.

## Replica notes

Truth is the three tables. The Invites dialog is read on open and after a
successful attach; there is no notify channel and no live refresh, by the
"read on open" rule in the root `CONTEXT.md` §0.

## Telemetry

- `late_ssh_referral_attaches_total{source,outcome}`: every attach, by source
  (`ssh`/`settings`) and outcome (`attached`, each refusal, `failed`).
- `late_ssh_referral_settlements_total{outcome}`: the sweeper's moves
  (`qualified`, `expired`, `paid`, `deferred`, `failed`). `deferred` is a
  payout stopped by the cap after the row was queued (the inviter's earlier
  invite filled it in the same sweep, or another replica did): once per row
  and replica, not once per sweep.
- `late_ssh_newcomer_minutes_total{result}`: reported minutes (`counted`,
  `already_counted`, `failed`).
- Spans: `referral.attach`, `referral.load_overview`, `referral.sweep`; sweep
  failures are `error_span!` `referral_judge_failed` / `referral_pay_failed`.

## Tests

- `state_test.rs`: code parsing, attach checks, the bar's exact thresholds,
  expiry, and the clock driven step by step.
- `svc_test.rs` (DB): attach once plus every refusal, the monthly cap with both
  credits, an idempotent re-pay and the capped row leaving the pay queue,
  recorded minutes carrying an invitee to `qualified`, expiry against an open
  window.
- `late-core/src/models/referral_test.rs` (DB): code minting, once-only
  attach, the per-minute dedupe across sessions, the window's last day.
- `late-ssh/src/ssh_test.rs`: an `invite-<code>` login attaches on the connect
  that creates the account and on no other.
- `late-ssh/src/app/input_flow_test.rs`: the Invites dialog list with a
  username longer than its name column.

## Known gaps

- Any input counts as activity, mouse movement included, so the bar is
  scriptable: a client that sends a byte a minute for 14 days passes it.
  This is accepted on purpose for now. The controls are the inviter-older
  rule, the monthly cap, the 60-day wait, and the public #lounge line,
  which names every paid pair so farming is easy to spot by eye. Watch
  `late_ssh_referral_settlements_total{outcome="paid"}` and the #lounge
  headlines.
- There is no IP or device check between inviter and invitee.
- `MONTHLY_PAID_CAP` bounds one inviter, not one person. Every invitee has
  its own code and its own cap, and the inviter-older rule only needs a few
  seconds between accounts, so a chain of alts (each inviting the next) can
  all be created on one day and all qualify 14 days later. Each alt mints
  60,000 (50,000 to its inviter, 10,000 to itself), with no ceiling on how
  many.
- The minted chips can be moved to one account afterwards. Gifts do it
  directly and a poker table does it by folding every hand to one seat, so a
  cap on gifts alone would not contain a farm.
- The sweeper does not look at bans. A banned inviter or invitee is still
  paid and still named in the #lounge line, and a `qualified` row held back
  by the cap keeps paying in later months. Deleting the account is what
  removes its referral rows (cascade); a ban does not.
- There is no off switch. Payouts start with the process and stop only with
  a deploy.

The accepted control is the owner reading #lounge: every paid pair is named
there, after the chips are minted.

## If cheating shows up

None of this is built. In the order to reach for it:

1. **Delete the farm's accounts.** That cascades their referral rows away,
   pending and qualified alike, so nothing more pays.
2. **An off switch.** A row that `sweep` reads before the pay step (a
   process-wide switch is a row, root CONTEXT.md §0; no code reads a
   switch table today, so this brings one back). Judging keeps
   running, `qualified` rows wait, nothing is lost, and payouts stop without
   a deploy.
3. **A site-wide monthly cap on paid invites**, next to the per-inviter one:
   one more condition in `Referral::list_payable` and one more count in
   `pay`. It bounds the mint itself (cap x 60,000 a month) however many alts
   exist and however the chips are moved afterwards. The catch: the queue is
   oldest-qualified first, so a farm's rows hold real invites back until its
   accounts are deleted.
4. **Engaged days**, below, which makes the bar itself harder to script.

### Engaged days

Recorded here at the owner's request. The planned tightening is a
third requirement, "engaged on 7+ distinct UTC days", where a day counts if
the invitee did something a person does rather than merely sent input:

- spoke in a public room,
- moved chips through play: a house-table bet, a daily puzzle or daily match
  win, a quest reward, a queued song, a bar order (all already in
  `chip_ledger`, so one query by user and UTC day),
- set an Arcade score (Lateris, 2048, Snake, Traffic), which writes no ledger
  row and needs its own reads of the high-score tables.

Leave out the one-tap daily care rewards (bonsai, pet, aquarium): a single
keypress a day is as cheap to script as idle input. A script would then have
to actually play, which is the point.
