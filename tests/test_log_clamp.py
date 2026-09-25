"""A Log tapped beyond its legal area is pulled to the area's edge, not refused (spells.ILLEGAL_SPELL_TAP).

WHAT THIS PINS. On client 15.535.29, a blue Log tapped on the enemy half with every tower alive is cast from the
own half's boundary row: two taps 2,615 apart on the enemy half hit a Knight on the same tick as a Log tapped at
(11500, 14500). With a princess down, a tap inside the pocket her fall opens stays where it is, and a deeper tap is
pulled to the pocket's edge: the rule is a CLAMP to the legal area, then the tile snap. The engine refused the cast
(OUT_OF_TERRITORY).

THE CHECK. The 15.535.29 log-vs-knight scenario: a red Knight at (9500, 19499) from tick 100, a blue Log cast on 159. An
engine comparison with itself: the Log tapped on the enemy half at (11156, 17885) must hit the Knight on the same tick
as the Log tapped at (11500, 14500) (the 15.535.29 Log hits one tick later than the engine on every cast, an
unattributed lag this test must not encode).
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "spells.ILLEGAL_SPELL_TAP"
CAST = 159


def log_cast(arm: str, tap: tuple):
    """(the play's reason code, the tick the Knight is first hurt or None)."""
    b = royalesim.Battle(["Knight", "Log"], [[0, 1, 2], [0, 1, 2]], calibration_overrides={KEY: json.dumps(arm)})
    b.reset(2, [[1] * 8, [0] * 8], 0, 0, None, None, [])
    b.step([], 100)
    b.step([(1, 0, 9500 * SUB, 19499 * SUB)], CAST - 100)
    r = b.step([(0, 0, tap[0] * SUB, tap[1] * SUB)], 0)
    if r[0][1] != 0:
        return r[0][1], None
    for _ in range(60):
        b.step([], 1)
        k = next((e for e in json.loads(b.state_json())["entities"] if e[1] == 1 and 1000 < e[8] < 3000), None)
        if k and k[7] < k[8]:
            return 0, json.loads(b.state_json())["tick"]
    return 0, None


def test_an_enemy_half_log_tap_is_cast_from_the_boundary_row():
    arm = "client16402_clamp_to_legal_edge"
    reason, hit = log_cast(arm, (11156, 17885))
    assert reason == 0, "the enemy-half Log tap was refused"
    assert hit is not None
    assert hit == log_cast(arm, (11500, 14500))[1]


def test_the_old_arm_refuses_it():
    assert log_cast("refuse", (11156, 17885))[0] != 0
