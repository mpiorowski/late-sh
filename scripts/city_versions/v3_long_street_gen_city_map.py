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
    L['WALKERS'] = []
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
    """Top-down, one tile per thing. A long side street in three legs with
    a dogleg between each, alleys off it north and south, a back lane
    behind the second leg, a canal under the first two legs with a bank of
    warehouses beyond it, rooms you walk into, a ledge over the drop along
    the third leg, and the screen closing the street at the east end."""
    new_grid(440, 44, all_solid=True)
    L = {'STYLE': 'Tiles'}
    L['TITLE'] = frame(TITLE)
    rng = random.Random(3)

    neon_signs = []   # (zone, color): the shop names in the street-facing walls
    tags = []         # (zone, color): glyph-script graffiti on the walls
    buildings = []    # (zone, color or None): walls take the color
    vents = []        # steam rises off these
    puddles = []
    lamps = []
    walkers = []      # (x0, x1, y, glyph, period, phase): pacing a floor path
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

    def grate(x, y):
        tile(x, y, '≡', block=False)
        vents.append((x, y))

    def stall(name, x, y, row, vendor_x, color):
        """A counter of `=` with the goods on it and the vendor beside it."""
        put(x, y, row)
        tile(vendor_x, y, '@')
        x0, x1 = min(x, vendor_x), max(x + len(row) - 1, vendor_x)
        stalls[name] = (zone(x0, y, x1, y), zone(vendor_x, y, vendor_x, y), color)
        reach[name] = zone(x0, y, x1, y)
        dist[name] = 1

    def walker(x0, x1, y, glyph, period):
        walkers.append((x0, x1, y, glyph, period, rng.randrange(2 * (x1 - x0))))

    def tenement(x0, y0, x1, y1, door_y, windows_y, sleepers=1):
        """A block of rooms nobody sells anything in: a corridor along the
        street wall, a partition with a door per room, beds, a cabinet or
        two, someone asleep. The corridor is on the street side."""
        south_facing = door_y == y1
        room(x0, y0, x1, y1, None)
        w = x1 - x0 - 1
        door_x = x0 + 1 + rng.randrange(max(1, w - 2)) + 1
        door_x = min(max(door_x, x0 + 2), x1 - 2)
        tile(door_x, door_y, '+', block=False)
        for wx in range(x0 + 3, x1 - 2, 6):
            if wx != door_x and abs(wx - door_x) > 1:
                tile(wx, windows_y, '╬')
        # the partition two rows in from the street wall
        part_y = y1 - 2 if south_facing else y0 + 2
        rooms_y0, rooms_y1 = (y0 + 1, part_y - 1) if south_facing else (part_y + 1, y1 - 1)
        if rooms_y1 - rooms_y0 < 1:
            return
        cols = list(range(x0 + 1, x1, 6))
        doors = [min(c + 3, x1 - 2) for c in cols]
        wall(x0 + 1, part_y, x1 - 1, part_y, doors=[(d, part_y) for d in doors if x0 < d < x1])
        for c in cols[1:]:
            wall(c, rooms_y0, c, rooms_y1)
        for k, c in enumerate(cols):
            bx = c + 1
            if bx >= x1 - 1:
                continue
            by = rooms_y0 + (rooms_y1 - rooms_y0) // 2
            tile(bx, by, '▬')
            if k % 3 == 1 and bx + 2 < x1 - 1:
                tile(bx + 2, rooms_y0, '∩')
            if sleepers > 0 and k % 2 == 1:
                tile(bx, min(by + 1, rooms_y1), '@')
                sleepers -= 1

    def alley(x0, x1, y0, y1, back=True):
        """A gap between buildings: floor, something at the dead end, a
        grate, a lantern or two, sometimes a rat."""
        carve(x0, y0, x1, y1)
        if back:
            tile(x0, y0, '▒')
            if x1 > x0:
                tile(x1, y0, rng.choice('▒▪'))
        mid = (x0 + x1) // 2
        span = y1 - y0
        if span >= 5:
            grate(mid, y0 + span // 2)
        if span >= 3:
            lantern(mid, y0 + 2)
        if span >= 8:
            lantern(mid, y1 - 2)
        r = rng.random()
        if r < 0.35:
            tile(x0, y0 + 1, 'r')
        elif r < 0.6:
            tile(x1, y0 + 1, 'c')

    # ------------------------------------------------------------ the street
    # Leg one runs west to east at rows 17-20; leg two drops two rows at
    # x=128; leg three climbs three rows at x=270 and runs along the ledge
    # to the yard under the screen.
    carve(1, 17, 130, 20)
    carve(128, 19, 272, 22)
    carve(270, 16, 400, 19)
    carve(270, 16, 272, 22)
    carve(401, 14, 430, 19)
    L['STREET'] = zone(1, 14, 430, 22)

    # =========================================================== leg one
    # ---- north side (south walls at row 16)
    tenement(2, 9, 18, 16, door_y=16, windows_y=16)
    tag(3, 16, '▚', MAGENTA)
    tile(2, 18, '>', block=False)
    L['STAIRS_SIGNS'] = [zone(2, 18, 2, 18)]
    reach['Stairs'] = zone(2, 18, 2, 18); dist['Stairs'] = 0

    alley(19, 21, 5, 16)
    tile(21, 6, 'c')

    room(22, 10, 36, 16, RED, doors=[(33, 16)], windows=[(35, 16), (27, 10)],
         sign=(24, 16, 'ARMORER'))
    for gx, g in ((24, ')'), (26, ')'), (28, '/'), (32, '['), (34, '[')):
        tile(gx, 11, g)
    put(24, 13, '=' * 11)
    tile(29, 12, '@')
    reach['Armorer'] = zone(24, 14, 34, 14); dist['Armorer'] = 0

    carve(37, 12, 37, 16); grate(37, 13)

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

    alley(53, 54, 6, 16)

    room(55, 11, 69, 16, CYAN, doors=[(66, 16)], windows=[(68, 16)],
         sign=(57, 16, 'LOCKERS'))
    for lx in range(56, 69):
        tile(lx, 12, '∩'); tile(lx, 14, '∩')
    reach['Lockers'] = zone(56, 13, 68, 15); dist['Lockers'] = 0
    L['DEAD_LETTER'] = (61, 16)   # the E

    # a long alley north to a hidden court: a shrine, a plant, a cat
    carve(70, 8, 71, 16)
    carve(64, 3, 78, 7)
    tile(68, 5, '♣'); tile(74, 4, '_'); tile(78, 7, '▒')
    for lx in (65, 69, 73, 77):
        lantern(lx, 3)
    lantern(70, 12); grate(71, 9)
    walker(65, 77, 6, 'c', 5)

    tenement(72, 7, 90, 16, door_y=16, windows_y=16, sleepers=2)
    carve(91, 13, 91, 16); grate(91, 14)

    # a clinic: cots, bottles, someone on duty. Nothing to do here.
    room(92, 10, 108, 16, WHITE, doors=[(105, 16)], windows=[(94, 16), (100, 10)],
         sign=(94, 16, 'CLINIC'))
    for bx in (94, 97, 100):
        tile(bx, 12, '▬')
    for gx in (103, 105, 107):
        tile(gx, 11, '!')
    tile(105, 13, '@')

    alley(109, 111, 4, 16)
    tenement(112, 10, 130, 16, door_y=16, windows_y=16)

    # ---- south side (north walls at row 21)
    tenement(2, 21, 28, 32, door_y=21, windows_y=21, sleepers=2)
    alley(29, 31, 21, 32, back=False)
    stall('Noodles', 34, 20, '=%=%=', 33, AMBER)
    vents.append((35, 20)); vents.append((37, 20))

    room(32, 21, 58, 32, None, doors=[(45, 21)])
    for (x0, y0, x1, y1) in ((34, 28, 36, 31), (40, 25, 42, 26), (50, 29, 54, 31), (55, 23, 57, 24)):
        for y in range(y0, y1 + 1):
            put(x0, y, '▪' * (x1 - x0 + 1))
    tile(45, 23, '@'); grate(50, 24)
    tag(33, 21, '▞', CYAN)

    stall('Umbrellas', 62, 20, '=T=T=', 67, CYAN)
    room(59, 21, 69, 32, RED, sign=(62, 21, 'PAWN'), shutter=(67, 21))
    for gx, g in ((61, '$'), (63, ')'), (65, '"'), (67, '[')):
        tile(gx, 23, g)
    put(61, 25, '=' * 7)

    alley(70, 71, 21, 32, back=False)
    tenement(72, 21, 100, 32, door_y=21, windows_y=21, sleepers=2)
    alley(101, 103, 21, 32, back=False)

    # the baths: a pool inside, steam, a few people. Nothing to do here.
    room(104, 21, 127, 32, CYAN, doors=[(110, 21)], windows=[(115, 21), (122, 21)],
         sign=(112, 21, 'BATHS'))
    for y in range(24, 30):
        put(108, y, '≈' * 14)
    for (px, py) in ((106, 23), (124, 26), (107, 30), (123, 31)):
        tile(px, py, '@')
    vents.append((110, 26)); vents.append((118, 28))

    # =========================================================== leg two
    # ---- north side (south walls at row 18)
    room(131, 11, 146, 18, GREEN, doors=[(142, 18)], windows=[(133, 18), (144, 18), (138, 11)],
         sign=(134, 18, 'BANDS'))
    for gx in (134, 138, 142):
        tile(gx, 12, 'o')
    for gx in (133, 136, 139, 142, 145):
        tile(gx, 13, 'Y')
    put(133, 15, '=' * 12)
    tile(138, 14, '@')
    reach['Bands'] = zone(133, 16, 144, 16); dist['Bands'] = 0

    carve(147, 13, 147, 18); grate(147, 15)

    room(148, 12, 168, 18, AMBER, doors=[(163, 18)],
         windows=[(150, 18), (166, 18), (154, 12), (162, 12)], sign=(151, 18, 'DEAD AIR'))
    for gx in (152, 154, 156, 160, 162, 164):
        tile(gx, 13, '!')
    tile(158, 13, '@')
    put(151, 14, '=' * 15)
    for gx in (152, 155, 158, 161, 164):
        tile(gx, 15, 'o')
    tile(150, 16, '♪')
    reach['Bar'] = zone(151, 15, 165, 16); dist['Bar'] = 0

    alley(169, 170, 9, 18)

    room(171, 13, 181, 18, WHITE, doors=[(179, 18)], windows=[(172, 18)],
         sign=(173, 18, 'PATCH'))
    for gx, g in ((173, '/'), (175, '\\'), (177, 'x'), (179, '/')):
        tile(gx, 14, g)
    tile(176, 15, '@')
    put(173, 16, '=' * 7)
    reach['Repairs'] = zone(173, 17, 179, 17); dist['Repairs'] = 0

    carve(182, 15, 182, 18); tile(182, 16, 'r')

    room(183, 10, 195, 18, CYAN, doors=[(193, 18)], windows=[(184, 18), (185, 10), (193, 10)],
         sign=(186, 18, 'SLEEP'))
    wall(184, 16, 194, 16, doors=[(186, 16), (189, 16), (192, 16)])
    wall(187, 11, 187, 15); wall(191, 11, 191, 15)
    for bx in (185, 189, 193):
        tile(bx, 12, '▬')
    tile(185, 13, '@')

    # the back lane: two rows behind the next three buildings, reached
    # from the alleys either end and their back doors
    carve(196, 5, 262, 6)
    alley(196, 198, 7, 18, back=False)
    for lx in range(202, 260, 8):
        lantern(lx, 5)
    grate(230, 6)
    walker(200, 258, 5, '@', 4)

    # the arcade: cabinets in rows, a change machine, two people playing
    room(199, 7, 217, 18, MAGENTA, doors=[(212, 18), (208, 7)], windows=[(202, 18), (215, 18)],
         sign=(202, 18, 'COIN'))
    for y in (10, 13):
        for gx in range(201, 216, 3):
            tile(gx, y, '▓')
    tile(203, 11, '@'); tile(212, 14, '@'); tile(215, 9, '$')

    carve(218, 14, 218, 18); grate(218, 16)

    # a shrine: an altar, candles, one plant, nobody
    room(219, 9, 237, 18, AMBER, doors=[(228, 18)], windows=[(221, 18), (235, 18)],
         sign=(222, 18, 'SHRINE'))
    tile(228, 11, '_')
    for gx in (225, 227, 229, 231):
        tile(gx, 12, '°')
    tile(221, 11, '♣'); tile(235, 11, '♣')
    tile(228, 15, '@')

    alley(238, 239, 7, 18, back=False)

    # a market hall: counters of food and drink, three sellers, two doors
    room(240, 9, 259, 18, GREEN, doors=[(244, 18), (255, 18), (250, 9)], windows=[(250, 18)],
         sign=(242, 18, 'MARKET'))
    for y in (12, 15):
        put(242, y, '=%=!=%=')
        put(251, y, '="=%=!=')
    tile(245, 11, '@'); tile(254, 11, '@'); tile(245, 14, '@')
    vents.append((243, 12)); vents.append((253, 15))

    alley(260, 262, 7, 18, back=False)
    tenement(263, 10, 269, 18, door_y=18, windows_y=18, sleepers=0)

    # ---- south side (north walls at row 23)
    tenement(128, 23, 160, 32, door_y=23, windows_y=23, sleepers=3)
    tile(143, 22, '?')
    L['BOARD'] = zone(143, 22, 143, 22)
    L['BOARD_SIGN'] = None
    reach['Board'] = zone(143, 22, 143, 22); dist['Board'] = 1

    alley(161, 162, 23, 32, back=False)

    # a garage: the roll door down, a wreck inside, crates
    room(163, 23, 200, 32, None, doors=[(196, 23)])
    put(178, 23, '▓▓▓')
    put(172, 27, '▬▬▬')
    for (x0, y0, x1, y1) in ((165, 29, 168, 31), (190, 25, 194, 26)):
        for y in range(y0, y1 + 1):
            put(x0, y, '▪' * (x1 - x0 + 1))
    tile(185, 28, '@'); grate(180, 30)
    tag(164, 23, '▚', GREEN)

    stall('Blades', 205, 22, '=)=)=', 210, RED)
    tenement(201, 23, 230, 32, door_y=23, windows_y=23, sleepers=2)
    alley(231, 232, 23, 32, back=False)

    # a dock: pallets, a forklift-sized gap, nobody about
    room(233, 23, 269, 32, None, doors=[(240, 23), (262, 23)])
    for (x0, y0, x1, y1) in ((236, 27, 240, 29), (245, 25, 249, 26), (255, 28, 260, 31), (263, 25, 266, 26)):
        for y in range(y0, y1 + 1):
            put(x0, y, '▪' * (x1 - x0 + 1))
    grate(252, 30)

    # ---- the canal under legs one and two: a walkway, water, a bank
    carve(1, 33, 270, 33)
    for y in (34, 35, 36):
        put(1, y, '≈' * 270)
    carve(1, 37, 270, 37)
    for bx in (60, 61, 190, 191):
        for y in (34, 35, 36):
            tile(bx, y, '=', block=False)
    for lx in range(8, 270, 24):
        lantern(lx, 33)
    tile(120, 33, '▒'); tile(200, 37, '▒')
    walker(40, 118, 33, 'r', 3)
    walker(125, 260, 33, '@', 5)
    walker(10, 180, 37, '@', 6)
    # the far bank: long warehouses, few doors
    room(2, 38, 90, 42, None, doors=[(20, 38), (61, 38)])
    room(91, 38, 180, 42, None, doors=[(140, 38)])
    room(181, 38, 270, 42, None, doors=[(191, 38), (250, 38)])
    for (x0, y0, x1, y1) in ((30, 40, 40, 41), (100, 39, 112, 40), (160, 40, 170, 41), (220, 39, 240, 41)):
        for y in range(y0, y1 + 1):
            put(x0, y, '▪' * (x1 - x0 + 1))
    tile(75, 40, '@'); tile(205, 41, '@')

    # ========================================================= leg three
    # ---- north side (south walls at row 15)
    tenement(273, 4, 290, 15, door_y=15, windows_y=15, sleepers=3)
    carve(291, 12, 291, 15); grate(291, 13)

    # a chop shop: tools, parts, a man under a lamp
    room(292, 9, 310, 15, GREEN, doors=[(306, 15)], windows=[(294, 15), (300, 9)],
         sign=(295, 15, 'TEK'))
    for gx, g in ((294, '/'), (296, 'x'), (298, '\\'), (300, 'x')):
        tile(gx, 10, g)
    for (x0, y0, x1, y1) in ((303, 10, 308, 11),):
        for y in range(y0, y1 + 1):
            put(x0, y, '▪' * (x1 - x0 + 1))
    put(294, 12, '=' * 9)
    tile(298, 11, '@'); tile(302, 13, '*'); lamps.append((302, 13))

    alley(311, 313, 6, 15)

    # a video store: shelves, one clerk, one browser
    room(314, 8, 334, 15, MAGENTA, doors=[(330, 15)], windows=[(316, 15), (324, 15), (320, 8)],
         sign=(317, 15, 'VIDS'))
    for y in (10, 12):
        for gx in range(316, 332, 2):
            tile(gx, y, '"')
    tile(332, 13, '@'); tile(320, 13, '@')

    carve(335, 11, 335, 15); grate(335, 12)
    tenement(336, 8, 356, 15, door_y=15, windows_y=15, sleepers=2)
    alley(357, 359, 4, 15)

    # an aerial lot: masts behind a fence, a gate, the hum
    room(360, 8, 380, 15, GREEN, doors=[(370, 15)], sign=(363, 15, 'AERIAL'))
    for gx in range(362, 380, 4):
        tile(gx, 10, 'Y'); tile(gx + 2, 13, 'Y')
    grate(370, 12)

    carve(381, 12, 381, 15); tile(381, 13, 'r')
    tenement(382, 10, 400, 15, door_y=15, windows_y=15)

    # ---- the ledge: the railing along the south of leg three, the drop
    for rx in range(285, 431):
        tile(rx, 20, '╪' if rx % 8 == 0 else '═')
    tile(300, 20, '>', block=False)
    L['RAIL_Y'] = 20
    L['WIRE'] = zone(299, 20, 301, 20)
    L['WIRE_SIGN'] = None
    L['SPAWN'] = (300, 20)
    reach['Wire'] = zone(300, 20, 300, 20); dist['Wire'] = 0
    L['DROP'] = zone(285, 21, W - 2, H - 2)
    drop_lights = []
    drng = random.Random(7)
    for y in range(21, H - 1):
        for lx in range(285, W - 1):
            if drng.random() < 0.05:
                grid[y][lx] = drng.choice('·∙·▪·')
                drop_lights.append((lx, y))
    L['DROP_LIGHTS'] = drop_lights
    # the corner west of the railing: a dumpster and a stack of crates
    tile(273, 20, '▒'); tile(274, 20, '▒'); put(276, 20, '▪▪▪')

    # ---- the yard under the screen
    wall(401, 13, 430, 13)
    carve(410, 8, 411, 12); tile(410, 8, '▒'); lantern(411, 10); grate(411, 11)
    for lx in (404, 412, 420, 428):
        lantern(lx, 14)
    tile(403, 15, '$')
    L['BITS_SCREEN'] = zone(403, 15, 403, 15)
    reach['Bits'] = zone(403, 15, 403, 15); dist['Bits'] = 1
    put(420, 18, '===')
    tile(421, 17, '@'); tile(420, 17, '°'); tile(422, 17, '°')
    stalls['Reader'] = (zone(420, 17, 422, 18), zone(421, 17, 421, 17), MAGENTA)
    reach['Reader'] = zone(420, 17, 422, 18); dist['Reader'] = 1
    put(407, 19, '==='); tile(410, 19, '▪')   # a cart nobody came back for
    put(431, 12, '###'); put(431, 21, '###')
    for y in range(13, 21):
        put(431, y, '░░░')
    L['SCREEN_FACE'] = zone(431, 13, 433, 20)
    reach['Screen'] = zone(430, 14, 430, 19); dist['Screen'] = 0
    L['SKY'] = None
    L['MAST_LIGHT'] = None
    put((W - len('╡ the wire ╞')) // 2, H - 1, '╡ the wire ╞')

    # --------------------------------------------------------------- lamps
    for (lx, ly) in ((12, 17), (46, 17), (86, 17), (120, 17), (30, 20), (100, 20),
                     (150, 22), (200, 22), (250, 22), (175, 19), (225, 19),
                     (285, 16), (330, 16), (375, 16), (405, 19), (426, 14)):
        lamp(lx, ly)

    # ------------------------------------------------------------- walkers
    walker(5, 60, 18, '@', 4)
    walker(66, 124, 18, '@', 5)
    walker(30, 110, 19, '@', 6)
    walker(133, 200, 20, '@', 4)
    walker(210, 268, 21, '@', 5)
    walker(155, 245, 20, 'c', 7)
    walker(275, 395, 17, '@', 4)
    walker(290, 360, 18, '@', 6)
    walker(340, 398, 17, 'c', 5)
    walker(403, 428, 16, 'r', 3)

    # -------------------------------------------------------------- puddles
    def puddle(px, py, ch):
        if grid[py][px] == ' ' and not solid[py][px]:
            grid[py][px] = ch
            puddles.append((px, py))

    for _ in range(90):
        px = rng.randrange(2, W - 6)
        py = rng.randrange(2, H - 2)
        n = rng.randrange(1, 5)
        for i in range(n):
            puddle(px + i, py, '≈' if i % 2 == 0 else '~')
        if n >= 4:
            puddle(px + 1, py + 1, '~')

    # ------------------------------------------------------------- texture
    # Wet ground: a scatter of `.` `,` and grit on every open tile.
    trng = random.Random(11)
    for y in range(1, H - 1):
        for x in range(1, W - 1):
            if solid[y][x] or grid[y][x] != ' ':
                continue
            r = trng.random()
            if r < 0.22:
                grid[y][x] = '.'
            elif r < 0.27:
                grid[y][x] = ','
            elif r < 0.29:
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
    L['WALKERS'] = walkers
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

    for (x0, x1, y, glyph, _period, _phase) in L['WALKERS']:
        for x in range(x0, x1 + 1):
            assert walkable(x, y), ('walker path blocked', glyph, x, y)

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

/// Someone pacing a stretch of open floor, back and forth: people, cats,
/// rats. Drawn by `ui.rs` as a pure function of the tick, so they need
/// no state and never stand on anything solid (the generator checks the
/// whole path).
#[derive(Debug, Clone, Copy)]
pub struct Walker {
    pub x0: u16,
    pub x1: u16,
    pub y: u16,
    pub glyph: char,
    /// Ticks per step.
    pub period: u64,
    pub phase: u64,
}

#[rustfmt::skip]
pub const WALKERS: &[Walker] = &[
__WALKERS__
];

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
           .replace('__WALKERS__', '\n'.join(
               f"    Walker {{ x0: {x0}, x1: {x1}, y: {y}, glyph: '{g}', period: {p}, phase: {ph} }},"
               for (x0, x1, y, g, p, ph) in L['WALKERS']))
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
