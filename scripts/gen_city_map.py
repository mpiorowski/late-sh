#!/usr/bin/env python3
"""Generate the deadchannel night city street (late-ssh/src/app/deadchannel/city/map.rs).

The city is authored here as prop stamps on a grid, validated (row widths,
single-width glyphs only, every landmark reachable from the spawn), and
written out as the plain Rust literal that gets committed. Nobody should
hand-edit the 232-column MAP strings; tweak this script and re-run:

    python3 scripts/gen_city_map.py --print   # preview in the terminal
    python3 scripts/gen_city_map.py --write   # regenerate map.rs

Every zone constant in map.rs is emitted from the numbers below, so moving a
prop here moves its zone, its popover reach, and its animation cells in one
step. The renderer (`ui.rs`) and the tests read those constants only.

The register (GAME.md, "Theme"): the neon undercity inside the machine.
Blade Runner streets, machine substance. Rain that falls as static, signs
written in the glyph alphabet nobody can read, a screen tuned to a dead
channel at the end of the street. One glyph per person (your runner's mark),
multi-cell buildings, carts and stalls: the Rangedrifter density, a tighter
zoom than the clubhouse's three-row figures, so the city reads as big.
"""

import sys
import unicodedata
from collections import deque

W, H = 232, 52
grid = [[' '] * W for _ in range(H)]
# Cells a prop claims: nothing walks through a cart or a wall.
solid = [[False] * W for _ in range(H)]


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


# ---------------------------------------------------------------- frame
grid[0] = list('═' * W); grid[0][0] = '╔'; grid[0][W - 1] = '╗'
grid[H - 1] = list('═' * W); grid[H - 1][0] = '╚'; grid[H - 1][W - 1] = '╝'
for y in range(H):
    solid[y][0] = solid[y][W - 1] = True
for x in range(W):
    solid[0][x] = solid[H - 1][x] = True
for y in range(1, H - 1):
    grid[y][0] = '║'; grid[y][W - 1] = '║'
SIGN = '╡ ▚ STATIC ROW ▞ ╞'
SIGN_X = (W - len(SIGN)) // 2
put(SIGN_X, 0, SIGN)
TITLE_ZONE = zone(SIGN_X, 0, SIGN_X + len(SIGN) - 1, 0)

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
import random
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

# ---------------------------------------------------------------- validate
for y, row in enumerate(grid):
    assert len(row) == W, (y, len(row))
    for x, ch in enumerate(row):
        assert unicodedata.east_asian_width(ch) not in ('W', 'F'), (x, y, ch)
        assert not unicodedata.combining(ch), (x, y, ch)


def walkable(x, y):
    return 0 < x < W - 1 and 0 < y < H - 1 and not solid[y][x]


assert walkable(*SPAWN), SPAWN


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


reach = flood(SPAWN)
for name, (x0, y0, x1, y1) in doors.items():
    cells = [(x, y) for y in range(y0, y1 + 1) for x in range(x0, x1 + 1)]
    assert any(c in reach for c in cells), ('unreachable door', name)
for name, (z, _sign, _c) in carts.items():
    x0, y0, x1, y1 = z
    ring = [(x, y) for y in range(y0 - 1, y1 + 2) for x in range(x0 - 1, x1 + 2)
            if x < x0 or x > x1 or y < y0 or y > y1]
    assert any(c in reach for c in ring), ('unreachable cart', name)

# ---------------------------------------------------------------- output
if '--print' in sys.argv:
    for row in grid:
        print(''.join(row))
    sys.exit(0)


def zone_lit(z):
    x0, y0, x1, y1 = z
    return f'Zone {{ x0: {x0}, y0: {y0}, x1: {x1}, y1: {y1} }}'


def cells_lit(cells):
    return ', '.join(f'({x}, {y})' for (x, y) in cells)


