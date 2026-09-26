"""A unit a spawner emits takes its first step on the tick it is created (spawner.SPAWNED_FIRST_STEP).

WHAT THIS PINS. On the 16.402 corpus, every periodic Tombstone Skeleton's path runs 1 tick behind in the engine on a
Tombstone's first emission (7 of 8) and 2 ticks behind on later ones (115 of 124); spawner.TIMER_LEFTOVER is the
second tick. The first is this: the game's emitted unit has already moved one step on its first frame, the engine's
stands on the emission point. On the 15.535.29 client the first member of 8 of 8 waves is displaced 89.8 from the
tangent point, one step at a Skeleton's speed, with no scatter. A dying Tombstone's four Skeletons show the same step
on their first frame, all identical.

THE SCENARIO. A blue Tombstone placed mid-way down its own half, alone. Its first Skeleton comes out on tick 0 at the
tangent point, (9000, 9500) native for a Tombstone at (9000, 8000); the test measures how far from that point the
Skeleton stands on its first frame: one Skeleton step (about 90) under client16402_same_tick, 0 under none.

WHICH ARM. client16402_same_tick is the SHIPPED arm since the 2026-09-26 flip. none is the engine before the flip.
The tests below pin each arm BY NAME through the battle's calibration, never through the shipped value, and the last
one runs the shipped build with no override at all.
"""

from __future__ import annotations

import json
import math

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
#: explicit, because catalogue ids are positional
DECK = ["Tombstone", "Knight", "Musketeer", "Archer", "Giant", "Minions", "Cannon", "Tesla"]
IDS = list(range(len(DECK)))
TS = 0
AT = (9000, 8000)
EMISSION_POINT = (9000, 9500)  # the Tombstone's 1000 plus the Skeleton's 500, forward for Blue
KEY = "spawner.SPAWNED_FIRST_STEP"
SHIPPED_ARM, NONE_ARM = "client16402_same_tick", "none"
# ENTITY_FIELDS: 0 uid, 1 team, 2 kind (1 = building), 4 tower_slot, 5 x, 6 y
UID, TEAM, KIND, SLOT, X, Y = 0, 1, 2, 4, 5, 6


def first_skeleton(arm: str | None) -> tuple:
    """The first emitted Skeleton's first-frame position, native. `arm` None runs the shipped build."""
    b = royalesim.Battle(
        card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]],
        calibration_overrides={} if arm is None else {KEY: json.dumps(arm)},
    )
    b.reset(0, [IDS, IDS], 0, 200, [10_000, 10_000], None, [(0, TS, AT[0] * SUB, AT[1] * SUB, -1)])
    for _ in range(20):
        b.step([], 1)
        units = [e for e in json.loads(b.state_json())["entities"] if e[TEAM] == 0 and e[SLOT] < 0 and e[KIND] != 1]
        if units:
            first = min(units, key=lambda e: e[UID])
            return first[X] // SUB, first[Y] // SUB
    raise AssertionError("the Tombstone emitted nothing within 20 ticks")


def test_an_emitted_skeleton_has_taken_one_step_on_its_first_frame():
    x, y = first_skeleton(SHIPPED_ARM)
    moved = math.dist((x, y), EMISSION_POINT)
    assert 60 <= moved <= 100, (
        f"the first Skeleton stands {moved:.0f} from the emission point on its first frame; the game: one step"
    )


def test_a_death_spawn_takes_the_same_step_all_together():
    """With the death point at the emission point (spawner.DEATH_SPAWN_AT_EMISSION_POINT), a dying Tombstone's four
    Skeletons stand on ONE point one step past it on their first frame: the corpus' (+-63, 1563) on 46 of 49 deaths."""
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    dp = ledger.get("spawner", {}).get("DEATH_SPAWN_AT_EMISSION_POINT", {}).get("value")
    if dp is None:
        pytest.fail("spawner.DEATH_SPAWN_AT_EMISSION_POINT is not in the compiled-in ledger")
    overrides = {
        KEY: json.dumps(SHIPPED_ARM),
        "spawner.DEATH_SPAWN_AT_EMISSION_POINT": json.dumps({**dp, "arm": "client16402_measured_list"}),
    }
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(0, [IDS, IDS], 0, 200, [10_000, 10_000], None, [(0, TS, AT[0] * SUB, AT[1] * SUB, 3)])
    seen = set()
    for t in range(12):
        b.step([], 1)
        units = [e for e in json.loads(b.state_json())["entities"] if e[TEAM] == 0 and e[SLOT] < 0 and e[KIND] != 1]
        new = [e for e in units if e[UID] not in seen]
        seen.update(e[UID] for e in units)
        if t > 0 and len(new) == 4:
            pts = {(e[X] // SUB, e[Y] // SUB) for e in new}
            assert len(pts) == 1, f"the four death Skeletons do not share one point: {sorted(pts)}"
            moved = math.dist(next(iter(pts)), EMISSION_POINT)
            assert 60 <= moved <= 100, f"the death spawn stands {moved:.0f} from the emission point; the game: one step"
            return
    raise AssertionError("the 3-hp Tombstone did not die with four Skeletons within 12 ticks")


def test_the_none_arm_leaves_the_skeleton_on_the_emission_point():
    x, y = first_skeleton(NONE_ARM)
    assert (x, y) == EMISSION_POINT, (
        f"the none arm must leave the first Skeleton on the emission point; it is at {(x, y)}"
    )


def test_the_shipped_build_takes_the_first_step():
    """No override at all. The compiled-in ledger ships SHIPPED_ARM, so the shipped build behaves as the client."""
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    shipped = ledger["spawner"]["SPAWNED_FIRST_STEP"]["value"]
    assert shipped == SHIPPED_ARM, f"{KEY} ships {shipped!r}, not the arm this file names as shipped"
    moved = math.dist(first_skeleton(None), EMISSION_POINT)
    assert 60 <= moved <= 100, f"the shipped build: the first Skeleton stands {moved:.0f} from the emission point"
