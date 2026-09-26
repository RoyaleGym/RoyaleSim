"""An Electro Wizard fires two bolts an attack: both at a lone target, one each at two (combat.MULTIPLE_TARGETS).

WHAT THIS PINS. The card sets MultipleTargets = 2 and AllTargetsHit; its Damage is one bolt, 117 at level 11. On the
15.535.29 client a lone Giant loses 234 an attack, and once a Knight is in range too, the Giant and the Knight each lose
117 on the same frame. The 16.402 corpus's lone-target hit is 234 as well. The engine read neither column: one bolt,
117, at the target only.

WHY THE CONTROLS ARE HERE. "A lone target loses 234" also passes for a build that doubles Damage, which would then hit
two targets for 234 each; so the pair case must read 117 and 117. The old arm must be today's engine, 117 on the lone
Giant.

PLANTS. Each is a cfg in the engine source, aimed at the tests named. Prove one on a plant build of the module
in a scratch venv (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant maturin develop
--release`), then run this file: the named tests go red and the rest stay green.
  * `multiple_targets_one_bolt` -- the new arm delivers one bolt an attack, as not_read does:
    test_a_lone_target_takes_both_bolts, test_two_targets_take_one_bolt_each.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.MULTIPLE_TARGETS"
NEW_ARM, OLD_ARM = "client_bolts_per_target", "not_read"
# ENTITY_FIELDS: 1 team, 3 card_id, 4 tower_slot, 7 hp
TEAM, CARD, SLOT, HP = 1, 3, 4, 7


def drops(arm: str, victims: list, ticks: int = 300) -> list:
    """A blue Electro Wizard at (9000, 18000), out of every tower's reach, and red `victims` side by side 4,500 in front
    of it. Per tick with any drop, {victim card: hp lost}."""
    deck = ["ElectroWizard", *victims]
    b = royalesim.Battle(deck, [[0, 1, 2], [0, 1, 2]], calibration_overrides={KEY: json.dumps(arm)})
    units = [(0, 0, 9000 * SUB, 18000 * SUB, -1)]
    units += [(1, 1 + k, (9000 + 1150 * k) * SUB, 22500 * SUB, -1) for k in range(len(victims))]
    b.reset(0, [[0, 1] * 4, [0, 1] * 4], 0, 200, [10_000, 10_000], None, units)
    last, out = {}, []
    for _ in range(ticks):
        row = {}
        for e in json.loads(b.state_json())["entities"]:
            if e[TEAM] == 1 and e[SLOT] < 0 and 1 <= e[CARD] <= len(victims):
                name = deck[e[CARD]]
                if name in last and e[HP] < last[name]:
                    row[name] = last[name] - e[HP]
                last[name] = e[HP]
        if row:
            out.append(row)
        b.step([], 1)
    return out


def test_a_lone_target_takes_both_bolts():
    got = drops(NEW_ARM, ["Giant"])
    assert [d["Giant"] for d in got[:4]] == [234] * 4, got


def test_two_targets_take_one_bolt_each():
    got = drops(NEW_ARM, ["Giant", "Knight"])
    assert {"Giant": 117, "Knight": 117} in got, got


def test_the_old_arm_is_todays_engine():
    """Checked on 7bfcfd2: 117 on the lone Giant every 36 ticks (212, 248, 284, ...), and with a Knight beside it the
    Knight is never hit."""
    got = drops(OLD_ARM, ["Giant"])
    assert [d["Giant"] for d in got[:4]] == [117] * 4, got
