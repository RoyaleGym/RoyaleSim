"""A unit a hook dragged plans a fresh route from where the drag left it (combat.HOOK_DRAG_ROUTE).

WHAT THIS PINS. The Fisherman's hook (combat.SPECIAL_HOOK) drags its victim straight at him, 510 a tick, until the next
step would bring the centres within DragMargin plus both radii. A victim that walks on after the drag (a Giant, which
targets buildings only and never attacks the Fisherman) is off the route it was walking, and that route's next waypoint
lies behind it. Measured on the client 16.402 corpus (20260920-081819, both drags of the red Giant, both seats): the
drag holds the route unchanged, the frame after the stop shows no route, and the frame after that a fresh route planned
from the drag's end point, which the Giant walks on toward its tower. Today's engine keeps the old route, so the Giant
walks back toward the waypoint it had when the hook landed.

THE SCENE. A blue Fisherman at (3500, 12000) and a red Giant at (3500, 22000) on the left lane. The Giant walks south
toward the blue princess tower; the Fisherman stops, hooks it and drags it about 4,600 south, across the river.

WHY THE CONTROLS ARE HERE. "The Giant walks south after the drag" also holds for a Giant whose old route happened to
lead south, so the scene is checked to put the old route's next waypoint north of the Giant when the drag ends, and the
old arm must walk north. A Knight in the Giant's place attacks the Fisherman after the drag, which drops its route on
that transition under either arm: it must then walk the same track under both arms (the null of the client 15.535.29
Knight runs, which cannot separate the arms).

PLANTS. `hook_drag_route_kept` (the new arm keeps the pre-drag route):
  test_the_dragged_giant_walks_on_from_where_the_drag_left_it.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.HOOK_DRAG_ROUTE"
NEW_ARM, OLD_ARM = "client16402_dropped", "kept"
LEDGER = Path(__file__).resolve().parents[1] / "data" / "calibration.json"
FISHERMAN_AT, VICTIM_AT = (3500, 12000), (3500, 22000)
DRAG_STEP_FLOOR = 500  # a drag step is 510 a tick; a walk is far below this
TICKS = 140


def overrides(arm: str) -> dict:
    return {KEY: json.dumps(arm)}


def run(arm: str, victim: str = "Giant") -> list:
    """Per tick after the tick: the victim's (x, y) and route (goal first), native units and half-tile cells."""
    b = royalesim.Battle(["Fisherman", victim], [[0, 1, 0], [0, 1, 0]], calibration_overrides=overrides(arm))
    units = [
        (0, 0, FISHERMAN_AT[0] * SUB, FISHERMAN_AT[1] * SUB, -1),
        (1, 1, VICTIM_AT[0] * SUB, VICTIM_AT[1] * SUB, -1),
    ]
    b.reset(0, [[0, 1] * 4, [0, 1] * 4], 0, 200, [10_000, 10_000], None, units)
    rows = []
    for _ in range(TICKS):
        b.step([], 1)
        v = next((u for u in b.debug_units() if u[1] == victim), None)
        rows.append(None if v is None else ((v[2] // SUB, v[3] // SUB), list(v[6])))
    return rows


def drag(rows: list, what: str) -> tuple:
    """(first, last) row of the drag: the rows whose step is a drag step."""
    steps = [
        i
        for i in range(1, len(rows))
        if rows[i] and rows[i - 1] and abs(rows[i][0][1] - rows[i - 1][0][1]) >= DRAG_STEP_FLOOR
    ]
    assert steps, f"{what}: the scene drifted: the hook never dragged the victim"
    assert steps[-1] - steps[0] + 1 == len(steps), f"{what}: the scene drifted: the drag is not one run of ticks"
    return steps[0], steps[-1]


def first_walk_after(rows: list, last: int, what: str) -> int:
    """The first row after the drag on which the victim moves."""
    for i in range(last + 1, min(last + 15, len(rows))):
        if rows[i] and rows[i][0] != rows[i - 1][0]:
            return i
    raise AssertionError(f"{what}: the scene drifted: the victim never walked within 14 ticks of the drag's end")


def centre_y(cell: tuple) -> int:
    return cell[1] * 500 + 250


def test_the_scene_leaves_the_old_route_behind_the_giant():
    """Precondition of both arms' tests: when the drag ends, the route the Giant held points north of it."""
    rows = run(OLD_ARM)
    first, last = drag(rows, "old arm")
    held = rows[first - 1][1]
    assert held, "the scene drifted: the Giant had no route when the hook landed"
    y_end = rows[last][0][1]
    assert centre_y(held[-1]) > y_end + 1000, (
        f"the scene drifted: the held route's next node {held[-1]} is not well north of the drag's end (y {y_end})"
    )


def test_the_dragged_giant_walks_on_from_where_the_drag_left_it():
    """client16402_dropped: the Giant's first step after the drag goes south, on a route planned from the end point.
    Plant: hook_drag_route_kept."""
    rows = run(NEW_ARM)
    first, last = drag(rows, "new arm")
    k = first_walk_after(rows, last, "new arm")
    (x0, y0), (x1, y1) = rows[k - 1][0], rows[k][0]
    route = rows[k][1]
    assert y1 < y0, (
        f"new arm: after the drag ended at ({x0}, {y0}) the Giant stepped ({x1 - x0}, {y1 - y0}), back toward the "
        f"route it held when the hook landed (next node {rows[first - 1][1][-1]})"
    )
    assert route, "new arm: the Giant walks with no route"
    assert centre_y(route[-1]) <= y0, f"new arm: the Giant's next node {route[-1]} is not south of it"


def test_the_old_arm_walks_back_to_the_held_route():
    """kept, today's engine: the Giant's first step after the drag goes north, toward the node it held."""
    rows = run(OLD_ARM)
    _, last = drag(rows, "old arm")
    k = first_walk_after(rows, last, "old arm")
    (x0, y0), (x1, y1) = rows[k - 1][0], rows[k][0]
    assert y1 > y0, f"old arm: after the drag the Giant stepped ({x1 - x0}, {y1 - y0}), not back north"


def test_a_knight_that_attacks_after_the_drag_runs_the_same_under_both_arms():
    """The null: a victim that attacks after the drag leaves its route on that transition, so the arms do not part.
    The positions are compared, not the routes: on the stop tick itself the new arm has already dropped the route
    that the old arm drops on the next tick, when the Knight goes into its attack."""
    new, old = run(NEW_ARM, "Knight"), run(OLD_ARM, "Knight")
    drag(new, "Knight, new arm")
    track = lambda rows: [r and r[0] for r in rows]  # noqa: E731
    assert track(new) == track(old), "a Knight dragged and then attacking walked differently under the two arms"


def test_the_shipped_value_is_the_new_arm():
    entry = json.loads(LEDGER.read_text(encoding="utf-8"))["combat"]["HOOK_DRAG_ROUTE"]
    assert entry["value"] == NEW_ARM
    assert entry["candidates"] == [OLD_ARM, NEW_ARM]
    assert entry["status"] == "measured"
