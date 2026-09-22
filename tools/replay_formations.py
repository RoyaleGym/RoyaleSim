#!/usr/bin/env python3
"""The formation, stagger and reach numbers the corpus report reads off the truth.

python tools/replay_formations.py [<fixture.replay.json> ...] [--card NAME] [--json]

Default: every fixture in data/derived/replay/. Per DEPLOY group of the fixtures:

  formation   each member's offset from the group's deploy position (the tap tile
              for a multi-unit group, else the centroid), native units, on the
              group's first frame -- the ring the game lays N summons on
              (the formation gap)
  stagger     each member's deploy-end tick (its first frame in a state other than
              4 / 11) minus the group's spawn tick -- the SummonDeployDelay
              stagger ("member k leaves at spawn + 19 + 2k")
  reach       for a group whose members attack a tower: the distance from the
              member's centre to the tower's centre on its first attacking frame
              (state 2), native units -- the melee reach the engine lacks 600 of
              (the reach gap)

Reads only the fixture (tools/make_replay_fixture.py's truth); nothing here touches
the engine. `--json` prints the rows as JSON for a diff.
"""

from __future__ import annotations

import argparse
import glob
import json
import math
import os
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPLAY = os.path.join(ROOT, "data", "derived", "replay")
DEPLOY_STATES = (4, 11)
ATTACK_STATE = 2


def decode(col: list) -> list:
    out: list = []
    for v, run in zip(col[::2], col[1::2], strict=True):
        out.extend([v] * run)
    return out


def rows_of(e: dict) -> list[tuple]:
    """[(x, y, hp, target, path_n, state), ...] over the entity's frames (None = absent)."""
    cols = [decode(e[c]) for c in ("x", "y", "hp", "target", "path_n", "state")]
    return list(zip(*cols, strict=True))


def group_rows(fx: dict) -> list[dict]:
    ticks = fx["truth"]["ticks"]
    ents = {e["key"]: e for e in fx["truth"]["entities"]}
    towers = {e["key"]: e for e in fx["truth"]["entities"] if e["card_id"] == -1}
    tower_pos = {k: (rows_of(e)[0][0], rows_of(e)[0][1]) for k, e in towers.items()}
    out = []
    for d in fx["deploys"]:
        if d["kind"] == "spell" or not d.get("keys"):
            continue
        px, py = d["pos"]
        members = []
        for key in d["keys"]:
            e = ents.get(key)
            if e is None:
                continue
            rows = rows_of(e)
            first = next((r for r in rows if r[0] is not None), None)
            if first is None:
                continue
            end = next(
                (
                    ticks[e["t0"] + i]
                    for i, r in enumerate(rows)
                    if r[5] is not None and r[5] not in DEPLOY_STATES
                ),
                None,
            )
            reach = None
            for r in rows:
                if r[5] == ATTACK_STATE and r[3] in tower_pos:
                    tx, ty = tower_pos[r[3]]
                    reach = round(math.hypot(r[0] - tx, r[1] - ty))
                    break
            members.append(
                {
                    "key": key,
                    "offset": [first[0] - px, first[1] - py],
                    "deploy_end_tick": end,
                    "stagger": (end - d["tick"]) if end is not None else None,
                    "reach_to_tower": reach,
                }
            )
        out.append(
            {
                "fixture": fx["capture"].split(".native.oracle")[0],
                "tick": d["tick"],
                "side": d["side"],
                "card": d["card"],
                "count": d["count"],
                "pos": d["pos"],
                "source": d["source"],
                "members": members,
            }
        )
    return out


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("fixtures", nargs="*")
    ap.add_argument("--card", default=None)
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()
    paths = args.fixtures or sorted(glob.glob(os.path.join(REPLAY, "*.replay.json")))
    rows = []
    for p in paths:
        with open(p, encoding="utf-8") as fh:
            fx = json.load(fh)
        if not fx.get("truth"):
            continue
        rows.extend(g for g in group_rows(fx) if args.card is None or g["card"] == args.card)
    if args.json:
        json.dump(rows, sys.stdout, indent=1)
        print()
        return 0
    for g in rows:
        print(
            f"{g['fixture']} t{g['tick']} side {g['side']} {g['card']} x{g['count']}"
            f" at {g['pos']} ({g['source']})"
        )
        for m in g["members"]:
            print(
                f"    key {m['key']}: offset {m['offset']}, deploy end {m['deploy_end_tick']}"
                f" (spawn + {m['stagger']}), reach to tower {m['reach_to_tower']}"
            )
    return 0


if __name__ == "__main__":
    sys.exit(main())
