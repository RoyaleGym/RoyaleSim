"""A Battle Healer heals her own troops when she is deployed (spawner.SPAWN_AREA_OBJECT_SCOPE).

WHAT THIS PINS. On client 15.535.29 (the Battle Healer spawn-heal scenario) a Knight at 1,766 max hp that has taken
damage gains +50 on the Battle Healer's 4th, 9th, 14th and 19th frames, four times, from her SpawnAreaObject
BattleHealerSpawnHeal: radius 3000, own troops, buff BattleHealerSpawnBuff at 79 hp a second every 250 ms, which is
50 a pulse at level 11. The engine reads SpawnAreaObject on morph targets at most, so she heals nobody.

THE AMOUNT needs status.BUFF_PULSE_AMOUNT = scaled_per_second_times_frequency: 79 at level 11 is 202 a second, and a
quarter of that is 50. Today's arm takes the quarter first (19) and scales it (48).

WHY THE CONTROLS ARE HERE. "The Knight gained hp" needs a heal from somewhere, so the control is the same Knight with no
Battle Healer, which gains nothing; and the pulses are pinned to her frames, so a heal on the wrong clock fails.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `spawn_area_effect_unread` -- the Battle Healer heals nobody on deploy under every_row too:
    test_her_spawn_heal_pulses_four_times_on_her_frames.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "spawner.SPAWN_AREA_OBJECT_SCOPE"
NEW_ARM, OLD_ARM = "every_row", "morph_targets_only"
# ENTITY_FIELDS: 1 team, 3 card_id, 4 tower_slot, 7 hp
TEAM, CARD, SLOT, HP = 1, 3, 4, 7
HEAL, HEAL_FRAMES = 50, [4, 9, 14, 19]
PULSE_KEY, PULSE_ARM = "status.BUFF_PULSE_AMOUNT", "scaled_per_second_times_frequency"


def knight_gains(arm: str | None, with_healer: bool = True, ticks: int = 40) -> list[tuple[int, int]]:
    """A blue Knight at (9000, 13000) with 1,000 hp, and (optionally) a blue Battle Healer at (11000, 13000), 2,000
    away; no enemy troop, out of every tower's reach. The Knight's hp gains as (tick, amount), with tick 0 the frame
    both first appear."""
    overrides = {} if arm is None else {KEY: json.dumps(arm), PULSE_KEY: json.dumps(PULSE_ARM)}
    b = royalesim.Battle(["Knight", "BattleHealer"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    units = [(0, 0, 9000 * SUB, 13000 * SUB, 1000)]
    if with_healer:
        units.append((0, 1, 11000 * SUB, 13000 * SUB, -1))
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, units)
    prev, gains = None, []
    for t in range(ticks):
        k = next(
            (e for e in json.loads(b.state_json())["entities"] if e[TEAM] == 0 and e[SLOT] < 0 and e[CARD] == 0), None
        )
        if k is None:
            break
        if prev is not None and k[HP] > prev:
            gains.append((t, k[HP] - prev))
        prev = k[HP]
        b.step([], 1)
    return gains


def test_her_spawn_heal_pulses_four_times_on_her_frames():
    gains = knight_gains(NEW_ARM)
    assert gains == [(f, HEAL) for f in HEAL_FRAMES], gains


def test_no_healer_no_heal():
    assert knight_gains(NEW_ARM, with_healer=False) == []


def test_the_old_arm_is_todays_engine():
    """Checked on the shared build of 2026-09-25 (1830284) with the key dropped: the Knight gains nothing."""
    assert knight_gains(OLD_ARM) == []
