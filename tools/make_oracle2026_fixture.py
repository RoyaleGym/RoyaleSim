#!/usr/bin/env python3
"""REGENERATE crates/royalesim/tests/fixtures/oracle2026/first_paths.json.

    RoyaleSim\\.venv\\Scripts\\python.exe tools/make_oracle2026_fixture.py          # rewrite
    ... tools/make_oracle2026_fixture.py --check                                     # diff only

WHY THIS FILE EXISTS AT ALL
    The fixture used to be produced by a script that lived only in a pass report, so
    nothing in a checkout could re-derive it -- and it was wrong in a way nobody could
    see: four of its 36 cases took the occluding building's centre from the deploy
    COMMAND instead of from the entity the trace records. The game SNAPS a deploy to a
    tile, so `cannon_dx-0.5` commanded x = 3000 while the Cannon stood at x = 3500;
    the phantom box at 3000 then claimed four cells of the oracle's own path, and the
    case went into the fixture as "the occlusion model refuses this one". It does not.
    Every number below now comes from the trace or from csv_logic, and `--check` fails
    if the committed fixture disagrees with the traces.

WHAT ONE CASE IS
    The FIRST path the live 2026 game published for one unit: the `path_nodes` list on
    its first moving tick, together with everything the pathfinder was called with.

      start_native   the unit's position on the tick BEFORE it first moved -- the
                     position the plan was made from.
      target_native  the enemy crown tower the unit is walking at. DERIVED as the
                     enemy tower nearest the published GOAL cell's centre, because the
                     entity's own `target` field is still null on the first moving tick
                     in 13 of the 16 lane-sweep runs (the trace ends before contact).
                     The inversion is safe because the goal cell is DEFINED by the
                     target (spec 6.1) and oracle2026.rs `g4_the_goal_rule_holds_two_
                     sided_on_every_oracle_path` re-checks the forward direction: the
                     goal is in reach of this tower and its predecessor is not.
      reach_native   Range + the mover's own CollisionRadius, read from
                     data/raw/cr-15.535.29/csv_logic (spec 6.1). NOT from
                     data/derived/cards.json, which is the 2018 data and disagrees for
                     MiniPekka (800 vs 1050) and Knight (1200 vs 1000) -- passing the
                     live reach in keeps oracle2026.rs a test of the PATHFINDER rather
                     than of the card vintage.
      occluders      EVERY crown tower and every building standing on that tick, BOTH
                     SIDES, each at the position the recorded FRAME gives and with its
                     own csv_logic CollisionRadius (Tesla 500, not the 600 that was
                     assumed).

                     NOT friendly-only, although the datamined
                     PATHFINDING_FRIENDLYONLY_OCCLUSIONS says TRUE: on the live 16.402
                     corpus friendly-only occlusion fails 119 of 785 first paths and
                     both sides fails 6 (calibration pathfinding.OCCLUSION_MODEL). The
                     OFFLINE corpus this fixture is built from cannot see the
                     difference -- no interior cell of any of its first paths lies
                     inside an enemy tower box -- so both sides changes only the
                     occluder LISTS here, not one admits flag or one gated cost. It is
                     written that way anyway, because a fixture that encodes a refuted
                     occluder set quietly re-asserts it every time the gate runs green.

    `occlusion_model_admits_this_path` is DERIVED here, not declared: it is false when
    the oracle's own published path has an INTERIOR cell inside one of those boxes,
    which would mean the measured half-open AABB refutes a path the game actually
    took. With the occluders read from the frames it is true on all 36.

WHICH TRACES
    walk, building_Giant and repath_Giant in full. lane_sweep_Knight has 128 traces and
    the fixture carries the 16-trace sample the 2026-09-18 measurement put in it; the
    sample is listed explicitly below so this script reproduces the committed file
    rather than silently changing what is gated. Widening it is a deliberate edit.
"""

from __future__ import annotations

