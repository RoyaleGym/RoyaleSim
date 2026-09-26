"""The Electro Giant reflects each melee hit onto its attacker and stuns it (combat.REFLECT_ATTACK).

WHAT THIS PINS. On client 15.535.29 (the Electro Giant sweep scenario) every hit a Knight lands on the Electro Giant
is answered in the same tick by 192 damage to the Knight (ReflectedAttackDamage 75 at level 11) and a ZapFreeze stun of
ReflectedAttackBuffDuration 500 ms that holds the Knight's attack progress without resetting it, so the Knight hits
every 33 ticks instead of 24. Today's engine reads none of the ReflectedAttack columns.

WHY THE CONTROLS ARE HERE. "The Knight lost hp" could come from anything near it, so each reflected hit must fall on the
same tick as a Knight hit on the Giant, and a plain Giant in the same place must reflect nothing. The period pins the
stun: a reflect without the stun keeps 24, and a stun that resets progress would stretch it further.

WHY TWO HITS. In this scene the Electro Giant walks on toward a tower while the Knight is stunned, so the Knight's third
swing ends with the Giant 3117 away. That is beyond the Knight's reach (Range 1200 plus both radii) and beyond the
reflect's radius. The client cancels such a hit (a melee hit on a target beyond the cancel range deals nothing), so
there is nothing to reflect. Only the first two hits, both in reach, are this key's.
"""

from __future__ import annotations

import json
from itertools import pairwise

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.REFLECT_ATTACK"
NEW_ARM, OLD_ARM = "client_reflect_stun", "not_read"
# ENTITY_FIELDS: 1 team, 4 tower_slot, 7 hp
TEAM, SLOT, HP = 1, 4, 7
REFLECT, PERIOD_STUNNED, PERIOD_FREE = 192, 33, 24


def run(card: str, arm: str, ticks: int = 110):
    """A blue `card` at (9000, 13500) and a red Knight at (9000, 14700), out of every tower's reach for 110 ticks (a
    tower arrow reaches the Knight on tick 147). Returns the hp
    drops (tick, amount) of the blue giant and of the Knight."""
    b = royalesim.Battle([card, "Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides={KEY: json.dumps(arm)})
    units = [(0, 0, 9000 * SUB, 13500 * SUB, -1), (1, 1, 9000 * SUB, 14700 * SUB, -1)]
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, units)
    prev, drops = {}, {0: [], 1: []}
    for t in range(ticks):
        ents = json.loads(b.state_json())["entities"]
        for side in (0, 1):
            u = next((e for e in ents if e[TEAM] == side and e[SLOT] < 0), None)
            if u is None:
                continue
            if side in prev and u[HP] < prev[side]:
                drops[side].append((t, prev[side] - u[HP]))
            prev[side] = u[HP]
        b.step([], 1)
    return drops[0], drops[1]


def test_each_knight_hit_is_reflected_and_stuns():
    on_giant, on_knight = run("ElectroGiant", NEW_ARM)
    assert len(on_giant) >= 2, on_giant
    assert [a for _, a in on_knight[:2]] == [REFLECT] * 2, on_knight
    assert [t for t, _ in on_knight[:2]] == [t for t, _ in on_giant[:2]], (on_giant, on_knight)
    assert on_giant[1][0] - on_giant[0][0] == PERIOD_STUNNED, on_giant


def test_a_plain_giant_reflects_nothing():
    on_giant, on_knight = run("Giant", NEW_ARM)
    assert on_giant, "the Knight never hit the Giant"
    assert on_knight == [], on_knight


def test_the_old_arm_is_todays_engine():
    """Checked on the shared build of 2026-09-25 with the key dropped: the Knight hits the Electro Giant on ticks 10,
    34, 58 and 82 (every 24) and takes nothing."""
    on_giant, on_knight = run("ElectroGiant", OLD_ARM)
    assert on_knight == []
    assert [b - a for (a, _), (b, _) in pairwise(on_giant[:3])] == [PERIOD_FREE] * 2, on_giant
