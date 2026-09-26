"""A hidden Tesla surfaces straight into attacking (hide.RISE_LAW).

WHAT THIS PINS. On client 15.535.29 and the 16.402 corpus, a hidden Tesla that finds an enemy goes from hidden to
ATTACKING on one tick: its first hit lands 7 ticks after it surfaces (15.535.29 240 -> 247, corpus 674 -> 681 and 1980
-> 1987). An enemy can target it from the tick AFTER it surfaces (15.535.29 241; corpus 674 -> 675, 1980 -> 1981). The
engine held it in a 16-tick Rising phase before it could attack (first hit 263) and let an enemy lock on to it on the
surfacing tick itself (240).

THE SCENARIO. The 15.535.29 tesla-vs-knight scenario: a blue Tesla placed at (9000, 11000) on tick 100, a red Knight at
(9500, 19499) commanded on 105. The Knight turns toward the Tesla, goal cell (20, 23); the Tesla's hit on it is 220.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `tesla_rise_kept` -- the new arm keeps the UpTimeMs rise with no target and no attack:
    test_the_teslas_first_hit_lands_seven_ticks_after_it_surfaces.
  * `tesla_surface_tick_targetable` -- the surfacing tick is targetable under the new arm, as a rise is under the old:
    test_the_knight_locks_on_the_tick_after_the_tesla_surfaces.
  * `tesla_hide_wait_hidetime` -- the new arm stays up HideTimeMs after its loss, as the old one does: NO TEST HERE.
    Nothing in this file reaches the 6-tick hide after a loss, so this plant lands on nothing until a test of the
    Tesla going back under on loss + 6 is added.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "hide.RISE_LAW"


def battle(arm: str):
    b = royalesim.Battle(["Tesla", "Knight"], [[0, 1, 2], [0, 1, 2]], calibration_overrides={KEY: json.dumps(arm)})
    b.reset(2, [[0] * 8, [1] * 8], 0, 0, None, None, [])
    b.step([], 100)
    b.step([(0, 0, 9000 * SUB, 11000 * SUB)], 4)
    b.step([(1, 0, 9500 * SUB, 19499 * SUB)], 1)
    return b


def knight_turns(arm: str):
    """The tick the Knight's path goal becomes the Tesla's cell (20, 23)."""
    b = battle(arm)
    for _ in range(200):
        tick = json.loads(b.state_json())["tick"]
        if any(u[6] and tuple(u[6][0]) == (20, 23) for u in b.debug_units()):
            return tick
        b.step([], 1)
    return None


def first_tesla_hit(arm: str):
    """The tick of the Tesla's first 220-damage hit on the Knight (another source's hit must not pass for it)."""
    b = battle(arm)
    prev = None
    for _ in range(200):
        b.step([], 1)
        k = next((e for e in json.loads(b.state_json())["entities"] if e[1] == 1 and 1000 < e[8] < 3000), None)
        if k is None:
            return None
        if prev is not None and prev - k[7] == 220:
            return json.loads(b.state_json())["tick"]
        prev = k[7]
    return None


def test_the_knight_locks_on_the_tick_after_the_tesla_surfaces():
    assert knight_turns("client16402_surface_attacking") == 241


def test_the_teslas_first_hit_lands_seven_ticks_after_it_surfaces():
    assert first_tesla_hit("client16402_surface_attacking") == 247


def test_the_old_arm_is_todays_engine():
    assert knight_turns("engine_rising_phase") == 240
    assert first_tesla_hit("engine_rising_phase") == 263
