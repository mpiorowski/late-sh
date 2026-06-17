# Dragon DAT Tables

These are Dragon's own committed data tables.

They are not extracted LORD files. They are original working tables shaped by
the kind of data the old game needed: monsters, equipment, events, daily news,
social actions, progression, and player fields.

Format:

- files use `.dat` names because the game plan is table-first;
- each non-empty line is one JSON object;
- no comments inside `.dat` files;
- use `id` values as stable references from future game code;
- preserve these as source-of-truth assets until implementation decides where
  runtime content should live.

Files:

- `manifest.dat`: inventory of the table files.
- `player-fields.dat`: persistent player state shape.
- `level-table.dat`: level gates and daily allowances.
- `monsters.dat`: level-bucketed forest enemies.
- `weapons.dat`: weapon tier table.
- `armor.dat`: armor tier table.
- `town-locations.dat`: menu locations and commands.
- `skills.dat`: skill path table.
- `events.dat`: explicit event catalog.
- `daily-news.dat`: public happenings template catalog.
- `social-actions.dat`: player-to-player social action catalog.

Raw reverse-engineering or original package dumps belong in
`assets/dragon/raw/`, which is ignored by git.
