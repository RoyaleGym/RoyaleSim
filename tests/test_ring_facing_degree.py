"""A spawner that sets SpawnAngleShift lays its ring out at its facing's ROUNDED angle (spawner.SPAWN_POINT).

WHAT THIS PINS. A Night Witch (SpawnRadius 1500, SpawnAngleShift 90) emits her two Bats on a ring of 1500 around her
position after her move on the emission tick, at her facing's angle + 90 and + 270 degrees. On client 15.535.29 that
angle is the facing's direction ROUNDED to a whole degree: her facing (255, 22) points at 4.93 degrees, and the Bats
come out at 95 and 275. The engine's measured arm picks the degree whose round(sin x 1024) table direction agrees
with the facing most, and for (255, 22) the table's rounding makes that 4. Everything else in the arm reproduces the
client: with degree 5 the ring points are the client's creation points exactly, (10742, 13157) and (10482, 16145).
Measured on client 15.535.29, over the 7 Night Witch emissions in the 15.535.29 scenario runs: the rounded angle of
her facing after the move, around her position after the move, gives both creation points exactly in 7 of 7; the
table's best degree gives 6 of 7, missing this scene.

THE SCENARIO. The client 15.535.29 scene: a blue Night Witch played at (9500, 14500) and a red Giant at (14500, 18499)
on the same tick. The Night Witch walks toward the Giant and emits her first two Bats 38 ticks later, while facing
(255, 22). On the client the Bats' first frames are (10825, 13242) and (10599, 16170): each has taken its first
step (spawner.SPAWNED_FIRST_STEP).

WHICH ARM. client16402_measured is the shipped arm; client_rounded_facing_degree is the proposed arm. The tests pin each
arm BY NAME through the battle's calibration.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "spawner.SPAWN_POINT"
NEW_ARM, OLD_ARM = "client_rounded_facing_degree", "client16402_measured"
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
CARDS = ["DarkWitch", "Giant"]
WITCH_AT, GIANT_AT = (9500, 14500), (14500, 18499)
PLAY_STEP = 101
#: the client's first frames of the first two Bats
CLIENT_BATS = sorted([(10825, 13242), (10599, 16170)])
#: today's engine, the same Bats one degree around the ring
ENGINE_BATS = sorted([(10799, 13238), (10625, 16173)])


def overrides(arm) -> dict:
    """The battle's calibration pinning `arm` BY NAME."""
    return {KEY: json.dumps(arm)}


def first_bats(arm):
    """(the step the first Bats appear, their positions on that step, the witch's facing the step before)."""
    b = royalesim.Battle(CARDS, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(arm))
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [100_000, 100_000], None, [])
    witch, facing = None, None
    for t in range(1, 200):
        plays = [(0, 0, WITCH_AT[0] * SUB, WITCH_AT[1] * SUB), (1, 0, GIANT_AT[0] * SUB, GIANT_AT[1] * SUB)]
        b.step(plays if t == PLAY_STEP else [], 1)
        ents = [e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0 and e[F["team"]] == 0]
        witches = [e for e in ents if e[F["max_hp"]] > 800]
        bats = [e for e in ents if e[F["max_hp"]] < 200]
        if witches:
            witch = witches[0]
        if bats:
            assert len(bats) == 2, f"precondition: the first emission made {len(bats)} Bats"
            return t, sorted((e[F["x"]] // SUB, e[F["y"]] // SUB) for e in bats), facing, witch
        if witch is not None:
            facing = tuple(witch[F["facing"]])
    raise AssertionError("the Night Witch emitted no Bats within 200 steps")


def test_the_ring_uses_the_facing_rounded_to_a_whole_degree():
    t, bats, _, witch = first_bats(NEW_ARM)
    assert tuple(witch[F["facing"]]) == (255, 22), f"precondition: the witch faces {witch[F['facing']]}, not (255, 22)"
    assert bats == CLIENT_BATS, f"the first Bats stand at {bats} on step {t}; the client's are {CLIENT_BATS}"


def test_the_shipped_arm_puts_the_ring_one_degree_short():
    _, bats, _, witch = first_bats(OLD_ARM)
    assert tuple(witch[F["facing"]]) == (255, 22), f"precondition: the witch faces {witch[F['facing']]}, not (255, 22)"
    assert bats == ENGINE_BATS, f"client16402_measured: the first Bats stand at {bats}, not {ENGINE_BATS}"
