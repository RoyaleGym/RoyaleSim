#!/usr/bin/env python3
"""The measured summon formations, as a small committed fixture for tests/formations.rs.

python tools/make_formation_fixture.py [--check] [--out PATH]

WHAT IT WRITES: crates/royalesim/tests/fixtures/formations/measured.json -- for every
multi-unit CARD of the live 16.402 replay fixtures (data/derived/replay/*.replay.json,
tools/make_replay_fixture.py), a handful of CLEAN deploy groups per (card, side, lane
half, TAP ROW): the tap in native units, each member's first-frame offset from it and its
deploy-end stagger, exactly as tools/replay_formations.py reads them off the truth.
tests/formations.rs replays each group through the engine's `formation_preview` and
compares member by member, so the numbers the gate pins are the game's, not pasted.

WHY IT EXISTS: the engine's square-grid formation put every swarm 500-1500 native
from where the game lays it (the formation gap of the corpus report: 33 of 48 first
divergences); the layout of formation.rs was fitted to these groups and this is the
evidence it is held to.

CLEAN means: the group is one the capture saw ARRIVE (every member in a deploy
state on its first frame -- a recording's opening frame reports units already
walking and the detector groups those like a deploy), the group's tap is a
placement-log tile (`tap_tile`; a `centroid` source is admitted for a card with
no tap-tile group on that side / lane, and flagged), every
member has a first frame and a deploy end, no OTHER unit (any side, towers and the
group's own members excluded) stood within CLEAR_NATIVE of any member on the group's
first frame, and no GROUND member stands within a crown tower's collision box widened
by TOWER_MARGIN (the contact law throws a member laid inside a tower's box out over
its first frames, and its siblings with it: the Spear Goblins of
capture 20260920-072148 t2272 tapped onto the Red king) -- so the only thing that
can have moved a member off its ring point is a sibling it overlaps (the Minions'
577-ring at CollisionRadius 500, the Skeleton Army's inner spiral), which the test
tolerates per member by the engine's own overlap test on its predicted point. Each
group also carries `towers_down`: the crown towers (side, slot) already destroyed on
its tick, which the game's ground clamp depends on (a fallen tower opens the column
past the river and drops the clamp).

What it cannot catch: a group pushed by a unit the capture does not hold on that frame,
or a tap the placement log recorded a tile away from where the game centred the
formation (a deploy snapped off a footprint): the test matches those up to one shift.

NAMES: a group's `fixture` is the replay fixture's `capture` name
(tools/make_replay_fixture.py NAMES: the seats are letters, A, B, ... in sort order,
so the two seats of one battle are "<stamp>-A" and "<stamp>-B").

--check: exit 1 if the committed fixture differs from what the fixtures on disk give.
Exit 0 clean, 1 defect / stale, 2 usage.
"""

from __future__ import annotations

import argparse
import glob
import json
import math
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPLAY = os.path.join(ROOT, "data", "derived", "replay")
OUT = os.path.join(ROOT, "crates", "royalesim", "tests", "fixtures", "formations", "measured.json")
sys.path.insert(0, os.path.join(ROOT, "tools"))
from replay_formations import DEPLOY_STATES, group_rows, rows_of  # noqa: E402

CLEAR_NATIVE = 2500
TILE_NATIVE = 1000
TOWER_MARGIN = 500
PER_BUCKET = 2
CENTRE_X = 9000
# The seat letter of a capture name ("20260920-083112-A", "20260920-074051-A.b2").
SEAT_LETTER = re.compile(r"-[A-Z](?=\.|$)")


def battle_of(fixture: str) -> str:
    """The capture name without its seat letter: the two seats of one battle share it."""
    return SEAT_LETTER.sub("", fixture)


def first_frame_positions(fx: dict, tick: int) -> dict[int, tuple[int, int]]:
    """key -> (x, y) of every entity present on the first truth frame at or after `tick`."""
    ticks = fx["truth"]["ticks"]
    fi = next((i for i, t in enumerate(ticks) if t >= tick), None)
    if fi is None:
        return {}
    out = {}
    for e in fx["truth"]["entities"]:
        if e["card_id"] == -1:
            continue  # towers
        i = fi - e["t0"]
        rows = rows_of(e)
        if 0 <= i < len(rows) and rows[i][0] is not None:
            out[e["key"]] = (rows[i][0], rows[i][1])
    return out


