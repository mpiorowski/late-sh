#!/usr/bin/env python3
"""Render the site's link-preview card, late-web/static/og-image.png.

The card is the night-city skyline as an Artboard would hold it, drawn by
the same renderer that makes every gallery and poster PNG
(late-web/src/pages/shared/board_png.rs), so the card and the herald's
daily pictures share one look. The script builds the canvas from the art
below, parks it in the dev database under a scratch key, fetches the PNG
from the running dev web service, removes the row, and scales the result
to the 1200 by 630 card size with ImageMagick.

Needs the dev stack up (`make start`) and `magick` on the path.

    python3 scripts/gen_og_image.py
"""

import json
import random
import subprocess
import sys
import tempfile
from pathlib import Path

OUT = Path("late-web/static/og-image.png")
KEY = "scratch:og-image"
WEB = "http://localhost:3000"
# 84 columns render at 18 pixels a cell (board_png aims for 1536 wide), so
# 22 rows give 1512 by 792, the card's 1.91:1 within a hair.
COLS, ROWS = 84, 22
CARD = "1200x630!"

ART = r"""
                                                                  .      *
        *                  .                                 .
                 .                   *                  .          *
                               .                                       *
      .                                      .                              .

                      ___  ___  ___                           .
                     |:::||:::||:::|      __
         .      _____|:::||:::||:::|_____|::|____          ____
               |:::::|:::||:::||:::|:::::|::|::::|   .    |::::|    _
          _____|:::::|:::||:::||:::|:::::|::|::::|________|::::|___|:|__
         |:::::|:::::|:::||:::||:::|:::::|::|::::|::::::::|::::|:::|:|::|
    _____|:::::|:::::|:::||:::||:::|:::::|::|::::|::::::::|::::|:::|:|::|___
   |:::::|:::::|:::::|:::||:::||:::|:::::|::|::::|::::::::|::::|:::|:|::|:::|
   |:::::|:::::|:::::|:::||:::||:::|:::::|::|::::|::::::::|::::|:::|:|::|:::|
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
      $ ssh late.sh
      > welcome to the clubhouse. the night is young.
      > _
"""

SKY = {"r": 148, "g": 163, "b": 184}
WALL = {"r": 71, "g": 85, "b": 105}
EDGE = {"r": 148, "g": 163, "b": 184}
WINDOW_ON = {"r": 251, "g": 191, "b": 36}
WINDOW_OFF = {"r": 51, "g": 65, "b": 85}
WATER = {"r": 56, "g": 189, "b": 248}
PROMPT = {"r": 134, "g": 239, "b": 172}
TEXT = {"r": 209, "g": 213, "b": 219}


def canvas() -> dict:
    """The art as dartboard-core's canvas wire: cells and colours by position."""
    random.seed(7)
    lines = ART.strip("\n").split("\n")
    art_w = max(len(line) for line in lines)
    x0 = (COLS - art_w) // 2
    y0 = (ROWS - len(lines)) // 2
    cells, colors = [], []
    for dy, line in enumerate(lines):
        for dx, ch in enumerate(line):
            if ch == " ":
                continue
            pos = {"x": x0 + dx, "y": y0 + dy}
            if dy <= 4:
                color = SKY
            elif ch == "~":
                color = WATER
            elif dy >= len(lines) - 3:
                color = PROMPT if ch in "$>" else TEXT
            elif ch == ":":
                color = WINDOW_ON if random.random() < 0.28 else WINDOW_OFF
            elif ch in "|_":
                color = EDGE
            else:
                color = WALL
            cells.append([pos, {"Narrow": ch}])
            colors.append([pos, color])
    return {"width": COLS, "height": ROWS, "cells": cells, "colors": colors}


def psql(sql: str) -> None:
    subprocess.run(
        ["docker", "compose", "exec", "-T", "postgres", "psql", "-U", "postgres",
         "-d", "postgres", "-q", "-v", "ON_ERROR_STOP=1", "-c", sql],
        check=True,
    )


def main() -> None:
    board = json.dumps(canvas()).replace("'", "''")
    psql(
        "INSERT INTO artboard_snapshots (board_key, canvas, provenance) "
        f"VALUES ('{KEY}', '{board}'::jsonb, '{{\"cells\": []}}'::jsonb) "
        "ON CONFLICT (board_key) DO UPDATE SET canvas = EXCLUDED.canvas"
    )
    try:
        with tempfile.NamedTemporaryFile(suffix=".png") as raw:
            subprocess.run(
                ["curl", "-sf", "-o", raw.name, f"{WEB}/gallery/{KEY}/image.png"],
                check=True,
            )
            subprocess.run(["magick", raw.name, "-resize", CARD, str(OUT)], check=True)
    finally:
        psql(f"DELETE FROM artboard_snapshots WHERE board_key = '{KEY}'")
    print(f"wrote {OUT}", file=sys.stderr)


if __name__ == "__main__":
    main()
