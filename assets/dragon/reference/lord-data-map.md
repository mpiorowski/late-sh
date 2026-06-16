# Classic LORD Reference Map

Purpose: identify the important data shapes worth learning from before writing
original Dragon content.

Do not treat this as a content import plan. The new game should use original
names, prose, NPCs, item names, monster names, jokes, and event text.

## What We Can See

From the local official 4.07 package, useful reference material is spread across
plain docs, binary data files, Lady script/data files, and compiled executable
strings.

High-confidence data tables:

| Classic file | What it contains | Dragon takeaway |
| --- | --- | --- |
| `LENEMY.DAT` | Forest monster records | Build monsters in level buckets with stat/reward spread. |
| `WEAPONS.DAT` | Weapon shop records | Separate item display name from stat tier. |
| `ARMOR.DAT` | Armor shop records | Same as weapons, but defense gated. |
| `PLAYER.DAT` | Persistent player records | Keep public identity, daily counters, combat stats, equipment, romance flags, and long-term wins. |
| `NODE*.DAT` | Per-node runtime config | Not needed for native Dragon. |
| `3RDPARTY.DAT` | Add-on menu entries | Later plugin/event hooks can be native, not DOS-style. |

Important text/event files:

| Classic file | What it suggests | Dragon takeaway |
| --- | --- | --- |
| `EVENTS.DAT`, `EVENTS.LDY` | Forest and scripted event surfaces | Build a data-driven event catalog with explicit triggers and outcomes. |
| `LORDTXT.DAT`, `LGAMETXT.DAT` | Core display/game text | Keep content modular and editable. |
| `LORDRIP.DAT` | Alternate RIP/client presentation text | Native Dragon can ignore RIP but should separate content from renderer. |
| `BARDSONG.LDY`, `INN.LDY`, `BANK.LDY`, `LORD.LDY` | Area-specific scripted behavior | Each town location should own a small event/action table. |
| `BADSAY.DAT`, `GOODSAY.DAT`, `NORMSAY.DAT` | NPC speech pools | Dragon needs reusable voice pools for rumors, insults, boasts, and flavor. |

## Monster And Item Shapes

The bundled Pascal structure notes describe forest monsters as:

```text
name
strength
gold reward
weapon text
experience reward
hit points
death text
```

The same notes say monsters are grouped by level, with eleven monsters per
level in file order. The local 4.07 `LENEMY.DAT` has 131 decoded monster-sized
records, so verify bucket handling before copying the exact count into Dragon.

Weapon and armor records are conceptually:

```text
display name
price
stat requirement
```

Player records separate display equipment names from numeric equipment tiers.
That matters: it is the mechanic behind renaming a strong weapon to look weak.
Dragon should keep that separation:

```text
weapon_tier
weapon_display_name
armor_tier
armor_display_name
```

## Event Access

We do not have every event as clean data.

Accessible reference:

- many visible strings in data files and executable strings;
- Lady script/data files for some areas;
- player, monster, weapon, armor, and IGM structure notes;
- observed runtime screens and daily news output.

Not fully accessible without deeper reverse engineering:

- exact forest-event probabilities;
- exact trigger conditions;
- exact combat formulas;
- exact branching for hidden and stateful events;
- registration/version gates;
- compiled interactions inside `LORD.EXE`.

Dragon should therefore use the classic game as a reference taxonomy, then write
an original event system with explicit conditions, weights, outcomes, and news
templates.

## Reference Categories To Preserve

- Daily action counters.
- Forest monster buckets by level.
- Forest random events.
- Town locations.
- Shops and upgrade gates.
- Healer and recovery costs.
- Bank deposits, interest, and risk of carrying gold.
- Trainer/master fights for level advancement.
- PvP attempts and offline attacks.
- Inn room/rest state.
- Public rankings.
- Daily happenings/news.
- Mail, flirting, proposals, marriage, kids, insults, and gossip.
- Player-authored boasts/last words.
- NPC speech pools.
- Hidden locations and rare events.
- Skill paths with daily uses.
- Long-term dragon wins and partial reset after victory.
- Add-on/plugin-like extensibility, but native and controlled.