def tower_radii() -> dict[str, int]:
    """KingTower / PrincessTower collision radius (native) from cards.json."""
    return {t["name"]: t["collision_radius_milli"] for t in cards_doc()["towers"]}


def cards_doc() -> dict:
    with open(os.path.join(ROOT, "data", "derived", "cards.json"), encoding="utf-8") as fh:
        return json.load(fh)


def flying_cards() -> set[str]:
    """Cards whose unit flies (no building contact): their members are never thrown by a tower."""
    return {c["name"] for c in cards_doc()["cards"] if (c.get("flying_height") or 0) > 0}


def towers_state(fx: dict, tick: int, radii: dict[str, int]) -> tuple[list[dict], list[list[int]]]:
    """(the six towers with box radius and liveness, the (side, slot) pairs dead at `tick`)."""
    ticks = fx["truth"]["ticks"]
    fi = next((i for i, t in enumerate(ticks) if t >= tick), len(ticks) - 1)
    boxes, down = [], []
    for t in fx["towers"]:
        rec = next(
            (
                e
                for e in fx["truth"]["entities"]
                if e["card_id"] == -1
                and e["side"] == t["side"]
                and rows_of(e)[0][:2] == (t["x"], t["y"])
            ),
            None,
        )
        alive = False
        if rec is not None:
            i = fi - rec["t0"]
            rows = rows_of(rec)
            alive = 0 <= i < len(rows) and rows[i][0] is not None and rows[i][2] > 0
        if not alive:
            down.append([t["side"], t["slot"]])
        r = radii["KingTower" if t["slot"] == 0 else "PrincessTower"]
        boxes.append({"x": t["x"], "y": t["y"], "r": r, "alive": alive})
    return boxes, down


def spawn_tick_slack(d: dict) -> int:
    """How many ticks EARLIER than the group's recorded tick its spawn may have been.

    A capture holds one frame per tick and misses frames now and then, so a group's
    spawn tick is only bounded: it lies in (the previous recorded frame's tick, the
    first-seen tick] -- tools/make_replay_fixture.py's own rule, which records the
    width as `first_seen_gap` and its verdict as `tick_evidence`. Where that verdict
    is not "exact" the range still held several ticks and the LATEST was taken, so
    the true spawn can be that many ticks earlier and every member's deploy end then
    looks that much earlier than the engine's. Without this the stagger gate reads a
    capture's frame loss as an engine defect: capture 20260918-122757.b1 t2415, whose
    own tick_evidence is "range [2414, 2415] (deploy-end transition), latest used",
    is the one group of the corpus that needs it.
    """
    if str(d.get("tick_evidence", "")).startswith("exact"):
        return 0
    gap = d.get("first_seen_gap") or 1
    first_seen = d.get("first_seen")
    if first_seen is None:  # a spell cast, or a group the capture never shows arriving
        return 0
    return max(0, d["tick"] - (first_seen - gap + 1))


def spawned_deploying(fx: dict, g: dict) -> bool:
    """Is this group a DEPLOY the capture actually saw arrive?

    A recording's first frame reports every unit already on the board, and the
    deploy detector groups those by (side, card) like any other arrival: capture
    20260918-130203.b1 t2604 is the start of a recording and its "Archer deploy"
    stands two Archers nine tiles apart, already walking. A real deploy's members
    are in a deploy state (4 or 11) on their first frame, so require that.
    """
    ents = {e["key"]: e for e in fx["truth"]["entities"]}
    for m in g["members"]:
        e = ents.get(m["key"])
        if e is None:
            return False
        first = next((r for r in rows_of(e) if r[0] is not None), None)
        if first is None or first[5] not in DEPLOY_STATES:
            return False
    return True


