# Green Dragon door (`late-ssh/src/app/door/greendragon`)

A native, in-process door game: an open-source remake of LORD, modeled on
*Legend of the Green Dragon* (LoGD). Single-player, turn-based, DB-persisted
(one character per user). It uses the **Lateania integration pattern** (native
ratatui + a service + a `DoorGame` impl), not the nethack/rebels PTY-proxy
pattern, because LoGD is a web app with no terminal to proxy — only its balance
data and mechanics are reused.

## Module map (flat)

| File | Owns |
|---|---|
| `data.rs` | Canonical LoGD balance tables, transcribed from the DragonPrime seed (`jimlunsford/lotgd`): the weapon/armor cost ladder, per-level creature stat blocks (1–16), the exp curve with dragonkill scaling, the 14 named masters, the dragon's `45/25/300`. Pure constants + lookups. |
| `combat.rs` | The pure round resolver mirroring LoGD `rolldamage`: triangular `bell_rand`, 5% triple-crit, glancing hits floored to zero, reroll-until-progress. Takes `&mut impl Rng` so it's seed-testable. Plus `simulate_fight` for balance checks. |
| `model.rs` | The persistent `Character` and all rules on it: stat derivation (`max_hp = 10*level`, `attack = level + weapon_tier`, `defense = level + armor_tier`), leveling, shop pricing with 75% trade-in, healer cost (`round(ln(level)*(missing+10))`), banking, forest death (gold→0, exp×0.9), new-day reset, dragon-kill run reset. Serde-able with field defaults. |
| `persist.rs` | JSON save envelope (`schema_version` + `character`), tolerant of missing fields. |
| `svc.rs` | `GreenDragonService` (cheap `Clone`, `Arc`-backed): async character load via a `watch` channel, fire-and-forget save/delete over `greendragon_characters`. Holds `ActivityPublisher`/`ChipService` for the not-yet-wired dragon-kill reward. |
| `state.rs` | Per-session `State`: owns the authoritative `Character` (single-player, no shared world), a `Mode` machine (Village/Forest/Fight/shops/Healer/Bank/Training/Graveyard), the active `Encounter`, a capped message log, and every player action as a method. Drains the load channel in `tick()`. Pure menu builders are unit-tested. |
| `ui.rs` | Rendering only: the live page (stat rail + mode panel + event log) and the two-column Games-hub landing card. |
| `screen.rs` | The `DoorGame` impl (`GAME`), launcher/active key+arrow handling, and `leave` (save + return to the Games hub). |

## Persistence

`greendragon_characters` (migration `092`, model `late-core/src/models/greendragon_character.rs`) is one JSONB blob per user, exactly like `mud_characters` — the character shape evolves without new migrations. The service computes a UTC day-number to drive the once-per-day forest-turn/heal reset on load.

## Integration points (mirror Lateania)

`Screen::GreenDragon`, `HubGame::GreenDragon`, `DoorGameId::GreenDragon`,
`App::{greendragon_service, greendragon_state, enter_greendragon,
leave_greendragon}`, `SessionConfig`/server-`State` service injection
(main/ssh/session_bootstrap/test-helpers), render draw arm, input dispatch +
Esc, and the hub launch/landing. Leaving is centralized: Esc forwards to the
game so it backs out one menu level and only leaves to the hub from the village.

## Known gaps / deferred (v1)

- **No dashboard activity feed** (`activity_game()` returns `None`) and **no chip/profile-award** for slaying the dragon — the service holds the deps but the reward path isn't wired (would need a `reward_templates` seed migration like Lateania's `086`).
- **Bank interest** (`Character::apply_bank_interest`) exists but isn't applied on new day.
- Forest has the three LoGD intensities; **PvP, Stables, Gypsy (DK upgrades), Gardens** are not implemented.
- Combat omits LoGD buff `dmgmod`s; the round resolver is otherwise faithful.
