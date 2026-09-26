"""A crown tower drops a started shot 500 beyond its range; units keep their started swings
(targeting.LOGIC_PRESERVE_TARGET_IF_HIT_STARTED = "projectile_attackers_only").

ONE RULE WITH tests/test_reach_loss_switch.py. A crown tower fires a projectile, and under the scoped value every
projectile attacker holds its target to Range + both radii + 500, then rescans; a tower's rescan finds nothing past
its range, so it drops the target. A direct striker (the Knight of the control) is not held by the lock at all: it
rescans past its keep radius and, with no other enemy, takes the same target back and finishes its swing.

WHAT THIS PINS. On client 15.535.29 (the Giant, Golem and Royal Giant sweep scenarios) a princess tower shooting a
Knight that walks away drops it, with the shot under way, on the first tick whose start-of-tick centre distance is
more than 9500: Range 7500 + the tower's CollisionRadius 1000 + the Knight's 500 + 500. The kept and dropped
distances bracket it on all three: kept at 9487.2, 9486.9 and 9440.4, dropped at 9547.2, 9546.1 and 9500.5. Today's
engine holds a started shot to targeting.LOGIC_CANCEL_HIT_FROM_LONG_DISTANCE_RANGE (1500) beyond, so the tower fires
once more and the Knight takes one more 109.

WHY THE CONTROLS ARE HERE. A single global 500 reproduces the three tower drops and is refuted by units: on client
15.535.29 a Mega Minion's started swing ran to its hit with the target 666 beyond, Bats 827, and a Knight 2578. The
control is a Knight whose second swing lands on a Giant more than 500 beyond; a global 500 cancels that swing, so the
control fails on any implementation that is not scoped to crown towers.
"""

from __future__ import annotations

import json
import math
from itertools import pairwise

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "targeting.LOGIC_PRESERVE_TARGET_IF_HIT_STARTED"
NEW_ARM, OLD_ARM = "projectile_attackers_only", True
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
TOWER_AT = (14500, 6500)  # the blue right princess tower
#: Range 7500 + tower radius 1000 + Knight radius 500, and the 500 the new arm adds
IN_RANGE, LIMIT = 9000, 9500
TOWER_HIT, KNIGHT_HIT = 109, 202
#: the Knight's reach on a Giant: Range 1200 + radii 500 and 750
KNIGHT_ON_GIANT = 2450


def overrides(arm) -> dict:
    return {KEY: json.dumps(arm)}


def battle(arm, spawns):
    b = royalesim.Battle(["Giant", "Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    units = [(t, c, x * SUB, y * SUB, -1) for t, c, x, y in spawns]
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, units)
    return b


def dist(a, b) -> float:
    return math.hypot(a[F["x"]] - b[F["x"]], a[F["y"]] - b[F["y"]]) / SUB


def tower_track(arm, ticks: int = 60) -> list[dict]:
    """A blue Giant at (14500, 18800) walks north and a red Knight at (14500, 14500) chases it out of the blue right
    princess tower's range. One row per tick: whether the tower targets the Knight, whether it fires at the Knight
    this tick (attack phase 2), the Knight's centre distance to the tower and its hp."""
    b = battle(arm, [(0, 0, 14500, 18800), (1, 1, 14500, 14500)])
    rows = []
    for t in range(ticks):
        ents = json.loads(b.state_json())["entities"]
        tower = next(e for e in ents if e[F["team"]] == 0 and e[F["tower_slot"]] >= 0
                     and (e[F["x"]], e[F["y"]]) == (TOWER_AT[0] * SUB, TOWER_AT[1] * SUB))
        knight = next(e for e in ents if e[F["team"]] == 1 and e[F["tower_slot"]] < 0)
        on = tower[F["target_uid"]] == knight[F["uid"]]
        rows.append({"t": t, "on": on, "fired": on and tower[F["attack_phase"]] == 2, "d": dist(tower, knight),
                     "hp": knight[F["hp"]]})
        b.step([], 1)
    return rows


def assert_tower_drop_rule(rows: list[dict]) -> int:
    """The tower drops the Knight on the first tick whose START-OF-TICK distance (the previous row's) exceeds LIMIT,
    and never fires from beyond it. Returns the drop tick. Shared with the truth check in the README."""
    late = [(b["t"], round(a["d"])) for a, b in pairwise(rows) if b["fired"] and a["d"] > LIMIT]
    assert late == [], f"the tower fired from beyond {LIMIT}: (tick, start-of-tick distance) {late}"
    first = next(i for i, r in enumerate(rows) if r["on"])
    drop = next(i for i in range(first, len(rows)) if not rows[i]["on"])
    assert rows[drop - 1]["d"] > LIMIT >= rows[drop - 2]["d"], (
        f"dropped on tick {rows[drop]['t']}; (tick, distance) before it: "
        f"{[(r['t'], round(r['d'])) for r in rows[max(first, drop - 4):drop]]}")
    return rows[drop]["t"]


def hits(rows: list[dict], amount: int) -> list[int]:
    return [b["t"] for a, b in pairwise(rows) if a["hp"] - b["hp"] == amount]


def test_a_tower_drops_a_started_shot_500_beyond_range():
    rows = tower_track(NEW_ARM)
    assert rows[1]["on"], "the tower takes the Knight on tick 1"
    assert rows[1]["d"] < IN_RANGE, rows[:2]
    assert assert_tower_drop_rule(rows) == 27
    assert hits(rows, TOWER_HIT) == [32], "one shot lands, the one fired in range on tick 16"


def test_a_unit_keeps_the_global_cancel_range():
    """A red Knight at (14500, 16800), out of every blue tower's reach, swings at a blue Giant at (14500, 18400) that
    walks away north. Its second hit lands with the Giant more than 500 beyond its reach at the start of the tick."""
    b = battle(NEW_ARM, [(0, 0, 14500, 18400), (1, 1, 14500, 16800)])
    prev, landed = None, []
    for t in range(45):
        ents = json.loads(b.state_json())["entities"]
        knight = next(e for e in ents if e[F["team"]] == 1 and e[F["tower_slot"]] < 0)
        giant = next(e for e in ents if e[F["team"]] == 0 and e[F["tower_slot"]] < 0)
        if prev is not None and prev[1] - giant[F["hp"]] == KNIGHT_HIT:
            landed.append((t, round(prev[0] - KNIGHT_ON_GIANT)))
        prev = (dist(knight, giant), giant[F["hp"]])
        b.step([], 1)
    assert len(landed) >= 2, f"the Knight's second swing did not land: {landed}"
    assert landed[1][1] > 500, f"(tick, start-of-tick distance beyond reach) of the Knight's hits: {landed}"


def test_the_old_arm_is_todays_engine():
    """Checked on the shared build of 2026-09-25 with the key dropped: the tower fires on ticks 16 and 32, the second
    from 9822 (start of tick), drops the Knight on 33, and both shots land (109 on 32 and 50)."""
    rows = tower_track(OLD_ARM)
    fired = [(b["t"], round(a["d"])) for a, b in pairwise(rows) if b["fired"]]
    assert fired == [(16, 8877), (32, 9822)], fired
    drop = next(r["t"] for r in rows[1:] if not r["on"])
    assert drop == 33
    assert hits(rows, TOWER_HIT) == [32, 50]
