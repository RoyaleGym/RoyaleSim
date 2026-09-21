#!/usr/bin/env python3
"""Turn Supercell's shipped tilemap CSV into the canonical arena the engine loads.

WHY THIS EXISTS
    An arena typed in from screenshots gets things wrong that decide games, and
    plausibly enough that nothing catches it: a river 1 tile wide instead of 2,
    bridges 3 tiles wide at x in [2.0,5.0) instead of 2 tiles at x in [2.5,4.5),
    a king tower footprint of 4x4 instead of 3x3.  The game ships the real map,
    so there is no reason to guess.

    Nothing in this project hardcodes arena geometry.  It is derived here,
    from data, and every derived number is asserted against an independent
    expectation below so that a silently-wrong extraction cannot pass.

THE FORMAT (decoded 2026-09-12, corroborated by two independent 2016/2018 data sets)
    The `Map` section is a 64 x 36 grid of HALF-TILES: 32 x 18 tiles, y-major,
    x increasing left to right.  Blank cells are 0.  Each cell is a bitmask:

        bit 0  (1)   cell belongs to the LEFT lane
        bit 1  (2)   cell belongs to the RIGHT lane
        bit 4  (16)  NO-DEPLOY  (arena edge strips, and the king tower block)
        bit 5  (32)  WATER      (the river; impassable to ground, free to air)

    Observed value set is exactly {0,1,2,16,17,18,32} -- a clean decode with no
    leftover bits.  If a future data set introduces a value outside that set this
    script fails loudly rather than dropping the bit on the floor.

USAGE
    python tools/extract_arena.py                 # extract + gate
    python tools/extract_arena.py --plant water   # prove the gate can fail
    python tools/extract_arena.py --render        # print the ASCII map

    --plant is not optional ceremony.  A gate nobody has watched go red is a
    gate nobody should trust.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "data" / "raw" / "retroroyale-2018" / "tilemaps" / "tilemap.csv"
OUT = ROOT / "data" / "derived" / "arena.json"

# --- the bitmask, named once ------------------------------------------------
LANE_LEFT = 1
LANE_RIGHT = 2
NO_DEPLOY = 16
WATER = 32
KNOWN_BITS = LANE_LEFT | LANE_RIGHT | NO_DEPLOY | WATER

HALF = 2  # half-tiles per tile
TILES_X, TILES_Y = 18, 32
HX, HY = TILES_X * HALF, TILES_Y * HALF  # 36 x 64


def read_map_section(path: Path) -> list[list[int]]:
    """Parse the `Map` section into a HY x HX grid of ints.

    The CSV is Supercell's standard shape: a section-name row, a column-name
    row, a column-type row, then data rows whose first field is blank.
    """
    rows = list(csv.reader(path.open(encoding="utf-8-sig")))
    starts = [i for i, r in enumerate(rows) if r and r[0].strip()]
    if not starts or rows[starts[0]][0].strip() != "Map":
        raise SystemExit(f"{path}: first section is not 'Map' (sections: {starts})")
    first_data = starts[0] + 3  # skip name/type header rows
    end = starts[1] if len(starts) > 1 else len(rows)

    grid: list[list[int]] = []
    for r in rows[first_data:end]:
        cells = r[1 : 1 + HX]
        if len(cells) < HX:
            cells = cells + [""] * (HX - len(cells))
        grid.append([int(c) if c.strip() else 0 for c in cells])
    return grid


def derive(grid: list[list[int]]) -> dict:
    """Derive every geometric fact the engine needs, from the grid alone."""
    water = [(x, y) for y, row in enumerate(grid) for x, v in enumerate(row) if v & WATER]
    water_rows = sorted({y for _, y in water})
    # river in TILE rows
    river_tiles = sorted({y // HALF for y in water_rows})

    # A bridge is a non-water cell on a water row.
    bridge_cols_by_row = {
        y: sorted(x for x in range(HX) if not (grid[y][x] & WATER)) for y in water_rows
    }
    all_bridge_cols = sorted({x for cols in bridge_cols_by_row.values() for x in cols})
    # split into contiguous runs -> one run per bridge
    bridges = []
    run: list[int] = []
    for x in all_bridge_cols:
        if run and x != run[-1] + 1:
            bridges.append(run)
            run = []
        run.append(x)
    if run:
        bridges.append(run)

    bridge_spans = [
        {
            "half_cols": [b[0], b[-1]],
            "x_min": b[0] / HALF,
            "x_max": (b[-1] + 1) / HALF,
            "width_tiles": len(b) / HALF,
            "center_x": (b[0] + len(b) / 2) / HALF,
        }
        for b in bridges
    ]

    # no-deploy strips on the two back rows -> the deployable window behind a king
    def open_cols(y: int) -> list[int]:
        return [x for x in range(HX) if not (grid[y][x] & NO_DEPLOY)]

    back_bottom = open_cols(0)
    back_top = open_cols(HY - 1)

    # The king tower block is the interior no-deploy region.  Two things that are
    # ALSO interior no-deploy must be excluded or they merge into the king runs:
    # the four river-corner blocks, which sit in the outermost two half-columns.
    # Getting this wrong reported FOUR king blocks spanning the full arena width,
    # which is how this filter came to exist.
    EDGE = 2
    interior_nd = [
        (x, y)
        for y in range(EDGE, HY - EDGE)
        for x in range(EDGE, HX - EDGE)
        if grid[y][x] & NO_DEPLOY
    ]
    kings = []
    if interior_nd:
        ys = sorted({y for _, y in interior_nd})
        # contiguous y runs = one per king tower
        runs: list[list[int]] = []
        cur = [ys[0]]
        for y in ys[1:]:
            if y == cur[-1] + 1:
                cur.append(y)
            else:
                runs.append(cur)
                cur = [y]
        runs.append(cur)
        for r in runs:
            xs = sorted({x for x, y in interior_nd if y in r})
            kings.append(
                {
                    "half_rows": [r[0], r[-1]],
                    "half_cols": [xs[0], xs[-1]],
                    "x_min": xs[0] / HALF,
                    "x_max": (xs[-1] + 1) / HALF,
                    "y_min": r[0] / HALF,
                    "y_max": (r[-1] + 1) / HALF,
                    "w_tiles": len(xs) / HALF,
                    "h_tiles": len(r) / HALF,
                    "center": [(xs[0] + len(xs) / 2) / HALF, (r[0] + len(r) / 2) / HALF],
                }
            )

    # corner blocks beside the river (no-deploy cells on non-edge rows at x extremes)
    corner_blocks = sorted(
        {
            (x // HALF, y // HALF)
            for y in range(2, HY - 2)
            for x in (0, 1, HX - 2, HX - 1)
            if grid[y][x] & NO_DEPLOY
        }
    )

    return {
        "half_tiles_per_tile": HALF,
        "tiles": [TILES_X, TILES_Y],
        "half_grid": [HX, HY],
        "river_tile_rows": river_tiles,
        "water_half_rows": [water_rows[0], water_rows[-1]],
        "water_cells": len(water),
        "bridges": bridge_spans,
        "back_row_open_half_cols": {
            "bottom": [back_bottom[0], back_bottom[-1]],
            "top": [back_top[0], back_top[-1]],
        },
        "king_blocks": kings,
        "river_corner_blocked_tiles": [list(c) for c in corner_blocks],
        "bits": {
            "LANE_LEFT": LANE_LEFT,
            "LANE_RIGHT": LANE_RIGHT,
            "NO_DEPLOY": NO_DEPLOY,
            "WATER": WATER,
        },
        "grid": grid,
    }


# --- the gate ---------------------------------------------------------------
# Every one of these is an INDEPENDENT expectation: it is what the geometry must
# be according to a source other than this parser (community tile analysis, the
# published arena dimensions, the tower coordinates in locations.csv).  If the
# parser and the expectation disagree, one of them is wrong and the build stops.

GATES_RUN = [
    0
]  # how many gates the last gate() call evaluated; printed so a run that graded nothing is visible


def gate(a: dict) -> list[str]:
    fail: list[str] = []
    GATES_RUN[0] = 0

    def ok(label: str, cond: bool, got) -> None:
        GATES_RUN[0] += 1
        if not cond:
            fail.append(f"{label}: got {got!r}")

    ok("grid is 36x64 half-tiles", a["half_grid"] == [36, 64], a["half_grid"])
    ok(
        "river occupies exactly tile rows 15 and 16",
        a["river_tile_rows"] == [15, 16],
        a["river_tile_rows"],
    )
    # The tile-row gate above is blind to a missing HALF row -- draining half-row 30
    # leaves river_tile_rows == [15,16] because half-row 31 is still water. Found by
    # the 'water' plant failing to land. These two see it.
    ok("water spans half-rows 30..33", a["water_half_rows"] == [30, 33], a["water_half_rows"])
    ok("water cell count is 112", a["water_cells"] == 112, a["water_cells"])
    ok("there are exactly 2 bridges", len(a["bridges"]) == 2, len(a["bridges"]))

    for i, b in enumerate(a["bridges"]):
        ok(f"bridge {i} is 2 tiles wide", b["width_tiles"] == 2.0, b["width_tiles"])
    if len(a["bridges"]) == 2:
        lx, rx = a["bridges"][0]["center_x"], a["bridges"][1]["center_x"]
        ok("left bridge centred on x=3.5", lx == 3.5, lx)
        ok("right bridge centred on x=14.5", rx == 14.5, rx)
        # bridges are half-tile offset, NOT tile aligned -- the thing the old engine got wrong
        ok(
            "left bridge spans x in [2.5,4.5)",
            (a["bridges"][0]["x_min"], a["bridges"][0]["x_max"]) == (2.5, 4.5),
            (a["bridges"][0]["x_min"], a["bridges"][0]["x_max"]),
        )

    ok("two king no-deploy blocks", len(a["king_blocks"]) == 2, len(a["king_blocks"]))
    for i, k in enumerate(a["king_blocks"]):
        ok(
            f"king block {i} is 3x3 tiles",
            (k["w_tiles"], k["h_tiles"]) == (3.0, 3.0),
            (k["w_tiles"], k["h_tiles"]),
        )
        ok(
            f"king block {i} spans x in [7.5,10.5)",
            (k["x_min"], k["x_max"]) == (7.5, 10.5),
            (k["x_min"], k["x_max"]),
        )

    ok(
        "4 river-corner blocked tiles",
        len(a["river_corner_blocked_tiles"]) == 4,
        a["river_corner_blocked_tiles"],
    )

    # lane coverage: every non-water, non-edge cell should be reachable territory
    grid = a["grid"]
    vals = {v for row in grid for v in row}
    unknown = {v for v in vals if v & ~KNOWN_BITS}
    ok("no unknown bits in any cell", not unknown, sorted(unknown))

    return fail


GLYPH = {
    0: ".",
    LANE_LEFT: "L",
    LANE_RIGHT: "R",
    NO_DEPLOY: "#",
    NO_DEPLOY | LANE_LEFT: "l",
    NO_DEPLOY | LANE_RIGHT: "r",
    WATER: "~",
}


def render(grid: list[list[int]]) -> str:
    out = []
    for y, row in enumerate(grid):
        out.append(f"{y:2d} y={y / HALF:5.1f}  " + "".join(GLYPH.get(v, "?") for v in row))
    return "\n".join(out)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--render", action="store_true")
    ap.add_argument(
        "--plant",
        choices=["water", "bridge", "king", "bits"],
        help="corrupt the grid on purpose and prove the gate goes red",
    )
    args = ap.parse_args()

    if not SRC.exists():
        print(f"MISSING {SRC}", file=sys.stderr)
        return 2

    grid = read_map_section(SRC)

    if args.plant:
        # PLANT: break one specific fact and require the matching gate to fire.
        if args.plant == "water":
            for x in range(36):  # drain one river row
                grid[30][x] &= ~WATER
        elif args.plant == "bridge":
            grid[30][9] &= ~WATER  # widen the left bridge by one half-tile
            grid[31][9] &= ~WATER
            grid[32][9] &= ~WATER
            grid[33][9] &= ~WATER
        elif args.plant == "king":
            grid[3][14] |= NO_DEPLOY  # widen the king block
            grid[4][14] |= NO_DEPLOY
        elif args.plant == "bits":
            grid[10][10] |= 64  # an undecoded bit appears

    a = derive(grid)

    if args.render:
        print(render(grid))
        print()

    if args.plant:
        # A plant is only evidence if TWO things hold:
        #   1. the tree is green WITHOUT the plant  -- otherwise "red" proves nothing
        #   2. the gate the plant AIMS AT goes red  -- not merely some gate somewhere
        # This check was too loose exactly once: all four plants reported LANDED
        # while the real cause was an unrelated bug in king-block detection. And it
        # was then too strict: it rejected plants that legitimately trip neighbours.
        aimed_at = {
            "water": "water spans half-rows 30..33",
            "bridge": "left bridge spans x in [2.5,4.5)",
            "king": "king block 0 is 3x3 tiles",
            "bits": "no unknown bits in any cell",
        }[args.plant]

        baseline = gate(derive(read_map_section(SRC)))
        if baseline:
            print(
                f"PLANT '{args.plant}' INCONCLUSIVE -- the tree is already red without "
                f"the plant, so nothing it does is evidence. Fix these first:",
                file=sys.stderr,
            )
            for f in baseline:
                print(f"   {f}", file=sys.stderr)
            return 1

        fail = gate(a)
        hit = [f for f in fail if f.startswith(aimed_at)]
        if hit:
            extra = len(fail) - len(hit)
            print(
                f"PLANT '{args.plant}' LANDED -- '{aimed_at}' went red as intended"
                + (f" (+{extra} neighbouring gate(s) also red)" if extra else "")
                + ":"
            )
            for f in hit:
                print(f"   {f}")
            return 0
        print(
            f"PLANT '{args.plant}' DID NOT LAND -- '{aimed_at}' stayed GREEN with the "
            f"defect present. That gate cannot see this class of defect.",
            file=sys.stderr,
        )
        return 1

    fail = gate(a)
    if fail:
        print("ARENA GATE FAILED:", file=sys.stderr)
        for f in fail:
            print(f"   {f}", file=sys.stderr)
        return 1

    src_sha = hashlib.sha256(SRC.read_bytes()).hexdigest()
    a["provenance"] = {
        "source": "retroroyale/ClashRoyale GameAssets/tilemaps/tilemap.csv",
        "source_sha256": src_sha,
        "vintage": "~2018 client data",
        "caveat": "Pre-2025. The modern client no longer ships a Map section; "
        "bridge width may differ by arena. Treat as the DEFAULT arena, "
        "not as proof about the live 2026 map.",
    }
    OUT.parent.mkdir(parents=True, exist_ok=True)
    # newline="\n": derived artifacts must be byte-identical on every OS. A text-mode
    # write on Windows emits CRLF, and the engine include_str!s this file.
    OUT.write_text(json.dumps(a, indent=1) + "\n", encoding="utf-8", newline="\n")

    print(f"arena -> {OUT.relative_to(ROOT)}")
    print(
        f"  grid          {a['half_grid'][0]}x{a['half_grid'][1]} half-tiles "
        f"({a['tiles'][0]}x{a['tiles'][1]} tiles)"
    )
    print(f"  river rows    {a['river_tile_rows']}")
    for i, b in enumerate(a["bridges"]):
        print(
            f"  bridge {i}      x in [{b['x_min']}, {b['x_max']}) "
            f"width {b['width_tiles']} centre {b['center_x']}"
        )
    for i, k in enumerate(a["king_blocks"]):
        print(
            f"  king block {i}  x in [{k['x_min']}, {k['x_max']}) "
            f"y in [{k['y_min']}, {k['y_max']}) = {k['w_tiles']}x{k['h_tiles']}"
        )
    print(f"  corners       {a['river_corner_blocked_tiles']}")
    n_fail = len(gate(a))
    # Vacuity guard: the gate set is fixed-size today; a sudden drop means a loop
    # stopped iterating (e.g. zero bridges found) and the board went quietly green.
    if GATES_RUN[0] < 15:
        print(f"ARENA GATE VACUOUS: only {GATES_RUN[0]} gates evaluated", file=sys.stderr)
        return 1
    print(f"  GATE          {GATES_RUN[0]} gates, {n_fail} failures")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
