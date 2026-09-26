"""An Inferno's damage ramps with its attack progress on the current target (combat.VARIABLE_DAMAGE).

WHAT THIS PINS. On the 16.402 corpus an Inferno Tower at level 11 hits a Giant for 43, 43, 43, 43
(progress 400 to 1600), then 158 five times (2000 to 3600), then 847 (4000 on): Damage, VariableDamage2 and
VariableDamage3 of the 15.535.29 table, scaled by level, switched at VariableDamageTime1 = 2000 and at 2000 +
VariableDamageTime2 = 4000 of attack progress. The progress is not reset by a hit; it restarts when the Inferno takes a
new target. The Inferno Dragon does the same with 35, 120 and 422. The engine dealt the first stage on every hit.

WHY THE CONTROLS ARE HERE. A ramp keyed to wall time since the Inferno was PLACED, or one that never restarts, passes
the first Giant; a second Giant, taken after the first dies, must start again at the first stage. And the old arm must
be today's engine, which is also what shows the scenario reaches the second stage at all.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `variable_damage_first_stage` -- the Inferno deals its first-stage Damage on every hit:
    test_the_inferno_ramps_on_its_attack_progress.
  * `stun_keeps_variable_damage_ramp` -- a stun pauses the ramp, so a Zapped Inferno resumes in its second stage:
    test_a_stun_resets_the_ramp.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.VARIABLE_DAMAGE"
NEW_ARM, OLD_ARM = "client16402_attack_progress_stages", "not_modelled"
# ENTITY_FIELDS: 0 uid, 1 team, 3 card_id, 4 tower_slot, 5 x, 6 y, 7 hp
UID, TEAM, CARD, SLOT, X, Y, HP = 0, 1, 3, 4, 5, 6, 7
#: the corpus's level-11 hit sequences on a Giant, first ten hits
TOWER_RAMP = [43] * 4 + [158] * 5 + [847]
DRAGON_RAMP = [35] * 4 + [120] * 5 + [422]


def victim_drops(arm: str, inferno: str, victims: list, ticks: int = 400) -> dict:
    """A blue Inferno at (9000, 17500), just over the river, out of every tower's reach, and red `victims` (card names)
    queued in front of it, the first nearest. Every hp drop of each victim, in order, keyed by its card name."""
    deck = [inferno, *victims]
    b = royalesim.Battle(deck, [[0, 1, 2], [0, 1, 2]], calibration_overrides={KEY: json.dumps(arm)})
    units = [(0, 0, 9000 * SUB, 17500 * SUB, -1)]
    units += [(1, 1 + k, 9000 * SUB, (20000 + 3500 * k) * SUB, -1) for k in range(len(victims))]
    b.reset(0, [[0, 1] * 4, [0, 1] * 4], 0, 200, [10_000, 10_000], None, units)
    last, drops = {}, {}
    for _ in range(ticks):
        for e in json.loads(b.state_json())["entities"]:
            if e[TEAM] == 1 and e[SLOT] < 0 and 1 <= e[CARD] <= len(victims):
                name = deck[e[CARD]]
                if name in last and e[HP] < last[name]:
                    drops.setdefault(name, []).append(last[name] - e[HP])
                last[name] = e[HP]
        b.step([], 1)
    return drops


def giant_drops(arm: str, inferno: str) -> list:
    return victim_drops(arm, inferno, ["Giant"]).get("Giant", [])


@pytest.mark.parametrize(("inferno", "ramp"), [("InfernoTower", TOWER_RAMP), ("InfernoDragon", DRAGON_RAMP)])
def test_the_inferno_ramps_on_its_attack_progress(inferno, ramp):
    got = giant_drops(NEW_ARM, inferno)
    assert got[: len(ramp)] == ramp, got


def test_the_ramp_restarts_on_a_new_target():
    """A Musketeer first: it dies in the second stage. The Giant behind it must then start again at 43; a counter that
    ran on would hit it for 158 or 847 at once."""
    drops = victim_drops(NEW_ARM, "InfernoTower", ["Musketeer", "Giant"], ticks=500)
    assert 158 in drops.get("Musketeer", []), f"the Musketeer did not reach the second stage: {drops}"
    assert drops.get("Giant", [])[:4] == [43] * 4, drops


def zapped_drops(arm: str) -> list:
    """The same Inferno Tower and one Giant; red Zaps the Inferno once the Giant has taken seven Inferno hits (progress
    2800, the second stage). Every Giant drop, in order."""
    deck = ["InfernoTower", "Giant", "Zap"]
    b = royalesim.Battle(deck, [[0, 1, 2], [0, 1, 2]], calibration_overrides={KEY: json.dumps(arm)})
    units = [(0, 0, 9000 * SUB, 17500 * SUB, -1), (1, 1, 9000 * SUB, 20000 * SUB, -1)]
    b.reset(0, [[0, 1, 2, 0, 1, 2, 0, 1]] * 2, 0, 200, [10_000, 10_000], None, units)
    last, drops, zapped = None, [], False
    for _ in range(400):
        ents = json.loads(b.state_json())["entities"]
        g = next((e for e in ents if e[TEAM] == 1 and e[SLOT] < 0 and e[CARD] == 1), None)
        if g is None:
            break
        if last is not None and g[HP] < last:
            drops.append(last - g[HP])
        last = g[HP]
        if len(drops) == 7 and not zapped:
            played = b.step([(1, 2, 9000 * SUB, 17500 * SUB)], 1)
            assert played, "the Zap play returned nothing"
            assert played[0][1] == 0, f"the Zap was refused: {played}"
            zapped = True
        else:
            b.step([], 1)
    return drops


def test_a_stun_resets_the_ramp():
    """The 15.535.29 inferno-zap scenario: a Zap at progress 2950 drops the Inferno's progress to 0; its next hits are
    43 x4, then 158 again. Zap damages the Inferno, not the Giant, so every drop here is the Inferno's."""
    got = zapped_drops(NEW_ARM)
    assert got[:12] == [43] * 4 + [158] * 3 + [43] * 4 + [158], got


def test_the_old_arm_is_todays_engine():
    got = giant_drops(OLD_ARM, "InfernoTower")
    assert got[:10] == [43] * 10, got
