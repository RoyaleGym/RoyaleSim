"""A walker on a tick with no step (a stomp pause) still drops a reached waypoint (pathfinding.ZERO_STEP_WAYPOINT_TEST).

WHAT THIS PINS. The reached test (pathfinding.WAYPOINT_ARRIVE_RULE: drop the next node once the remaining distance
along the frozen segment is at most 1000) runs on every tick a unit walks its route, including a tick whose step is
zero because its stomp pause holds it. Measured on client 15.535.29 and on client 16.402: on the zero-step walk ticks
where that test passes, the node is dropped on the tick itself, 6 of 6 on 15.535.29 (Giants in their pause, Bandits
standing before a dash) and 7 battles of 7 on 16.402 (Giants and an Ice Golem in their pause, Goblins stopped). A
knockback ladder's zero tick is not a walk tick and drops nothing: 28 of 28 on 15.535.29, 4 of 4 on 16.402.

THE SCENE. The 15.535.29 scenario of a red Giant walking at a blue Mortar: the Mortar at (9500, 13500) and the Giant
at (9500, 18499), both put down on tick 101. On tick 133 the Giant stands in its stomp pause and replans, and its new
route starts with the node (21, 35) about 852 away. The client drops that node on 133 and freezes its segment toward
the next one, (202, -156). The engine before this key waited for the next moving tick, 135, and froze the segment from
the moved position.

THE CHECK. The run under each arm is searched for the first tick whose step is zero while the Giant walks, whose route
changed on that tick other than by a pop, and whose new next node lies within the reached distance. Under "run" that
node is gone on the tick itself and the segment points at the node after it; under "skipped" it is still there, and
goes on the first moving tick.

WHICH ARM. skipped is the SHIPPED arm until its flip; run is the measured one. The tests pin each arm BY NAME through
the battle's calibration.
"""

from __future__ import annotations

import json
import math

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "pathfinding.ZERO_STEP_WAYPOINT_TEST"
SHIPPED_ARM, NEW_ARM = "skipped", "run"
MORTAR, GIANT = (9500, 13500), (9500, 18499)
DEPLOY_TICK, TICKS = 101, 170
ARRIVE = 1000


def centre(cell):
    return cell[0] * 500 + 250, cell[1] * 500 + 250


def trunc0(n, d):
    q = abs(n) // abs(d)
    return q if (n >= 0) == (d > 0) else -q


def norm256(dx, dy):
    """A segment direction: trunc0(v * 256 / isqrt(v.v)) per component."""
    n = math.isqrt(dx * dx + dy * dy)
    return trunc0(dx * 256, n), trunc0(dy * 256, n)


def remaining(node, pos, seg):
    """The reached test's quantity: the distance left along the frozen segment, summed per axis."""
    c = centre(node)
    return trunc0((c[0] - pos[0]) * seg[0], 256) + trunc0((c[1] - pos[1]) * seg[1], 256)


def overrides(arm):
    return {KEY: json.dumps(arm)}


def play(arm):
    """The Giant's (tick, position, route goal first, segment) after every tick from its deploy."""
    b = royalesim.Battle(["Mortar", "Giant"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [100000, 100000], None, [])
    rows = []
    for t in range(1, TICKS + 1):
        plays = [(0, 0, MORTAR[0] * SUB, MORTAR[1] * SUB), (1, 0, GIANT[0] * SUB, GIANT[1] * SUB)] \
            if t == DEPLOY_TICK else []
        b.step(plays, 1)
        giant = [u for u in b.debug_units() if u[1] == "Giant"]
        if giant:
            u = giant[0]
            seg = next((c[2], c[3]) for c in b.debug_contact() if c[0] == u[0])
            rows.append((t, (u[2] // SUB, u[3] // SUB), [tuple(c) for c in u[6]], seg))
    return rows


def paused_replan(rows):
    """The first zero-step tick with a replanned route whose next node is within the reached distance, under the
    SKIPPED arm (where that node is still on the route): (index, the node)."""
    for k in range(1, len(rows)):
        (_, p0, r0, _), (_, p1, r1, _) = rows[k - 1], rows[k]
        if p1 == p0 and r1 and r1 != r0 and r1 != r0[:-1]:
            node = r1[-1]
            if math.dist(centre(node), p1) <= ARRIVE:
                return k, node
    raise AssertionError("no paused replan with a reached node in the scene: it drifted")


def test_a_paused_walker_drops_a_reached_node_on_the_pause_tick():
    old, new = play(SHIPPED_ARM), play(NEW_ARM)
    k, node = paused_replan(old)
    assert old[:k] == new[:k], "precondition: the two arms part before the paused replan"
    t, pos, route, seg = new[k]
    assert pos == old[k - 1][1], f"precondition: the Giant moved on tick {t}"
    assert route == old[k][2][:-1], f"tick {t}: the node {node} is still on the route {route[-2:]}"
    assert seg == norm256(*(c - p for c, p in zip(centre(route[-1]), pos, strict=True))), (
        f"tick {t}: the segment {seg} is not frozen from the unmoved position toward {route[-1]}")


def test_the_skipped_arm_keeps_the_node_until_the_next_moving_tick():
    old = play(SHIPPED_ARM)
    k, node = paused_replan(old)
    moved = next(j for j in range(k + 1, len(old)) if old[j][1] != old[j - 1][1])
    held = [old[j][0] for j in range(k, moved) if old[j][2][-1] != node]
    assert not held, f"skipped: the node {node} left the route on the pause ticks {held}"
    assert old[moved][2][-1] != node, f"skipped: the node {node} was not dropped on the moving tick {old[moved][0]}"


def test_a_pause_without_a_reached_node_drops_nothing():
    """Control, under the measured arm: on the other pause ticks the next node is further along the segment than the
    reached distance, and it stays."""
    new = play(NEW_ARM)
    paused = [k for k in range(1, len(new))
              if new[k][1] == new[k - 1][1] and new[k - 1][2] and new[k - 1][3] != (0, 0)
              and new[k][2] in (new[k - 1][2], new[k - 1][2][:-1])  # no replan on the tick
              and remaining(new[k - 1][2][-1], new[k][1], new[k - 1][3]) > ARRIVE]
    assert len(paused) >= 4, f"precondition: only {len(paused)} such pause ticks in the scene"
    dropped = [new[k][0] for k in paused if new[k][2] == new[k - 1][2][:-1]]
    assert not dropped, f"run: a node further than {ARRIVE} along the segment was dropped on the pause ticks {dropped}"