RUST_TEMPLATE = '''//! The night city street: static art plus the metadata the runtime needs
//! (collision, landmarks, animation cells). Generated by
//! `scripts/gen_city_map.py --write`; do not hand-edit the literal or the
//! zone constants, tweak the script and re-run.
//!
//! GAME.md, "The night city": the wallet. Shops, the armorer, repairs, the
//! board, the tailor, all on one rainy street with a screen tuned to a
//! dead channel at the end of it. One glyph per person (your runner's
//! mark), multi-cell buildings, carts and stalls: a tighter zoom than the
//! clubhouse, so the city reads as big. Single-width glyphs only.

#[rustfmt::skip]
pub const MAP_W: u16 = __W__;
#[rustfmt::skip]
pub const MAP_H: u16 = __H__;

#[rustfmt::skip]
pub const MAP: [&str; MAP_H as usize] = [
__MAP__
];

/// Cells nothing walks through, packed one bit per cell, row-major.
/// Walls, facades, carts, the railing and the drop are solid; the street,
/// the sidewalks, the alleys and the stairwell are open.
#[rustfmt::skip]
const SOLID: [&str; MAP_H as usize] = [
__SOLID__
];

/// Where your runner appears: at the top of the stairwell from the wire.
#[rustfmt::skip]
pub const SPAWN: (u16, u16) = (__SPAWN_X__, __SPAWN_Y__);

/// The street sign on the top wall.
#[rustfmt::skip]
pub const TITLE: Zone = __TITLE__;
/// The far skyline: rain falls across it.
#[rustfmt::skip]
pub const SKY: Zone = __SKY__;
/// The antenna mast's light.
#[rustfmt::skip]
pub const MAST_LIGHT: (u16, u16) = (__MAST_X__, __MAST_Y__);
/// The asphalt band between the curb and the railing.
#[rustfmt::skip]
pub const STREET: Zone = Zone { x0: 1, y0: __STREET_Y0__, x1: MAP_W - 2, y1: __STREET_Y1__ };
/// The railing over the drop.
#[rustfmt::skip]
pub const RAIL_Y: u16 = __RAIL_Y__;
/// The lower city, far below the railing: lights only.
#[rustfmt::skip]
pub const DROP: Zone = Zone { x0: 1, y0: __DROP_Y0__, x1: MAP_W - 2, y1: __DROP_Y1__ };
/// The dashed lane line.
#[rustfmt::skip]
pub const LANE_Y: u16 = __LANE_Y__;
/// The screen's face: static, and now and then the test pattern.
#[rustfmt::skip]
pub const SCREEN_FACE: Zone = __SCREEN_FACE__;
/// The bits machine's little display.
#[rustfmt::skip]
pub const BITS_SCREEN: Zone = __BITS_SCREEN__;
/// The board kiosk and its title row.
#[rustfmt::skip]
pub const BOARD: Zone = __BOARD__;
#[rustfmt::skip]
pub const BOARD_SIGN: Zone = __BOARD_SIGN__;
/// The stairwell down and its two signs.
#[rustfmt::skip]
pub const STAIRS_SIGNS: [Zone; 2] = [__STAIRS_SIGN__, __STAIRS_SIGN2__];
/// The stairwell up to the wire (the way out) and its sign.
#[rustfmt::skip]
pub const WIRE: Zone = __WIRE__;
#[rustfmt::skip]
pub const WIRE_SIGN: Zone = __WIRE_SIGN__;
/// The one dead letter on the lockers' sign: always dark.
#[rustfmt::skip]
pub const DEAD_LETTER: (u16, u16) = (__DEAD_X__, __DEAD_Y__);

/// A neon sign: the letters, and the color they burn.
#[derive(Debug, Clone, Copy)]
pub struct Sign {
    pub zone: Zone,
    pub color: Neon,
}

/// The shop signs over the doors.
#[rustfmt::skip]
pub const SIGNS: [Sign; __N_SIGNS__] = [
__SIGNS__
];

/// The vertical banners in the city's script (the glyph alphabet).
#[rustfmt::skip]
pub const BANNERS: [Sign; __N_BANNERS__] = [
__BANNERS__
];

/// The carts and stalls in the street: the prop and its name row.
#[rustfmt::skip]
pub const CART_SIGNS: [Sign; __N_CARTS__] = [
__CART_SIGNS__
];

/// The awnings hanging over the sidewalk, one per facade.
#[rustfmt::skip]
pub const AWNINGS: [Zone; __N_AWNINGS__] = [
__AWNINGS__
];

/// Window fields whose lit panes flicker.
#[rustfmt::skip]
pub const WINDOWS: [Zone; __N_WINDOWS__] = [
__WINDOWS__
];

/// Steam rises from these cells (vents in the alleys, the noodle bowls).
#[rustfmt::skip]
pub const VENTS: [(u16, u16); __N_VENTS__] = [__VENTS__];

/// Puddle cells: they catch the neon.
#[rustfmt::skip]
pub const PUDDLES: [(u16, u16); __N_PUDDLES__] = [__PUDDLES__];

/// Street lamps: a cone of light on the sidewalk below each.
#[rustfmt::skip]
pub const LAMPS: [(u16, u16); __N_LAMPS__] = [__LAMPS__];

/// The lower city's lights, seen from the railing.
#[rustfmt::skip]
pub const DROP_LIGHTS: [(u16, u16); __N_DROP_LIGHTS__] = [__DROP_LIGHTS__];

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

    /// The cells that put the player within reach: the sidewalk in front
    /// of a door, the ring around a cart.
    #[rustfmt::skip]
    pub fn reach(self) -> Zone {
        match self {
__REACH__
        }
    }

    /// How close the player has to stand.
    fn reach_distance(self) -> u16 {
        match self {
            Landmark::Noodles
            | Landmark::Umbrellas
            | Landmark::Blades
            | Landmark::Reader
            | Landmark::Board
            | Landmark::Bits => 1,
            Landmark::Armorer
            | Landmark::Tailor
            | Landmark::Lockers
            | Landmark::Bands
            | Landmark::Bar
            | Landmark::Screen
            | Landmark::Repairs
            | Landmark::Stairs
            | Landmark::Wire => 0,
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
/// walls, facades, carts, the railing and the drop.
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

if '--write' in sys.argv:
    import os

    def esc(s):
        return s.replace('\\', '\\\\').replace('"', '\\"')

    map_lines = ',\n'.join('    "' + esc(''.join(row).rstrip()) + '"' for row in grid)
    solid_lines = ',\n'.join(
        '    "' + ''.join('#' if s else '.' for s in row) + '"' for row in solid
    )
    sign_lits = [f'    Sign {{ zone: {zone_lit(z)}, color: Neon::{c} }},' for (z, c) in neon_signs.values()]
    banner_lits = [f'    Sign {{ zone: {zone_lit(z)}, color: Neon::{c} }},' for (z, c) in banners]
    cart_lits = [f'    Sign {{ zone: {zone_lit(s)}, color: Neon::{c} }},' for (_z, s, c) in carts.values()]
    reach = {
        'Armorer': doors['Armorer'], 'Tailor': doors['Tailor'], 'Lockers': doors['Lockers'],
        'Bands': doors['Bands'], 'Bar': doors['Bar'], 'Screen': doors['Screen'],
        'Repairs': doors['Repairs'], 'Board': BOARD_ZONE, 'Bits': BITS_ZONE,
        'Noodles': carts['Noodles'][0], 'Umbrellas': carts['Umbrellas'][0],
        'Blades': carts['Blades'][0], 'Reader': carts['Reader'][0],
        'Stairs': doors['Stairs'], 'Wire': doors['Wire'],
    }
    reach_arms = '\n'.join(f'            Landmark::{k} => {zone_lit(v)},' for k, v in reach.items())

    out = (RUST_TEMPLATE
           .replace('__W__', str(W)).replace('__H__', str(H))
           .replace('__MAP__', map_lines).replace('__SOLID__', solid_lines)
           .replace('__SPAWN_X__', str(SPAWN[0])).replace('__SPAWN_Y__', str(SPAWN[1]))
           .replace('__TITLE__', zone_lit(TITLE_ZONE)).replace('__SKY__', zone_lit(SKY_ZONE))
           .replace('__MAST_X__', str(MAST_LIGHT[0])).replace('__MAST_Y__', str(MAST_LIGHT[1]))
           .replace('__STREET_Y0__', str(STREET_Y0)).replace('__STREET_Y1__', str(STREET_Y1))
           .replace('__RAIL_Y__', str(RAIL_Y))
           .replace('__DROP_Y0__', str(DROP_Y0)).replace('__DROP_Y1__', str(DROP_Y1))
           .replace('__LANE_Y__', str(LANE_Y))
           .replace('__SCREEN_FACE__', zone_lit(SCREEN_FACE))
           .replace('__BITS_SCREEN__', zone_lit(BITS_SCREEN))
           .replace('__BOARD_SIGN__', zone_lit(BOARD_SIGN)).replace('__BOARD__', zone_lit(BOARD_ZONE))
           .replace('__STAIRS_SIGN2__', zone_lit(STAIRS_SIGN2)).replace('__STAIRS_SIGN__', zone_lit(STAIRS_SIGN))
           .replace('__WIRE_SIGN__', zone_lit(WIRE_SIGN)).replace('__WIRE__', zone_lit(WIRE_ZONE))
           .replace('__DEAD_X__', str(DEAD_LETTER[0])).replace('__DEAD_Y__', str(DEAD_LETTER[1]))
           .replace('__N_SIGNS__', str(len(sign_lits))).replace('__SIGNS__', '\n'.join(sign_lits))
           .replace('__N_BANNERS__', str(len(banner_lits))).replace('__BANNERS__', '\n'.join(banner_lits))
           .replace('__N_CARTS__', str(len(cart_lits))).replace('__CART_SIGNS__', '\n'.join(cart_lits))
           .replace('__N_AWNINGS__', str(len(awnings))).replace('__AWNINGS__', '\n'.join(f'    {zone_lit(z)},' for z in awnings))
           .replace('__N_WINDOWS__', str(len(windows))).replace('__WINDOWS__', '\n'.join(f'    {zone_lit(z)},' for z in windows))
           .replace('__N_VENTS__', str(len(vents))).replace('__VENTS__', cells_lit(vents))
           .replace('__N_PUDDLES__', str(len(puddles))).replace('__PUDDLES__', cells_lit(puddles))
           .replace('__N_LAMPS__', str(len(lamps))).replace('__LAMPS__', cells_lit(lamps))
           .replace('__N_DROP_LIGHTS__', str(len(DROP_LIGHTS))).replace('__DROP_LIGHTS__', cells_lit(DROP_LIGHTS))
           .replace('__REACH__', reach_arms))
    path = os.path.join(os.path.dirname(__file__), '..', 'late-ssh', 'src', 'app', 'deadchannel', 'city', 'map.rs')
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w') as f:
        f.write(out)
    print(f'wrote {os.path.normpath(path)}')
