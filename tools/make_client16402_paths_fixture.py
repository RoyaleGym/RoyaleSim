#!/usr/bin/env python3
"""REGENERATE crates/royalesim/tests/fixtures/oracle2026/client16402_first_paths.json.

    python tools/make_client16402_paths_fixture.py            # rewrite
    python tools/make_client16402_paths_fixture.py --check    # diff only (exit 1 on a difference)

WHERE THE INPUTS ARE
    The live captures and the sampler that turns them into cases come from the sibling RoyaleLive
    checkout, the client instrument that records ground-truth traces from the real game:
      ROYALELIVE_DIR      the RoyaleLive folder (default ../RoyaleLive next to this repo)
      ROYALELIVE_SAMPLER  the dotted module path of the sampler inside it; when unset it is
                          read from <ROYALELIVE_DIR>/.sampler
      ROYALELIVE_REPORTS  the captures folder (default <ROYALELIVE_DIR>/reports)
    The offline 15.535 traces stay in this repo (data/oracle-native/, gitignored).

WHAT IT HOLDS
    Every first path the LIVE client (CR 16.402, the RoyaleLive captures) and the offline
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

    CASE NAMES are `<capture>:<entity>:<Card>`. A live capture recorded from both seats
    reaches the sampler twice; the fixture names the seats A and B, so a case name carries the
    capture and the seat and nothing else.

    The same corpus scored the same way in Python: 435/438 live, 128/128 offline.
"""
from __future__ import annotations

import importlib
import json
import os
import pathlib
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LIVE_DIR = pathlib.Path(os.environ.get("ROYALELIVE_DIR")
                        or os.path.join(os.path.dirname(ROOT), "RoyaleLive"))
# Which module in that checkout does the sampling is the checkout's own business, so this
# repo does not name it: take it from the environment, or from a one-line `.sampler` file
# beside the captures.
_sampler_file = LIVE_DIR / ".sampler"
SAMPLER = (os.environ.get("ROYALELIVE_SAMPLER")
           or (_sampler_file.read_text(encoding="utf-8").strip()
               if _sampler_file.is_file() else ""))
if not SAMPLER:
    sys.exit(f"set ROYALELIVE_SAMPLER, or put the sampler's dotted module path in {_sampler_file}")
if not (LIVE_DIR / pathlib.Path(*SAMPLER.split("."))).with_suffix(".py").is_file():
    sys.exit(f"sampler {SAMPLER} not found under {LIVE_DIR} (set ROYALELIVE_DIR)")
sys.path.insert(0, str(LIVE_DIR))
LE = importlib.import_module(SAMPLER)

if os.environ.get("ROYALELIVE_REPORTS"):
    LE.LIVE = pathlib.Path(os.environ["ROYALELIVE_REPORTS"])

OUT = os.path.join(ROOT, "crates", "royalesim", "tests", "fixtures", "oracle2026", "client16402_first_paths.json")

# Seats. A sampler name is `<stamp>[-<seat>][.<battle>]:<entity>:<Card>`; two names from one
# capture differ only in the `<seat>` field. The fixture rewrites it to a letter in sort order
# (A, B, ...), so case names are stable and carry only the capture and the seat. Stamps are
# 8+6 digits and never match.
SEAT_TAG = re.compile(r"-(\d{5})(?=[.:])")


def seat_letters(names: list[str]) -> dict[str, str]:
    tags = sorted({m.group(1) for n in names for m in [SEAT_TAG.search(n)] if m})
    return {tag: chr(ord("A") + i) for i, tag in enumerate(tags)}


def case_name(raw: str, seats: dict[str, str]) -> str:
    return SEAT_TAG.sub(lambda m: "-" + seats[m.group(1)], raw)

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
    seats = seat_letters([s.name for s in live] + [s.name for s in offline])
    cases = []
    for group, samples in (("live_16402", live), ("offline_15535_lane_sweep", offline)):
        for s in samples:
            occl = [[x, y, r] for (x, y, r) in s.occluders] + [[x, y, r] for (x, y, r) in s.enemy_occluders]
            cases.append({
                "name": case_name(s.name, seats),
                "group": group,
                "card": s.card,
                "side": s.side,
                "start_native": list(s.start_xy),
                "target_native": list(s.target_xy),
                "reach_native": s.reach,
                "occluders_native": occl,
                "oracle_cells_goal_first": [[c, r] for (c, r) in s.oracle[::-1]],
                "moving_target": case_name(s.name, seats) in MOVING_TARGET,
            })
    return {
        "$comment": "GENERATED by tools/make_client16402_paths_fixture.py from the live trace corpus "
                    "and the offline lane_sweep_Knight traces; do not edit by hand. oracle_cells_goal_first is the "
                    "list the game published (start cell and any node already consumed on that tick are absent, "
                    "so the gate compares the engine list's TAIL). Coordinates are native millitiles, absolute.",
        "cases": cases,
    }


def main() -> None:
    # Any other argument used to fall through to a rewrite (2026-09-21: `--help` regenerated
    # the gate fixture, 570 -> 736 cases); the docstring is the help.
    if sys.argv[1:] not in ([], ["--check"]):
        print(__doc__)
        sys.exit(0 if sys.argv[1:] in (["-h"], ["--help"]) else 2)
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
