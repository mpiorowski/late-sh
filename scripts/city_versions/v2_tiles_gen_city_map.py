#!/usr/bin/env python3
"""Generate the deadchannel night city (late-ssh/src/app/deadchannel/city/map.rs).

Two registers, one wiring. Pick with `--style`:

    tiles   (default) top-down, one tile per thing, the Dwarf Fortress
            register: `#` walls, `+` doors, `=` counters, `)` blades, `[`
            plate, `%` bowls, `@` people, `c` cats. A side street with
            alleys, courts and rooms you walk into. No multi-cell drawings.
    drawn   front-on, multi-cell facades and carts under a skyline: the
            first pass, kept for comparison.

The city is authored here as stamps on a grid, validated (row widths,
single-width glyphs only, every landmark reachable from the spawn), and
written out as the plain Rust literal that gets committed. Nobody should
hand-edit the MAP strings; tweak this script and re-run:

    python3 scripts/gen_city_map.py --print [--style drawn]
    python3 scripts/gen_city_map.py --write [--style drawn]

Every zone constant in map.rs is emitted from the numbers below, so moving
a prop here moves its zone, its popover reach, and its animation cells in
one step. The renderer (`ui.rs`) and the tests read those constants only,
and branch on `STYLE` where the registers differ.

The register (GAME.md, "Theme"): the neon undercity inside the machine.
Blade Runner streets, machine substance. Rain that falls as static, signs
written in the glyph alphabet nobody can read, a screen tuned to a dead
channel at the end of the street. One glyph per person (your runner's
mark).
"""

import random
import sys
import unicodedata
from collections import deque

W, H = 0, 0
grid = []
solid = []


def new_grid(w, h, all_solid):
    global W, H, grid, solid
    W, H = w, h
    grid = [[' '] * W for _ in range(H)]
    solid = [[all_solid] * W for _ in range(H)]


def put(x, y, s, transparent=False, block=True):
    assert 0 <= y < H, (x, y, s)
    assert x >= 0 and x + len(s) <= W, (x, y, len(s), s)
    for i, ch in enumerate(s):
        if transparent and ch == ' ':
            continue
        grid[y][x + i] = ch
        if block:
            solid[y][x + i] = True


def stamp(x, y, rows, transparent=True, block=True):
    for dy, r in enumerate(rows):
        put(x, y + dy, r, transparent, block)


def clear(x0, y0, x1, y1):
    """Open floor: nothing drawn, nothing blocked."""
    for y in range(y0, y1 + 1):
        for x in range(x0, x1 + 1):
            grid[y][x] = ' '
            solid[y][x] = False


