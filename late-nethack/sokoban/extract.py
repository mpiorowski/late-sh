# Rebuild levels/*.txt from a NetHack nhdat: the des.map of each soko*.lua
# plus its boulder, trap and stair placements, hero on the arrival spot.
# usage: python3 -I extract.py <nhdat> <out dir>
import re, sys
data = open(sys.argv[1], "rb").read()
out = sys.argv[2]
index = re.findall(rb"n(soko\d-\d)\.lua\s+(\d+)", data)
offs = sorted((int(o), n.decode()) for n, o in re.findall(rb"n([a-z0-9-]+)\.lua\s+(\d+)", data))
for name, off in index:
    name = name.decode()
    off = int(off)
    nxt = min(o for o, _ in offs if o > off)
    lua = data[off:nxt].decode("latin-1")
    rows = lua.split("des.map([[\n", 1)[1].split("]]", 1)[0].split("\n")[:-1]
    w = max(len(r) for r in rows)
    grid = [list(r.ljust(w)) for r in rows]
    def put(x, y, ch):
        grid[y][x] = ch
    for x, y in re.findall(r'des\.object\("boulder",\s*(\d+),\s*(\d+)\)', lua):
        put(int(x), int(y), "0")
    for kind, x, y in re.findall(r'des\.trap\("(hole|pit|rolling boulder)",\s*(\d+),\s*(\d+)\)', lua):
        put(int(x), int(y), "R" if kind == "rolling boulder" else "^")
    for kind, x, y in re.findall(r'des\.stair\("(up|down)",\s*(\d+),\s*(\d+)\)', lua):
        put(int(x), int(y), "<" if kind == "up" else ">")
    m = re.search(r'des\.levregion\(\{ region = \{(\d+),(\d+),', lua)
    if m:
        hx, hy = int(m.group(1)), int(m.group(2))
    else:
        hy = next(i for i, r in enumerate(grid) if ">" in r)
        hx = grid[hy].index(">")
    put(hx, hy, "@")
    open(f"{out}/{name}.txt", "w").write("\n".join("".join(r).rstrip() for r in grid) + "\n")
    print(name)
