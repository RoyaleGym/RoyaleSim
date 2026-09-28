"""Where a troop tap on an own crown tower's box goes (placement.TOWER_TAP_PUSH, under placement.TROOP_TOWER_TAPS).

WHAT THIS PINS. On client 15.535.29 a Knight was tapped on every tile centre of all four own princess boxes, both
seats (36 taps), with 4 taps one tile outside and 2 on a downed princess's box. 42 of 42 fit one rule: push the unit
out along the axis where the RAW tap is farther from the tower's centre; on an exact tie take the first OUTWARD
direction in the fixed ARENA order -y, -x, +y, +x; land on the first tile centre beyond the box, on the tapped tile's
row or column. A tap outside the box, or on a downed princess's box, stays. The four boxes agree, so the order is not
mirrored between the seats. Today's engine (ring_nearest) takes the nearest fitting tile and breaks ties column-major
in the placer's frame: it lands 12 of the 36 box taps on another tile. The 16.402 corpus has two ties, side 1 at
(4500, 24500) in two battles; both went -y to (4499, 23499), and ring_nearest sends them +x.

THE RAW TAP, NOT ITS TILE: pairs of taps on one tile, on the two sides of its diagonal, went opposite ways on client
15.535.29 (6 of 6). So the rule reads the tap before placement.TAP_SNAP snaps it; the pairs are checked under both
TAP_SNAP arms.

THE KING'S TIES, in arena coordinates: side 0's (10500, 1500) went -y, side 1's (7500, 30500) -x and side 1's
(10500, 30500) +y (standing on 31000 under formation.GROUND_Y_CLAMP). ring_nearest gets 1 of the 3.

The landing is compared by tile: a ground unit's one-native deploy-point shift is formation.GROUND_DEPLOY_POINT's.
"""

from __future__ import annotations

import json
import pathlib

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
KEY = "placement.TOWER_TAP_PUSH"
NEW_ARM, OLD_ARM = "client16402_axis_push", "ring_nearest"
RELOCATE = {"placement.TROOP_TOWER_TAPS": json.dumps("client16402_half_open_relocate")}
DECK = ["Knight", "Archer", "Giant", "Musketeer", "Cannon", "Tesla", "Goblins", "Minions"]
KNIGHT = 0
LEDGER = pathlib.Path(__file__).resolve().parent.parent / "data" / "calibration.json"

#: (side, tap, where the Knight stood on client 15.535.29, as its tile centre), native units, arena frame. The 36 box
#: taps, then the 4 taps one tile outside (they stay).
BOX_TAPS_15535 = [
    (0, (2500, 5500), (2500, 4500)), (0, (3500, 5500), (3500, 4500)), (0, (4500, 5500), (4500, 4500)),
    (0, (2500, 6500), (1500, 6500)), (0, (3500, 6500), (3500, 4500)), (0, (4500, 6500), (5500, 6500)),
    (0, (2500, 7500), (1500, 7500)), (0, (3500, 7500), (3500, 8500)), (0, (4500, 7500), (4500, 8500)),
    (0, (13500, 5500), (13500, 4500)), (0, (14500, 5500), (14500, 4500)), (0, (15500, 5500), (15500, 4500)),
    (0, (13500, 6500), (12500, 6500)), (0, (14500, 6500), (14500, 4500)), (0, (15500, 6500), (16500, 6500)),
    (0, (13500, 7500), (12500, 7500)), (0, (14500, 7500), (14500, 8500)), (0, (15500, 7500), (15500, 8500)),
    (1, (2500, 24500), (2500, 23500)), (1, (3500, 24500), (3500, 23500)), (1, (4500, 24500), (4500, 23500)),
    (1, (2500, 25500), (1500, 25500)), (1, (3500, 25500), (3500, 23500)), (1, (4500, 25500), (5500, 25500)),
    (1, (2500, 26500), (1500, 26500)), (1, (3500, 26500), (3500, 27500)), (1, (4500, 26500), (4500, 27500)),
    (1, (13500, 24500), (13500, 23500)), (1, (14500, 24500), (14500, 23500)), (1, (15500, 24500), (15500, 23500)),
    (1, (13500, 25500), (12500, 25500)), (1, (14500, 25500), (14500, 23500)), (1, (15500, 25500), (16500, 25500)),
    (1, (13500, 26500), (12500, 26500)), (1, (14500, 26500), (14500, 27500)), (1, (15500, 26500), (15500, 27500)),
    (0, (5500, 6500), (5500, 6500)), (0, (12500, 6500), (12500, 6500)),
    (1, (5500, 25500), (5500, 25500)), (1, (12500, 25500), (12500, 25500)),
]
#: the 12 box taps today's ring search lands elsewhere: every centre, and the corners and edges its order gets wrong
RING_MISSES = {
    (0, (2500, 5500)), (0, (3500, 6500)), (0, (13500, 5500)), (0, (14500, 6500)),
    (1, (2500, 26500)), (1, (3500, 25500)), (1, (4500, 24500)), (1, (4500, 26500)),
    (1, (13500, 26500)), (1, (14500, 25500)), (1, (15500, 24500)), (1, (15500, 26500)),
}
#: side 1's arena-right princess downed: its centre and its (-1, +1) corner were accepted where tapped
DOWNED_15535 = [(1, (14500, 25500)), (1, (13500, 26500))]
#: taps on one tile, either side of its diagonal (side 0): the tap's side decides, not the tile
PAIRS_15535 = [
    ((10501, 1001), (10500, 500)), ((10999, 1499), (11500, 1500)),   # the king, tile (10, 1)
    ((2600, 5400), (2500, 4500)), ((2400, 5600), (1500, 5500)),      # the left princess, tile (2, 5)
    ((4400, 7600), (4500, 8500)), ((4600, 7400), (5500, 7500)),      # the left princess, tile (4, 7)
]
#: the king's three ties, arena frame; side 1's +y stands on 31000 (formation.GROUND_Y_CLAMP)
KING_TIES_15535 = [
    (0, (10500, 1500), (10500, 500)), (1, (7500, 30500), (6500, 30500)), (1, (10500, 30500), (10500, 31000)),
]


