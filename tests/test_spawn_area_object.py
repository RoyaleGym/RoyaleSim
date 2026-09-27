"""A Battle Healer heals her own troops when she is deployed (spawner.SPAWN_AREA_OBJECT_SCOPE).

WHAT THIS PINS. On client 15.535.29 (the Battle Healer spawn-heal scenario) a Knight at 1,766 max hp that has taken
damage gains +50 on the Battle Healer's 4th, 9th, 14th and 19th frames, four times, from her SpawnAreaObject
BattleHealerSpawnHeal: radius 3000, own troops, buff BattleHealerSpawnBuff at 79 hp a second every 250 ms, which is
50 a pulse at level 11. The engine reads SpawnAreaObject on morph targets at most, so she heals nobody.

THE AMOUNT needs status.BUFF_PULSE_AMOUNT = scaled_per_second_times_frequency: 79 at level 11 is 202 a second, and a
quarter of that is 50. Today's arm takes the quarter first (19) and scales it (48).

THE SCENE is the client's: she is played from the hand, and the frames count from her first frame, the tick after the
play. Her heal lands on that frame. Placed at reset instead, she would land it one tick after her first frame (state.rs
`spawn_now`, the spawn area block), and every pulse would move one tick later.

THE FRAMES. Her buff pulses every 250 ms, and its first pulse falls 200 ms after her heal lands: the tick it lands
counts on its clock (spell.rs `step_spells`, the one-shot area branch). A Poison's buff, by contrast, first pulses a
whole HitFrequency after its area applies it (tests/test_area_effect_clock.py), so status.BUFF_PULSE_TIMING stays as it
is.

WHY THE CONTROLS ARE HERE. "The Knight gained hp" needs a heal from somewhere, so the control is the same Knight with no
Battle Healer, which gains nothing; and the pulses are pinned to her frames, so a heal on the wrong clock fails.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `spawn_area_effect_unread` -- the Battle Healer heals nobody on deploy under every_row too:
    test_her_spawn_heal_pulses_four_times_on_her_frames.
  * `spawn_area_pulse_full_period` -- her buff first pulses a whole HitFrequency after it lands, as a Poison's does:
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
    """A blue Knight at (9000, 13000) with 1,000 hp, placed at reset, and (optionally) a blue Battle Healer played from
    the hand at (11000, 13000), 2,000 away; no enemy troop, out of every tower's reach. The Knight's hp gains as (tick,
    amount), with tick 0 the tick after the play: her first frame."""
    overrides = {} if arm is None else {KEY: json.dumps(arm), PULSE_KEY: json.dumps(PULSE_ARM)}
    b = royalesim.Battle(["Knight", "BattleHealer"], [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides)
    b.reset(0, [[1] * 8, [1] * 8], 0, 200, [10_000, 10_000], None, [(0, 0, 9000 * SUB, 13000 * SUB, 1000)])
    played = b.step([(0, 0, 11000 * SUB, 13000 * SUB)] if with_healer else [], 1)
    if with_healer:
        assert played, "the Battle Healer play returned nothing"
        assert played[0][1] == 0, f"the Battle Healer was refused: {played}"
    prev, gains = None, []
    for t in range(ticks):
        ents = [e for e in json.loads(b.state_json())["entities"] if e[TEAM] == 0 and e[SLOT] < 0]
        if t == 0 and with_healer:
            assert any(e[CARD] == 1 for e in ents), "precondition: tick 0 is not her first frame"
        k = next((e for e in ents if e[CARD] == 0), None)
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
