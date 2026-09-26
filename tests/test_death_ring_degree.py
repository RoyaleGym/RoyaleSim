"""A Battle Ram's death ring lies at its heading ROUNDED to a whole degree, and its Barbarians keep that heading
(spawner.DEATH_SPAWN_LAYOUT).

WHAT THIS PINS. A Battle Ram that dies releases two Barbarians 600 ahead of and behind its death point along its
heading (the facing_ring arm, measured on client 16.402). On client 15.535.29 two details of that differ from the
engine. The ring's direction is the heading ROUNDED to a whole degree, through the round(sin x 1024) table. And both
Barbarians start with the Ram's heading, not with their side's forward. Measured over the 17 Battle Ram deaths in the
client 15.535.29 scenario runs: in 17 of 17 both Barbarians' first heading is the same vector, along the ring. The
ring around the two creation points' midpoint, at that heading rounded, is exact in 13 of 17; the other 4 are one
degree away, where the Ram's heading on its death tick is not recorded.

THE SCENARIO. The client 15.535.29 scene: a red Battle Ram played at (3499, 17499) charges the blue left princess
tower and dies on it. Its heading on the death tick points at the tower, (28, -254), at -83.7 degrees, so the ring
lies at -84. On the client the Barbarians' first frames are (3199, 9261) and (3323, 8069), both heading (28, -254).
The engine lays them along the exact heading, at (3195, 9261) and (3326, 8068), facing (0, -256).

WHICH ARM. facing_ring is the shipped arm; facing_ring_rounded is the proposed arm. The tests pin each arm BY NAME
through the battle's calibration.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "spawner.DEATH_SPAWN_LAYOUT"
NEW_ARM, OLD_ARM = "facing_ring_rounded", "facing_ring"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
CARDS = ["BattleRam", "Knight"]
RAM_AT, PLAY_STEP = (3499, 17499), 101
CLIENT_POINTS = sorted([(3199, 9261), (3323, 8069)])
CLIENT_HEADING = (28, -254)
ENGINE_POINTS = sorted([(3195, 9261), (3326, 8068)])
ENGINE_HEADING = (0, -256)


def overrides(arm) -> dict:
    """The battle's calibration pinning `arm` BY NAME."""
    return {KEY: json.dumps(arm)}


def barbarians(arm):
    """(the step the Barbarians appear, their first positions, their first headings)."""
    b = royalesim.Battle(CARDS, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[1] * 8, [0] * 8], 0, 200, [100_000, 100_000], None, [])
    ram = None
    for t in range(1, 400):
        b.step([(1, 0, RAM_AT[0] * SUB, RAM_AT[1] * SUB)] if t == PLAY_STEP else [], 1)
        units = [e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0 and e[F["team"]] == 1]
        if ram is None and len(units) == 1:
            ram = units[0][F["uid"]]
        new = [e for e in units if e[F["uid"]] != ram]
        if ram is not None and new:
            assert ram not in {e[F["uid"]] for e in units}, "precondition: Barbarians appeared while the Ram lives"
            assert len(new) == 2, f"precondition: the Ram's death released {len(new)} units"
            pts = sorted((e[F["x"]] // SUB, e[F["y"]] // SUB) for e in new)
            return t, pts, {tuple(e[F["facing"]]) for e in new}
    raise AssertionError("the Battle Ram never died")


def test_the_ring_lies_at_the_rounded_degree_and_the_barbarians_keep_the_heading():
    """The degree and the heading: the Barbarians face the client's heading exactly and stand within 1 of the client's
    first frames (the shipped arm is 3 to 4 away). The last native is the next test's."""
    t, pts, heads = barbarians(NEW_ARM)
    assert heads == {CLIENT_HEADING}, f"the Barbarians face {heads}; the client's face {CLIENT_HEADING}"
    off = [max(abs(e[0] - c[0]), abs(e[1] - c[1])) for e, c in zip(pts, CLIENT_POINTS, strict=True)]
    assert max(off) <= 1, f"the Barbarians stand at {pts} on step {t}; the client's are {CLIENT_POINTS}"


@pytest.mark.xfail(
    strict=True,
    reason="the ring offset in whole native units: around the Ram's death point (3261, 8665), which the engine "
    "reproduces exactly, the client lays (-62, +596) and (+62, -596), each axis of 600 x table / 1024 truncated toward "
    "zero; the engine lays (-62.67, +596.44) and (+62, -597)",
)
def test_the_ring_lands_on_the_clients_exact_points():
    """Exact, as measured: the day the engine lays the ring in whole native units this passes, and the strict xfail
    turns red to ask for the mark to go."""
    t, pts, _ = barbarians(NEW_ARM)
    assert pts == CLIENT_POINTS, f"the Barbarians stand at {pts} on step {t}; the client's are {CLIENT_POINTS}"


def test_the_shipped_arm_lays_the_ring_along_the_exact_heading_facing_forward():
    _, pts, heads = barbarians(OLD_ARM)
    assert pts == ENGINE_POINTS, f"facing_ring: the Barbarians stand at {pts}, not {ENGINE_POINTS}"
    assert heads == {ENGINE_HEADING}, f"facing_ring: the Barbarians face {heads}, not {ENGINE_HEADING}"
