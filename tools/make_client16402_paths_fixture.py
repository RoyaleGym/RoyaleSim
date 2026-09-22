#!/usr/bin/env python3
"""REGENERATE crates/royalesim/tests/fixtures/oracle2026/client16402_first_paths.json.

    python tools/make_client16402_paths_fixture.py            # rewrite
    python tools/make_client16402_paths_fixture.py --check    # diff only (exit 1 on a difference)

WHERE THE INPUTS ARE
    The live captures come from the client instrument that records ground-truth traces from
    the real game (CR 16.402). Two variables name them, neither with a default:
      ROYALELIVE_REPORTS  the captures folder
      ROYALELIVE_SAMPLER  the file that turns a capture into cases, as a filesystem path
    The offline 15.535 traces stay in this repo (data/oracle-native/, gitignored).

WHAT IT HOLDS
    Every first path the LIVE client (CR 16.402) and the offline
    oracle (CR 15.535, data/oracle-native/lane_sweep_Knight) ever published for a ground troop
    with a resolvable card row, exactly as the sampler samples them: the mover's
    position on the tick BEFORE the list appeared (the position the
    plan was made from), the target's position, Range + own CollisionRadius, every building of
    BOTH sides alive on that tick (absolute native centre, CollisionRadius) and the published
    node list, goal first. The Rust gate (tests/oracle2026.rs
    `g6_the_client_search_reproduces_every_published_node_sequence`) runs path16402.rs on
    each case and demands the exact sequence.

    `moving_target` marks the cases whose target was a TROOP on the move: the sample's target
    position is the previous tick's, so the goal cell the game chose is not recoverable and the
    gate skips them (they are listed by name in the test so a new one cannot hide).

    CASE NAMES are `<capture>:<entity>:<Card>`, the seats named A, B, ... in sort order
    (tools/capture_names.py `folder_seats`, computed over the whole captures folder so that
    every fixture spells a seat the same way), so a case name carries the capture and the seat and nothing
    else. Names are unique: a capture reaching the sampler twice contributes its cases once.
"""
from __future__ import annotations

import importlib
import json
import os
import pathlib
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from capture_names import argv_guard, folder_seats, public_name  # noqa: E402

LIVE = os.environ.get("ROYALELIVE_REPORTS")
if not LIVE:
    sys.exit("set ROYALELIVE_REPORTS to the folder holding the *.native.oracle.jsonl.gz captures")
# Which file does the sampling is the caller's business, so this repo neither names nor
# guesses it: it is a path, given outright. If that file sits in a package, import it under
# the name the package gives it, so its own imports resolve.
SAMPLER = os.environ.get("ROYALELIVE_SAMPLER")
if not SAMPLER:
    sys.exit("set ROYALELIVE_SAMPLER to the path of the file that samples a capture into cases")
_file = pathlib.Path(SAMPLER).resolve()
if not _file.is_file():
    sys.exit(f"no sampler at {_file} (ROYALELIVE_SAMPLER)")
_parts = [_file.stem]
_root = _file.parent
while (_root / "__init__.py").is_file():
    _parts.insert(0, _root.name)
    _root = _root.parent
sys.path.insert(0, str(_root))
LE = importlib.import_module(".".join(_parts))
LE.LIVE = pathlib.Path(LIVE)

OUT = os.path.join(ROOT, "crates", "royalesim", "tests", "fixtures", "oracle2026", "client16402_first_paths.json")

# Known residuals: units chasing a moving troop (see the module docstring). Anything else that
# fails the gate is a regression.
MOVING_TARGET = {
    "20260918-115249.b1:44:Goblins",
    "20260918-115249.b1:45:Goblins",
    "20260918-124946:69:Goblins",
}


def build() -> dict:
    live = LE.dedupe(LE.live_samples())
    offline = LE.offline_samples("lane_sweep_Knight")
    seats = folder_seats(LIVE)
    cases = []
    seen: dict[str, dict] = {}
    for group, samples in (("live_16402", live), ("offline_15535_lane_sweep", offline)):
        for s in samples:
            occl = [[x, y, r] for (x, y, r) in s.occluders] + [[x, y, r] for (x, y, r) in s.enemy_occluders]
            name = public_name(s.name, seats)
            case = {
                "name": name,
                "group": group,
                "card": s.card,
                "side": s.side,
                "start_native": list(s.start_xy),
                "target_native": list(s.target_xy),
                "reach_native": s.reach,
                "occluders_native": occl,
                "oracle_cells_goal_first": [[c, r] for (c, r) in s.oracle[::-1]],
                "moving_target": name in MOVING_TARGET,
            }
            # One case per name. A capture the folder carries under two names reaches the
            # sampler twice and would otherwise be scored twice by the gate; two DIFFERENT
            # measurements under one name is a naming fault and must not be papered over.
            if name in seen:
                if seen[name] != case:
                    raise SystemExit(f"two different cases named {name}")
                continue
            seen[name] = case
            cases.append(case)
    assert len({c["name"] for c in cases}) == len(cases)
    return {
        "$comment": "GENERATED by tools/make_client16402_paths_fixture.py from the live trace corpus "
                    "and the offline lane_sweep_Knight traces; do not edit by hand. oracle_cells_goal_first is the "
                    "list the game published (start cell and any node already consumed on that tick are absent, "
                    "so the gate compares the engine list's TAIL). Coordinates are native millitiles, absolute.",
        "cases": cases,
    }


def main() -> None:
    # Any other argument used to fall through to a rewrite of the committed gate fixture;
    # the docstring is the help.
    argv_guard(sys.argv[1:], __doc__)
    fixture = build()
    text = json.dumps(fixture, indent=1) + "\n"
    if "--check" in sys.argv:
        with open(OUT, encoding="utf-8") as f:
            if f.read() != text:
                print("FIXTURE DIFFERS from the traces; rerun without --check")
                sys.exit(1)
        print("fixture matches the traces")
        return
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    n = len(fixture["cases"])
    mt = sum(c["moving_target"] for c in fixture["cases"])
    print(f"wrote {OUT}: {n} cases ({mt} flagged moving_target)")


if __name__ == "__main__":
    main()
