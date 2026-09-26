"""Death spawns with DeathSpawnPushback appear on a fixed small ring and slide out (spawner.DEATH_SPAWN_PUSHBACK).

WHAT THIS PINS. On the live 16.402 corpus every Golem and LavaHound death (both have DeathSpawnPushback true) puts
its children at radius 250 from the death point, at the fixed angles -(k+1) x 360 / n in the native frame (0 = +x),
whatever the parent's heading. The children then slide straight outward, about 250 a tick, and stop at exactly
DeathSpawnRadius (Golem 1500, LavaHound 2500). The Battle Ram (DeathSpawnPushback blank) is different: its Barbarians
appear at 600 on its heading on the first frame and do not slide, which the engine already does. Today's engine places
every death spawn at DeathSpawnRadius on the parent's facing on the first frame.

WHY THE CONTROLS ARE HERE. A parent whose heading is a multiple of 360 / n cannot tell a fixed ring from one that
turns with it, so each scenario asserts that the heading is at least 15 degrees from every such multiple. The client
15.535.29 death-layout scenarios (a Golem and a Lava Hound of each side, the side-1 runs rotated 180 degrees) settle
the two questions the corpus left open: the ring does not turn with the owner either, and member k in CREATION order
sits at -(k+1) x 360 / n. So both sides are tested and the angles are checked member by member, in uid order. The
Battle Ram test holds the new arm to leaving a blank-DeathSpawnPushback row alone, and the old-arm test is today's
engine.
"""

from __future__ import annotations

import json
import math
from itertools import pairwise

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "spawner.DEATH_SPAWN_PUSHBACK"
NEW_ARM, OLD_ARM = "client_ring_slide", "not_read"
# ENTITY_FIELDS: 0 uid, 1 team, 4 tower_slot, 5 x, 6 y
UID, TEAM, SLOT, X, Y = 0, 1, 4, 5, 6
START_R, SLIDE_STEP = 250, 250


def run(card: str, killer: str, killer_y: int, arm: str | None, ticks: int = 140, side: int = 0):
    """`side`'s `card` at 1 hp at (5000, 11000) and the other side's `killer` at (5000, killer_y), both rotated 180
    degrees about the arena centre for side 1: the killer's first hit kills it while it walks, so its heading is well
    off the x axis. Returns (parent heading in degrees, the death centre, and per child, in creation (uid) order, the
    list of (tick, x, y)); the centre is the mean of the children's first positions."""
    overrides = {} if arm is None else {KEY: json.dumps(arm)}
    b = royalesim.Battle([card, killer], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)

    def at(x, y):
        return (x, y) if side == 0 else (18000 - x, 32000 - y)

    (px, py), (kx, ky) = at(5000, 11000), at(5000, killer_y)
    units = [(side, 0, px * SUB, py * SUB, 1), (1 - side, 1, kx * SUB, ky * SUB, -1)]
    b.reset(0, [[0, 1] * 4, [0, 1] * 4], 0, 200, [10_000, 10_000], None, units)
    parent, last, prev, kids = None, None, None, {}
    for t in range(ticks):
        own = [e for e in json.loads(b.state_json())["entities"] if e[TEAM] == side and e[SLOT] < 0]
        if parent is None and own:
            parent = own[0][UID]
        me = next((e for e in own if e[UID] == parent), None)
        if me is not None:
            prev, last = last, (me[X] / SUB, me[Y] / SUB)
        for e in own:
            if e[UID] != parent:
                kids.setdefault(e[UID], []).append((t, e[X] / SUB, e[Y] / SUB))
        b.step([], 1)
    assert kids, f"the {card} never died"
    heading = math.degrees(math.atan2(last[1] - prev[1], last[0] - prev[0])) % 360
    ordered = [kids[uid] for uid in sorted(kids)]
    firsts = [tr[0] for tr in ordered]
    centre = (sum(f[1] for f in firsts) / len(firsts), sum(f[2] for f in firsts) / len(firsts))
    return heading, centre, ordered


def polar(p, centre):
    dx, dy = p[1] - centre[0], p[2] - centre[1]
    return math.hypot(dx, dy), math.degrees(math.atan2(dy, dx)) % 360


def gap(a: float, b: float, period: float = 360) -> float:
    d = abs(a - b) % period
    return min(d, period - d)


