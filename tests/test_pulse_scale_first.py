"""A buff's pulse is the share of its LEVEL-SCALED per-second figure (status.BUFF_PULSE_AMOUNT).

WHAT THIS PINS. On client 15.535.29 (the Battle Healer spawn-heal scenario, against its control) a friendly Knight gains
exactly +50 on each of four pulses from BattleHealerSpawnBuff: HealPerSecond 79, HitFrequency 250, level 11. The
level-scaled per-second figure is 202, and a quarter of it is 50. Today's arm, per_second_times_frequency, takes the
quarter of the level-1 figure first (19) and scales that (48).

WHY THE CONTROLS ARE HERE. The old-arm case pins 48, so the test shows the two arms differ on this buff (a test that
passed under both would check nothing). A Poison pulses once a second, where both arms give the same figure: the new
arm must not move it.

PLANTS. None yet: the engine change for status.BUFF_PULSE_AMOUNT = scaled_per_second_times_frequency lands
with its own plant, to be named here.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
#: explicit, because catalogue ids are positional; the Poison first so it is in the opening hand
DECK = ["Poison", "Knight", "BattleHealer", "Fireball", "Bomber", "Giant", "Tesla", "IceSpirits"]
IDS = list(range(len(DECK)))
PO, KN, BH = 0, 1, 2
PULSE_KEY = "status.BUFF_PULSE_AMOUNT"
NEW_ARM, OLD_ARM = "scaled_per_second_times_frequency", "per_second_times_frequency"
SPAWN_KEY = "spawner.SPAWN_AREA_OBJECT_SCOPE"
# ENTITY_FIELDS: 1 team, 3 card_id, 4 tower_slot, 7 hp
TEAM, CARD, SLOT, HP = 1, 3, 4, 7


def battle(pulse_arm: str, spawns: list, heal_on_spawn: bool = False) -> object:
    overrides = {PULSE_KEY: json.dumps(pulse_arm)}
    if heal_on_spawn:
        overrides[SPAWN_KEY] = json.dumps("every_row")
    b = royalesim.Battle(card_names=DECK, slot_of_k=[[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(0, [IDS, IDS], 0, 200, [10_000, 10_000], None, [(t, c, x * SUB, y * SUB, hp) for t, c, x, y, hp in spawns])
    return b


def knight(b, team: int):
    return next(
        (e for e in json.loads(b.state_json())["entities"] if e[TEAM] == team and e[SLOT] < 0 and e[CARD] == KN), None
    )


def spawn_heal_pulses(pulse_arm: str) -> list[int]:
    """A blue Knight at 1,000 hp and a blue Battle Healer 2,000 away, with no enemy troop and out of every tower's
    reach: the Knight's hp gains over 40 ticks."""
    b = battle(pulse_arm, [(0, KN, 9000, 13000, 1000), (0, BH, 11000, 13000, -1)], heal_on_spawn=True)
    prev, gains = None, []
    for _ in range(40):
        k = knight(b, 0)
        if k is None:
            break
        if prev is not None and k[HP] > prev:
            gains.append(k[HP] - prev)
        prev = k[HP]
        b.step([], 1)
    return gains


def poison_first_pulse(pulse_arm: str) -> int:
    """A red Knight at (9000, 20000), out of every tower's reach, and a blue Poison cast on it: its first hp loss."""
    b = battle(pulse_arm, [(1, KN, 9000, 20000, -1)])
    b.step([], 1)
    prev = knight(b, 1)[HP]
    b.step([(0, PO, 9000 * SUB, 20000 * SUB)], 1)
    for _ in range(80):
        k = knight(b, 1)
        if k is None:
            break
        if k[HP] < prev:
            return prev - k[HP]
        prev = k[HP]
        b.step([], 1)
    raise AssertionError("the Poison never pulsed on the Knight")


def test_the_spawn_heal_pulse_is_the_scaled_share():
    assert spawn_heal_pulses(NEW_ARM) == [50, 50, 50, 50]


def test_the_old_arm_scales_the_share():
    """Today's engine: 79 x 250 / 1000 = 19, then 19 x 256% = 48."""
    assert spawn_heal_pulses(OLD_ARM) == [48, 48, 48, 48]


def test_a_once_a_second_poison_is_unchanged():
    new, old = poison_first_pulse(NEW_ARM), poison_first_pulse(OLD_ARM)
    assert new == old > 0, (new, old)