import argparse
import gzip
import itertools
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent          # RoyaleSim
TRACES = ROOT / "data" / "oracle-native"
CSV_LOGIC = ROOT / "data" / "raw" / "cr-15.535.29" / "csv_logic"
FIXTURE = ROOT / "crates" / "royalesim" / "tests" / "fixtures" / "oracle2026" / "first_paths.json"

CELL_NATIVE = 500
GRID_COLS = 36

# The lane-sweep sample the fixture carries (see WHICH TRACES).
LANE_SWEEP_SAMPLE = [
    "c00_r02", "c00_r22", "c02_r08", "c02_r24", "c04_r10", "c04_r28", "c06_r12",
    "c06_r30", "c08_r18", "c10_r04", "c10_r22", "c12_r08", "c12_r26", "c14_r12",
    "c14_r28", "c16_r14",
]

# Every card the corpus spawns: card_id -> (name, toml file, [CHARACTER|BUILDING].<x>).
CARD_OF_ID = {
    26000000: ("Knight", "characters/knight.toml", "CHARACTER.Knight"),
    26000003: ("Giant", "characters/giant.toml", "CHARACTER.Giant"),
    26000009: ("Golem", "characters/golem.toml", "CHARACTER.Golem"),
    26000010: ("Skeletons", "characters/skeleton.toml", "CHARACTER.Skeleton"),
    26000018: ("MiniPekka", "characters/minipekka.toml", "CHARACTER.MiniPekka"),
    26000021: ("HogRider", "characters/hogrider.toml", "CHARACTER.HogRider"),
    26000024: ("RoyalGiant", "characters/royalgiant.toml", "CHARACTER.RoyalGiant"),
    27000000: ("Cannon", "characters/cannon.toml", "BUILDING.Cannon"),
    27000004: ("BombTower", "characters/bombtower.toml", "BUILDING.BombTower"),
    27000006: ("Tesla", "characters/tesla.toml", "BUILDING.Tesla"),
}
BUILDING_IDS = {27000000, 27000004, 27000006}

# The crown towers' CollisionRadius. Datamined, and the only two the corpus needs;
# calibration pathfinding.OCCLUSION_MODEL records that the 500-unit cell quantum
# brackets them only to (500, 1000] and (1000, 2000] respectively.
TOWER_RADIUS = {"princess": 1000, "king": 1400}


# ------------------------------------------------------------------- csv_logic
_TOML_CACHE: dict[str, dict] = {}


def toml_stats(rel: str, section: str) -> dict:
    """Flat parser for one [SECTION] block of a csv_logic toml."""
    key = f"{rel}#{section}"
    if key not in _TOML_CACHE:
        text = (CSV_LOGIC / rel).read_text(encoding="utf-8", errors="replace")
        out: dict = {}
        for block in re.split(r"^\[", text, flags=re.M):
            if not block.startswith(section + "]"):
                continue
            for line in block.splitlines()[1:]:
                m = re.match(r"^([A-Za-z0-9_]+)\s*=\s*(.+?)\s*$", line)
                if m:
                    k, v = m.group(1), m.group(2)
                    out[k] = int(v) if re.fullmatch(r"-?\d+", v) else v.strip('"')
            break
        _TOML_CACHE[key] = out
    return _TOML_CACHE[key]


def stats_of(card_id: int) -> tuple[str, dict]:
    name, rel, section = CARD_OF_ID[card_id]
    return name, toml_stats(rel, section)


def reach_of(card_id: int) -> int:
    """spec 6.1: the goal cell is within Range + the MOVER's OWN CollisionRadius."""
    return int(stats_of(card_id)[1]["Range"]) + int(stats_of(card_id)[1]["CollisionRadius"])


def radius_of(card_id: int) -> int:
    return int(stats_of(card_id)[1]["CollisionRadius"])


# ---------------------------------------------------------------------- traces
def load(path: Path) -> tuple[dict, list[dict]]:
    header, frames = None, []
    with gzip.open(path, "rt", encoding="utf-8") as f:
        for line in f:
            rec = json.loads(line)
            if rec["record"] == "header":
                header = rec
            elif rec["record"] == "frame":
                frames.append(rec["state"])
    assert header is not None, f"{path}: no header"
    return header, frames


