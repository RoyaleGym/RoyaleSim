"""A Goblin Barrel releases its Goblins on a tight ring around the landing, one member forward
(spells.PROJECTILE_SPAWN_FORMATION = count_ring_tight).

WHAT THIS PINS. On client 15.535.29 (the centre-by-spawn-goblinbarrel scenario) the three released Goblins stand at
(0, +577), (+499, -288) and (-499, -288) from the landing point: a ring of three touching 500-radius units, whose
circumradius is 1000 / sqrt(3) = 577, one member toward the caster's forward axis. It is the ring the engine's own
formation layout lays for a 3-member deploy of that unit. The engine's engine_grid guess laid (-500, +500), (0, -500)
and (+500, +500): a triangle pointing backward. There is no Goblin Barrel in the 16.402 corpus, so this is the 15.535.29
law alone.

THE CHECK. A blue barrel cast at a tile centre (9500, 21500), so the tap snap is not part of it; the Goblins' first
positions relative to that point, within 2.

WHICH ARM. count_ring_tight is the SHIPPED arm since its flip. engine_grid is the engine before the flip. The tests pin
each arm BY NAME through the battle's calibration, and the last one runs the shipped build with no override at all.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "spells.PROJECTILE_SPAWN_FORMATION"
SHIPPED_ARM, OLD_ARM = "count_ring_tight", "engine_grid"
LANDING = (9500, 21500)
RING_15535 = sorted([(0, 577), (499, -288), (-499, -288)])


def released(arm: str | None) -> list:
    """The released Goblins' first positions relative to the landing; `arm` None runs the shipped build."""
    b = royalesim.Battle(
        ["Knight", "GoblinBarrel"], [[0, 1, 2], [0, 1, 2]],
        calibration_overrides={} if arm is None else {KEY: json.dumps(arm)},
    )
    b.reset(2, [[1] * 8, [0] * 8], 0, 0, None, None, [])
    b.step([], 100)
    b.step([(0, 0, LANDING[0] * SUB, LANDING[1] * SUB)], 0)
    for _ in range(70):
        b.step([], 1)
        new = [u for u in b.debug_units() if u[1] not in ("Knight", 0)]
        if new:
            return sorted((u[2] // SUB - LANDING[0], u[3] // SUB - LANDING[1]) for u in new)
    raise AssertionError("no Goblins released within 70 ticks")


def on_the_ring(got: list) -> bool:
    return len(got) == 3 and all(abs(a[0] - b[0]) <= 2 and abs(a[1] - b[1]) <= 2
                                 for a, b in zip(got, RING_15535, strict=True))


def test_the_barrel_lays_the_tight_577_ring():
    got = released(SHIPPED_ARM)
    assert on_the_ring(got), got


def test_the_engine_grid_arm_is_the_engine_before_the_flip():
    assert released(OLD_ARM) == sorted([(-500, 500), (0, -500), (500, 500)])


def test_the_shipped_build_lays_the_ring():
    """No override at all. The compiled-in ledger ships SHIPPED_ARM, so the shipped build behaves as the client."""
    ledger = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)
    shipped = ledger["spells"]["PROJECTILE_SPAWN_FORMATION"]["value"]
    assert shipped == SHIPPED_ARM, f"{KEY} ships {shipped!r}, not the arm this file names as shipped"
    got = released(None)
    assert on_the_ring(got), got