def collect(paths: list[str]) -> list[dict]:
    groups = []
    radii = tower_radii()
    flying = flying_cards()
    for p in paths:
        with open(p, encoding="utf-8") as fh:
            fx = json.load(fh)
        if not fx.get("truth"):
            continue
        slack_of = {(d["tick"], d["card"], d["side"]): spawn_tick_slack(d) for d in fx["deploys"]}
        for g in group_rows(fx):
            if len(g["members"]) < 2:
                continue
            if any(m["stagger"] is None for m in g["members"]):
                continue
            if not spawned_deploying(fx, g):
                continue
            present = first_frame_positions(fx, g["tick"])
            boxes, down = towers_state(fx, g["tick"], radii)
            own = set(m["key"] for m in g["members"])
            px, py = g["pos"]
            clean = True
            for m in g["members"]:
                mx, my = px + m["offset"][0], py + m["offset"][1]
                for key, (x, y) in present.items():
                    if key in own:
                        continue
                    if math.hypot(x - mx, y - my) < CLEAR_NATIVE:
                        clean = False
                        break
                if g["card"] not in flying:
                    for b in boxes:
                        reach = b["r"] + TOWER_MARGIN
                        if b["alive"] and abs(mx - b["x"]) < reach and abs(my - b["y"]) < reach:
                            clean = False
                            break
                if not clean:
                    break
            if not clean:
                continue
            groups.append(
                {
                    "fixture": g["fixture"],
                    "tick": g["tick"],
                    "side": g["side"],
                    "card": g["card"],
                    "source": g["source"],
                    "tap": g["pos"],
                    "tick_slack": slack_of.get((g["tick"], g["card"], g["side"]), 0),
                    "towers_down": down,
                    "members": [
                        {"offset": m["offset"], "stagger": m["stagger"]} for m in g["members"]
                    ],
                }
            )
    return groups


def select(groups: list[dict]) -> list[dict]:
    """A few per (card, side, lane half, TAP ROW), tap-tile sources first, earliest ticks first.

    The tap ROW is in the key because the ground clamp only bites near the ends of a
    column, so a bucket that collapsed every row kept the mid-arena deploys and threw
    away the one group that separates a bound from its seat rotation (calibration
    formation.GROUND_Y_CLAMP: capture 20260918-121158 t2439 is the whole corpus'
    evidence for side 1's river bound, and a row-blind bucket dropped it).
    """
    buckets: dict[tuple, list[dict]] = {}
    for g in sorted(groups, key=lambda g: (g["source"] != "tap_tile", g["fixture"], g["tick"])):
        b = (g["card"], g["side"], g["tap"][0] >= CENTRE_X, g["tap"][1] // TILE_NATIVE)
        buckets.setdefault(b, [])
        if len(buckets[b]) < PER_BUCKET:
            # one per battle-and-tap (the twin captures of one battle repeat the same deploy)
            same = (g["tap"], battle_of(g["fixture"]), g["tick"])
            if any((h["tap"], battle_of(h["fixture"]), h["tick"]) == same for h in buckets[b]):
                continue
            buckets[b].append(g)
    out = [g for b in sorted(buckets) for g in buckets[b]]
    return out


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--out", default=OUT)
    args = ap.parse_args()
    paths = sorted(glob.glob(os.path.join(REPLAY, "*.replay.json")))
    if not paths:
        print(f"no replay fixtures under {REPLAY}: run tools/make_replay_fixture.py --all first")
        return 2
    groups = select(collect(paths))
    cards = sorted(set(g["card"] for g in groups))
    if len(groups) < 20 or len(cards) < 8:
        print(f"VACUOUS: only {len(groups)} clean groups over {len(cards)} cards")
        return 1
    doc = {
        "source": (
            "tools/make_formation_fixture.py over data/derived/replay/*.replay.json "
            "(tools/replay_formations.py rows); native units; offsets from the tap on the "
            "group's first truth frame; stagger = deploy-end tick - spawn tick; "
            "tick_slack = how many ticks earlier the spawn may have been (frame loss)"
        ),
        "clear_native": CLEAR_NATIVE,
        "per_bucket": PER_BUCKET,
        "groups": groups,
    }
    text = json.dumps(doc, indent=1) + "\n"
    if args.check:
        try:
            with open(args.out, encoding="utf-8") as fh:
                old = fh.read()
        except FileNotFoundError:
            print(f"STALE: {args.out} missing")
            return 1
        if old != text:
            print(f"STALE: {args.out} differs from the replay fixtures on disk")
            return 1
        print(f"current: {len(groups)} groups, {len(cards)} cards")
        return 0
    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    with open(args.out, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)
    print(f"wrote {args.out}: {len(groups)} groups over {len(cards)} cards: {', '.join(cards)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
