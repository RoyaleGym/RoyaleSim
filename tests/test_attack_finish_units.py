"""The Zappies and the Electro Dragon take their next target a tick after a kill (combat.POST_KILL_RETARGET_WAIT).

WHAT THIS PINS. Under the shipped attack-finish rule a unit whose target is removed waits 6 ticks, unless its card sets
OverrideAttackFinishTime (or one of the rule's other two conditions holds), in which case it takes its next target after
1 tick. The value names those cards in `attack_finish_override_units` because cards.json does not carry the column. The
client 15.535.29 character table sets OverrideAttackFinishTime on seven troop rows and one hero form. The list names
four (Valkyrie, Bowler, Princess, ElectroWizard) and misses two basic cards: MiniZapMachine (the Zappies) and
ElectroDragon. The seventh is LittlePrince, a champion, left for the special cards. Measured on client 15.535.29, over
the scenario runs: 8 of 8 Zappies and 1 of 1 Electro Dragon whose target was removed took their next target after 1
tick. So did the listed Valkyrie (5 of 5) and Electro Wizard (2 of 2).

THE SCENARIO. A blue Zappy (MiniSparkys) with a red 100-hp Knight in reach, a full-hp red Knight beyond it. The Zappy's
first shot kills the weak Knight; the test reads the tick it takes the second.

WHICH VALUE. The shipped value lists four override units; the proposed one adds MiniZapMachine and ElectroDragon. The
tests pin each list through the battle's calibration.
"""

from __future__ import annotations

import json

import pytest

royalesim = pytest.importorskip("royalesim")

SUB = royalesim.SUBTILE_PER_MILLITILE
KEY = "combat.POST_KILL_RETARGET_WAIT"
ADDED = ["MiniZapMachine", "ElectroDragon"]
F = {name: i for i, name in enumerate(royalesim.ENTITY_FIELDS)}
CARDS = ["MiniSparkys", "Knight"]
SHIPPED = json.loads(royalesim.EMBEDDED_CALIBRATION_JSON)["combat"]["POST_KILL_RETARGET_WAIT"]["value"]


def overrides(extended) -> dict:
    v = dict(SHIPPED)
    units = [u for u in v["attack_finish_override_units"] if u not in ADDED]
    v["attack_finish_override_units"] = units + (ADDED if extended else [])
    return {KEY: json.dumps(v)}


def retarget_lag(extended):
    """Ticks from the weak Knight's removal to the Zappy's next target."""
    b = royalesim.Battle(CARDS, [[0, 1, 2], [0, 1, 2]], calibration_overrides=overrides(extended))
    b.reset(0, [[0] * 8, [1] * 8], 0, 200, [10_000, 10_000], None,
            [(0, 0, 9500 * SUB, 11000 * SUB, -1), (1, 1, 9500 * SUB, 13200 * SUB, 100),
             (1, 1, 10500 * SUB, 13600 * SUB, -1)])
    zappy = weak = None
    gone = None
    for t in range(1, 60):
        b.step([], 1)
        ents = {e[F["uid"]]: e for e in json.loads(b.state_json())["entities"] if e[F["tower_slot"]] < 0}
        if zappy is None:
            (zappy,) = [u for u, e in ents.items() if e[F["team"]] == 0]
            (weak,) = [u for u, e in ents.items() if e[F["team"]] == 1 and e[F["hp"]] == 100]
            assert ents[zappy][F["target_uid"]] == weak, "precondition: the Zappy did not start on the weak Knight"
        if gone is None and weak not in ents:
            gone = t
        if gone is not None and ents[zappy][F["target_uid"]] >= 0:
            return t - gone
    raise AssertionError("the weak Knight never died, or the Zappy never took another target")


def test_the_zappy_takes_its_next_target_a_tick_after_the_kill():
    assert retarget_lag(extended=True) == 1


def test_the_shipped_list_makes_the_zappy_wait_six_ticks():
    assert retarget_lag(extended=False) == 6


def test_the_shipped_list_names_every_loadable_override_unit():
    """The shipped value, with no override, names both units."""
    missing = [u for u in ADDED if u not in SHIPPED["attack_finish_override_units"]]
    assert not missing, f"{KEY}.value.attack_finish_override_units lacks {missing}"
