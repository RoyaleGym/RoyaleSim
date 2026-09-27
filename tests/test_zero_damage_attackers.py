"""No loaded attacker attacks for 0 (sim's window Z).

WHAT THIS PINS. On client 15.535.29 (the Berserker sweep scenario) a Berserker hits a Knight for 102 every 12 ticks
(t265, 277, 289, 301): its AttackSequenceList damage 40 at level 11, at HitSpeed 600. Three loaded basic cards once
attacked for nothing, because their damage lives somewhere the loader did not read and a missing damage defaults to
0: the Berserker (AttackSequenceList), the Princess (an unfired projectile) and the Firecracker (its projectile's
spawn_projectile, the sparks). The Berserker's is a data read. The other two ride a calibration key each, and both
new arms ship since the 2026-09-26 loader2 flip:
- the Princess, combat.CUSTOM_FIRST_PROJECTILE = client_first_of_volley. Her first arrow is PrincessProjectile, 168 at
  level 11, measured on client 15.535.29 (the princess-goblins scenario). Under not_read she fires only her
  decoration and deals nothing.
- the Firecracker, combat.SPAWN_PROJECTILE = client_spark_fan. Its rocket lands and releases 5 sparks of 64 at level
  11, 320 on the landing tick, measured on client 15.535.29 (its catalogue scenario). Under not_read the rocket
  releases nothing and deals nothing.
Each case names its arm, so the old arm stays covered.

WHY THE CONTROLS ARE HERE. The target is a Cannon, which every ground attacker and every building-targeter will hit.
A Cannon decays over its lifetime, so "the Cannon lost hp" is true with no attacker at all: each case subtracts a
control run without the attacker. Valkyrie and Musketeer are the controls that show the instrument registers damage.

PLANTS. None here. The Berserker's damage is a data read (tools/extract_cards.py `sequence_damage`), so a plant would
be a corrupted card table, not an engine cfg. The two keyed cases are read under both arms by name, and their plants
live in tests/test_custom_first_projectile.py and tests/test_firecracker_sparks.py.
"""

from __future__ import annotations

import json
from itertools import pairwise

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
# ENTITY_FIELDS: 1 team, 3 card_id, 4 tower_slot, 7 hp
TEAM, CARD, SLOT, HP = 1, 3, 4, 7
KEY_PRINCESS = "combat.CUSTOM_FIRST_PROJECTILE"
KEY_FIRECRACKER = "combat.SPAWN_PROJECTILE"
#: one hit's damage at level 11 under the new arm: the Princess's first arrow, the Firecracker's 5 sparks of 64
PER_HIT = {"Princess": 168, "Firecracker": 5 * 64}


def knight_drops(card: str, ticks: int = 80) -> list[tuple[int, int]]:
    """A blue `card` at (9000, 13500) beside a red Knight at (9000, 14500), out of every tower's reach: the Knight's
    hp drops as (tick, amount)."""
    b = royalesim.Battle([card, "Knight"], [[0, 1, 2], [0, 1, 2]])
    units = [(0, 0, 9000 * SUB, 13500 * SUB, -1), (1, 1, 9000 * SUB, 14500 * SUB, -1)]
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, units)
    prev, drops = None, []
    for t in range(ticks):
        k = next((e for e in json.loads(b.state_json())["entities"] if e[TEAM] == 1 and e[SLOT] < 0), None)
        if k is None:
            break
        if prev is not None and k[HP] < prev:
            drops.append((t, prev - k[HP]))
        prev = k[HP]
        b.step([], 1)
    return drops


def cannon_hp(card: str, with_attacker: bool, ticks: int = 150, overrides: dict | None = None) -> int:
    b = royalesim.Battle([card, "Cannon"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides or {})
    units = [(1, 1, 9000 * SUB, 14300 * SUB, -1)]
    if with_attacker:
        units.insert(0, (0, 0, 9000 * SUB, 13000 * SUB, -1))
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, units)
    b.step([], ticks)
    c = next((e for e in json.loads(b.state_json())["entities"] if e[TEAM] == 1 and e[SLOT] < 0 and e[CARD] == 1), None)
    return 0 if c is None else c[HP]


def damage_to_cannon(card: str, overrides: dict | None = None) -> int:
    return cannon_hp(card, False, overrides=overrides) - cannon_hp(card, True, overrides=overrides)


def test_berserker_hits_for_102_every_12_ticks():
    drops = knight_drops("Berserker")
    assert len(drops) >= 3, f"the Berserker dealt {drops}"
    assert all(amount == 102 for _, amount in drops[:3]), drops
    gaps = [b - a for (a, _), (b, _) in pairwise(drops[:3])]
    assert gaps == [12, 12], drops


@pytest.mark.parametrize("card", ["Berserker"])
def test_the_card_damages_what_it_attacks(card):
    """No key: the Berserker's damage is a data read."""
    assert damage_to_cannon(card) > 0, f"{card} attacked a Cannon for 150 ticks and dealt nothing"


@pytest.mark.parametrize(
    ("card", "key"), [("Princess", KEY_PRINCESS), ("Firecracker", KEY_FIRECRACKER)], ids=["Princess", "Firecracker"]
)
def test_the_new_arm_damages_what_it_attacks(card, key):
    """The shipped arm, named: the card deals whole hits of its measured damage."""
    arm = {"Princess": "client_first_of_volley", "Firecracker": "client_spark_fan"}[card]
    dealt = damage_to_cannon(card, {key: json.dumps(arm)})
    assert dealt > 0, f"{card} under {key} = {arm} attacked a Cannon for 150 ticks and dealt nothing"
    assert dealt % PER_HIT[card] == 0, f"{card} under {key} = {arm} dealt {dealt}, not whole hits of {PER_HIT[card]}"


@pytest.mark.parametrize(
    ("card", "key"), [("Princess", KEY_PRINCESS), ("Firecracker", KEY_FIRECRACKER)], ids=["Princess", "Firecracker"]
)
def test_the_old_arm_attacks_for_nothing(card, key):
    """The old arm, named: not_read leaves the card's damage where the loader does not look, so it deals 0."""
    dealt = damage_to_cannon(card, {key: json.dumps("not_read")})
    assert dealt == 0, f"{card} under {key} = not_read dealt {dealt}"


@pytest.mark.parametrize("card", ["Valkyrie", "Musketeer"])
def test_the_instrument_registers_damage(card):
    """Controls, green today: 619 and 619 on the shared build of 2026-09-25."""
    assert damage_to_cannon(card) > 0
