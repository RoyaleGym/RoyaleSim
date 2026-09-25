"""Every troop and spell tap snaps to its tile's centre, and a single ground unit takes the ground deploy point too
(placement.TAP_SNAP).

WHAT THIS PINS. On client 15.535.29 and the 16.402 corpus, a tap is snapped to the PLAIN centre of its tile,
tile * 1000 + 500 on both axes, both sides, both halves: spells and air troops land exactly there (37 of 37 corpus
spell casts that separate the rules; 5 of 5 left-half air deploys). A GROUND unit then takes the one-native deploy-
point offset the ledger already holds as formation.GROUND_DEPLOY_POINT (x - 1 on the absolute left half, y - 1 for
side 1), which the engine applied only to multi-member rings: the seven single Knights of the 15.535.29 scenarios
stand on it, 7 of 7. The engine laid a single unit and a spell at the raw tap.

THE CHECKS. The seven 15.535.29 Knight taps; and a Log cast from a raw tap must hit a Knight on the same tick as the
same Log cast from that tap's snapped centre (the 15.535.29 Log hits one tick later than the 16.402 corpus and the
engine on every cast, a difference this test must not encode, so it compares the engine with itself).
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "placement.TAP_SNAP"
#: (side, raw tap, where the single Knight stood on client 15.535.29), native units
KNIGHTS_15535 = [
    (1, (9000, 19000), (9500, 19499)),
    (0, (9000, 13000), (9500, 13500)),
    (0, (8999, 13000), (8499, 13500)),
    (0, (9001, 13000), (9500, 13500)),
    (1, (8999, 19000), (8499, 19499)),
    (1, (9001, 19000), (9500, 19499)),
    (1, (3500, 18000), (3499, 18499)),
]
CAST = 159  # the true cast tick of the 15.535.29 offset-60 spell scenarios


def arm(name: str) -> dict:
    return {KEY: json.dumps(name)}


def knight_lands(overrides: dict, side: int, tap: tuple) -> tuple:
    b = royalesim.Battle(["Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(2, [[0] * 8, [0] * 8], 0, 0, None, None, [])
    b.step([], 200)
    b.step([(side, 0, tap[0] * SUB, tap[1] * SUB)], 1)
    u = list(b.debug_units())
    return (u[0][2] // SUB, u[0][3] // SUB) if u else None


def log_hit_tick(overrides: dict, tap: tuple):
    """A red Knight at the scenario's (9500, 19499) from tick 100; a blue Log cast at `tap` on CAST. The tick the
    Knight is first hurt."""
    b = royalesim.Battle(["Knight", "Log"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(2, [[1] * 8, [0] * 8], 0, 0, None, None, [])
    b.step([], 100)
    b.step([(1, 0, 9500 * SUB, 19499 * SUB)], CAST - 100)
    r = b.step([(0, 0, tap[0] * SUB, tap[1] * SUB)], 0)
    assert r[0][1] == 0, f"the Log cast at {tap} was refused"
    for _ in range(60):
        b.step([], 1)
        k = next((e for e in json.loads(b.state_json())["entities"] if e[1] == 1 and 1000 < e[8] < 3000), None)
        if k and k[7] < k[8]:
            return json.loads(b.state_json())["tick"]
    return None


@pytest.mark.parametrize(("side", "tap", "want"), KNIGHTS_15535)
def test_a_single_knight_lands_where_client_15535_puts_it(side, tap, want):
    assert knight_lands(arm("client16402_tile_centre"), side, tap) == want


@pytest.mark.parametrize(("raw", "snapped"), [((11156, 12000), (11500, 12500)), ((11156, 10000), (11500, 10500))])
def test_a_raw_log_tap_hits_like_its_snapped_tap(raw, snapped):
    o = arm("client16402_tile_centre")
    assert log_hit_tick(o, raw) == log_hit_tick(o, snapped)


def fireball_hit(overrides: dict, tap: tuple):
    """A red Knight at (9500, 19499) from tick 100; a blue Fireball cast at `tap` on CAST. (hit tick, push
    direction)."""
    b = royalesim.Battle(["Knight", "Fireball"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(2, [[1] * 8, [0] * 8], 0, 0, None, None, [])
    b.step([], 100)
    b.step([(1, 0, 9500 * SUB, 19499 * SUB)], CAST - 100)
    b.step([(0, 0, tap[0] * SUB, tap[1] * SUB)], 0)
    prev = hit = None
    for _ in range(50):
        b.step([], 1)
        k = next(e for e in json.loads(b.state_json())["entities"] if e[1] == 1 and 1000 < e[8] < 3000)
        pos = (k[5] // SUB, k[6] // SUB)
        if hit is None and k[7] < k[8]:
            hit = json.loads(b.state_json())["tick"]
        elif hit is not None:
            dx, dy = pos[0] - prev[0], pos[1] - prev[1]
            length = (dx * dx + dy * dy) ** 0.5
            return hit, (round(dx / length, 3), round(dy / length, 3))
        prev = pos
    return hit, None


def test_a_raw_fireball_tap_matches_client_15535():
    """The 15.535.29 fireball-vs-knight scenario: hit on 184, push (0.992, -0.126). The engine gives exactly that from
    the snapped centre (11500, 17500); from the raw tap it gave 185 and (0.936, -0.352)."""
    assert fireball_hit(arm("client16402_tile_centre"), (11156, 17885)) == (184, (0.992, -0.126))


def test_the_old_arm_is_todays_engine():
    """Today a single unit stands on the raw tap."""
    assert knight_lands(arm("none"), 1, (9000, 19000)) == (9000, 19000)