def assert_ring_slide(card, killer, killer_y, n, radius, slide_by, side=0):
    heading, centre, kids = run(card, killer, killer_y, NEW_ARM, side=side)
    assert len(kids) == n, f"{len(kids)} children, not {n}"
    assert len({tr[0][0] for tr in kids}) == 1, "the children did not appear together"
    # a facing ring (heading + k x 360/n) and the fixed ring coincide when the heading is a multiple of 360/n
    assert gap(heading, 0, 360 / n) >= 15, f"heading {heading:.0f} cannot discriminate"
    firsts = [polar(tr[0], centre) for tr in kids]
    assert all(abs(r - START_R) <= 10 for r, _ in firsts), [round(r) for r, _ in firsts]
    # member k in creation order at -(k+1) x 360/n in the ARENA frame, on both sides (client 15.535.29, both sides)
    want = [(-(k + 1) * 360 / n) % 360 for k in range(n)]
    got = [a for _, a in firsts]
    assert all(gap(g, w) <= 5 for g, w in zip(got, want, strict=True)), (got, want)
    for tr in kids:
        radii = [polar(p, centre)[0] for p in tr[: slide_by + 1]]
        assert max(radii) <= radius + 3, radii
        # every child moves straight out for its first 3 steps and is past 1000 by then. After that a flyer may leave
        # the slide for its target: on client 15.535.29 the Pup that slides directly away from its target turns back
        # after about 4 ticks (radius 1442, then 1406), so no later frame is asserted here
        early = radii[:4]
        assert all(b > a for a, b in pairwise(early)), f"not sliding out: {radii}"
        assert early[3] >= 1000, f"not sliding out: {radii}"
    return kids, centre


@pytest.mark.parametrize("side", [0, 1])
def test_golemites_start_on_the_x_axis_at_250_and_slide_to_1500(side):
    kids, centre = assert_ring_slide("Golem", "Knight", 12700, n=2, radius=1500, slide_by=5, side=side)
    for tr in kids:
        radii = [polar(p, centre)[0] for p in tr[:7]]
        reach = next((i for i, r in enumerate(radii) if r >= 1497), None)
        assert reach == 5, f"reached 1500 on frame {reach}, not 5: {radii}"
        assert abs(radii[5] - 1500) <= 3, radii
        # the steps inside the slide, after the first (which carries the newborns' separation) and before the clamp;
        # after the clamp the Golemite walks, so later frames are not the slide
        middle = [b - a for a, b in pairwise(radii[1:reach])]
        assert all(abs(s - SLIDE_STEP) <= 5 for s in middle), radii


@pytest.mark.parametrize("side", [0, 1])
def test_lava_pups_start_on_a_fixed_hexagon_and_slide_out(side):
    assert_ring_slide("LavaHound", "Musketeer", 14000, n=6, radius=2500, slide_by=8, side=side)


def test_the_battle_ram_keeps_its_facing_ring():
    """DeathSpawnPushback is blank on the Battle Ram: 600 on its heading on the first frame, no slide (the corpus's two
    Ram deaths, and today's engine)."""
    heading, centre, kids = run("BattleRam", "Knight", 12700, NEW_ARM)
    assert len(kids) == 2
    firsts = [polar(tr[0], centre) for tr in kids]
    assert all(abs(r - 600) <= 5 for r, _ in firsts), firsts
    assert all(gap(a, heading, 180) <= 5 for _, a in firsts), (heading, firsts)


def test_the_old_arm_is_todays_engine():
    """Checked on the shared build of 2026-09-25 15:10 with the key dropped: the Golemites appear at 1505 on the Golem's
    heading (121) on the first frame, and the Pups at 2501 on the Hound's heading (91) plus k x 60."""
    cases = (("Golem", "Knight", 12700, 2, 1500), ("LavaHound", "Musketeer", 14000, 6, 2500))
    for card, killer, killer_y, n, radius in cases:
        heading, centre, kids = run(card, killer, killer_y, OLD_ARM)
        firsts = [polar(tr[0], centre) for tr in kids]
        assert len(kids) == n
        assert all(abs(r - radius) <= 10 for r, _ in firsts), (card, firsts)
        assert all(gap(a, heading, 360 / n) <= 5 for _, a in firsts), (card, heading, firsts)
