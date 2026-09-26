"""No loaded attacker attacks for 0 (sim's window Z).

WHAT THIS PINS. On client 15.535.29 (the Berserker sweep scenario) a Berserker hits a Knight for 102 every 12 ticks
(t265, 277, 289, 301): its AttackSequenceList damage 40 at level 11, at HitSpeed 600. Three loaded basic cards attack
for nothing in today's engine because their damage lives somewhere the loader does not read and a missing damage
defaults to 0: the Berserker (AttackSequenceList), the Princess (an unfired projectile) and the Firecracker (its
projectile's spawn_projectile, the sparks).

WHY THE CONTROLS ARE HERE. The target is a Cannon, which every ground attacker and every building-targeter will hit.
A Cannon decays over its lifetime, so "the Cannon lost hp" is true with no attacker at all: each case subtracts a
control run without the attacker. Valkyrie and Musketeer are the controls that show the instrument registers damage.

PLANTS. None: this item has no calibration key. The Berserker's damage is a data read (tools/extract_cards.py
`sequence_damage`), so a plant would be a corrupted card table, not an engine cfg.
"""

from __future__ import annotations

import json
from itertools import pairwise

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
# ENTITY_FIELDS: 1 team, 3 card_id, 4 tower_slot, 7 hp
TEAM, CARD, SLOT, HP = 1, 3, 4, 7


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


def cannon_hp(card: str, with_attacker: bool, ticks: int = 150) -> int:
    b = royalesim.Battle([card, "Cannon"], [[0, 1, 2], [0, 1, 2]])
    units = [(1, 1, 9000 * SUB, 14300 * SUB, -1)]
    if with_attacker:
        units.insert(0, (0, 0, 9000 * SUB, 13000 * SUB, -1))
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, units)
    b.step([], ticks)
    c = next((e for e in json.loads(b.state_json())["entities"] if e[TEAM] == 1 and e[SLOT] < 0 and e[CARD] == 1), None)
    return 0 if c is None else c[HP]


def damage_to_cannon(card: str) -> int:
    return cannon_hp(card, False) - cannon_hp(card, True)


def test_berserker_hits_for_102_every_12_ticks():
    drops = knight_drops("Berserker")
    assert len(drops) >= 3, f"the Berserker dealt {drops}"
    assert all(amount == 102 for _, amount in drops[:3]), drops
    gaps = [b - a for (a, _), (b, _) in pairwise(drops[:3])]
    assert gaps == [12, 12], drops


# The Berserker's read landed (tools/extract_cards.py `sequence_damage`). The other two are STRICT xfails, so the day
# either one starts damaging this turns red and asks for the mark to go: the Princess's damaging first arrow is fired
# only under combat.CUSTOM_FIRST_PROJECTILE = client_first_of_volley, which does not ship, and the Firecracker's sparks
# have no specified law and are not loaded.
@pytest.mark.parametrize(
    "card",
    [
        "Berserker",
        pytest.param(
            "Princess",
            marks=pytest.mark.xfail(
                strict=True, reason="her damage rides CustomFirstProjectile, fired only under client_first_of_volley"
            ),
        ),
        pytest.param(
            "Firecracker",
            marks=pytest.mark.xfail(strict=True, reason="the sparks (spawn_projectile) have no specified law yet"),
        ),
    ],
)
def test_the_card_damages_what_it_attacks(card):
    assert damage_to_cannon(card) > 0, f"{card} attacked a Cannon for 150 ticks and dealt nothing"


@pytest.mark.parametrize("card", ["Valkyrie", "Musketeer"])
def test_the_instrument_registers_damage(card):
    """Controls, green today: 619 and 619 on the shared build of 2026-09-25."""
    assert damage_to_cannon(card) > 0