def center(s, w):
    pad = w - len(s)
    return ' ' * (pad // 2) + s + ' ' * (pad - pad // 2)


def zone(x0, y0, x1, y1):
    return (x0, y0, x1, y1)


def frame(title):
    """The box around the map and the street sign let into the top wall."""
    grid[0] = list('═' * W); grid[0][0] = '╔'; grid[0][W - 1] = '╗'
    grid[H - 1] = list('═' * W); grid[H - 1][0] = '╚'; grid[H - 1][W - 1] = '╝'
    for y in range(H):
        solid[y][0] = solid[y][W - 1] = True
    for x in range(W):
        solid[0][x] = solid[H - 1][x] = True
    for y in range(1, H - 1):
        grid[y][0] = '║'; grid[y][W - 1] = '║'
    sx = (W - len(title)) // 2
    put(sx, 0, title)
    return zone(sx, 0, sx + len(title) - 1, 0)


CYAN, MAGENTA, RED, AMBER, GREEN, WHITE = 'Cyan', 'Magenta', 'Red', 'Amber', 'Green', 'White'
TITLE = '╡ ▚ STATIC ROW ▞ ╞'

# The landmark order is the popover priority; map.rs lists it the same way.
LANDMARKS = ['Armorer', 'Tailor', 'Lockers', 'Bands', 'Bar', 'Screen', 'Repairs',
             'Board', 'Bits', 'Noodles', 'Umbrellas', 'Blades', 'Reader', 'Stairs', 'Wire']


# ====================================================================== drawn
def layout_drawn():
    """Front-on facades and carts under a skyline: the first pass."""
    new_grid(232, 52, all_solid=False)
    L = {'STYLE': 'Drawn'}
    L['TITLE'] = frame(TITLE)


    # ---------------------------------------------------------------- bands
    SKY_Y0, SKY_Y1 = 1, 3          # the far skyline, rain falls here
    ROOF_Y = 4                     # every facade's parapet row
    GROUND_Y = 13                  # the ground-floor lintel row
    CURB_Y = 19                    # the north curb (awnings end above it)
    STREET_Y0, STREET_Y1 = 20, 37  # walkable asphalt
    RAIL_Y = 38                    # the railing over the drop
    DROP_Y0, DROP_Y1 = 39, H - 2   # the lower city, far below

    # ---------------------------------------------------------------- skyline
    # Distant towers: a silhouette band with a few lit windows. Rain streaks are
    # painted over it at render time. Drawn without blocking (it is scenery
    # behind the buildings; nothing walks up there anyway).
    SKYLINE = (
        "   ▄▀▀▄      ▄▄▄▄     ▄▀▄        ▄▄█▄▄      ▄▀▀▀▄   ▄▄     ▄▄▄▄▄▄      ▄▀▄     ▄▄▄       ▄▀▀▀▀▄     ▄▄▄▄▄     ▄▀▄         ▄▄▄▄      ▄▀▀▄     ▄▄▄▄▄▄▄      ▄▀▀▄      ▄▄▄      ▄▀▄   ",
        "  ▄█·▪█▄  ▄▄▄█▪·▪█▄▄  █·█▄▄   ▄▄▄█▪·▪█▄▄▄  ▄█·▪·█▄ ▄█▪█▄ ▄▄█▪·▪·▪█▄▄  ▄█▪█▄  ▄█·▪█▄  ▄▄█·▪▪·█▄▄  ▄█▪·▪·█▄  ▄█·█▄  ▄▄▄  ▄█▪·▪█▄  ▄█·▪█▄   ▄█·▪·▪·█▄   ▄▄█▪·█▄▄  ▄█▪·█▄  ▄▄█·█▄▄ ",
        "▄▄█▪·▪·█▄▄█·▪·▪·▪█▄▄█▪·▪█▄▄█▪·▪·▪·▪·█▄▄▄█▪·▪·▪█▄█·▪·█▄█▪·▪·▪·▪·▪█▄▄█·▪·█▄█▪·▪·▪█▄█·▪·▪·▪·█▄▄█·▪·▪·▪█▄█▪·▪█▄█▪·█▄█·▪·▪·█▄█▪·▪·▪█▄▄█▪·▪·▪·▪█▄▄█·▪·▪·█▄█·▪·▪█▄█▪·▪·▪█▄",
    )
    for i, row in enumerate(SKYLINE):
        tiled = (row * 3)[: W - 2]
        put(1, SKY_Y0 + i, tiled, transparent=True, block=False)
    # The antenna mast: its red light blinks.
    MAST_X = 150
    put(MAST_X, 1, '╫', block=False)
    put(MAST_X, 2, '╫', block=False)
    MAST_LIGHT = (MAST_X, 1)
    SKY_ZONE = zone(1, SKY_Y0, W - 2, SKY_Y1)
    for y in range(SKY_Y0, SKY_Y1 + 1):
        for sx in range(W):
            solid[y][sx] = True

    # ---------------------------------------------------------------- facades
    neon_signs = {}   # name -> (zone, color)
    banners = []      # (zone, color): vertical glyph-script signs
    awnings = []      # zone
    windows = []      # zone: window fields whose lit panes flicker
    doors = {}        # name -> zone: the reach cell band for the popover
    vents = []        # (x, y): steam rises above these
    puddles = []      # (x, y)
    lamps = []        # (x, y): street lamps, a cone of light below


    def facade(x0, x1, name, sign, color, storeys=('▪ ▪', '· ▪', '▪ ·')):
        """A building front from the parapet to the curb: windows above, the
        shopfront below the lintel, a door in the middle, an awning over the
        sidewalk. `sign` is the neon over the door."""
        w = x1 - x0 + 1
        inner = w - 2
        # parapet and walls
        put(x0, ROOF_Y, '╭' + '─' * inner + '╮')
        for y in range(ROOF_Y + 1, CURB_Y):
            put(x0, y, '│' + ' ' * inner + '│')
        put(x0, GROUND_Y, '├' + '─' * inner + '┤')
        # window rows: every other row between parapet and lintel
        rows = []
        for k, y in enumerate(range(ROOF_Y + 1, GROUND_Y, 2)):
            pat = storeys[k % len(storeys)]
            line = ''
            while len(line) < inner - 4:
                line += pat + '   '
            line = line[: inner - 4]
            put(x0 + 3, y, line, block=False)
            rows.append(y)
        windows.append(zone(x0 + 3, rows[0], x0 + 3 + (inner - 4) - 1, rows[-1]))
        # the neon sign, centered on the lintel's next row, boxed
        sw = len(sign) + 4
        sx = x0 + (w - sw) // 2
        put(sx, GROUND_Y + 1, '╭' + '─' * (sw - 2) + '╮')
        put(sx, GROUND_Y + 2, '│ ' + sign + ' │')
        put(sx, GROUND_Y + 3, '╰' + '─' * (sw - 2) + '╯')
        neon_signs[name] = (zone(sx + 2, GROUND_Y + 2, sx + 2 + len(sign) - 1, GROUND_Y + 2), color)
        # the door under the sign: two cells wide, glowing sill
        dx = x0 + w // 2 - 1
        put(dx - 1, GROUND_Y + 4, '┃  ┃')
        put(dx - 1, GROUND_Y + 5, '┃▒▒┃')
        # the awning: a half-block row hanging over the sidewalk
        put(x0 + 1, CURB_Y, '▀' * inner, block=False)
        awnings.append(zone(x0 + 1, CURB_Y, x1 - 1, CURB_Y))
        # the reach band: the sidewalk cells in front of the door
        doors[name] = zone(dx - 2, CURB_Y + 1, dx + 3, CURB_Y + 2)
        return dx


    def banner(x, y, glyphs, color):
        """A vertical sign in the city's script: glyph alphabet, top to bottom."""
        put(x, y, '╓─╖')
        for i, g in enumerate(glyphs):
            put(x, y + 1 + i, '║' + g + '║')
        put(x, y + 1 + len(glyphs), '╙─╜')
        banners.append((zone(x + 1, y + 1, x + 1, y + len(glyphs)), color))


    def alley(x0, x1):
        """A gap between buildings: fire escape on one wall, a vent, steam."""
        clear(x0, ROOF_Y, x1, CURB_Y)
        for y in range(ROOF_Y, GROUND_Y + 1, 2):
            put(x0, y, '╪', block=False)
            put(x0, y + 1, '│', block=False)
        put(x0 + 2, CURB_Y - 1, '≡', block=False)
        vents.append((x0 + 2, CURB_Y - 1))
        # a dumpster against the far wall, where the alley is wide enough
        if x1 - x0 >= 7:
            put(x1 - 3, CURB_Y - 2, '╭──╮', block=True)
            put(x1 - 3, CURB_Y - 1, '│▒▒│', block=True)


    # The row of buildings, west to east. Widths leave five-wide alleys.
    CYAN, MAGENTA, RED, AMBER, GREEN, WHITE = 'Cyan', 'Magenta', 'Red', 'Amber', 'Green', 'White'

    # west end: the stairwell down (not a building, a hole in the block)
    STAIRS_X0, STAIRS_X1 = 2, 15
    put(STAIRS_X0, ROOF_Y, '╭' + '─' * 12 + '╮')
    for y in range(ROOF_Y + 1, CURB_Y):
        put(STAIRS_X0, y, '│' + ' ' * 12 + '│')
    put(STAIRS_X0, CURB_Y, '╰' + '─' * 12 + '╯')
    put(STAIRS_X0 + 1, ROOF_Y + 1, ' ▼  LOWER  ▼')
    put(STAIRS_X0 + 1, ROOF_Y + 3, '  ░░░░░░░░  ')
    put(STAIRS_X0 + 1, ROOF_Y + 4, '   ▒▒▒▒▒▒   ')
    put(STAIRS_X0 + 1, ROOF_Y + 5, '    ▓▓▓▓    ')
    put(STAIRS_X0 + 1, ROOF_Y + 6, '     ██     ')
    put(STAIRS_X0 + 1, ROOF_Y + 8, '  ░░░░░░░░  ')
    put(STAIRS_X0 + 1, ROOF_Y + 9, '   ▒▒▒▒▒▒   ')
    put(STAIRS_X0 + 1, ROOF_Y + 10, '    ▓▓▓▓    ')
    put(STAIRS_X0 + 1, ROOF_Y + 11, '     ██     ')
    put(STAIRS_X0 + 1, ROOF_Y + 13, ' ▼  LOWER  ▼')
    STAIRS_SIGN = zone(STAIRS_X0 + 2, ROOF_Y + 1, STAIRS_X0 + 12, ROOF_Y + 1)
    STAIRS_SIGN2 = zone(STAIRS_X0 + 2, ROOF_Y + 13, STAIRS_X0 + 12, ROOF_Y + 13)
    doors['Stairs'] = zone(STAIRS_X0, CURB_Y + 1, STAIRS_X1, CURB_Y + 2)

    x = STAIRS_X1 + 1
    alley(x, x + 3); x += 4

    # the armorer
    ARMORER_X0 = x
    x1 = x + 27
    dx = facade(x, x1, 'Armorer', 'A R M O R E R', RED)
    # display windows either side of the door: blades left, plate right
    put(x + 2, GROUND_Y + 4, '[ † ╪ † ]', block=False)
    put(x + 2, GROUND_Y + 5, '[ / | \\ ]', block=False)
    put(x1 - 10, GROUND_Y + 4, '[ ▐▓▌▐▓▌ ]', block=False)
    put(x1 - 10, GROUND_Y + 5, '[ ▟█▙▟█▙ ]', block=False)
    banner(x + 1, ROOF_Y + 1, '╬╪╫', RED)
    x = x1 + 1
    alley(x, x + 3); x += 4

    # the tailor: mannequins in the window wear starter pieces
    TAILOR_X0 = x
    x1 = x + 29
    dx = facade(x, x1, 'Tailor', 'T A I L O R', MAGENTA)
    put(x + 2, GROUND_Y + 3, ' ╬═╬ ', block=False)
    put(x + 2, GROUND_Y + 4, '▐◈ ◈▌', block=False)
    put(x + 2, GROUND_Y + 5, ' ▟▓▙ ', block=False)
    put(x1 - 6, GROUND_Y + 3, ' ▚▞▚ ', block=False)
    put(x1 - 6, GROUND_Y + 4, '▐▚ ▞▌', block=False)
    put(x1 - 6, GROUND_Y + 5, ' ▟╬▙ ', block=False)
    banner(x1 - 3, ROOF_Y + 1, '▚▞▚', MAGENTA)
    x = x1 + 1
    alley(x, x + 3); x += 4

    # the lockers: a wall of them, one sign letter dead
    LOCKERS_X0 = x
    x1 = x + 23
    dx = facade(x, x1, 'Lockers', 'L O C K E R S', CYAN, storeys=('▪ ·', '· ·', '· ▪'))
    put(x + 2, GROUND_Y + 4, '▐▌▐▌▐▌', block=False)
    put(x + 2, GROUND_Y + 5, '▐▌▐▌▐▌', block=False)
    put(x1 - 7, GROUND_Y + 4, '▐▌▐▌▐▌', block=False)
    put(x1 - 7, GROUND_Y + 5, '▐▌▐▌▐▌', block=False)
    DEAD_LETTER = (neon_signs['Lockers'][0][0] + 8, GROUND_Y + 2)   # the E
    x = x1 + 1
    alley(x, x + 3); x += 4

    # the bands: a radio shop, antennas on the roof, dials in the window
    BANDS_X0 = x
    x1 = x + 25
    dx = facade(x, x1, 'Bands', 'B A N D S', GREEN, storeys=('· ▪', '▪ ▪', '· ·'))
    put(x + 5, ROOF_Y - 1, '╫', block=False)
    put(x + 13, ROOF_Y - 1, '╫', block=False)
    put(x + 20, ROOF_Y - 1, '╫', block=False)
    put(x + 2, GROUND_Y + 4, '(o) (o)', block=False)
    put(x + 2, GROUND_Y + 5, ' ▁▂▃ ▂▃▅', block=False)
    put(x1 - 9, GROUND_Y + 4, '(o) (o)', block=False)
    put(x1 - 9, GROUND_Y + 5, ' ▃▅▆ ▁▂▁', block=False)
    banner(x + 1, ROOF_Y + 1, '╫╫╫', GREEN)
    x = x1 + 1
    alley(x, x + 3); x += 4

    # the bar: dead air. the signal is warm in here.
    BAR_X0 = x
    x1 = x + 25
    dx = facade(x, x1, 'Bar', 'D E A D  A I R', AMBER, storeys=('▪ ▪', '▪ ▪', '· ▪'))
    put(x + 2, GROUND_Y + 4, '♪ ░▒▓░ ♪', block=False)
    put(x + 2, GROUND_Y + 5, '  ▐▌▐▌  ', block=False)
    put(x1 - 9, GROUND_Y + 4, '♪ ░▒▓░ ♪', block=False)
    put(x1 - 9, GROUND_Y + 5, '  ▐▌▐▌  ', block=False)
    banner(x1 - 3, ROOF_Y + 1, '▖▘▗', AMBER)
    x = x1 + 1
    alley(x, x + 3); x += 4

    # the screen: a building whose face is one screen, tuned to a dead channel
    SCREEN_X0 = x
    SCREEN_W = 30
    put(x, ROOF_Y, '╔' + '═' * (SCREEN_W - 2) + '╗')
    for y in range(ROOF_Y + 1, GROUND_Y + 1):
        put(x, y, '║' + '░' * (SCREEN_W - 2) + '║')
    put(x, GROUND_Y + 1, '╚' + '═' * (SCREEN_W - 2) + '╝')
    SCREEN_FACE = zone(x + 1, ROOF_Y + 1, x + SCREEN_W - 2, GROUND_Y)
    # the plinth under it: a ledge, the maintenance door, the cables
    for y in range(GROUND_Y + 2, CURB_Y):
        put(x, y, '│' + ' ' * (SCREEN_W - 2) + '│')
    put(x, CURB_Y, '╰' + '─' * (SCREEN_W - 2) + '╯')
    put(x + 2, GROUND_Y + 3, '╓───╖  ╓───╖   ╓───╖  ╓───╖', block=False)
    put(x + 2, GROUND_Y + 4, '║▒▒▒║  ║▒▒▒║   ║▒▒▒║  ║▒▒▒║', block=False)
    put(x + 2, GROUND_Y + 5, '╙───╜  ╙───╜   ╙───╜  ╙───╜', block=False)
    doors['Screen'] = zone(x + 2, CURB_Y + 1, x + SCREEN_W - 3, CURB_Y + 2)
    x += SCREEN_W
    alley(x, x + 3); x += 4

    # repairs: a narrow kiosk, more workshop than shop
    REPAIRS_X0 = x
    x1 = x + 17
    dx = facade(x, x1, 'Repairs', 'P A T C H', WHITE, storeys=('· ·', '▪ ·', '· ·'))
    put(x + 2, GROUND_Y + 4, '≋ ╳', block=False)
    put(x + 2, GROUND_Y + 5, '╳ ≋', block=False)
    put(x1 - 4, GROUND_Y + 4, '╳ ≋', block=False)
    put(x1 - 4, GROUND_Y + 5, '≋ ╳', block=False)
    x = x1 + 1
    assert x <= W - 1, x
    # whatever is left is the east wall: a fire escape and the dark
    if x < W - 1:
        alley(x, W - 2)

    # ---------------------------------------------------------------- street
    # The north sidewalk: lamps, a hydrant, the board, a bits machine.
    SIDEWALK_Y0, SIDEWALK_Y1 = CURB_Y + 1, CURB_Y + 2
    for lx in (22, 62, 102, 142, 182, 222):
        put(lx, SIDEWALK_Y0, '╥', block=True)
        lamps.append((lx, SIDEWALK_Y0))

    # the board: a kiosk pillar plastered with notices, on the sidewalk
    BOARD_X, BOARD_Y = 98, STREET_Y0 + 1
    stamp(BOARD_X, BOARD_Y, [
        '╔═══════════╗',
        '║ THE BOARD ║',
        '╠═══════════╣',
        '║ ▪≡≡ ▪≡≡ ▪ ║',
        '║ ≡≡▪ ≡ ▪≡≡ ║',
        '║ ▪≡ ≡≡▪ ≡▪ ║',
        '╚═══════════╝',
    ], transparent=False)
    BOARD_ZONE = zone(BOARD_X, BOARD_Y, BOARD_X + 12, BOARD_Y + 6)
    BOARD_SIGN = zone(BOARD_X + 2, BOARD_Y + 1, BOARD_X + 10, BOARD_Y + 1)

    # a bits machine: hums, has never once paid out
    BITS_X, BITS_Y = 170, STREET_Y0 + 1
    stamp(BITS_X, BITS_Y, [
        '╔══════╗',
        '║ BITS ║',
        '║ ▓▓▓▓ ║',
        '║ [──] ║',
        '╚══════╝',
    ], transparent=False)
    BITS_ZONE = zone(BITS_X, BITS_Y, BITS_X + 7, BITS_Y + 4)
    BITS_SCREEN = zone(BITS_X + 2, BITS_Y + 2, BITS_X + 5, BITS_Y + 2)

    # the lane: a dashed centre line down the whole street
    LANE_Y = STREET_Y0 + 9
    for lx in range(2, W - 2, 3):
        put(lx, LANE_Y, '╌', block=False)

    # drains and puddles
    for gx in (30, 88, 140, 196):
        put(gx, STREET_Y1 - 1, '▒▒', block=False)

    # ---------------------------------------------------------------- carts
    carts = {}


    def cart(name, x, y, rows, sign_row, sign_x0, sign_len, color):
        stamp(x, y, rows, transparent=False)
        carts[name] = (zone(x, y, x + len(rows[0]) - 1, y + len(rows) - 1),
                       zone(x + sign_x0, y + sign_row, x + sign_x0 + sign_len - 1, y + sign_row),
                       color)


    # noodles: steam off three bowls
    cart('Noodles', 40, STREET_Y0 + 4, [
        '╭───────────╮',
        '│  NOODLES  │',
        '╞═══════════╡',
        '│ (≈) (≈) (≈│',
        '╰o─────────o╯',
    ], 1, 3, 7, AMBER)
    # the steam comes up over the counter, where the sidewalk is open
    for i in range(3):
        vents.append((40 + 4 + i * 4, STREET_Y0 + 3))
    put(40 + 1, STREET_Y0 + 3, '@', block=True)   # the cook, behind the counter

    # umbrellas: a canopy, the handles glowing
    cart('Umbrellas', 66, STREET_Y0 + 11, [
        '  ╱▔▔▔▔▔▔▔▔▔▔▔╲  ',
        ' ╱ UMBRELLAS   ╲ ',
        ' ▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔ ',
        '   ╷ ╷ ╷ ╷ ╷ ╷   ',
        '   ┴ ┴ ┴ ┴ ┴ ┴   ',
    ], 1, 3, 9, CYAN)
    put(66 + 16, STREET_Y0 + 14, '@', block=True)

    # blades: what the armorer will not sell
    cart('Blades', 132, STREET_Y0 + 12, [
        '╭┈┈┈┈┈┈┈┈┈┈┈┈╮',
        '┊ †  /  †  ╪ ┊',
        '┊   BLADES   ┊',
        '╰o┈┈┈┈┈┈┈┈┈┈o╯',
    ], 2, 4, 6, RED)
    put(132 + 14, STREET_Y0 + 13, '@', block=True)

    # the reader: a tent, she reads the static
    cart('Reader', 192, STREET_Y0 + 4, [
        '     ╱╲     ',
        '    ╱  ╲    ',
        '   ╱ ◌◌ ╲   ',
        '  ╱READER ╲ ',
        '  ▔▔▔▔▔▔▔▔▔ ',
    ], 3, 3, 6, MAGENTA)


    # ---------------------------------------------------------------- puddles
    # Laid down last, only on open asphalt, so a puddle never eats a prop.
    PUDDLE_SPOTS = [
        (20, 26, 5), (34, 30, 4), (56, 22, 3), (62, 33, 6), (80, 27, 4),
        (108, 31, 5), (128, 25, 3), (134, 34, 5), (158, 28, 4), (166, 27, 3),
        (186, 32, 4), (44, 35, 3), (96, 35, 4), (150, 36, 3), (204, 30, 5),
        (216, 34, 3), (224, 23, 4), (12, 33, 4), (100, 23, 3),
    ]


    def puddle(px, py, ch):
        if grid[py][px] == ' ' and not solid[py][px]:
            grid[py][px] = ch
            puddles.append((px, py))


    for (px, py, n) in PUDDLE_SPOTS:
        for i in range(n):
            puddle(px + i, py, '≈' if i % 2 == 0 else '~')
        if n >= 4:
            for i in range(1, n - 1):
                puddle(px + i, py + 1, '~')

    # ---------------------------------------------------------------- the drop
    # The railing over the edge, with one gap for the stairwell from the wire.
    WIRE_W = 14
    WIRE_X0 = (W - WIRE_W) // 2
    for rx in range(1, W - 1):
        put(rx, RAIL_Y, '╪' if rx % 2 == 0 else '═', block=True)
    # the lower city: lights far below, nothing to stand on
    for y in range(DROP_Y0, DROP_Y1 + 1):
        for lx in range(1, W - 1):
            solid[y][lx] = True
    DROP_LIGHTS = []
    rng = random.Random(7)
    for y in range(DROP_Y0, DROP_Y1 + 1):
        for lx in range(2, W - 2):
            if rng.random() < 0.035:
                ch = rng.choice('·∙·▪·')
                grid[y][lx] = ch
                DROP_LIGHTS.append((lx, y))
    # a couple of lower signs, read from above
    for (sx, sy, s) in ((30, DROP_Y0 + 3, '▪▪▪▪'), (170, DROP_Y0 + 5, '▪▪▪'), (100, DROP_Y1 - 2, '▪▪▪▪▪'), (210, DROP_Y0 + 8, '▪▪▪▪')):
        put(sx, sy, s, block=True)
        for i in range(len(s)):
            DROP_LIGHTS.append((sx + i, sy))
    # the stairwell up to the wire: a gap in the rail, steps down into the dark
    clear(WIRE_X0, RAIL_Y, WIRE_X0 + WIRE_W - 1, RAIL_Y)
    put(WIRE_X0, RAIL_Y, '╡', block=True)
    put(WIRE_X0 + WIRE_W - 1, RAIL_Y, '╞', block=True)
    stamp(WIRE_X0, RAIL_Y + 1, [
        '│' + center('▲ THE WIRE ▲', WIRE_W - 2) + '│',
        '│' + center('▓▓▓▓▓▓▓▓', WIRE_W - 2) + '│',
        '│' + center('▒▒▒▒▒▒', WIRE_W - 2) + '│',
        '│' + center('░░░░', WIRE_W - 2) + '│',
        '╰' + '─' * (WIRE_W - 2) + '╯',
    ], transparent=False)
    WIRE_ZONE = zone(WIRE_X0, RAIL_Y, WIRE_X0 + WIRE_W - 1, RAIL_Y + 5)
    WIRE_SIGN = zone(WIRE_X0 + 2, RAIL_Y + 1, WIRE_X0 + WIRE_W - 3, RAIL_Y + 1)
    SPAWN = (WIRE_X0 + WIRE_W // 2, RAIL_Y)
    doors['Wire'] = zone(WIRE_X0 + 1, RAIL_Y - 1, WIRE_X0 + WIRE_W - 2, RAIL_Y)
    BOTTOM_SIGN = '╡ the wire ╞'
    put((W - len(BOTTOM_SIGN)) // 2, H - 1, BOTTOM_SIGN)

    L['SPAWN'] = SPAWN
    L['OPEN'] = (SPAWN[0], STREET_Y0 + 12)
    L['SKY'] = SKY_ZONE
    L['MAST_LIGHT'] = MAST_LIGHT
    L['STREET'] = zone(1, STREET_Y0, W - 2, STREET_Y1)
    L['RAIL_Y'] = RAIL_Y
    L['DROP'] = zone(1, DROP_Y0, W - 2, DROP_Y1)
    L['SCREEN_FACE'] = SCREEN_FACE
    L['BITS_SCREEN'] = BITS_SCREEN
    L['BOARD'] = BOARD_ZONE
    L['BOARD_SIGN'] = BOARD_SIGN
    L['STAIRS_SIGNS'] = [STAIRS_SIGN, STAIRS_SIGN2]
    L['WIRE'] = WIRE_ZONE
    L['WIRE_SIGN'] = WIRE_SIGN
    L['DEAD_LETTER'] = DEAD_LETTER
    L['SIGNS'] = list(neon_signs.values())
    L['BANNERS'] = banners
    L['CART_SIGNS'] = [(s, c) for (_z, s, c) in carts.values()]
    L['AWNINGS'] = awnings
    L['WINDOWS'] = windows
    L['BUILDINGS'] = []
    L['VENTS'] = vents
    L['PUDDLES'] = puddles
    L['LAMPS'] = lamps
    L['DROP_LIGHTS'] = DROP_LIGHTS
    L['reach'] = {
        'Armorer': doors['Armorer'], 'Tailor': doors['Tailor'], 'Lockers': doors['Lockers'],
        'Bands': doors['Bands'], 'Bar': doors['Bar'], 'Screen': doors['Screen'],
        'Repairs': doors['Repairs'], 'Board': BOARD_ZONE, 'Bits': BITS_ZONE,
        'Noodles': carts['Noodles'][0], 'Umbrellas': carts['Umbrellas'][0],
        'Blades': carts['Blades'][0], 'Reader': carts['Reader'][0],
        'Stairs': doors['Stairs'], 'Wire': doors['Wire'],
    }
    L['dist'] = {k: (1 if k in ('Noodles', 'Umbrellas', 'Blades', 'Reader', 'Board', 'Bits') else 0)
                 for k in LANDMARKS}
    return L


# ====================================================================== tiles
def layout_tiles():
    """Top-down, one tile per thing. A side street that jogs once, alleys
    off it north and south, rooms you walk into, a hidden court, a ledge
    over the drop at the east end and the screen closing the street."""
    new_grid(156, 36, all_solid=True)
    L = {'STYLE': 'Tiles'}
    L['TITLE'] = frame(TITLE)

    neon_signs = []   # (zone, color): the shop names in the street-facing walls
    tags = []         # (zone, color): glyph-script graffiti on the walls
    buildings = []    # (zone, color or None): walls take the color
    vents = []        # steam rises off these
    puddles = []
    lamps = []
    stalls = {}       # name -> (zone, vendor zone, color)
    reach = {}
    dist = {}

    def carve(x0, y0, x1, y1):
        clear(x0, y0, x1, y1)

    def tile(x, y, ch, block=True):
        """One tile. `block=False` opens the cell whatever was there (doors
        let into walls, the gap in the railing)."""
        put(x, y, ch, block=block)
        solid[y][x] = block

    def room(x0, y0, x1, y1, color, doors=(), windows=(), sign=None, shutter=None):
        """Walls of `#`, a floor inside, doors (`+`, open) and windows (`╬`)
        let into the walls, the name spelled in the street-facing wall."""
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                edge = x in (x0, x1) or y in (y0, y1)
                grid[y][x] = '#' if edge else ' '
                solid[y][x] = edge
        for (dx, dy) in doors:
            tile(dx, dy, '+', block=False)
        for (wx, wy) in windows:
            tile(wx, wy, '╬')
        if sign:
            sx, sy, text = sign
            put(sx, sy, text)
            neon_signs.append((zone(sx, sy, sx + len(text) - 1, sy), color))
        if shutter:
            tile(shutter[0], shutter[1], '▓')
        buildings.append((zone(x0, y0, x1, y1), color))

    def wall(x0, y0, x1, y1, doors=()):
        """An inner partition."""
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                tile(x, y, '#')
        for (dx, dy) in doors:
            tile(dx, dy, '+', block=False)

    def tag(x, y, glyph, color):
        tile(x, y, glyph)
        tags.append((zone(x, y, x, y), color))

    def lantern(x, y):
        tile(x, y, '°', block=False)

    def lamp(x, y):
        tile(x, y, '*')
        lamps.append((x, y))

    def stall(name, x, y, row, vendor_x, color):
        """A counter of `=` with the goods on it and the vendor beside it."""
        put(x, y, row)
        tile(vendor_x, y, '@')
        x0, x1 = min(x, vendor_x), max(x + len(row) - 1, vendor_x)
        stalls[name] = (zone(x0, y, x1, y), zone(vendor_x, y, vendor_x, y), color)
        reach[name] = zone(x0, y, x1, y)
        dist[name] = 1

    # ------------------------------------------------------------ the street
    # West leg, four tiles wide, then a dogleg south, then the east leg
    # along the ledge, opening into a small yard under the screen.
    carve(1, 17, 72, 20)
    carve(70, 19, 150, 22)
    carve(138, 17, 150, 22)
    L['STREET'] = zone(1, 17, 150, 22)

    # ------------------------------------------------------- north, west leg
    # A tenement: a lobby, a partition, beds behind it. Nobody sells anything.
    room(2, 9, 18, 16, None, doors=[(10, 16)], windows=[(5, 16), (14, 16), (9, 9)])
    wall(3, 12, 17, 12, doors=[(10, 12)])
    for bx in (4, 8, 13, 16):
        tile(bx, 10, '▬')
    tile(15, 14, '@')
    tag(3, 16, '▚', MAGENTA)
    # the stairs down to the lower city, at the west end of the street
    tile(2, 18, '>', block=False)
    L['STAIRS_SIGNS'] = [zone(2, 18, 2, 18)]
    reach['Stairs'] = zone(2, 18, 2, 18); dist['Stairs'] = 0

    # an alley: a dumpster at the back, a cat on it, lanterns on a string
    carve(19, 5, 21, 16)
    tile(19, 5, '▒'); tile(20, 5, '▒'); tile(21, 6, 'c')
    tile(20, 10, '≡', block=False); vents.append((20, 10))
    lantern(20, 8); lantern(20, 13)

    # the armorer: blades and plate on the shelf, the counter, the man
    room(22, 10, 36, 16, RED, doors=[(33, 16)], windows=[(35, 16), (27, 10)],
         sign=(24, 16, 'ARMORER'))
    for gx, g in ((24, ')'), (26, ')'), (28, '/'), (32, '['), (34, '[')):
        tile(gx, 11, g)
    put(24, 13, '=' * 11)
    tile(29, 12, '@')
    reach['Armorer'] = zone(24, 14, 34, 14); dist['Armorer'] = 0

    # a crack between two buildings, one tile wide
    carve(37, 12, 37, 16)
    tile(37, 13, '≡', block=False); vents.append((37, 13))

    # the tailor: a storeroom up top, the rack, the mirror, two mannequins
    room(38, 8, 52, 16, MAGENTA, doors=[(49, 16)], windows=[(47, 16), (43, 8)],
         sign=(40, 16, 'TAILOR'))
    wall(39, 10, 51, 10, doors=[(45, 10)])
    tile(40, 9, '['); tile(42, 9, '['); tile(50, 9, '▪'); tile(49, 9, '▪')
    for gx, g in ((40, '['), (42, '['), (44, '['), (47, '"'), (49, '"')):
        tile(gx, 11, g)
    put(40, 13, '=' * 11)
    tile(45, 12, '@')
    tile(51, 12, '▌')
    tile(40, 15, '&'); tile(50, 15, '&')
    reach['Tailor'] = zone(40, 14, 50, 14); dist['Tailor'] = 0

    # an alley with a rat in it
    carve(53, 6, 54, 16)
    tile(53, 6, '▒'); tile(54, 9, 'r')
    lantern(53, 11)

    # the lockers: two banks of them, a walk between
    room(55, 11, 69, 16, CYAN, doors=[(66, 16)], windows=[(68, 16)],
         sign=(57, 16, 'LOCKERS'))
    for lx in range(56, 69):
        tile(lx, 12, '∩'); tile(lx, 14, '∩')
    reach['Lockers'] = zone(56, 13, 68, 15); dist['Lockers'] = 0
    L['DEAD_LETTER'] = (61, 16)   # the E

    # a long alley north to a hidden court: a shrine, a plant, a cat
    carve(70, 8, 71, 16)
    carve(64, 3, 78, 7)
    tile(68, 5, '♣'); tile(74, 4, '_'); tile(70, 6, 'c'); tile(78, 7, '▒')
    for lx in (65, 69, 73, 77):
        lantern(lx, 3)
    lantern(70, 12)
    tile(71, 9, '≡', block=False); vents.append((71, 9))

    # ------------------------------------------------------- north, east leg
    # bands: dials and aerials on the shelves
    room(73, 11, 88, 18, GREEN, doors=[(84, 18)], windows=[(75, 18), (86, 18), (80, 11)],
         sign=(76, 18, 'BANDS'))
    for gx in (76, 80, 84):
        tile(gx, 12, 'o')
    for gx in (75, 78, 81, 84, 87):
        tile(gx, 13, 'Y')
    put(75, 15, '=' * 12)
    tile(80, 14, '@')
    reach['Bands'] = zone(75, 16, 86, 16); dist['Bands'] = 0

    carve(89, 13, 89, 18)
    tile(89, 15, '≡', block=False); vents.append((89, 15))

    # the bar: bottles, a long counter, stools, a jukebox in the corner
    room(90, 12, 110, 18, AMBER, doors=[(105, 18)],
         windows=[(92, 18), (108, 18), (96, 12), (104, 12)], sign=(93, 18, 'DEAD AIR'))
    for gx in (94, 96, 98, 102, 104, 106):
        tile(gx, 13, '!')
    tile(100, 13, '@')
    put(93, 14, '=' * 15)
    for gx in (94, 97, 100, 103, 106):
        tile(gx, 15, 'o')
    tile(92, 16, '♪')
    reach['Bar'] = zone(93, 15, 107, 16); dist['Bar'] = 0

    carve(111, 9, 112, 18)
    tile(111, 9, '▒')
    tile(111, 12, '≡', block=False); vents.append((111, 12))
    lantern(112, 10); lantern(112, 14)

    # repairs: tools on the wall, a bench
    room(113, 13, 123, 18, WHITE, doors=[(121, 18)], windows=[(114, 18)],
         sign=(115, 18, 'PATCH'))
    for gx, g in ((115, '/'), (117, '\\'), (119, 'x'), (121, '/')):
        tile(gx, 14, g)
    tile(118, 15, '@')
    put(115, 16, '=' * 7)
    reach['Repairs'] = zone(115, 17, 121, 17); dist['Repairs'] = 0

    carve(124, 15, 124, 18)
    tile(124, 16, 'r')

    # a motel: rooms off a corridor, someone asleep. Nothing to do here.
    room(125, 10, 137, 18, CYAN, doors=[(135, 18)], windows=[(126, 18), (127, 10), (135, 10)],
         sign=(128, 18, 'SLEEP'))
    wall(126, 16, 136, 16, doors=[(128, 16), (131, 16), (134, 16)])
    wall(129, 11, 129, 15); wall(133, 11, 133, 15)
    for bx in (127, 131, 135):
        tile(bx, 12, '▬')
    tile(127, 13, '@')

    # ------------------------------------------------------------- the yard
    carve(143, 10, 144, 16)
    tile(143, 10, '▒'); lantern(144, 12)
    tile(144, 14, '≡', block=False); vents.append((144, 14))
    for lx in (140, 144, 148):
        lantern(lx, 17)
    # the bits machine, against the motel's corner
    tile(139, 18, '$')
    L['BITS_SCREEN'] = zone(139, 18, 139, 18)
    L['BOARD_SIGN'] = None
    reach['Bits'] = zone(139, 18, 139, 18); dist['Bits'] = 1
    # the reader at her table, a candle either side
    put(146, 21, '===')
    tile(147, 20, '@'); tile(146, 20, '°'); tile(148, 20, '°')
    stalls['Reader'] = (zone(146, 20, 148, 21), zone(147, 20, 147, 20), MAGENTA)
    reach['Reader'] = zone(146, 20, 148, 21); dist['Reader'] = 1
    # the screen closes the street: three tiles of static, wall above and below
    put(151, 16, '###'); put(151, 24, '###')
    for y in range(17, 24):
        put(151, y, '░░░')
    L['SCREEN_FACE'] = zone(151, 17, 153, 23)
    reach['Screen'] = zone(150, 17, 150, 22); dist['Screen'] = 0

    # ------------------------------------------------------- south, west leg
    # a tenement: corridor, four rooms, beds, one tenant awake
    room(2, 21, 28, 30, None, doors=[(14, 21)], windows=[(6, 21), (22, 21)])
    wall(3, 23, 27, 23, doors=[(6, 23), (12, 23), (18, 23), (24, 23)])
    for cx in (9, 15, 21):
        wall(cx, 24, cx, 29)
    for bx in (4, 11, 17, 24):
        tile(bx, 26, '▬')
    tile(5, 28, '@'); tile(8, 25, '∩'); tile(20, 25, '∩')

    # an alley south, dead end
    carve(29, 21, 31, 32)
    tile(30, 32, '▒'); tile(29, 30, 'r')
    tile(30, 26, '≡', block=False); vents.append((30, 26))
    lantern(30, 23); lantern(30, 28)

    # the noodle stall, pressed against the lockup wall
    stall('Noodles', 34, 20, '=%=%=', 33, AMBER)
    vents.append((35, 20)); vents.append((37, 20))

    # a lockup: crates, a guard, nothing for sale
    room(32, 21, 58, 30, None, doors=[(45, 21)])
    for (x0, y0, x1, y1) in ((34, 27, 36, 29), (40, 25, 42, 26), (50, 28, 54, 29), (55, 23, 57, 24)):
        for y in range(y0, y1 + 1):
            put(x0, y, '▪' * (x1 - x0 + 1))
    tile(45, 23, '@')
    tile(50, 24, '≡', block=False)
    tag(33, 21, '▞', CYAN)

    # umbrellas
    stall('Umbrellas', 62, 20, '=T=T=', 67, CYAN)

    # a pawn shop, shuttered. The sign still burns.
    room(59, 21, 69, 30, RED, sign=(62, 21, 'PAWN'), shutter=(67, 21))
    for gx, g in ((61, '$'), (63, ')'), (65, '"'), (67, '[')):
        tile(gx, 23, g)
    put(61, 25, '=' * 7)

    # ------------------------------------------------------- south, east leg
    carve(70, 23, 71, 30)
    tile(70, 30, '▒'); tile(71, 25, 'r')
    tile(71, 27, '≡', block=False); vents.append((71, 27))

    # the flop: a corridor of rooms, most of them taken
    room(72, 23, 106, 32, None, doors=[(80, 23), (98, 23)], windows=[(75, 23), (89, 23), (103, 23)])
    wall(73, 25, 105, 25, doors=[(76, 25), (82, 25), (88, 25), (94, 25), (100, 25)])
    for cx in (79, 85, 91, 97, 103):
        wall(cx, 26, cx, 31)
    for bx in (75, 81, 87, 93, 99, 104):
        tile(bx, 28, '▬')
    tile(76, 30, '@'); tile(94, 30, '@')
    for cx in (78, 90, 102):
        tile(cx, 27, '∩')

    # the board: one pillar of notices against the flop's wall
    tile(87, 22, '?')
    L['BOARD'] = zone(87, 22, 87, 22)
    reach['Board'] = zone(87, 22, 87, 22); dist['Board'] = 1

    # blades, against the railing
    stall('Blades', 112, 22, '=)=)=', 117, RED)

    # ------------------------------------------------------------- the drop
    # The railing along the ledge, one gap for the stairs to the wire.
    for rx in range(107, 151):
        tile(rx, 23, '╪' if rx % 8 == 0 else '═')
    tile(126, 23, '>', block=False)
    L['RAIL_Y'] = 23
    L['WIRE'] = zone(125, 23, 127, 23)
    L['WIRE_SIGN'] = None
    L['SPAWN'] = (126, 23)
    reach['Wire'] = zone(126, 23, 126, 23); dist['Wire'] = 0
    L['DROP'] = zone(107, 24, W - 2, H - 2)
    drop_lights = []
    rng = random.Random(7)
    for y in range(24, H - 1):
        for lx in range(107, W - 1):
            if rng.random() < 0.05:
                grid[y][lx] = rng.choice('·∙·▪·')
                drop_lights.append((lx, y))
    L['DROP_LIGHTS'] = drop_lights
    L['SKY'] = None
    L['MAST_LIGHT'] = None
    put((W - len('╡ the wire ╞')) // 2, H - 1, '╡ the wire ╞')

    # --------------------------------------------------------------- lamps
    for (lx, ly) in ((12, 17), (30, 17), (57, 17), (86, 19), (110, 22), (134, 22), (141, 18)):
        lamp(lx, ly)

    # -------------------------------------------------------------- puddles
    def puddle(px, py, ch):
        if grid[py][px] == ' ' and not solid[py][px]:
            grid[py][px] = ch
            puddles.append((px, py))

    for (px, py, n) in ((8, 19, 3), (26, 18, 4), (44, 19, 3), (50, 17, 2), (66, 18, 3),
                        (78, 21, 4), (96, 20, 3), (104, 21, 2), (118, 20, 4), (131, 21, 3),
                        (140, 20, 3), (146, 18, 2), (72, 22, 2), (53, 14, 2), (66, 6, 2),
                        (20, 15, 2), (112, 16, 1), (143, 15, 2)):
        for i in range(n):
            puddle(px + i, py, '≈' if i % 2 == 0 else '~')
        if n >= 4:
            puddle(px + 1, py + 1, '~')

    # ------------------------------------------------------------- texture
    # Wet ground: a scatter of `.` `,` and grit on every open tile.
    rng = random.Random(11)
    for y in range(1, H - 1):
        for x in range(1, W - 1):
            if solid[y][x] or grid[y][x] != ' ':
                continue
            r = rng.random()
            if r < 0.30:
                grid[y][x] = '.'
            elif r < 0.36:
                grid[y][x] = ','
            elif r < 0.39:
                grid[y][x] = '`'

    L['OPEN'] = (44, 18)
    L['SIGNS'] = neon_signs
    L['BANNERS'] = tags
    L['CART_SIGNS'] = [(v, c) for (_z, v, c) in stalls.values()]
    L['AWNINGS'] = []
    L['WINDOWS'] = [z for (z, _c) in buildings]
    L['BUILDINGS'] = buildings
    L['VENTS'] = vents
    L['PUDDLES'] = puddles
    L['LAMPS'] = lamps
    L['reach'] = reach
    L['dist'] = dist
    return L


# =================================================================== validate
def validate(L):
    for y, row in enumerate(grid):
        assert len(row) == W, (y, len(row))
        for x, ch in enumerate(row):
            assert unicodedata.east_asian_width(ch) not in ('W', 'F'), (x, y, ch)
            assert not unicodedata.combining(ch), (x, y, ch)

    def walkable(x, y):
        return 0 < x < W - 1 and 0 < y < H - 1 and not solid[y][x]

    assert walkable(*L['SPAWN']), L['SPAWN']
    assert walkable(*L['OPEN']), L['OPEN']

    def flood(start):
        seen = {start}
        q = deque([start])
        while q:
            x, y = q.popleft()
            for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                nx, ny = x + dx, y + dy
                if walkable(nx, ny) and (nx, ny) not in seen:
                    seen.add((nx, ny))
                    q.append((nx, ny))
        return seen

    def distance(z, x, y):
        x0, y0, x1, y1 = z
        dx = max(x0 - x, x - x1, 0)
        dy = max(y0 - y, y - y1, 0)
        return max(dx, dy)

    def nearest(x, y):
        for name in LANDMARKS:
            if distance(L['reach'][name], x, y) <= L['dist'][name]:
                return name
        return None

    seen = flood(L['SPAWN'])
    for name in LANDMARKS:
        assert any(nearest(x, y) == name for (x, y) in seen), ('unreachable', name)
    assert nearest(*L['SPAWN']) == 'Wire', nearest(*L['SPAWN'])
    assert nearest(*L['OPEN']) is None, nearest(*L['OPEN'])


# ===================================================================== output
RUST_TEMPLATE = '''//! The night city street: static art plus the metadata the runtime needs
//! (collision, landmarks, animation cells). Generated by
//! `scripts/gen_city_map.py --write`; do not hand-edit the literal or the
//! zone constants, tweak the script and re-run.
//!
//! GAME.md, "The night city": the wallet. Shops, the armorer, repairs, the
//! board, the tailor, all on one rainy street with a screen tuned to a
//! dead channel at the end of it. One glyph per person (your runner's
//! mark). Single-width glyphs only. `STYLE` says which register this map
//! is in; `ui.rs` branches on it where the two differ.

#[rustfmt::skip]
pub const MAP_W: u16 = __W__;
#[rustfmt::skip]
pub const MAP_H: u16 = __H__;

/// Which register the map was generated in.
#[rustfmt::skip]
pub const STYLE: MapStyle = MapStyle::__STYLE__;

#[rustfmt::skip]
pub const MAP: [&str; MAP_H as usize] = [
__MAP__
];

/// Cells nothing walks through, one byte per cell, row-major: `#` solid,
/// `.` open.
#[rustfmt::skip]
const SOLID: [&str; MAP_H as usize] = [
__SOLID__
];

/// Where your runner appears: on the stairs up from the wire.
#[rustfmt::skip]
pub const SPAWN: (u16, u16) = __SPAWN__;
/// An open street cell with nothing within reach (for the tests).
#[rustfmt::skip]
pub const OPEN: (u16, u16) = __OPEN__;

/// The street sign on the top wall.
#[rustfmt::skip]
pub const TITLE: Zone = __TITLE__;
/// The far skyline, when the register has one: rain falls across it.
#[rustfmt::skip]
pub const SKY: Option<Zone> = __SKY__;
/// The antenna mast's light, when the register has one.
#[rustfmt::skip]
pub const MAST_LIGHT: Option<(u16, u16)> = __MAST_LIGHT__;
/// The street band: everything walkable lies in it, plus the stalls.
#[rustfmt::skip]
pub const STREET: Zone = __STREET__;
/// The railing over the drop.
#[rustfmt::skip]
pub const RAIL_Y: u16 = __RAIL_Y__;
/// The lower city, far below the railing: lights only.
#[rustfmt::skip]
pub const DROP: Zone = __DROP__;
/// The screen's face: static, and now and then the test pattern.
#[rustfmt::skip]
pub const SCREEN_FACE: Zone = __SCREEN_FACE__;
/// The bits machine's display (drawn) or the machine itself (tiles).
#[rustfmt::skip]
pub const BITS_SCREEN: Zone = __BITS_SCREEN__;
/// The board kiosk, and its title row when it has one.
#[rustfmt::skip]
pub const BOARD: Zone = __BOARD__;
#[rustfmt::skip]
pub const BOARD_SIGN: Option<Zone> = __BOARD_SIGN__;
/// The stairs down to the lower city: their signs (drawn) or the tile.
#[rustfmt::skip]
pub const STAIRS_SIGNS: &[Zone] = &[__STAIRS_SIGNS__];
/// The stairs up to the wire (the way out), and their sign when drawn.
#[rustfmt::skip]
pub const WIRE: Zone = __WIRE__;
#[rustfmt::skip]
pub const WIRE_SIGN: Option<Zone> = __WIRE_SIGN__;
/// The one dead letter on the lockers' sign: always dark.
#[rustfmt::skip]
pub const DEAD_LETTER: (u16, u16) = __DEAD_LETTER__;

/// A neon sign: the letters, and the color they burn.
#[derive(Debug, Clone, Copy)]
pub struct Sign {
    pub zone: Zone,
    pub color: Neon,
}

/// A building: its footprint, and the neon its walls take (`None` for
/// plain concrete). Tiles only; empty when drawn.
#[derive(Debug, Clone, Copy)]
pub struct Building {
    pub zone: Zone,
    pub color: Option<Neon>,
}

/// The shop names in neon.
#[rustfmt::skip]
pub const SIGNS: &[Sign] = &[
__SIGNS__
];

/// Signs and tags in the city's script (the glyph alphabet).
#[rustfmt::skip]
pub const BANNERS: &[Sign] = &[
__BANNERS__
];

/// The carts and stalls: their name row (drawn) or their vendor (tiles).
#[rustfmt::skip]
pub const CART_SIGNS: &[Sign] = &[
__CART_SIGNS__
];

/// The awnings hanging over the sidewalk, one per facade (drawn only).
#[rustfmt::skip]
pub const AWNINGS: &[Zone] = &[
__AWNINGS__
];

/// Where windows flicker: window fields (drawn) or whole buildings (tiles).
#[rustfmt::skip]
pub const WINDOWS: &[Zone] = &[
__WINDOWS__
];

/// The buildings, for wall colors (tiles only).
#[rustfmt::skip]
pub const BUILDINGS: &[Building] = &[
__BUILDINGS__
];

/// Steam rises from these cells (vents, grates, the noodle bowls).
#[rustfmt::skip]
pub const VENTS: &[(u16, u16)] = &[__VENTS__];

/// Puddle cells: they catch the neon.
#[rustfmt::skip]
pub const PUDDLES: &[(u16, u16)] = &[__PUDDLES__];

/// Street lamps: a pool of light around each.
#[rustfmt::skip]
pub const LAMPS: &[(u16, u16)] = &[__LAMPS__];

/// The lower city's lights, seen from the railing.
#[rustfmt::skip]
pub const DROP_LIGHTS: &[(u16, u16)] = &[__DROP_LIGHTS__];

/// The two registers the generator knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapStyle {
    /// Front-on facades and carts under a skyline.
    Drawn,
    /// Top-down, one tile per thing.
    Tiles,
}

/// The closed neon palette. Mapped onto the theme in `ui.rs`, never to raw
/// colors, so the city follows whatever palette the person picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Neon {
    Cyan,
    Magenta,
    Red,
    Amber,
    Green,
    White,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zone {
    pub x0: u16,
    pub y0: u16,
    pub x1: u16,
    pub y1: u16,
}

impl Zone {
    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }

    /// Chebyshev distance from a point to this rectangle (0 when inside).
    pub fn distance(&self, x: u16, y: u16) -> u16 {
        let dx = self.x0.saturating_sub(x).max(x.saturating_sub(self.x1));
        let dy = self.y0.saturating_sub(y).max(y.saturating_sub(self.y1));
        dx.max(dy)
    }
}

/// Everything on the street you can walk up to, in popover priority order.
/// The shops are the wallet (GAME.md); the rest is the street.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Landmark {
    Armorer,
    Tailor,
    Lockers,
    Bands,
    Bar,
    Screen,
    Repairs,
    Board,
    Bits,
    Noodles,
    Umbrellas,
    Blades,
    Reader,
    Stairs,
    Wire,
}

impl Landmark {
    /// Every landmark, in popover priority order.
    pub const ALL: [Landmark; 15] = [
        Landmark::Armorer,
        Landmark::Tailor,
        Landmark::Lockers,
        Landmark::Bands,
        Landmark::Bar,
        Landmark::Screen,
        Landmark::Repairs,
        Landmark::Board,
        Landmark::Bits,
        Landmark::Noodles,
        Landmark::Umbrellas,
        Landmark::Blades,
        Landmark::Reader,
        Landmark::Stairs,
        Landmark::Wire,
    ];

    /// The cells that put the player within reach: in front of a counter,
    /// around a stall, on the stairs.
    #[rustfmt::skip]
    pub fn reach(self) -> Zone {
        match self {
__REACH__
        }
    }

    /// How close the player has to stand.
    #[rustfmt::skip]
    fn reach_distance(self) -> u16 {
        match self {
__DIST__
        }
    }
}

/// The landmark the player is close enough to, if any.
pub fn nearest_landmark(x: u16, y: u16) -> Option<Landmark> {
    Landmark::ALL
        .into_iter()
        .find(|landmark| landmark.reach().distance(x, y) <= landmark.reach_distance())
}

/// The street as a padded char grid, decoded once per process.
pub fn grid() -> &'static [Vec<char>] {
    static GRID: std::sync::OnceLock<Vec<Vec<char>>> = std::sync::OnceLock::new();
    GRID.get_or_init(|| {
        MAP.iter()
            .map(|row| {
                let mut cells: Vec<char> = row.chars().collect();
                cells.resize(MAP_W as usize, ' ');
                cells
            })
            .collect()
    })
}

/// The map char at `(x, y)`; rows shorter than `MAP_W` read as open street.
pub fn char_at(x: u16, y: u16) -> char {
    if x >= MAP_W || y >= MAP_H {
        return ' ';
    }
    grid()[y as usize][x as usize]
}

/// Whether a runner can stand on `(x, y)`: the solid bitmap says no for
/// walls, counters, stalls, the railing and the drop.
pub fn walkable(x: u16, y: u16) -> bool {
    if x == 0 || y == 0 || x >= MAP_W - 1 || y >= MAP_H - 1 {
        return false;
    }
    SOLID[y as usize].as_bytes()[x as usize] == b'.'
}

#[cfg(test)]
#[path = "map_test.rs"]
mod map_test;
'''


def zone_lit(z):
    x0, y0, x1, y1 = z
    return f'Zone {{ x0: {x0}, y0: {y0}, x1: {x1}, y1: {y1} }}'


def opt(v, lit):
    return 'None' if v is None else f'Some({lit(v)})'


def pair(p):
    return f'({p[0]}, {p[1]})'


def cells_lit(cells):
    return ', '.join(pair(c) for c in cells)


def sign_lits(signs):
    return '\n'.join(f'    Sign {{ zone: {zone_lit(z)}, color: Neon::{c} }},' for (z, c) in signs)


def emit(L):
    import os

    def esc(s):
        return s.replace('\\', '\\\\').replace('"', '\\"')

    map_lines = ',\n'.join('    "' + esc(''.join(row).rstrip()) + '"' for row in grid)
    solid_lines = ',\n'.join(
        '    "' + ''.join('#' if s else '.' for s in row) + '"' for row in solid
    )
    reach_arms = '\n'.join(f'            Landmark::{k} => {zone_lit(L["reach"][k])},' for k in LANDMARKS)
    dist_arms = '\n'.join(f'            Landmark::{k} => {L["dist"][k]},' for k in LANDMARKS)
    building_lits = '\n'.join(
        f'    Building {{ zone: {zone_lit(z)}, color: {"None" if c is None else "Some(Neon::" + c + ")"} }},'
        for (z, c) in L['BUILDINGS']
    )
    out = (RUST_TEMPLATE
           .replace('__W__', str(W)).replace('__H__', str(H))
           .replace('__STYLE__', L['STYLE'])
           .replace('__MAP__', map_lines).replace('__SOLID__', solid_lines)
           .replace('__SPAWN__', pair(L['SPAWN'])).replace('__OPEN__', pair(L['OPEN']))
           .replace('__TITLE__', zone_lit(L['TITLE']))
           .replace('__SKY__', opt(L['SKY'], zone_lit))
           .replace('__MAST_LIGHT__', opt(L['MAST_LIGHT'], pair))
           .replace('__STREET__', zone_lit(L['STREET']))
           .replace('__RAIL_Y__', str(L['RAIL_Y']))
           .replace('__DROP__', zone_lit(L['DROP']))
           .replace('__SCREEN_FACE__', zone_lit(L['SCREEN_FACE']))
           .replace('__BITS_SCREEN__', zone_lit(L['BITS_SCREEN']))
           .replace('__BOARD_SIGN__', opt(L['BOARD_SIGN'], zone_lit))
           .replace('__BOARD__', zone_lit(L['BOARD']))
           .replace('__STAIRS_SIGNS__', ', '.join(zone_lit(z) for z in L['STAIRS_SIGNS']))
           .replace('__WIRE_SIGN__', opt(L['WIRE_SIGN'], zone_lit))
           .replace('__WIRE__', zone_lit(L['WIRE']))
           .replace('__DEAD_LETTER__', pair(L['DEAD_LETTER']))
           .replace('__SIGNS__', sign_lits(L['SIGNS']))
           .replace('__BANNERS__', sign_lits(L['BANNERS']))
           .replace('__CART_SIGNS__', sign_lits(L['CART_SIGNS']))
           .replace('__AWNINGS__', '\n'.join(f'    {zone_lit(z)},' for z in L['AWNINGS']))
           .replace('__WINDOWS__', '\n'.join(f'    {zone_lit(z)},' for z in L['WINDOWS']))
           .replace('__BUILDINGS__', building_lits)
           .replace('__VENTS__', cells_lit(L['VENTS']))
           .replace('__PUDDLES__', cells_lit(L['PUDDLES']))
           .replace('__LAMPS__', cells_lit(L['LAMPS']))
           .replace('__DROP_LIGHTS__', cells_lit(L['DROP_LIGHTS']))
           .replace('__REACH__', reach_arms)
           .replace('__DIST__', dist_arms))
    path = os.path.join(os.path.dirname(__file__), '..', 'late-ssh', 'src', 'app', 'deadchannel', 'city', 'map.rs')
    with open(path, 'w') as f:
        f.write(out)
    print(f'wrote {os.path.normpath(path)} ({L["STYLE"].lower()})')


def main():
    style = 'tiles'
    if '--style' in sys.argv:
        style = sys.argv[sys.argv.index('--style') + 1]
    if style == 'tiles':
        L = layout_tiles()
    elif style == 'drawn':
        L = layout_drawn()
    else:
        sys.exit(f'unknown style {style!r}: tiles or drawn')
    validate(L)
    if '--print' in sys.argv:
        for row in grid:
            print(''.join(row))
        return
    if '--write' in sys.argv:
        emit(L)
        return
    sys.exit('pass --print or --write')


if __name__ == '__main__':
    main()
