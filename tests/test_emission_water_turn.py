"""A spawner whose forward emission would put its unit over the river emits sideways, away from the centre line
(spawner.EMISSION_WATER_TURN).

WHAT THIS PINS. A Tombstone emits each Skeleton at the forward tangent of its circle and the Skeleton's, 1500 ahead
(spawner.SPAWN_POINT). One tile behind the river that point is the river's edge, and there both clients turn it: the
16.402 corpus's three off-bridge Tombstones at y 18500 emit all 30 Skeletons about 1500 to the side away from the
arena's centre line, and client 15.535.29 does the same on both sides and both lanes. A Tombstone on the bridge,
(3500, 18500), emits forward, and one two tiles back emits forward. The engine emitted forward everywhere.

WHY THE CONTROLS ARE HERE. "Sideways" also passes for a build that turns every emission one tile behind the river, so
the bridge Tombstone must stay forward; and for one that turns every emission, so the Tombstone two tiles back must stay
forward. The side-0 Tombstone at (2500, 13500) separates the unit's circle from its centre: its forward point is on the
bridge, and it turns because the circle hangs over the water beside it. The old arm must be today's engine.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `emission_water_turn_ignored` -- the new arm emits forward onto the river's edge, as the old one does:
    test_one_tile_behind_the_river_the_emission_turns_away_from_the_centre, test_the_death_spawn_turns_with_it.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "spawner.EMISSION_WATER_TURN"
NEW_ARM, OLD_ARM = "client16402_sideways_away_from_centre", "none"
# ENTITY_FIELDS: 0 uid, 1 team, 2 kind (1 = building), 3 card_id, 4 tower_slot, 5 x, 6 y
UID, TEAM, KIND, CARD, SLOT, X, Y = 0, 1, 2, 3, 4, 5, 6
TOMB = 0
#: how far the first Skeleton may stand from the emission point: its first step, if the engine takes it on the
#: emission tick (spawner.SPAWNED_FIRST_STEP), is at most one Skeleton step
STEP = 150


def first_emission(arm: str, side: int, tap: tuple) -> tuple:
    """The first Skeleton's first position against its Tombstone, native units, in the ABSOLUTE frame."""
    b = royalesim.Battle(["Tombstone", "Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides={KEY: json.dumps(arm)})
    b.reset(2, [[0, 1] * 4, [0, 1] * 4], 0, 0, None, None, [])
    b.step([], 100)
    played = b.step([(side, TOMB, tap[0] * SUB, tap[1] * SUB)], 1)
    assert played, f"the Tombstone play at {tap} returned nothing"
    assert played[0][1] == 0, f"the Tombstone at {tap} was refused: {played}"
    for _ in range(400):
        ents = json.loads(b.state_json())["entities"]
        tomb = next((e for e in ents if e[TEAM] == side and e[SLOT] < 0 and e[CARD] == TOMB and e[KIND] == 1), None)
        skel = next((e for e in ents if e[TEAM] == side and e[SLOT] < 0 and e[CARD] == TOMB and e[KIND] != 1), None)
        if skel:
            assert tomb, "the Tombstone is gone before its first emission"
            return (skel[X] // SUB - tomb[X] // SUB, skel[Y] // SUB - tomb[Y] // SUB)
        b.step([], 1)
    raise AssertionError("no Skeleton in 400 ticks")


def near(got: tuple, want: tuple) -> bool:
    return ((got[0] - want[0]) ** 2 + (got[1] - want[1]) ** 2) ** 0.5 <= STEP


FWD = {0: (0, 1500), 1: (0, -1500)}


@pytest.mark.parametrize(
    ("side", "tap", "want"),
    [
        (1, (2500, 18500), (-1500, 0)),  # corpus
        (1, (7500, 18500), (-1500, 0)),  # corpus
        (1, (13500, 18500), (1500, 0)),  # corpus, the right bridge's first column
        (0, (7500, 13500), (-1500, 0)),  # client 15.535.29
        (0, (10500, 13500), (1500, 0)),  # client 15.535.29
        (0, (15500, 13500), (1500, 0)),  # client 15.535.29, just past the right bridge
        (0, (2500, 13500), (-1500, 0)),  # client 15.535.29: the forward point is on the bridge, the circle is not
    ],
)
def test_one_tile_behind_the_river_the_emission_turns_away_from_the_centre(side, tap, want):
    assert near(first_emission(NEW_ARM, side, tap), want)


@pytest.mark.parametrize(
    ("side", "tap"),
    [
        (1, (3500, 18500)),  # corpus: on the left bridge
        (0, (7500, 12500)),  # client 15.535.29: two tiles back
        (1, (7500, 19500)),  # two tiles back, side 1
    ],
)
def test_otherwise_it_stays_forward(side, tap):
    assert near(first_emission(NEW_ARM, side, tap), FWD[side])


def death_spawn(arm: str, side: int, tap: tuple) -> list:
    """Where the dying Tombstone's Skeletons appear against its last position: every Skeleton first seen on the tick
    after the Tombstone is gone."""
    b = royalesim.Battle(["Tombstone", "Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides={KEY: json.dumps(arm)})
    b.reset(2, [[0, 1] * 4, [0, 1] * 4], 0, 0, None, None, [])
    b.step([], 100)
    b.step([(side, TOMB, tap[0] * SUB, tap[1] * SUB)], 1)
    seen, last, gone = set(), None, 0
    for _ in range(2000):
        ents = json.loads(b.state_json())["entities"]
        tomb = next((e for e in ents if e[TEAM] == side and e[SLOT] < 0 and e[CARD] == TOMB and e[KIND] == 1), None)
        new = [e for e in ents if e[TEAM] == side and e[CARD] == TOMB and e[KIND] != 1 and e[UID] not in seen]
        if tomb is None and last is not None:
            gone += 1
            if new or gone > 3:
                return [(e[X] // SUB - last[0], e[Y] // SUB - last[1]) for e in new]
        seen.update(e[UID] for e in new)
        last = (tomb[X] // SUB, tomb[Y] // SUB) if tomb else last
        b.step([], 1)
    raise AssertionError("the Tombstone did not die in 2000 ticks")


def test_the_death_spawn_turns_with_it():
    """The corpus's two river deaths put all four Skeletons on the side point: (-1424, 24) and (1434, 12)."""
    got = death_spawn(NEW_ARM, 1, (2500, 18500))
    assert len(got) == 4, got
    assert all(near(g, (-1500, 0)) for g in got), got


def test_the_old_arm_is_todays_engine():
    """Today the Skeleton comes out forward, onto the river's edge."""
    assert near(first_emission(OLD_ARM, 1, (7500, 18500)), FWD[1])