def tracks(header: dict, frames: list[dict]) -> dict[tuple[int, int], list[tuple[int, dict]]]:
    """(side, generation_key) -> [(tick, entity)] for every NON-tower entity."""
    towers = {(t["side"], t["x"], t["y"]) for t in header.get("towers", [])}
    out: dict[tuple[int, int], list[tuple[int, dict]]] = {}
    for st in frames:
        for e in st["entities"]:
            if e.get("card_id") == -1 or (e["side"], e["x"], e["y"]) in towers:
                continue
            out.setdefault((e["side"], e["generation_key"]), []).append((st["tick"], e))
    return out


def blocked_cells(occ: list[list[int]]) -> set[tuple[int, int]]:
    """spec 4.1-4.3: cells overlapping `[cx-R, cx+R) x [cy-R, cy+R)`, no mover pad."""
    out: set[tuple[int, int]] = set()
    for cx, cy, r in occ:
        for c in range((cx - r) // CELL_NATIVE, (cx + r - 1) // CELL_NATIVE + 1):
            for rr in range((cy - r) // CELL_NATIVE, (cy + r - 1) // CELL_NATIVE + 1):
                out.add((c, rr))
    return out


def case_for(path: Path, family: str) -> dict | None:
    header, frames = load(path)
    by_unit = tracks(header, frames)
    # The tracked unit is the first MOVER: a building_Giant trace's lowest generation
    # key is the Cannon / Bomb Tower / Tesla that was dropped in front of the Giant.
    for (side, key), track in sorted(by_unit.items()):
        first_move = next((t1 for (_t0, a), (t1, b) in itertools.pairwise(track)
                           if (a["x"], a["y"]) != (b["x"], b["y"])), None)
        if first_move is None:
            continue
        card_id = track[0][1].get("card_id")
        if card_id not in CARD_OF_ID:
            print(f"  {path.name}: card_id {card_id} is not in the table -- skipped", file=sys.stderr)
            return None
        ticks = {t: e for t, e in track}
        plan_from, at_move = ticks[first_move - 1], ticks[first_move]
        nodes = at_move.get("path_nodes") or []
        if not nodes:
            print(f"  {path.name}: no path_nodes on the first moving tick -- skipped", file=sys.stderr)
            return None
        cells = [[v % GRID_COLS, v // GRID_COLS] for v in nodes]

        goal_centre = (cells[0][0] * CELL_NATIVE + CELL_NATIVE // 2,
                       cells[0][1] * CELL_NATIVE + CELL_NATIVE // 2)
        enemy = [t for t in header.get("towers", []) if t["side"] != side]
        target = min(enemy, key=lambda t: ((t["x"] - goal_centre[0]) ** 2 + (t["y"] - goal_centre[1]) ** 2,
                                           t["type"] != "princess", t["x"], t["y"]))

        # THE OCCLUDERS, from the FRAME and never from the deploy command: the game
        # snaps a deploy to a tile, so the two differ by up to half a cell and the
        # command is not where the box stands.
        #
        # BOTH SIDES (see the docstring): every crown tower and every building, not
        # only the mover's own. The mover itself is excluded because it is not a
        # building; `okey == key` is kept so a building trace never occludes itself.
        occ: list[list[int]] = [[t["x"], t["y"], TOWER_RADIUS[t["type"]]]
                                for t in header.get("towers", [])]
        for (_oside, okey), otrack in sorted(by_unit.items()):
            if okey == key:
                continue
            e = dict(otrack).get(first_move)
            if e is None or e.get("card_id") not in BUILDING_IDS:
                continue
            occ.append([e["x"], e["y"], radius_of(e["card_id"])])

        blocked = blocked_cells(occ)
        return {
            # spec 4.6 exempts the GOAL cell, so only cells[1:] are checked.
            "occlusion_model_admits_this_path": all(tuple(c) not in blocked for c in cells[1:]),
            "trace": f"{family}/{path.name}",
            "card": stats_of(card_id)[0],
            "start_native": [plan_from["x"], plan_from["y"]],
            "target_native": [target["x"], target["y"]],
            "reach_native": reach_of(card_id),
            "occluders_native": occ,
            "oracle_cells_goal_first": cells,
        }
    return None


COMMENT = [
    "ORACLE FIRST PATHS -- one case per trace. GENERATED by tools/",
    "make_oracle2026_fixture.py from the offline trace corpus; that script's docstring says",
    "how every field is derived, and `--check` re-derives them and fails on a",
    "difference. Every number is the LIVE 2026 game's: native arena units (1000 per",
    "tile), cells as [col, row] on the 36x64 half-tile grid, lists GOAL-FIRST exactly",
    "as the game publishes path_nodes. reach_native is Range + the mover's own",
    "CollisionRadius from csv_logic/characters/*.toml of 15.535.29 -- NOT from",
    "data/derived/cards.json, which is the 2018 data and disagrees for MiniPekka",
    "(800 vs 1050) and Knight (1200 vs 1000). occluders_native is [x, y,",
    "CollisionRadius] of EVERY tower and building standing on that tick, BOTH SIDES",
    "-- not only the mover's own, which the live 16.402 corpus refutes: friendly-only",
    "occlusion fails 119 of 785 first paths and both sides fails 6 (calibration",
    "pathfinding.OCCLUSION_MODEL). On THIS corpus the change is inert --",
    "every case gained its three enemy crown towers and not one admits flag, gated",
    "cost or engine path moved -- because no interior cell of any offline first path",
    "lies inside an enemy tower box. Taken from the recorded",
    "FRAME: the game snaps a deploy to a tile, so the deploy command's coordinate is",
    "NOT the box's centre (cannon_dx-0.5 commanded 3000 and stands at 3500), which a",
    "hand-built version of this file got wrong on four cases.",
    "Provenance: the offline trace corpus (CR 15.535.29).",
]


def trace_files() -> list[tuple[Path, str]]:
    out: list[tuple[Path, str]] = []
    for fam in ("walk", "lane_sweep_Knight", "building_Giant", "repath_Giant"):
        d = TRACES / fam
        if not d.is_dir():
            continue
        for p in sorted(d.glob("*.jsonl.gz")):
            if fam == "lane_sweep_Knight" and p.name.split(".jsonl")[0] not in LANE_SWEEP_SAMPLE:
                continue
            out.append((p, fam))
    return out


def build() -> dict:
    cases = [c for c in (case_for(p, fam) for p, fam in trace_files()) if c is not None]
    return {"$comment": COMMENT, "cases": cases}


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--check", action="store_true",
                    help="re-derive from the traces, print every difference, exit non-zero on any")
    args = ap.parse_args()
    if not TRACES.is_dir():
        print(f"{TRACES} is absent -- nothing to generate", file=sys.stderr)
        return 0
    fresh = build()
    if args.check:
        old = {c["trace"]: c for c in json.loads(FIXTURE.read_text(encoding="utf-8"))["cases"]}
        new = {c["trace"]: c for c in fresh["cases"]}
        bad = 0
        for k in sorted(set(old) | set(new)):
            if k not in old:
                print(f"MISSING from the fixture: {k}")
                bad += 1
            elif k not in new:
                print(f"EXTRA in the fixture (the traces do not yield it): {k}")
                bad += 1
            else:
                for f in ("card", "start_native", "target_native", "reach_native",
                          "occluders_native", "oracle_cells_goal_first",
                          "occlusion_model_admits_this_path"):
                    if old[k][f] != new[k][f]:
                        print(f"{k}: {f}\n  fixture {old[k][f]}\n  traces  {new[k][f]}")
                        bad += 1
        print(f"{len(new)} cases re-derived, {bad} differences")
        return 1 if bad else 0
    FIXTURE.write_text(json.dumps(fresh, indent=1) + "\n", encoding="utf-8")
    refused = sum(1 for c in fresh["cases"] if not c["occlusion_model_admits_this_path"])
    print(f"wrote {FIXTURE}: {len(fresh['cases'])} cases, {refused} refused by the occlusion box")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