def tile(p):
    return (p[0] // 1000, p[1] // 1000)


def land(side, tap, arm, downed=None, tap_snap=None):
    """Where a Knight tapped at `tap` stands after its play (native), or None when the play is refused. `arm` None runs
    the build's own value of the key; `downed` is (side, x) of a princess destroyed before the tap."""
    overrides = dict(RELOCATE)
    if arm is not None:
        overrides[KEY] = json.dumps(arm)
    if tap_snap is not None:
        overrides["placement.TAP_SNAP"] = json.dumps(tap_snap)
    b = royalesim.Battle(DECK, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    hp = None
    if downed is not None:
        towers = b.tower_positions()
        hp = [[4824, 3052, 3052], [4824, 3052, 3052]]
        k = next(k for k, (x, _) in enumerate(towers[downed[0]]) if k > 0 and x == downed[1] * SUB)
        hp[downed[0]][k] = 0
    ids = list(range(len(DECK)))
    b.reset(0, [ids, ids], 0, 200, [10_000, 10_000], hp, [])
    b.step([], 1)
    got = b.step([(side, KNIGHT, tap[0] * SUB, tap[1] * SUB)], 1)
    if not got or got[0][1] != 0:
        return None
    rows = [e for e in json.loads(b.state_json())["entities"] if e[F["team"]] == side and e[F["tower_slot"]] < 0]
    assert len(rows) == 1, rows
    return rows[0][F["x"]] // SUB, rows[0][F["y"]] // SUB


@pytest.mark.parametrize("arm", [NEW_ARM])
def test_every_princess_box_tap_lands_as_on_client_15535(arm):
    off = []
    for side, tap, want in BOX_TAPS_15535:
        got = land(side, tap, arm)
        if got is None or tile(got) != tile(want):
            off.append((side, tap, "client", want, "engine", got))
    assert not off, f"{arm}: {len(off)} of {len(BOX_TAPS_15535)} taps land off the client's tile: {off}"


@pytest.mark.parametrize("arm", [NEW_ARM, OLD_ARM])
def test_a_tap_on_a_downed_princess_box_stays(arm):
    for side, tap in DOWNED_15535:
        standing = land(side, tap, arm)
        assert standing is not None, f"the scene drifted: with the princess standing, {tap} is refused"
        assert tile(standing) != tile(tap), f"the scene drifted: with the princess standing, {tap} is not moved"
        got = land(side, tap, arm, downed=(side, 14500))
        assert got is not None, f"{arm}: {tap} on a downed princess's box is refused"
        assert tile(got) == tile(tap), f"{arm}: {tap} on a downed princess's box landed on {got}"


@pytest.mark.parametrize("tap_snap", ["none", "client16402_tile_centre"])
@pytest.mark.parametrize("arm", [NEW_ARM])
def test_the_raw_tap_not_its_tile_decides(arm, tap_snap):
    off = []
    for tap, want in PAIRS_15535:
        got = land(0, tap, arm, tap_snap=tap_snap)
        if got is None or tile(got) != tile(want):
            off.append((tap, "client", want, "engine", got))
    assert not off, f"{arm}, TAP_SNAP {tap_snap}: {off}"


@pytest.mark.parametrize("arm", [NEW_ARM])
def test_the_kings_ties_go_the_arena_order(arm):
    off = []
    for side, tap, want in KING_TIES_15535:
        got = land(side, tap, arm)
        if got is None or tile(got) != tile(want):
            off.append((side, tap, "client", want, "engine", got))
    assert not off, f"{arm}: {off}"


def test_the_old_arm_is_todays_ring_search():
    """The table separates the arms: under ring_nearest exactly these 12 box taps land off the client's tile."""
    off = set()
    for side, tap, want in BOX_TAPS_15535:
        got = land(side, tap, OLD_ARM)
        if got is None or tile(got) != tile(want):
            off.add((side, tap))
    assert off == RING_MISSES, f"ring_nearest now misses {sorted(off)}"


def test_the_shipped_value_is_the_axis_push():
    # Switched on at the 2026-09-28 placement batch, with placement.TAP_SNAP.
    entry = json.loads(LEDGER.read_text(encoding="utf-8"))["placement"]["TOWER_TAP_PUSH"]
    assert entry["value"] == NEW_ARM
    assert entry["candidates"] == [OLD_ARM, NEW_ARM]
