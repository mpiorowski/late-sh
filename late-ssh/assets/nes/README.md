# NES ROM Assets

Bundled ROMs are explicitly licensed homebrew/demo ROMs used by the Arcade NES
Cabinet. Do not add commercial console ROMs or "free download" ROMs without a
clear redistribution license.

| File | Game | License evidence | Source |
| --- | --- | --- | --- |
| `squirrel_domino.nes` | Squirrel Domino | zlib source license; keep unmodified and preserve attribution | https://novasquirrel.itch.io/squirrel-domino |
| `thwaite128.nes` | Thwaite | GPL-3.0-or-later | https://github.com/pinobatch/thwaite-nes |
| `dabg.nes` | DABG: Double Action Blaster Guys | GPL-3.0-or-later; source included in `DABG.zip` | http://novasquirrel.com/dl/DABG.zip |
| `falling.nes` | Falling | MIT | https://github.com/xram64/falling-nes |
| `brickbreaker.nes` | Brick Breaker | MIT | https://github.com/AleffCorrea/BrickBreaker |
| `escape_from_pong.nes` | Escape from Pong | BSD-style binary redistribution license in revision 6 archive | https://hcs64.com/files/Escape_from_Pong_r6.zip |
| `rhde.nes` | RHDE: Furniture Fight | GNU all-permissive style notice | https://github.com/pinobatch/rhde-nes |
| `concentration_room.nes` | Concentration Room | GPL-3.0-or-later with exact-ROM redistribution exception | https://github.com/pinobatch/croom-nes |
| `zap_ruder.nes` | Zap Ruder | zlib-style redistribution notice | https://github.com/pinobatch/zap-ruder |
| `2048.nes` | 2048 | redistribution notice in `readme.2048.txt` | https://bitbucket.org/tsone/neskit/ |

Notes:
- GPL ROMs remain third-party programs with their own licenses; do not treat
  them as covered by the repository FSL. Preserve source links when packaging.
- `squirrel_domino.nes` is included as an unmodified ROM. Its source README asks
  not to reuse background graphics in other projects.
- `escape_from_pong.nes` is the `efpbw.nes` reversed-control ROM from the
  revision 6 archive; the original-control `efp.nes` is not bundled.
